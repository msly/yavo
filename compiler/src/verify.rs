//! Evidence-bound verification. A build, a target execution, a sanitizer run and
//! an audit are distinct checks; missing evidence never becomes a pass.
use crate::{check, codegen, diag, process, project, toolchain};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct Options {
    input: PathBuf,
    target: Option<&'static toolchain::Target>,
    sanitize: bool,
    reproducible: bool,
    json: bool,
    no_run: bool,
    output_dir: PathBuf,
    timeout: Duration,
    build_timeout: Duration,
    stdin: Option<PathBuf>,
    expect_stdout: Option<PathBuf>,
    expect_stderr: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut opts = Self {
            input: PathBuf::new(), target: None, sanitize: false, reproducible: false,
            json: false, no_run: false, output_dir: PathBuf::from("target/yavo-verify"),
            timeout: Duration::from_secs(5), build_timeout: Duration::from_secs(120),
            stdin: None, expect_stdout: None, expect_stderr: None,
        };
        let mut input = None;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--json" | "--json-diagnostics" => opts.json = true,
                "--sanitize" => opts.sanitize = true,
                "--reproducible" => opts.reproducible = true,
                "--no-run" => opts.no_run = true,
                "--target" => {
                    let name = it.next().ok_or("--target needs a target name")?;
                    opts.target = Some(toolchain::Target::lookup(name)
                        .ok_or_else(|| format!("unknown target {name}; supported targets: {}", toolchain::Target::names()))?);
                }
                "--output-dir" => opts.output_dir = it.next().ok_or("--output-dir needs a path")?.into(),
                "--stdin" => opts.stdin = Some(it.next().ok_or("--stdin needs a path")?.into()),
                "--expect-stdout" => opts.expect_stdout = Some(it.next().ok_or("--expect-stdout needs a path")?.into()),
                "--expect-stderr" => opts.expect_stderr = Some(it.next().ok_or("--expect-stderr needs a path")?.into()),
                "--timeout-ms" | "--build-timeout-ms" => {
                    let value: u64 = it.next().ok_or_else(|| format!("{arg} needs milliseconds"))?
                        .parse().map_err(|_| format!("{arg} must be a positive integer"))?;
                    if value == 0 || value > 3_600_000 {
                        return Err(format!("{arg} must be between 1 and 3600000 milliseconds"));
                    }
                    if arg == "--timeout-ms" { opts.timeout = Duration::from_millis(value); }
                    else { opts.build_timeout = Duration::from_millis(value); }
                }
                flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
                path if input.is_none() => input = Some(PathBuf::from(path)),
                extra => return Err(format!("unexpected argument {extra}")),
            }
        }
        opts.input = input.ok_or("verify needs an input file")?;
        Ok(opts)
    }
}

pub struct Error { pub message: String, pub exit_code: i32 }
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self { Self { message: e.to_string(), exit_code: 4 } }
}
fn input_error(path: &Path, e: io::Error) -> Error {
    Error { message: format!("cannot read {}: {e}", path.display()), exit_code: 2 }
}

fn digest(bytes: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(bytes)) }
fn digest_parts(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("sha256:{:x}", hash.finalize())
}
fn file_hash(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hash.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}
fn read_optional(path: &Option<PathBuf>) -> Result<Option<Vec<u8>>, Error> {
    path.as_ref().map(|p| fs::read(p).map_err(|e| input_error(p, e))).transpose()
}

fn host_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-linux-{}", if cfg!(target_env = "musl") { "musl" } else { "gnu" }),
        "windows" => format!("{arch}-windows-{}", if cfg!(target_env = "msvc") { "msvc" } else { "gnu" }),
        os => format!("{arch}-{os}"),
    }
}
fn sanitizer_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-linux-gnu"),
        "windows" => format!("{arch}-windows-gnu"),
        os => format!("{arch}-{os}"),
    }
}

fn run_directory(parent: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(parent)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    for counter in 0..100 {
        let dir = parent.join(format!("run-{}-{stamp}-{counter}", std::process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => return fs::canonicalize(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "cannot create a fresh verification directory"))
}

fn pending(id: &str, name: &str, required: bool, target: &str, reason: &str) -> Value {
    json!({"id":id, "name":name, "required":required, "status":"not_run",
        "target":target, "artifact":null, "artifact_path":null, "ran_on":null, "reason":reason})
}
fn set_status(check: &mut Value, status: &str, reason: &str) {
    check["status"] = json!(status);
    check["reason"] = json!(reason);
}

/// Keep OS facilities needed to start the compiler, but do not inherit compiler,
/// loader, sanitizer or language flags from a development shell.
fn clean_environment(command: &mut Command) {
    let allowed = ["PATH", "HOME", "USERPROFILE", "LOCALAPPDATA", "SystemRoot",
        "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"];
    let vars: Vec<_> = allowed.iter()
        .filter_map(|key| std::env::var_os(key).map(|v| (*key, v))).collect();
    command.env_clear();
    for (key, value) in vars { command.env(key, value); }
    command.env("LC_ALL", "C").env("LANG", "C").env("TZ", "UTC").env("SOURCE_DATE_EPOCH", "0");
}

fn command_record(command: &Command) -> Value {
    let mut record = json!({"program":command.get_program().to_string_lossy(),
        "args":command.get_args().map(|x| x.to_string_lossy().into_owned()).collect::<Vec<_>>()});
    if let Some(cwd) = command.get_current_dir() { record["cwd"] = json!(cwd); }
    record
}
fn execution(cap: &process::Capture, command: &Command, elapsed: Duration) -> Value {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        cap.output.status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    json!({
        "command":command_record(command), "exit_code":cap.output.status.code(),
        "signal":signal,
        "success":cap.output.status.success(), "timed_out":cap.timed_out,
        "duration_ms":elapsed.as_millis(),
        "stdout":String::from_utf8_lossy(&cap.output.stdout),
        "stderr":String::from_utf8_lossy(&cap.output.stderr),
        "text_encoding":"utf-8-lossy",
        "stdout_valid_utf8":std::str::from_utf8(&cap.output.stdout).is_ok(),
        "stderr_valid_utf8":std::str::from_utf8(&cap.output.stderr).is_ok(),
        "stdout_truncated":cap.stdout_truncated, "stderr_truncated":cap.stderr_truncated,
        "stdout_bytes":cap.stdout_bytes, "stderr_bytes":cap.stderr_bytes,
        "stdout_sha256":cap.stdout_sha256.as_ref().map(|s| format!("sha256:{s}")),
        "stderr_sha256":cap.stderr_sha256.as_ref().map(|s| format!("sha256:{s}")),
        "capture_limit_bytes":process::CAPTURE_LIMIT
    })
}
fn merge(value: &mut Value, extra: Value) {
    value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
}

struct Artifact { path: PathBuf, hash: String, target: String }
impl Artifact {
    fn bind(&self, check: &mut Value) {
        check["artifact"] = json!(self.hash);
        check["artifact_path"] = json!(self.path);
        check["target"] = json!(self.target);
    }
}

struct Build { evidence: Value, artifact: Option<Artifact> }

#[cfg(windows)]
fn compiler_relative_path<'a>(path: &'a Path, directory: &Path) -> io::Result<&'a Path> {
    let relative = path.strip_prefix(directory).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "C compiler path is outside the verification directory")
    })?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "C compiler path must stay beneath the verification directory"));
    }
    Ok(relative)
}

fn compile_artifact(
    tc: &toolchain::Toolchain, c_file: &Path, include: &Path, output: &Path,
    target: &str, sanitizer: Option<&str>, fresh_cache: Option<&Path>, timeout: Duration,
) -> Result<Build, Error> {
    #[cfg(windows)]
    let program = std::path::absolute(&tc.program)?;
    #[cfg(not(windows))]
    let program = tc.program.clone();
    let mut cmd = Command::new(&program);
    // Keep canonical Win32/UNC paths for identity and filesystem access. Clang's
    // include search rejects verbatim paths, so give it paths beneath this cwd.
    #[cfg(windows)]
    cmd.current_dir(include);
    clean_environment(&mut cmd);
    // Bind the installed library directory instead of accepting ZIG_LIB_DIR.
    if let Some(dir) = program.parent() { cmd.env("ZIG_LIB_DIR", dir.join("lib")); }
    if let Some(cache) = fresh_cache {
        let local = cache.join("local");
        let global = cache.join("global");
        fs::create_dir_all(cache)?;
        // create_dir, not create_dir_all: reproducibility must start from empty caches.
        fs::create_dir(&local)?;
        fs::create_dir(&global)?;
        #[cfg(windows)]
        cmd.env("ZIG_LOCAL_CACHE_DIR", compiler_relative_path(&local, include)?)
            .env("ZIG_GLOBAL_CACHE_DIR", compiler_relative_path(&global, include)?);
        #[cfg(not(windows))]
        cmd.env("ZIG_LOCAL_CACHE_DIR", &local).env("ZIG_GLOBAL_CACHE_DIR", &global);
    }
    cmd.args(&tc.prefix_args).args(["-std=c17", "-fno-strict-aliasing", "-target", target]);
    if let Some(kind) = sanitizer {
        cmd.args(["-O1", "-g", "-fno-omit-frame-pointer", "-fno-sanitize-recover=all"]);
        // The pinned Zig driver drops address instrumentation when address and
        // undefined are passed as one comma-separated option. Separate options
        // expose missing runtimes; negative probes still verify actual detection.
        for tool in kind.split(',') { cmd.arg(format!("-fsanitize={tool}")); }
    } else {
        cmd.args(["-O2", "-s"]);
    }
    #[cfg(windows)]
    cmd.arg("-I").arg(".").arg(compiler_relative_path(c_file, include)?)
        .arg("-o").arg(compiler_relative_path(output, include)?);
    #[cfg(not(windows))]
    cmd.arg("-I").arg(include).arg(c_file).arg("-o").arg(output);
    cmd.stdin(Stdio::null());
    let start = Instant::now();
    let cap = match process::capture(&mut cmd, timeout) {
        Ok(cap) => cap,
        Err(e) => return Ok(Build { evidence: json!({
            "status":if e.kind() == io::ErrorKind::TimedOut { "timeout" } else { "failed" },
            "reason":format!("cannot execute pinned C compiler: {e}"),
            "command":command_record(&cmd), "ran_on":null, "runner_host":host_target()
        }), artifact: None }),
    };
    let mut evidence = execution(&cap, &cmd, start.elapsed());
    evidence["ran_on"] = json!(host_target());
    evidence["allocation_check"] = json!(false);
    evidence["cache"] = match fresh_cache {
        Some(path) => json!({"bypassed":true, "local":path.join("local"), "global":path.join("global")}),
        None => json!({"bypassed":false, "mode":"pinned_toolchain_default"}),
    };
    let (status, reason) = if cap.timed_out { ("timeout", "C compilation exceeded the deadline") }
        else if !cap.output.status.success() { ("failed", "C compiler rejected the generated translation unit") }
        else if cap.stdout_truncated || cap.stderr_truncated { ("failed", "C compiler output exceeded the capture limit") }
        else if !output.is_file() { ("failed", "C compiler succeeded without producing the requested artifact") }
        else { ("passed", "pinned C compiler produced the requested artifact") };
    set_status(&mut evidence, status, reason);
    let artifact = if status == "passed" {
        Some(Artifact { hash: file_hash(output)?, path: output.to_path_buf(), target: target.to_string() })
    } else { None };
    if let Some(a) = &artifact { a.bind(&mut evidence); }
    Ok(Build { evidence, artifact })
}

struct Assertions {
    stdin: Option<(PathBuf, Vec<u8>)>,
    stdout: Option<Vec<u8>>,
    stderr: Option<Vec<u8>>,
}
fn run_artifact(
    artifact: &Artifact, assertions: &Assertions, timeout: Duration, sanitizer: bool,
) -> Result<Value, Error> {
    let mut result = json!({"ran_on":host_target(), "tests_run":1,
        "test_kind":if assertions.stdout.is_some() || assertions.stderr.is_some() { "output_assertion" } else { "exit_status" },
        "expected_exit_code":0});
    artifact.bind(&mut result);
    let mut cmd = Command::new(&artifact.path);
    clean_environment(&mut cmd);
    if let Some((path, bytes)) = &assertions.stdin {
        fs::write(path, bytes)?;
        cmd.stdin(fs::File::open(path)?);
    }
    else { cmd.stdin(Stdio::null()); }
    if sanitizer {
        cmd.env("ASAN_OPTIONS", "halt_on_error=1:detect_leaks=1")
            .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
            .env("TSAN_OPTIONS", "halt_on_error=1:exitcode=66");
        result["sanitizer_environment"] = json!({
            "ASAN_OPTIONS":"halt_on_error=1:detect_leaks=1",
            "UBSAN_OPTIONS":"halt_on_error=1:print_stacktrace=1",
            "TSAN_OPTIONS":"halt_on_error=1:exitcode=66"
        });
    }
    let start = Instant::now();
    let cap = match process::capture(&mut cmd, timeout) {
        Ok(cap) => cap,
        Err(e) => {
            set_status(&mut result, if e.kind() == io::ErrorKind::TimedOut { "timeout" } else { "failed" },
                &format!("cannot execute artifact: {e}"));
            result["tests_run"] = json!(0);
            result["ran_on"] = Value::Null;
            result["command"] = command_record(&cmd);
            return Ok(result);
        }
    };
    merge(&mut result, execution(&cap, &cmd, start.elapsed()));
    // A retained prefix is not the stream. Never claim an exact match after truncation
    // or before EOF, even when the prefix happens to equal the expected file.
    result["stdout_matches"] = if cap.stdout_truncated || cap.stdout_sha256.is_none() { Value::Null }
        else { assertions.stdout.as_ref().map(|s| json!(*s == cap.output.stdout)).unwrap_or(Value::Null) };
    result["stderr_matches"] = if cap.stderr_truncated || cap.stderr_sha256.is_none() { Value::Null }
        else { assertions.stderr.as_ref().map(|s| json!(*s == cap.output.stderr)).unwrap_or(Value::Null) };
    let status_reason = if cap.timed_out { ("timeout", "artifact execution exceeded the deadline") }
        else if !cap.output.status.success() { ("failed", "artifact did not exit successfully") }
        else if cap.stdout_truncated || cap.stderr_truncated { ("failed", "artifact output exceeded the capture limit") }
        else if assertions.stdout.as_ref().is_some_and(|s| *s != cap.output.stdout) { ("failed", "stdout did not match the expected bytes") }
        else if assertions.stderr.as_ref().is_some_and(|s| *s != cap.output.stderr) { ("failed", "stderr did not match the expected bytes") }
        else { ("passed", "artifact execution and requested assertions passed") };
    set_status(&mut result, status_reason.0, status_reason.1);
    match file_hash(&artifact.path) {
        Ok(after) if after == artifact.hash => result["artifact_unchanged"] = json!(true),
        _ => {
            result["artifact_unchanged"] = json!(false);
            set_status(&mut result, "failed", "artifact changed or disappeared during execution; its hash no longer binds this run");
        }
    }
    Ok(result)
}

fn check_toolchain(check: &mut Value, report: &mut Value, timeout: Duration) -> Result<Option<toolchain::Toolchain>, Error> {
    if std::env::var_os("YAVO_CC").is_some() {
        set_status(check, "unsupported", "verify requires the pinned Zig toolchain; unset YAVO_CC");
        return Ok(None);
    }
    let tc = match toolchain::find_pinned() {
        Ok(tc) => tc,
        Err(e) => { set_status(check, "unsupported", &e); return Ok(None); }
    };
    let exe_hash = match file_hash(&tc.program) {
        Ok(hash) => hash,
        Err(e) => {
            set_status(check, "unsupported", &format!("cannot hash pinned compiler: {e}"));
            return Ok(None);
        }
    };
    let lock_hash = digest(toolchain::lock_contents().as_bytes());
    report["inputs"]["toolchain"] = json!(digest_parts(&[
        toolchain::lock_contents().as_bytes(), exe_hash.as_bytes()
    ]));
    report["toolchain"] = json!({
        "expected_version":toolchain::lock().version, "executable":tc.program,
        "executable_sha256":exe_hash, "lock_sha256":lock_hash,
        "hash_scope":["embedded_toolchain_lock", "zig_executable"],
        "installed_archive_reverified":false, "compiler_libraries_hashed":false,
        "environment":"OS allowlist; compiler and loader overrides removed",
        "library_dir":tc.program.parent().map(|p| p.join("lib"))
    });
    check["artifact"] = json!(exe_hash);
    check["artifact_path"] = json!(tc.program);
    let mut command = Command::new(&tc.program);
    clean_environment(&mut command);
    command.arg("version").stdin(Stdio::null());
    let start = Instant::now();
    let cap = match process::capture(&mut command, timeout) {
        Ok(cap) => cap,
        Err(e) => {
            set_status(check, if e.kind() == io::ErrorKind::TimedOut { "timeout" } else { "unsupported" }, &e.to_string());
            return Ok(None);
        }
    };
    merge(check, execution(&cap, &command, start.elapsed()));
    check["ran_on"] = json!(host_target());
    let version = String::from_utf8_lossy(&cap.output.stdout).trim().to_string();
    report["toolchain"]["actual_version"] = json!(version);
    if cap.timed_out {
        set_status(check, "timeout", "pinned compiler version probe exceeded the deadline");
        return Ok(None);
    }
    if !cap.output.status.success() || version != toolchain::lock().version
        || cap.stdout_truncated || cap.stderr_truncated {
        set_status(check, "failed", "installed compiler version does not match the embedded toolchain lock");
        return Ok(None);
    }
    set_status(check, "passed", "pinned compiler path and actual version match; executable and lock were hashed");
    Ok(Some(tc))
}

/// A clean probe only proves that a binary starts. Some compiler drivers accept
/// sanitizer flags without enabling every requested instrumentation pass. Require
/// each tool to report a representative, deliberately introduced defect as well.
fn sanitizer_negative_probe(
    tc: &toolchain::Toolchain, dir: &Path, include: &Path, target: &str,
    profile: &str, tool: &str, opts: &Options,
) -> Result<Value, Error> {
    let (issue, source) = match tool {
        "address" => ("heap-use-after-free", r#"#include <stdlib.h>
#include <stdio.h>
__attribute__((noinline)) static int read_after_free(volatile int *p) { return *p; }
int main(void) {
    int *p = malloc(sizeof(*p));
    if (!p) return 2;
    *p = 73;
    free(p);
    printf("value=%d\n", read_after_free(p));
    return 0;
}
"#),
        "undefined" => ("signed integer overflow", r#"#include <limits.h>
#include <stdio.h>
int main(void) {
    volatile int maximum = INT_MAX;
    volatile int one = 1;
    printf("overflow=%d\n", maximum + one);
    return 0;
}
"#),
        "thread" => ("data race", r#"#include <pthread.h>
#include <stdio.h>
static volatile int shared;
static void *race(void *unused) {
    (void)unused;
    for (int i = 0; i < 100000; i++) shared++;
    return NULL;
}
int main(void) {
    pthread_t first, second;
    if (pthread_create(&first, NULL, race, NULL)) return 2;
    if (pthread_create(&second, NULL, race, NULL)) return 3;
    pthread_join(first, NULL);
    pthread_join(second, NULL);
    printf("shared=%d\n", shared);
    return 0;
}
"#),
        _ => unreachable!(),
    };
    let c_file = dir.join(format!("capability-{tool}.c"));
    fs::write(&c_file, source)?;
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let built = compile_artifact(tc, &c_file, include, &dir.join(format!("capability-{tool}{suffix}")),
        target, Some(profile), None, opts.build_timeout)?;
    let mut result = json!({
        "tool":tool, "issue":issue, "source_sha256":digest(source.as_bytes()),
        "source_path":c_file, "build":built.evidence, "status":"unsupported",
        "expected":"nonzero termination and a diagnostic identifying the injected defect",
        "scope":"representative runtime instrumentation capability, not exhaustive sanitizer correctness"
    });
    let Some(artifact) = built.artifact else {
        let state = if result["build"]["status"] == "timeout" { "timeout" } else { "unsupported" };
        set_status(&mut result, state, "cannot build the negative instrumentation probe");
        return Ok(result);
    };
    let mut command = Command::new(&artifact.path);
    clean_environment(&mut command);
    command.stdin(Stdio::null())
        .env("ASAN_OPTIONS", "halt_on_error=1:detect_leaks=1")
        .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
        .env("TSAN_OPTIONS", "halt_on_error=1:exitcode=66");
    let start = Instant::now();
    let cap = match process::capture(&mut command, opts.timeout) {
        Ok(cap) => cap,
        Err(e) => {
            set_status(&mut result, if e.kind() == io::ErrorKind::TimedOut { "timeout" } else { "unsupported" },
                &format!("cannot execute the negative instrumentation probe: {e}"));
            return Ok(result);
        }
    };
    let stderr = String::from_utf8_lossy(&cap.output.stderr);
    let marker = match tool {
        "address" => stderr.contains("AddressSanitizer") && stderr.contains("heap-use-after-free"),
        "undefined" => stderr.contains("signed integer overflow") &&
            (stderr.contains("runtime error:") || stderr.to_lowercase().contains("ubsan")),
        "thread" => stderr.contains("ThreadSanitizer") && stderr.contains("data race"),
        _ => false,
    };
    let detected = !cap.timed_out && !cap.output.status.success()
        && !cap.stdout_truncated && !cap.stderr_truncated
        && cap.stderr_sha256.is_some() && marker;
    let mut run = execution(&cap, &command, start.elapsed());
    artifact.bind(&mut run);
    run["ran_on"] = json!(host_target());
    run["expected_defect_detected"] = json!(detected);
    result["run"] = run;
    if cap.timed_out {
        set_status(&mut result, "timeout", "negative instrumentation probe exceeded the deadline");
    } else if detected {
        set_status(&mut result, "passed", "the requested sanitizer detected its deliberately injected defect");
    } else {
        set_status(&mut result, "unsupported", "the requested sanitizer did not demonstrate detection of its injected defect");
    }
    Ok(result)
}

fn sanitizer_check(
    item: &mut Value, tc: &toolchain::Toolchain, c_file: &Path, c_code: &str, dir: &Path,
    assertions: &Assertions, opts: &Options, kind: &str,
) -> Result<(), Error> {
    item["tools"] = json!(kind.split(',').collect::<Vec<_>>());
    if !opts.sanitize { return Ok(()); }
    if opts.no_run {
        set_status(item, "not_run", "--no-run disables sanitizer capability probes and artifact execution");
        return Ok(());
    }
    let target = sanitizer_target();
    // Application execution may write files. Recreate compiler inputs from the
    // in-memory snapshot before each supplemental build.
    fs::write(c_file, c_code)?;
    fs::write(dir.join("yavo_rt.h"), codegen::RUNTIME_HEADER)?;
    let subdir = dir.join(if kind == "thread" { "thread" } else { "address-undefined" });
    fs::create_dir(&subdir)?;
    let probe_c = subdir.join("probe.c");
    fs::write(&probe_c, "int main(int argc, char **argv) { (void)argv; volatile int v[2] = {argc, 0}; return v[0] < 0; }\n")?;
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let probe = compile_artifact(tc, &probe_c, dir, &subdir.join(format!("probe{suffix}")),
        &target, Some(kind), None, opts.build_timeout)?;
    item["probe_build"] = probe.evidence;
    let Some(probe_artifact) = probe.artifact else {
        let status = if item["probe_build"]["status"] == "timeout" { "timeout" } else { "unsupported" };
        set_status(item, status, "host compiler cannot build the requested sanitizer capability probe");
        return Ok(());
    };
    let probe_run = run_artifact(&probe_artifact,
        &Assertions { stdin: None, stdout: None, stderr: None }, opts.timeout, true)?;
    let probe_ok = probe_run["status"] == "passed";
    let probe_timeout = probe_run["status"] == "timeout";
    item["probe_run"] = probe_run;
    if !probe_ok {
        set_status(item, if probe_timeout { "timeout" } else { "unsupported" },
            "host sanitizer runtime cannot execute its capability probe");
        return Ok(());
    }
    let mut probes = Vec::new();
    for tool in kind.split(',') {
        probes.push(sanitizer_negative_probe(tc, &subdir, dir, &target, kind, tool, opts)?);
    }
    let timed_out = probes.iter().any(|p| p["status"] == "timeout");
    let demonstrated = probes.iter().all(|p| p["status"] == "passed");
    item["demonstrated_tools"] = json!(probes.iter().filter(|p| p["status"] == "passed")
        .map(|p| p["tool"].clone()).collect::<Vec<_>>());
    item["capability_probes"] = json!(probes);
    if !demonstrated {
        set_status(item, if timed_out { "timeout" } else { "unsupported" },
            "one or more requested sanitizers did not detect the injected capability-probe defect; accepting flags is insufficient");
        return Ok(());
    }
    let built = compile_artifact(tc, c_file, dir, &subdir.join(format!("program{suffix}")),
        &target, Some(kind), None, opts.build_timeout)?;
    item["build"] = built.evidence;
    let Some(artifact) = built.artifact else {
        let status = if item["build"]["status"] == "timeout" { "timeout" } else { "failed" };
        set_status(item, status, "sanitizer capability probe passed, but the application build failed");
        return Ok(());
    };
    merge(item, run_artifact(&artifact, assertions, opts.timeout, true)?);
    Ok(())
}

fn reproducibility_check(
    item: &mut Value, tc: &toolchain::Toolchain, source: &[u8], project: &project::Project, dir: &Path,
    target: &toolchain::Target, timeout: Duration,
) -> Result<(), Error> {
    let mut builds = Vec::new();
    let mut hashes = Vec::new();
    let mut generated = Vec::new();
    let mut status = "passed";
    let mut backend_attempts = 0;
    let snapshot = if project.modular { project.snapshot(&dir.join("project"))? } else { dir.join("source.yv") };
    // Both processes use the same logical source path. Each process constructs
    // its own parser, type tables, monomorphizations and generated translation unit.
    item["scope"] = json!("source_to_binary");
    item["source_path"] = json!(snapshot);
    item["source_sha256"] = json!(digest(source));
    if project.modular {
        item["scope"] = json!("project_to_binary");
        item["project_sha256"] = json!(format!("sha256:{}", project.hash()));
        item["source_files"] = project.manifest();
    }
    for number in 1..=2 {
        let subdir = dir.join(format!("repro-{number}"));
        fs::create_dir(&subdir)?;
        if project.modular { project.snapshot(&dir.join("project"))?; }
        else { fs::write(&snapshot, source)?; }
        let c_file = subdir.join("main.c");
        fs::write(subdir.join("yavo_rt.h"), codegen::RUNTIME_HEADER)?;
        let mut frontend = Command::new(std::env::current_exe()?);
        clean_environment(&mut frontend);
        frontend.arg("check").arg(&snapshot).arg("--emit-c").arg(&c_file).stdin(Stdio::null());
        let start = Instant::now();
        let cap = match process::capture(&mut frontend, timeout) {
            Ok(cap) => cap,
            Err(e) => {
                let state = if e.kind() == io::ErrorKind::TimedOut { "timeout" } else { "failed" };
                if state == "timeout" || status != "timeout" { status = state; }
                builds.push(json!({"status":state, "frontend":{
                    "status":state, "command":command_record(&frontend), "reason":e.to_string(),
                    "ran_on":null}, "backend":null}));
                continue;
            }
        };
        let mut front = execution(&cap, &frontend, start.elapsed());
        front["ran_on"] = json!(host_target());
        let front_status = if cap.timed_out { "timeout" }
            else if !cap.output.status.success() || cap.stdout_truncated || cap.stderr_truncated
                || !c_file.is_file() { "failed" }
            else { "passed" };
        front["status"] = json!(front_status);
        if front_status != "passed" {
            if front_status == "timeout" || status != "timeout" { status = front_status; }
            builds.push(json!({"status":front_status, "frontend":front, "backend":null}));
            continue;
        }
        let c_hash = file_hash(&c_file)?;
        generated.push(c_hash.clone());
        front["artifact"] = json!(c_hash);
        front["artifact_path"] = json!(c_file);
        let suffix = if target.os == "windows" { ".exe" } else { "" };
        backend_attempts += 1;
        let built = compile_artifact(tc, &c_file, &subdir, &subdir.join(format!("program{suffix}")),
            target.zig_triple, None, Some(&subdir.join("cache")), timeout)?;
        if let Some(artifact) = built.artifact {
            hashes.push(artifact.hash.clone());
            artifact.bind(item);
        } else if built.evidence["status"] == "timeout" { status = "timeout"; }
        else if status != "timeout" { status = "failed"; }
        builds.push(json!({"status":built.evidence["status"], "frontend":front, "backend":built.evidence}));
    }
    item["builds"] = json!(builds);
    item["builds_attempted"] = json!(2);
    item["frontend_processes_attempted"] = json!(2);
    item["backend_builds_attempted"] = json!(backend_attempts);
    item["cache_bypassed"] = json!(backend_attempts == 2);
    item["ran_on"] = json!(host_target());
    item["hashes"] = json!(hashes);
    item["generated_c_hashes"] = json!(generated);
    item["generated_c_identical"] = json!(generated.len() == 2 && generated[0] == generated[1]);
    if status != "passed" {
        set_status(item, status, "at least one independent source-to-binary build did not complete successfully");
    } else if hashes.len() == 2 && hashes[0] == hashes[1] {
        set_status(item, "passed", "two independent Yavo frontend processes and separate empty Zig caches produced identical artifact bytes");
    } else {
        set_status(item, "failed", "independent source-to-binary builds produced different artifact hashes");
    }
    Ok(())
}
fn finish(mut report: Value, checks: Vec<Value>, opts: &Options, dir: &Path) -> Result<i32, Error> {
    report["policy"]["required"] = json!(checks.iter().filter(|c| c["required"] == true)
        .map(|c| c["id"].clone()).collect::<Vec<_>>());
    let failed = checks.iter().any(|c| c["required"] == true && (c["status"] == "failed" || c["status"] == "timeout"));
    let complete = checks.iter().all(|c| c["required"] != true || c["status"] == "passed");
    let (overall, code) = if failed { ("failed", 1) } else if complete { ("verified", 0) } else { ("not_verified", 5) };
    report["overall"] = json!(overall);
    report["exit_code"] = json!(code);
    report["checks"] = json!(checks);
    let text = serde_json::to_string_pretty(&report).unwrap();
    let partial = dir.join("report.json.partial");
    fs::write(&partial, format!("{text}\n"))?;
    fs::rename(partial, dir.join("report.json"))?;
    if opts.json { println!("{text}"); }
    else {
        println!("verification: {overall}");
        for check in report["checks"].as_array().unwrap() {
            println!("  {} [{}]: {} -- {}", check["id"].as_str().unwrap(),
                check["target"].as_str().unwrap_or(""), check["status"].as_str().unwrap(),
                check["reason"].as_str().unwrap_or(""));
        }
        println!("report: {}", dir.join("report.json").display());
    }
    Ok(code)
}

pub fn run(opts: Options) -> Result<i32, Error> {
    let source = fs::read(&opts.input).map_err(|e| input_error(&opts.input, e))?;
    let stdin = read_optional(&opts.stdin)?;
    let expected_out = read_optional(&opts.expect_stdout)?;
    let expected_err = read_optional(&opts.expect_stderr)?;
    let target = match opts.target.or_else(|| toolchain::TARGETS.iter().find(|t| t.runs_on_host())) {
        Some(t) => t,
        None => return Err(Error { message:"no supported verification target for this host; specify --target".into(), exit_code:2 }),
    };
    let dir = run_directory(&opts.output_dir)?;
    fs::write(dir.join("source.yv"), &source)?;
    let stdin_path = stdin.as_ref().map(|bytes| -> io::Result<PathBuf> {
        let path = dir.join("stdin.bin");
        fs::write(&path, bytes)?;
        Ok(path)
    }).transpose()?;
    if let Some(bytes) = &expected_out { fs::write(dir.join("expected-stdout.bin"), bytes)?; }
    if let Some(bytes) = &expected_err { fs::write(dir.join("expected-stderr.bin"), bytes)?; }
    let mut report = json!({
        "schema":"yavo-verify/1", "source_path":fs::canonicalize(&opts.input)?,
        "artifact_dir":dir, "report_path":dir.join("report.json"),
        "inputs":{
            "source":digest(&source), "dependencies":digest(codegen::RUNTIME_HEADER.as_bytes()),
            "toolchain":digest(toolchain::lock_contents().as_bytes()),
            "stdin":stdin.as_ref().map(|s| digest(s)),
            "expected_stdout":expected_out.as_ref().map(|s| digest(s)),
            "expected_stderr":expected_err.as_ref().map(|s| digest(s))
        },
        "compiler":{"version":env!("CARGO_PKG_VERSION"), "executable_sha256":file_hash(&std::env::current_exe()?)?},
        "policy":{
            "required":["front_end","toolchain","build","tests","audit"],
            "sanitize_requested":opts.sanitize, "reproducible_requested":opts.reproducible,
            "no_run":opts.no_run, "timeout_ms":opts.timeout.as_millis(),
            "build_timeout_ms":opts.build_timeout.as_millis(),
            "audit_implemented":false, "release":true, "allocation_check":false,
            "test_scope":"one program execution; no unrequested test suite is inferred"
        },
        "dependencies":{"runtime_header_sha256":digest(codegen::RUNTIME_HEADER.as_bytes()),
            "scope":"generated translation unit and embedded Yavo runtime; C toolchain libraries are not content-audited"}
    });
    let host = host_target();
    let san_target = sanitizer_target();
    let mut checks = vec![
        pending("front_end", "front_end", true, target.name, "source checking has not completed"),
        pending("toolchain", "toolchain", true, &host, "front end has not completed"),
        pending("build", "build", true, target.name, "pinned toolchain has not been validated"),
        pending("tests", "tests", true, target.name, "requested target artifact has not been built"),
        pending("audit", "audit", true, target.name, "source and dependency audit is not implemented in this prototype"),
        pending("address-undefined", "sanitizers", opts.sanitize, &san_target, if opts.sanitize { "pinned toolchain has not been validated" } else { "not requested" }),
        pending("thread", "sanitizers", opts.sanitize, &san_target, if opts.sanitize { "pinned toolchain has not been validated" } else { "not requested" }),
        pending("reproducibility", "reproducible_build", opts.reproducible, target.name, if opts.reproducible { "pinned toolchain has not been validated" } else { "not requested" }),
    ];
    checks[4]["scope"] = json!({"app":"not_audited", "dependencies":"not_audited",
        "stdlib":"not_audited", "c_runtime":"not_audited"});
    checks[4]["runtime_header_sha256"] = json!(digest(codegen::RUNTIME_HEADER.as_bytes()));
    checks[0]["artifact"] = json!(digest(&source));
    checks[0]["artifact_path"] = json!(dir.join("source.yv"));
    checks[0]["ran_on"] = json!(host);
    let mut diags = diag::Diags::default();
    let src = match std::str::from_utf8(&source) {
        Ok(src) => src,
        Err(_) => {
            diags.error("E0020", diag::Span { start:0, end:0, line:1, col:1 }, "source file is not valid UTF-8");
            report["diagnostics"] = diag::render_json_value(&opts.input.to_string_lossy(), "", &diags);
            report["diagnostics"]["source_sha256"] = json!(format!("{:x}", Sha256::digest(&source)));
            set_status(&mut checks[0], "failed", "source is not valid UTF-8");
            return finish(report, checks, &opts, &dir);
        }
    };
    let project = project::Project::load(&opts.input, src, &mut diags).map_err(|e| input_error(&opts.input, e))?;
    if project.modular {
        project.snapshot(&dir.join("project"))?;
        let mut manifest = project.manifest();
        for item in manifest.as_array_mut().unwrap() {
            item["snapshot_path"] = json!(dir.join("project").join(item["relative_path"].as_str().unwrap()));
        }
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let manifest_path = dir.join("project-manifest.json");
        fs::write(&manifest_path, &bytes)?;
        report["inputs"]["project"] = json!(format!("sha256:{}", project.hash()));
        report["dependencies"]["source_files"] = manifest;
        checks[0]["artifact"] = json!(digest(&bytes));
        checks[0]["artifact_path"] = json!(manifest_path);
    }
    let typed = if diags.has_errors() { None } else { check::check(&project.program, &mut diags) };
    diags.sort();
    report["diagnostics"] = project.json(&diags);
    if diags.has_errors() || typed.is_none() {
        set_status(&mut checks[0], "failed", "Yavo front end rejected the program");
        return finish(report, checks, &opts, &dir);
    }
    set_status(&mut checks[0], "passed", "lexing, parsing and implemented semantic checks passed");
    let c_code = codegen::generate(&typed.unwrap(), &opts.input.to_string_lossy().replace('\\', "/"));
    let c_file = dir.join("main.c");
    fs::write(&c_file, &c_code)?;
    fs::write(dir.join("yavo_rt.h"), codegen::RUNTIME_HEADER)?;
    report["inputs"]["dependencies"] = json!(digest_parts(&[c_code.as_bytes(), codegen::RUNTIME_HEADER.as_bytes()]));
    report["dependencies"]["generated_c_sha256"] = json!(digest(c_code.as_bytes()));
    let Some(tc) = check_toolchain(&mut checks[1], &mut report, opts.build_timeout)? else {
        set_status(&mut checks[2], "unsupported", "the pinned toolchain is unavailable or rejected; no build was attempted");
        return finish(report, checks, &opts, &dir);
    };
    checks[4]["scope"]["c_runtime"] = json!("trusted_pinned");
    checks[4]["trust_basis"] = json!("pinned toolchain path and version; archive and compiler libraries are not reverified");
    let suffix = if target.os == "windows" { ".exe" } else { "" };
    let built = compile_artifact(&tc, &c_file, &dir, &dir.join(format!("program{suffix}")),
        target.zig_triple, None, None, opts.build_timeout)?;
    merge(&mut checks[2], built.evidence);
    let assertions = Assertions {
        stdin: stdin_path.zip(stdin), stdout: expected_out, stderr: expected_err,
    };
    if let Some(artifact) = built.artifact {
        artifact.bind(&mut checks[2]);
        artifact.bind(&mut checks[3]);
        artifact.bind(&mut checks[4]);
        if opts.no_run {
            set_status(&mut checks[3], "not_run", "--no-run disables target execution");
        } else if !target.runs_on_host() {
            set_status(&mut checks[3], "not_run", "no runner for the requested target is available on this host");
        } else {
            merge(&mut checks[3], run_artifact(&artifact, &assertions, opts.timeout, false)?);
        }
    }
    sanitizer_check(&mut checks[5], &tc, &c_file, &c_code, &dir, &assertions, &opts, "address,undefined")?;
    sanitizer_check(&mut checks[6], &tc, &c_file, &c_code, &dir, &assertions, &opts, "thread")?;
    if opts.reproducible {
        reproducibility_check(&mut checks[7], &tc, &source, &project, &dir, target, opts.build_timeout)?;
    }
    fs::write(dir.join("source.yv"), &source)?;
    if project.modular { project.snapshot(&dir.join("project"))?; }
    fs::write(&c_file, &c_code)?;
    fs::write(dir.join("yavo_rt.h"), codegen::RUNTIME_HEADER)?;
    if let Some((path, bytes)) = &assertions.stdin { fs::write(path, bytes)?; }
    if let Some(bytes) = &assertions.stdout { fs::write(dir.join("expected-stdout.bin"), bytes)?; }
    if let Some(bytes) = &assertions.stderr { fs::write(dir.join("expected-stderr.bin"), bytes)?; }
    finish(report, checks, &opts, &dir)
}
