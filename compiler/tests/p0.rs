//! Runs every `tests/p0/*.yv` through the real `yavo` binary. Expectations live in the
//! file's header comments:
//!
//!   // expect-stdout:          followed by `// | line` lines (exact stdout)
//!   // expect-panic: CAT [L:C] program must panic with that category (at that position)
//!   // expect-error: CODE L:C  `yavo check` must report exactly these errors...
//!   // expect-warning: CODE L:C   ...and exactly these warnings

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod common;

#[derive(Default)]
struct Expect {
    kind: Option<String>,
    stdout: Option<String>,
    panic: Option<(String, Option<String>)>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn parse_expect(src: &str) -> Expect {
    let mut e = Expect::default();
    let mut in_stdout = false;
    for line in src.lines() {
        let Some(c) = line.strip_prefix("//") else { break };
        let c = c.strip_prefix(' ').unwrap_or(c);
        if c == "expect-stdout:" {
            in_stdout = true;
            e.stdout = Some(String::new());
            continue;
        }
        if in_stdout {
            if let Some(text) = c.strip_prefix('|') {
                let text = text.strip_prefix(' ').unwrap_or(text);
                let out = e.stdout.as_mut().unwrap();
                out.push_str(text);
                out.push('\n');
                continue;
            }
            in_stdout = false;
        }
        let pair = |rest: &str| {
            let (a, b) = rest.trim().split_once(' ').expect("expected `NAME L:C`");
            (a.to_string(), b.trim().to_string())
        };
        if let Some(rest) = c.strip_prefix("expect-panic:") {
            let rest = rest.trim();
            e.panic = Some(match rest.split_once(' ') {
                Some((cat, at)) => (cat.to_string(), Some(at.trim().to_string())),
                None => (rest.to_string(), None),
            });
        } else if let Some(rest) = c.strip_prefix("kind:") {
            e.kind = Some(rest.trim().to_string());
        } else if let Some(rest) = c.strip_prefix("expect-error:") {
            let (code, at) = pair(rest);
            e.errors.push(format!("{code} {at}"));
        } else if let Some(rest) = c.strip_prefix("expect-warning:") {
            let (code, at) = pair(rest);
            e.warnings.push(format!("{code} {at}"));
        }
    }
    e
}

fn yavo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yavo"))
}

fn check_diagnostics(path: &Path, e: &Expect) -> Result<(), String> {
    let out = common::output(yavo().arg("check").arg(path).arg("--json-diagnostics")
        .stdin(Stdio::null()), common::RUN_TIMEOUT)?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|err| format!("bad JSON: {err}"))?;
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for d in json["diagnostics"].as_array().unwrap() {
        let code = d["code"].as_str().unwrap();
        let item = format!("{code} {}:{}", d["line"], d["column"]);
        if code.starts_with('W') { warnings.push(item) } else { errors.push(item) }
    }
    let mut want_errors = e.errors.clone();
    let mut want_warnings = e.warnings.clone();
    for v in [&mut errors, &mut warnings, &mut want_errors, &mut want_warnings] {
        v.sort();
    }
    if errors != want_errors || warnings != want_warnings {
        return Err(format!(
            "diagnostics differ\n  expected errors   {want_errors:?}\n  actual errors     {errors:?}\n  expected warnings {want_warnings:?}\n  actual warnings   {warnings:?}"
        ));
    }
    let expected_exit = if want_errors.is_empty() { 0 } else { 1 };
    if out.status.code() != Some(expected_exit) {
        return Err(format!("`yavo check` exited with {:?}, expected {expected_exit}", out.status.code()));
    }
    Ok(())
}

fn check_run(path: &Path, e: &Expect, release: bool) -> Result<(), String> {
    let scratch = common::Scratch::new("p0-run").map_err(|e| e.to_string())?;
    let exe = scratch.exe("fixture");
    let mut command = yavo();
    command.arg("build").arg(path).arg("-o").arg(&exe).stdin(Stdio::null());
    if release {
        command.arg("--release");
    }
    let built = common::output(&mut command, common::BUILD_TIMEOUT)?;
    if !built.status.success() {
        return Err(format!("build exited with {:?}\n{}", built.status.code(), String::from_utf8_lossy(&built.stderr)));
    }
    let out = common::output(Command::new(&exe).stdin(Stdio::null()), common::RUN_TIMEOUT)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(want) = &e.stdout {
        if stdout != *want {
            return Err(format!("stdout differs\n--- expected\n{want}--- actual\n{stdout}--- stderr\n{stderr}"));
        }
    }
    match &e.panic {
        Some((category, at)) => {
            let line = stderr.lines().find(|l| l.starts_with("yavo panic [")).unwrap_or("");
            let ok = line.starts_with(&format!("yavo panic [{category}] at "))
                && at.as_ref().is_none_or(|at| line.contains(&format!(":{at}: ")))
                && out.status.code() == Some(101);
            if !ok {
                let at = at.as_deref().unwrap_or("any position");
                return Err(format!("expected panic [{category}] at {at} (exit 101), got exit {:?}\n{stderr}", out.status.code()));
            }
        }
        None if !out.status.success() => {
            return Err(format!("exited with {:?}\n{stderr}", out.status.code()));
        }
        None => {}
    }
    Ok(())
}

fn run_p0_programs(release: bool) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/p0");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "yv"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no tests found in {}", dir.display());

    let mut failures = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        let e = parse_expect(&src);
        let mut result = check_diagnostics(path, &e);
        if result.is_ok() && e.errors.is_empty() {
            result = check_run(path, &e, release);
        }
        if let Err(msg) = result {
            failures.push(format!("FAIL {}\n{msg}", path.file_name().unwrap().to_string_lossy()));
        }
    }
    assert!(failures.is_empty(), "{} of {} P0 programs failed:\n\n{}", failures.len(), files.len(), failures.join("\n\n"));
    println!("P0 {}: {} fixture files passed", if release { "C -O2" } else { "C -O0" }, files.len());
}

#[test]
fn p0_programs_debug() {
    run_p0_programs(false);
}

#[test]
fn p0_programs_release() {
    run_p0_programs(true);
}

/// Progress report against the full conformance suite (`yavo-tests/`), whose tests target
/// P1+. Programs the P0 front end rejects are counted as "outside P0" rather than failures.
/// Run with `cargo test --release -- --ignored --nocapture conformance`.
#[test]
#[ignore]
fn conformance_report() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "yv") {
                out.push(p);
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../yavo-tests");
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let (mut passed, mut failed, mut outside) = (Vec::new(), Vec::new(), 0);
    for path in &files {
        let e = parse_expect(&std::fs::read_to_string(path).unwrap());
        if !matches!(e.kind.as_deref(), Some("run-pass" | "run-panic")) {
            continue;
        }
        let checked = common::output(yavo().arg("check").arg(path).stdin(Stdio::null()),
            common::RUN_TIMEOUT).expect("bounded conformance check");
        if !checked.status.success() {
            outside += 1;
            continue;
        }
        let name = path.strip_prefix(&root).unwrap().display().to_string();
        match check_run(path, &e, false) {
            Ok(()) => passed.push(name),
            Err(msg) => failed.push(format!("{name}: {}", msg.lines().next().unwrap_or(""))),
        }
    }
    println!("conformance run tests: {} pass, {} fail, {outside} outside P0", passed.len(), failed.len());
    for p in &passed {
        println!("  pass {p}");
    }
    for f in &failed {
        println!("  FAIL {f}");
    }
    assert!(failed.is_empty(), "programs accepted by the P0 front end must run correctly");
}
