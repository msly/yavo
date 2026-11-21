mod allocations;
mod ast;
mod cache;
mod check;
mod closure_free;
mod codegen;
mod diag;
mod fix;
mod fix_plan;
mod lexer;
mod parser;
mod process;
mod project;
mod toolchain;
mod types;
mod verify;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use diag::Diags;

const EXIT_COMPILE_ERROR: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_TOOLCHAIN: i32 = 3;
const EXIT_INTERNAL: i32 = 4;

const USAGE: &str = "\
yavo - the Yavo compiler prototype

USAGE:
    yavo check <file.yv> [--emit-c <file.c>] [--json-diagnostics]
    yavo fix   <file.yv> [--dry-run] [--json-diagnostics]
    yavo build <file.yv> [-o <output>] [--release] [--target <target>] [--emit-c <file.c>] [--json-diagnostics]
    yavo run   <file.yv> [--release] [--target <target>] [--json-diagnostics]
    yavo allocations <function> --file <file.yv> [--json]
    yavo verify <file.yv> [--target <target>] [--sanitize] [--reproducible] [--json]
                [--output-dir <dir>] [--timeout-ms <ms>] [--build-timeout-ms <ms>]
                [--stdin <file>] [--expect-stdout <file>] [--expect-stderr <file>] [--no-run]
    yavo toolchain [status | install]
    yavo --version

FIXES:
    `fix` applies verified automatic repairs to the source file. Candidates are
    reported for a separate choice. `--dry-run` previews without writing.
    `--json` is an alias for `--json-diagnostics`.

TARGETS (cross-compilation uses the pinned zig toolchain; `run` only accepts targets this machine can execute):
    x86_64-linux-musl, aarch64-linux-musl, x86_64-windows-gnu, aarch64-windows-gnu, x86_64-macos, aarch64-macos

EXIT CODES:
    0 success, 1 compile/check failure, 2 usage error, 3 C toolchain unavailable, 4 internal error,
    5 verification incomplete. The prototype records the required audit as not_run.
    `yavo run` exits with the program's own status (101 on panic; 128+N if signal N killed it).

BUILD CACHE:
    Binaries are cached by the generated C, runtime, toolchain, flags and target, so rebuilding
    unchanged code skips the C compiler. YAVO_NO_CACHE=1 disables it, YAVO_CACHE_DIR moves it
    (default ~/.cache/yavo), YAVO_CACHE_MAX_ENTRIES limits it (256). `verify` never uses it.";

struct Options {
    input: PathBuf,
    output: Option<PathBuf>,
    emit_c: Option<PathBuf>,
    release: bool,
    json: bool,
    dry_run: bool,
    target: Option<&'static toolchain::Target>,
}

fn usage_error(msg: &str) -> ! {
    eprintln!("error: {msg}\n\n{USAGE}");
    exit(EXIT_USAGE);
}

fn parse_options(args: &[String], for_fix: bool) -> Options {
    let mut input = None;
    let mut opts =
        Options { input: PathBuf::new(), output: None, emit_c: None, release: false, json: false, dry_run: false, target: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--release" => opts.release = true,
            "--json-diagnostics" | "--json" => opts.json = true,
            "--dry-run" if for_fix => opts.dry_run = true,
            "-o" => opts.output = Some(it.next().unwrap_or_else(|| usage_error("`-o` needs a path")).into()),
            "--emit-c" => opts.emit_c = Some(it.next().unwrap_or_else(|| usage_error("`--emit-c` needs a path")).into()),
            "--target" => {
                let name = it.next().unwrap_or_else(|| usage_error("`--target` needs a target name"));
                opts.target = Some(toolchain::Target::lookup(name).unwrap_or_else(|| {
                    usage_error(&format!("unknown target `{name}`; supported targets: {}", toolchain::Target::names()))
                }));
            }
            flag if flag.starts_with('-') => usage_error(&format!("unknown option `{flag}`")),
            path if input.is_none() => input = Some(PathBuf::from(path)),
            extra => usage_error(&format!("unexpected argument `{extra}`")),
        }
    }
    opts.input = input.unwrap_or_else(|| usage_error("missing input file"));
    if for_fix && (opts.output.is_some() || opts.emit_c.is_some() || opts.release || opts.target.is_some()) {
        usage_error("`fix` only accepts an input file, --dry-run and --json-diagnostics (or --json)");
    }
    opts
}

/// Front end: source text to C. Prints diagnostics and returns `None` on errors.
fn compile_to_c(opts: &Options) -> Option<String> {
    let display = opts.input.to_string_lossy().replace('\\', "/");
    let bytes = fs::read(&opts.input).unwrap_or_else(|e| {
        eprintln!("error: cannot read {display}: {e}");
        exit(EXIT_USAGE);
    });
    let mut diags = Diags::default();
    let src = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            let span = diag::Span { start: 0, end: 0, line: 1, col: 1 };
            diags.error("E0020", span, "source file is not valid UTF-8");
            report(&display, "", &mut diags, opts.json);
            return None;
        }
    };
    let project = project::Project::load(&opts.input, &src, &mut diags).unwrap_or_else(|e| {
        eprintln!("error: cannot load {display}: {e}");
        exit(EXIT_USAGE);
    });
    let typed = if diags.has_errors() { None } else { check::check(&project.program, &mut diags) };
    if !project.modular {
        if let Err(e) = fix::prepare(&src, &project.program, &mut diags) {
            eprintln!("error: {e}");
            exit(EXIT_INTERNAL);
        }
    }
    report_project(&project, &mut diags, opts.json);
    typed.map(|p| codegen::generate(&p, &display))
}

fn report(path: &str, src: &str, diags: &mut Diags, json: bool) {
    diags.sort();
    if json {
        println!("{}", diag::render_json(path, src, diags));
    } else if !diags.items.is_empty() {
        eprint!("{}", diag::render_human(path, src, diags));
    }
}

fn report_project(project: &project::Project, diags: &mut Diags, json: bool) {
    diags.sort();
    if json { println!("{}", project.json(diags)); }
    else if !diags.items.is_empty() { eprint!("{}", project.human(diags)); }
}

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("yavo-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap_or_else(|e| {
        eprintln!("error: cannot create {}: {e}", dir.display());
        exit(EXIT_INTERNAL);
    });
    dir
}

fn default_output(input: &Path, target: Option<&toolchain::Target>) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_os_string()).unwrap_or_else(|| "a".into());
    let mut out = PathBuf::from(stem);
    let windows = match target {
        Some(t) => t.os == "windows",
        None => cfg!(windows),
    };
    if windows {
        out.set_extension("exe");
    }
    out
}

fn build(opts: &Options, output: &Path) {
    let Some(c_code) = compile_to_c(opts) else { exit(EXIT_COMPILE_ERROR) };
    let write = |p: &Path, s: &str| {
        fs::write(p, s).unwrap_or_else(|e| {
            eprintln!("error: cannot write {}: {e}", p.display());
            exit(EXIT_INTERNAL);
        })
    };
    if let Some(path) = &opts.emit_c {
        write(path, &c_code);
    }
    let tc = toolchain::find().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        exit(EXIT_TOOLCHAIN);
    });
    if opts.target.is_some() && !tc.can_cross_compile() {
        eprintln!("error: `--target` needs the pinned zig toolchain; unset YAVO_CC ({})", tc.description);
        exit(EXIT_TOOLCHAIN);
    }
    if tc.can_cross_compile() {
        if let Some(root) = cache::root() {
            match build_cached(&tc, &root, &c_code, opts, output) {
                Ok(()) => return,
                // A cache I/O problem (read-only home, full disk) must not fail the build.
                Err(CacheError::Io(e)) => {
                    if std::env::var_os("YAVO_CACHE_TRACE").is_some() {
                        eprintln!("yavo: build cache unavailable ({e}); building without it");
                    }
                }
                Err(CacheError::Compile(e)) => {
                    eprintln!("error: {e}");
                    exit(EXIT_INTERNAL);
                }
            }
        }
    }
    let scratch = scratch_dir();
    let c_file = scratch.join("main.c");
    write(&c_file, &c_code);
    write(&scratch.join("yavo_rt.h"), codegen::RUNTIME_HEADER);
    let result = tc.compile(&c_file, &scratch, output, opts.release, opts.target);
    let _ = fs::remove_dir_all(&scratch);
    if let Err(e) = result {
        eprintln!("error: {e}");
        exit(EXIT_INTERNAL);
    }
}

enum CacheError {
    Io(std::io::Error),
    Compile(String),
}

impl From<std::io::Error> for CacheError {
    fn from(e: std::io::Error) -> Self { CacheError::Io(e) }
}

/// Builds through the content-addressed back-end cache (see `cache.rs`).
fn build_cached(
    tc: &toolchain::Toolchain, root: &Path, c_code: &str, opts: &Options, output: &Path,
) -> Result<(), CacheError> {
    let flags = tc.flags(opts.release, opts.target);
    let key = cache::key(&cache::Inputs {
        c_code,
        runtime_header: codegen::RUNTIME_HEADER,
        toolchain_lock: toolchain::lock_contents(),
        compiler: &tc.program,
        args: &flags,
        native: opts.target.is_none(),
    });
    let trace = std::env::var_os("YAVO_CACHE_TRACE").is_some();
    let entry = cache::Entry::open(root, &key)?;
    if entry.restore(output) {
        if trace { eprintln!("yavo: build cache hit {key}"); }
        return Ok(());
    }
    if trace { eprintln!("yavo: build cache miss {key}"); }
    entry.stage(c_code, codegen::RUNTIME_HEADER)?;
    let built = entry.scratch_output();
    let result = tc.compile(&entry.c_file(), &entry.dir, &built, opts.release, opts.target);
    if let Err(e) = result {
        let _ = fs::remove_file(&built);
        return Err(CacheError::Compile(e));
    }
    let published = entry.publish(&built, output);
    let _ = fs::remove_file(&built);
    published?;
    cache::evict(root, &key);
    Ok(())
}

/// Reports how the program ended when `yavo run` cannot pass its exit code through.
fn program_status(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            let name = match sig {
                4 => "SIGILL",
                6 => "SIGABRT",
                7 => "SIGBUS",
                8 => "SIGFPE",
                9 => "SIGKILL",
                11 => "SIGSEGV",
                13 => "SIGPIPE",
                15 => "SIGTERM",
                24 => "SIGXCPU",
                _ => "signal",
            };
            let hint = if matches!(sig, 7 | 11) {
                "\n  note: the runtime reports stack overflow as P-STACK, so this points to a bug in the Yavo runtime or compiler; please report it\n  note: output the program had not yet flushed is lost"
            } else {
                ""
            };
            eprintln!("error: the program was terminated by signal {sig} ({name}){hint}");
            return 128 + sig;
        }
    }
    eprintln!("error: the program ended without an exit status");
    EXIT_INTERNAL
}

fn run_allocations(args: &[String]) -> i32 {
    let mut subject = None;
    let mut input = None;
    let mut json = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--file" => {
                if input.is_some() { usage_error("duplicate --file"); }
                input = Some(PathBuf::from(it.next().unwrap_or_else(|| usage_error("--file needs a path"))));
            }
            "--json" | "--json-diagnostics" => json = true,
            flag if flag.starts_with('-') => usage_error(&format!("unknown option {flag}")),
            name if subject.is_none() => subject = Some(name.to_string()),
            extra => usage_error(&format!("unexpected argument {extra}")),
        }
    }
    let subject = subject.unwrap_or_else(|| usage_error("allocations needs a function name"));
    let input = input.unwrap_or_else(|| usage_error("allocations needs --file <file.yv>"));
    let source = match fs::read_to_string(&input) {
        Ok(source) => source,
        Err(e) => { eprintln!("error: cannot read {}: {e}", input.display()); return EXIT_USAGE; }
    };
    let path = input.to_string_lossy().replace('\\', "/");
    let mut diags = Diags::default();
    let project = match project::Project::load(&input, &source, &mut diags) {
        Ok(project) => project,
        Err(e) => { eprintln!("error: cannot load {}: {e}", input.display()); return EXIT_USAGE; }
    };
    let typed = if diags.has_errors() { None } else { check::check(&project.program, &mut diags) };
    diags.sort();
    if diags.has_errors() || typed.is_none() {
        report_project(&project, &mut diags, json);
        return EXIT_COMPILE_ERROR;
    }
    match allocations::analyze(&project.program, &typed.unwrap(), &project.query_name(&subject), &path, &project.source) {
        Ok(mut result) => {
            if project.modular {
                project.map_operation_locations(&mut result);
                result["subject"] = serde_json::json!(subject);
                result["source_sha256"] = serde_json::json!(diag::source_hash(&source));
                result["project_sha256"] = serde_json::json!(project.hash());
                result["sources"] = project.manifest();
            }
            result["diagnostics"] = project.json(&diags);
            if json {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprint!("{}", project.human(&diags));
                println!("{}: {}", subject, result["status"].as_str().unwrap_or("unknown"));
                println!("{}", serde_json::to_string_pretty(&result["operations"]).unwrap());
            }
            0
        }
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({"schema":"yavo-allocations/1", "subject":subject,
                    "status":"unknown", "error":e, "source_sha256":diag::source_hash(&source)}));
            } else { eprintln!("error: {e}"); }
            EXIT_USAGE
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        println!("{USAGE}");
        exit(EXIT_USAGE);
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "--version" | "-V" => println!("yavo {} (prototype, zig {})", env!("CARGO_PKG_VERSION"), toolchain::lock().version),
        "--help" | "-h" | "help" => println!("{USAGE}"),
        "check" => {
            let opts = parse_options(rest, false);
            let Some(code) = compile_to_c(&opts) else { exit(EXIT_COMPILE_ERROR) };
            if let Some(path) = &opts.emit_c {
                if let Err(e) = fs::write(path, code) {
                    eprintln!("error: cannot write {}: {e}", path.display());
                    exit(EXIT_INTERNAL);
                }
            }
        }
        "fix" => {
            let opts = parse_options(rest, true);
            if let Ok(source) = fs::read_to_string(&opts.input) {
                let mut diagnostics = Diags::default();
                let tokens = lexer::lex_file(&source, &mut diagnostics);
                let program = parser::parse(tokens, &source, &mut diagnostics);
                if let Some(import) = program.uses.iter().find(|u| !u.path.starts_with("std/")) {
                    let mut diagnostics = Diags::default();
                    diagnostics.error("U0001", import.span, "multi-file source repairs are not implemented yet")
                        .note("check/build/run/allocations/verify load the full project; fix does not write any file");
                    report(&opts.input.to_string_lossy(), &source, &mut diagnostics, opts.json);
                    exit(EXIT_COMPILE_ERROR);
                }
            }
            match fix::run(&opts.input, opts.dry_run, opts.json) {
                Ok(code) => exit(code),
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    exit(e.exit_code);
                }
            }
        }
        "allocations" => exit(run_allocations(rest)),
        "verify" => {
            let options = verify::Options::parse(rest).unwrap_or_else(|e| usage_error(&e));
            match verify::run(options) {
                Ok(code) => exit(code),
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    exit(e.exit_code);
                }
            }
        }
        "build" => {
            let opts = parse_options(rest, false);
            let output = opts.output.clone().unwrap_or_else(|| default_output(&opts.input, opts.target));
            build(&opts, &output);
        }
        "run" => {
            let opts = parse_options(rest, false);
            if let Some(t) = opts.target {
                if !t.runs_on_host() {
                    usage_error(&format!(
                        "a `{}` binary cannot run on this machine; use `yavo build --target {}` and run it there",
                        t.name, t.name
                    ));
                }
            }
            let dir = scratch_dir().with_extension("bin");
            let _ = fs::create_dir_all(&dir);
            let exe = dir.join(default_output(&opts.input, opts.target));
            build(&opts, &exe);
            let status = Command::new(&exe).status();
            let _ = fs::remove_dir_all(&dir);
            match status {
                Ok(s) => exit(program_status(s)),
                Err(e) => {
                    eprintln!("error: cannot start {}: {e}", exe.display());
                    exit(EXIT_INTERNAL);
                }
            }
        }
        "toolchain" => match rest.first().map(String::as_str).unwrap_or("status") {
            "status" => match toolchain::find() {
                Ok(tc) => println!("C toolchain: {}", tc.description),
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(EXIT_TOOLCHAIN);
                }
            },
            "install" => match toolchain::install() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(EXIT_TOOLCHAIN);
                }
            },
            other => usage_error(&format!("unknown toolchain command `{other}`")),
        },
        other => usage_error(&format!("unknown command `{other}`")),
    }
}
