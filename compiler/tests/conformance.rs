//! Spec conformance against `yavo-tests/MANIFEST.json`.
//!
//! Every case is classified as one of:
//!   pass         the compiler did what the spec test expects
//!   wrong        the compiler accepted/rejected/ran it differently from the spec (a real bug or a spec gap)
//!   unsupported  the compiler reported U0001 (a valid v0.5 feature this compiler does not implement yet)
//!   not_run      the case needs something this runner or compiler does not have yet (`yavo fix`, `verify`,
//!                fixtures, sanitizers, other platforms)
//!
//! This is not an acceptance gate. It fails only when a case listed in `tests/conformance_baseline.txt`
//! stops passing (a regression). After real progress, refresh the baseline with
//! `YAVO_UPDATE_BASELINE=1 cargo test --test conformance`.
//! The full per-case report is written to `$CARGO_TARGET_TMPDIR/conformance-report.json`.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

mod common;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Status {
    Pass,
    Wrong,
    Unsupported,
    NotRun,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Wrong => "wrong",
            Status::Unsupported => "unsupported",
            Status::NotRun => "not_run",
        }
    }
}

struct Outcome {
    status: Status,
    reason: String,
}

fn outcome(status: Status, reason: impl Into<String>) -> Outcome {
    Outcome { status, reason: reason.into() }
}

fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../yavo-tests")
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance_baseline.txt")
}

fn yavo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yavo"))
}

/// Manifest fields that need runner or compiler features not available yet.
const NEEDS_HARNESS: &[&str] = &["platforms", "sanitize_mode", "sanitizer_profiles", "fault_scope"];

/// `fixture` + `fixture_hex` (+ `cwd: test-dir`): the fixture is written, after checking it against
/// `fixture_sha256`, into a fresh directory of its own, which becomes the working directory.
fn fixture_dir(entry: &Value) -> Result<Option<PathBuf>, Outcome> {
    if entry["fixture"].is_null() && entry["cwd"].is_null() {
        return Ok(None);
    }
    if let Some(cwd) = entry["cwd"].as_str() {
        if cwd != "test-dir" {
            return Err(outcome(Status::NotRun, format!("needs `cwd: {cwd}` support in the runner")));
        }
    }
    let id = entry["id"].as_str().unwrap_or("case");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("conformance").join(id);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| outcome(Status::NotRun, format!("cannot create {}: {e}", dir.display())))?;
    if let Some(name) = entry["fixture"].as_str() {
        if name.contains("..") || name.starts_with('/') {
            return Err(outcome(Status::Wrong, format!("fixture path must be a plain relative path: {name}")));
        }
        let hex = entry["fixture_hex"].as_str().unwrap_or("");
        let bytes: Option<Vec<u8>> =
            (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect();
        let Some(bytes) = bytes.filter(|_| hex.len() % 2 == 0) else {
            return Err(outcome(Status::Wrong, "fixture_hex is not valid hex"));
        };
        if let Some(want) = entry["fixture_sha256"].as_str() {
            if format!("{:x}", Sha256::digest(&bytes)) != want {
                return Err(outcome(Status::Wrong, "fixture_hex does not match fixture_sha256 in MANIFEST.json"));
            }
        }
        std::fs::write(dir.join(name), bytes).map_err(|e| outcome(Status::NotRun, format!("cannot write fixture: {e}")))?;
    }
    Ok(Some(dir))
}

type Errors = Vec<(String, u64, String)>;

fn run_check(path: &Path) -> Result<Errors, Outcome> {
    let result = common::capture(yavo()
        .arg("check")
        .arg(path)
        .arg("--json-diagnostics")
        .stdin(Stdio::null()), common::RUN_TIMEOUT)
        .map_err(|e| outcome(Status::NotRun, format!("could not start yavo: {e}")))?;
    if result.timed_out {
        return Err(outcome(Status::Wrong, "yavo check exceeded its 10 second deadline"));
    }
    let out = result.output;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| outcome(Status::Wrong, format!("diagnostics are not valid JSON ({e}): {stdout}")))?;
    let diagnostics = v["diagnostics"].as_array()
        .ok_or_else(|| outcome(Status::Wrong, "diagnostics JSON has no diagnostics array"))?;
    let errors: Errors = diagnostics
        .iter()
        .filter(|d| d["severity"] == "ERROR")
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("").to_string(),
                d["line"].as_u64().unwrap_or(0),
                d["message"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let expected_exit = if errors.is_empty() { 0 } else { 1 };
    if out.status.code() != Some(expected_exit) {
        return Err(outcome(Status::Wrong, format!("yavo check exit {:?}; expected {expected_exit}", out.status.code())));
    }
    Ok(errors)
}

fn describe(errors: &Errors) -> String {
    if errors.is_empty() {
        return "no errors".into();
    }
    errors.iter().map(|(c, l, m)| format!("{c}@{l} {m}")).collect::<Vec<_>>().join("; ")
}

fn classify(entry: &Value, root: &Path) -> Outcome {
    let file = entry["file"].as_str().unwrap_or("");
    let path = root.join(file);
    let Ok(src) = std::fs::read(&path) else {
        return outcome(Status::Wrong, format!("fixture missing: {file}"));
    };
    if let Some(want) = entry["source_sha256"].as_str() {
        if format!("{:x}", Sha256::digest(&src)) != want {
            return outcome(Status::Wrong, "fixture does not match source_sha256 in MANIFEST.json");
        }
    }
    let kind = entry["kind"].as_str().unwrap_or("");
    match kind {
        "check-pass" | "check-fail" | "run-pass" | "run-panic" => {}
        "fix-auto" | "fix-candidate" => return outcome(Status::NotRun, "`yavo fix` is not implemented"),
        "report" => return outcome(Status::NotRun, "`yavo verify` is not implemented"),
        other => return outcome(Status::NotRun, format!("runner does not handle kind `{other}` yet")),
    }
    let errors = match run_check(&path) {
        Ok(e) => e,
        Err(o) => return o,
    };
    // Every spec case is valid v0.5 syntax (negative cases must parse too), so a syntax error is
    // always a compiler bug, even when the case also uses unsupported features.
    if let Some((_, line, msg)) = errors.iter().find(|(code, _, _)| code == "E0020") {
        return outcome(Status::Wrong, format!("syntax error on valid v0.5 syntax at line {line}: {msg}"));
    }
    if let Some((_, _, msg)) = errors.iter().find(|(code, _, _)| code.starts_with('U')) {
        return outcome(Status::Unsupported, msg.clone());
    }
    match kind {
        "check-pass" => {
            if errors.is_empty() {
                outcome(Status::Pass, "")
            } else {
                outcome(Status::Wrong, format!("rejected a valid program: {}", describe(&errors)))
            }
        }
        "check-fail" => {
            let want: BTreeSet<(String, u64)> = entry["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["severity"] == "ERROR")
                .map(|d| (d["code"].as_str().unwrap_or("").to_string(), d["line"].as_u64().unwrap_or(0)))
                .collect();
            let got: BTreeSet<(String, u64)> = errors.iter().map(|(c, l, _)| (c.clone(), *l)).collect();
            if want == got {
                outcome(Status::Pass, "")
            } else {
                let want: Vec<String> = want.iter().map(|(c, l)| format!("{c}@{l}")).collect();
                outcome(Status::Wrong, format!("expected [{}], got: {}", want.join(", "), describe(&errors)))
            }
        }
        _ => {
            if !errors.is_empty() {
                return outcome(Status::Wrong, format!("rejected a runnable program: {}", describe(&errors)));
            }
            if let Some(field) = NEEDS_HARNESS.iter().find(|f| !entry[**f].is_null()) {
                return outcome(Status::NotRun, format!("needs `{field}` support in the runner"));
            }
            // `modes: ["debug", "release"]` means the case must hold in both generated-C optimisation modes.
            let modes: Vec<String> = entry["modes"]
                .as_array()
                .map(|m| m.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_else(|| vec!["debug".to_string()]);
            let dir = match fixture_dir(entry) {
                Ok(d) => d,
                Err(o) => return o,
            };
            for mode in &modes {
                let o = run_once(entry, &path, kind, mode == "release", dir.as_deref());
                if o.status != Status::Pass {
                    return outcome(o.status, format!("[{mode}] {}", o.reason));
                }
            }
            outcome(Status::Pass, "")
        }
    }
}

fn runtime_limit(entry: &Value) -> Result<Duration, String> {
    let Some(value) = entry.get("timeout_ms") else { return Ok(common::RUN_TIMEOUT); };
    let millis = match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }.filter(|n| *n > 0).ok_or_else(|| "timeout_ms must be a positive integer or decimal string".to_string())?;
    Ok(Duration::from_millis(millis))
}

fn run_once(entry: &Value, path: &Path, kind: &str, release: bool, dir: Option<&Path>) -> Outcome {
    let limit = match runtime_limit(entry) {
        Ok(limit) => limit,
        Err(e) => return outcome(Status::Wrong, e),
    };
    let scratch = match common::Scratch::new("conformance-run") {
        Ok(scratch) => scratch,
        Err(e) => return outcome(Status::NotRun, format!("cannot create build directory: {e}")),
    };
    let exe = scratch.exe("fixture");
    let mut cmd = yavo();
    cmd.arg("build").arg(path).arg("-o").arg(&exe).stdin(Stdio::null());
    if release {
        cmd.arg("--release");
    }
    let built = match common::capture(&mut cmd, common::BUILD_TIMEOUT) {
        Ok(o) if o.timed_out => return outcome(Status::Wrong, "C build exceeded its 120 second deadline"),
        Ok(o) => o.output,
        Err(e) => return outcome(Status::NotRun, format!("could not start yavo: {e}")),
    };
    if built.status.code() == Some(3) {
        return outcome(Status::NotRun, "C toolchain unavailable (run `yavo toolchain install`)");
    }
    if !built.status.success() {
        return outcome(Status::Wrong, format!("build exit {:?}: {}", built.status.code(), String::from_utf8_lossy(&built.stderr)));
    }
    let mut run = Command::new(&exe);
    run.stdin(Stdio::null());
    if let Some(d) = dir { run.current_dir(d); }
    let out = match common::capture(&mut run, limit) {
        Ok(o) if o.timed_out => return outcome(Status::Wrong, format!("program exceeded its {} ms deadline", limit.as_millis())),
        Ok(o) => o.output,
        Err(e) => return outcome(Status::NotRun, format!("could not run program: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if kind == "run-pass" {
        let want = entry["expected_stdout"].as_str().unwrap_or("");
        if out.status.code() == Some(0) && stdout == want {
            outcome(Status::Pass, "")
        } else {
            outcome(
                Status::Wrong,
                format!("exit {:?}, stdout {stdout:?} (expected {want:?}); stderr: {}", out.status.code(), stderr.trim()),
            )
        }
    } else {
        let cat = entry["expect_panic"].as_str().or(entry["expect"].as_str()).unwrap_or("");
        let line = stderr.lines().find(|l| l.starts_with("yavo panic [")).unwrap_or("");
        if out.status.code() == Some(101) && line.starts_with(&format!("yavo panic [{cat}]")) {
            outcome(Status::Pass, "")
        } else {
            outcome(
                Status::Wrong,
                format!("expected panic [{cat}] with exit 101, got exit {:?}: {}", out.status.code(), stderr.trim()),
            )
        }
    }
}

#[test]
fn conformance() {
    let root = tests_root();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("MANIFEST.json")).expect("read MANIFEST.json"))
            .expect("parse MANIFEST.json");
    let entries = manifest.as_array().expect("MANIFEST.json must be an array");

    let rows: Vec<(String, String, String, Outcome)> = entries
        .iter()
        .map(|e| {
            (
                e["id"].as_str().unwrap_or("?").to_string(),
                e["phase"].as_str().unwrap_or("?").to_string(),
                e["category"].as_str().unwrap_or("?").to_string(),
                classify(e, &root),
            )
        })
        .collect();

    // Summary by phase.
    let mut by_phase: BTreeMap<(String, Status), usize> = BTreeMap::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    for (_, phase, _, o) in &rows {
        *by_phase.entry((phase.clone(), o.status)).or_default() += 1;
        *by_phase.entry(("total".into(), o.status)).or_default() += 1;
        if o.status == Status::Unsupported {
            *unsupported.entry(o.reason.clone()).or_default() += 1;
        }
    }
    println!("\nconformance: {} cases", rows.len());
    println!("{:<8}{:>7}{:>7}{:>13}{:>9}", "phase", "pass", "wrong", "unsupported", "not_run");
    let mut phases: Vec<String> = rows.iter().map(|r| r.1.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    phases.push("total".into());
    for p in &phases {
        let n = |s| by_phase.get(&(p.clone(), s)).copied().unwrap_or(0);
        println!("{:<8}{:>7}{:>7}{:>13}{:>9}", p, n(Status::Pass), n(Status::Wrong), n(Status::Unsupported), n(Status::NotRun));
    }
    println!("\nwrong:");
    for (id, _, _, o) in rows.iter().filter(|r| r.3.status == Status::Wrong) {
        println!("  {id}: {}", o.reason);
    }
    println!("\nmost common unsupported features:");
    let mut reasons: Vec<_> = unsupported.into_iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (r, n) in reasons.iter().take(12) {
        println!("  {n:>3}  {r}");
    }

    // Machine-readable report.
    let report: Vec<Value> = rows
        .iter()
        .map(|(id, phase, cat, o)| json!({"id": id, "phase": phase, "category": cat, "status": o.status.name(), "reason": o.reason}))
        .collect();
    let report_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance-report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap()).expect("write report");
    println!("\nreport: {}", report_path.display());

    // Regression ratchet. Updating must not erase regressions from the old set.
    let passing: BTreeSet<String> = rows.iter().filter(|r| r.3.status == Status::Pass).map(|r| r.0.clone()).collect();
    let baseline = read_baseline(&baseline_path()).expect("read existing conformance baseline");
    require_monotonic(&baseline, &passing).unwrap_or_else(|e| panic!("{e}"));
    if std::env::var_os("YAVO_UPDATE_BASELINE").is_some() {
        update_baseline(&baseline_path(), &passing).expect("update baseline without losing passing cases");
        println!("baseline updated: {} passing cases", passing.len());
        return;
    }
    let newly: Vec<&String> = passing.difference(&baseline).collect();
    if !newly.is_empty() {
        println!("newly passing (refresh the baseline): {newly:?}");
    }
}

fn read_baseline(path: &Path) -> Result<BTreeSet<String>, String> {
    Ok(std::fs::read_to_string(path).map_err(|e| e.to_string())?
        .lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from).collect())
}

fn require_monotonic(baseline: &BTreeSet<String>, passing: &BTreeSet<String>) -> Result<(), String> {
    let regressed: Vec<_> = baseline.difference(passing).collect();
    if regressed.is_empty() { Ok(()) }
    else { Err(format!("spec cases that used to pass no longer pass: {regressed:?}")) }
}

fn update_baseline(path: &Path, passing: &BTreeSet<String>) -> Result<(), String> {
    require_monotonic(&read_baseline(path)?, passing)?;
    let mut text = String::from("# Spec cases that must keep passing. Regenerate with YAVO_UPDATE_BASELINE=1.\n");
    for id in passing { text.push_str(id); text.push('\n'); }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[test]
fn timeout_metadata_accepts_numbers_and_strings_and_rejects_invalid_limits() {
    assert_eq!(runtime_limit(&json!({})).unwrap(), common::RUN_TIMEOUT);
    for value in [json!(3000), json!("3000")] {
        assert_eq!(runtime_limit(&json!({"timeout_ms": value})).unwrap(), Duration::from_millis(3000));
    }
    for value in [json!(0), json!("0"), json!(-1), json!(1.5), json!("bad"), json!(null), json!(true)] {
        assert!(runtime_limit(&json!({"timeout_ms": value})).is_err(), "{value:?}");
    }
}

#[test]
fn baseline_update_cannot_erase_a_regression() {
    let dir = common::Scratch::new("baseline-test").unwrap();
    let path = dir.0.join("baseline.txt");
    let original = "# saved baseline\nT-A\nT-B\n";
    std::fs::write(&path, original).unwrap();
    let regressed = BTreeSet::from(["T-A".to_string(), "T-C".to_string()]);
    assert!(update_baseline(&path, &regressed).unwrap_err().contains("T-B"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let progressed = BTreeSet::from(["T-A".to_string(), "T-B".to_string(), "T-C".to_string()]);
    update_baseline(&path, &progressed).unwrap();
    assert_eq!(read_baseline(&path).unwrap(), progressed);
}

#[test]
fn runtime_deadline_is_enforced_separately_from_the_build() {
    let dir = common::Scratch::new("manifest-timeout").unwrap();
    let path = dir.0.join("loop.yv");
    std::fs::write(&path, "fn main() {\n    while true {}\n}\n").unwrap();
    let entry = json!({"timeout_ms": "100", "expected_stdout": ""});
    let outcome = run_once(&entry, &path, "run-pass", false, None);
    assert_eq!(outcome.status, Status::Wrong, "{}", outcome.reason);
    assert!(outcome.reason.contains("program exceeded its 100 ms deadline"), "{}", outcome.reason);
}
