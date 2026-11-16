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
use std::process::Command;

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
const NEEDS_HARNESS: &[&str] =
    &["platforms", "fixture", "fixture_hex", "cwd", "sanitize_mode", "sanitizer_profiles", "fault_scope"];

type Errors = Vec<(String, u64, String)>;

fn run_check(path: &Path) -> Result<Errors, Outcome> {
    let out = yavo()
        .arg("check")
        .arg(path)
        .arg("--json-diagnostics")
        .output()
        .map_err(|e| outcome(Status::NotRun, format!("could not start yavo: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| outcome(Status::Wrong, format!("diagnostics are not valid JSON ({e}): {stdout}")))?;
    Ok(v["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["severity"] == "ERROR")
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("").to_string(),
                d["line"].as_u64().unwrap_or(0),
                d["message"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect())
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
            for mode in &modes {
                let o = run_once(entry, &path, kind, mode == "release");
                if o.status != Status::Pass {
                    return outcome(o.status, format!("[{mode}] {}", o.reason));
                }
            }
            outcome(Status::Pass, "")
        }
    }
}

fn run_once(entry: &Value, path: &Path, kind: &str, release: bool) -> Outcome {
    let mut cmd = yavo();
    cmd.arg("run").arg(path);
    if release {
        cmd.arg("--release");
    }
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => return outcome(Status::NotRun, format!("could not start yavo: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.code() == Some(3) {
        return outcome(Status::NotRun, "C toolchain unavailable (run `yavo toolchain install`)");
    }
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

    // Regression ratchet.
    let passing: BTreeSet<String> = rows.iter().filter(|r| r.3.status == Status::Pass).map(|r| r.0.clone()).collect();
    if std::env::var_os("YAVO_UPDATE_BASELINE").is_some() {
        let mut text = String::from("# Spec cases that must keep passing. Regenerate with YAVO_UPDATE_BASELINE=1.\n");
        for id in &passing {
            text.push_str(id);
            text.push('\n');
        }
        std::fs::write(baseline_path(), text).expect("write baseline");
        println!("baseline updated: {} passing cases", passing.len());
        return;
    }
    let baseline: BTreeSet<String> = std::fs::read_to_string(baseline_path())
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    let newly: Vec<&String> = passing.difference(&baseline).collect();
    if !newly.is_empty() {
        println!("newly passing (refresh the baseline): {newly:?}");
    }
    let regressed: Vec<&String> = baseline.difference(&passing).collect();
    assert!(regressed.is_empty(), "spec cases that used to pass no longer pass: {regressed:?}");
}
