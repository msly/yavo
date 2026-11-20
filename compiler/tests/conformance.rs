//! Spec conformance against `yavo-tests/MANIFEST.json`.
//!
//! Every case is classified as one of:
//!   pass         the compiler did what the spec test expects
//!   wrong        the compiler accepted/rejected/ran it differently from the spec (a real bug or a spec gap)
//!   unsupported  the compiler reported U0001 or an explicitly probed tool capability is unavailable
//!   not_run      the case needs an execution environment or harness feature this runner does not have
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

#[allow(dead_code)]
#[path = "../src/diag.rs"]
mod diag;
#[allow(dead_code)]
#[path = "../src/lexer.rs"]
mod lexer;

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

fn run_check_json(path: &Path) -> Result<Value, Outcome> {
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
    let errors = diagnostics.iter().filter(|d| d["severity"] == "ERROR").count();
    let expected_exit = if errors == 0 { 0 } else { 1 };
    if out.status.code() != Some(expected_exit) {
        return Err(outcome(Status::Wrong, format!("yavo check exit {:?}; expected {expected_exit}", out.status.code())));
    }
    Ok(v)
}

fn run_check(path: &Path) -> Result<Errors, Outcome> {
    let v = run_check_json(path)?;
    Ok(v["diagnostics"].as_array().unwrap()
        .iter()
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
        "fix-auto" | "fix-candidate" => return classify_fix(entry, &path, &src),
        "report" => return classify_report(entry, &path, &src),
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

fn classify_report(entry: &Value, path: &Path, source: &[u8]) -> Outcome {
    match check_report(entry, path, source) {
        Ok(()) => outcome(Status::Pass, ""),
        Err(o) => o,
    }
}

fn report_check<'a>(report: &'a Value, name: &str, target: &str) -> Result<&'a Value, Outcome> {
    report["checks"].as_array().into_iter().flatten()
        .find(|c| c["name"] == name && c["target"] == target)
        .ok_or_else(|| outcome(Status::Wrong, format!("report has no {name} check for {target}")))
}

fn report_artifact(check: &Value) -> Result<Vec<u8>, Outcome> {
    let path = check["artifact_path"].as_str()
        .ok_or_else(|| outcome(Status::Wrong, "reported source artifact has no path"))?;
    let bytes = std::fs::read(path)
        .map_err(|e| outcome(Status::Wrong, format!("cannot read reported artifact {path}: {e}")))?;
    if bytes.is_empty() || check["artifact"] != format!("sha256:{:x}", Sha256::digest(&bytes)) {
        return Err(outcome(Status::Wrong, "artifact hash does not describe the actual file"));
    }
    Ok(bytes)
}

fn report_sanitizer_capability(check: &Value) -> Result<(), Outcome> {
    let bad = |s| outcome(Status::Wrong, s);
    let tools: &[&str] = match check["id"].as_str() {
        Some("address-undefined") => &["address", "undefined"],
        Some("thread") => &["thread"],
        _ => return Err(bad("unknown sanitizer profile".into())),
    };
    if check["tools"] != json!(tools) {
        return Err(bad("sanitizer profile omits a requested tool".into()));
    }
    let probes = check["capability_probes"].as_array()
        .ok_or_else(|| bad("sanitizer passed without negative capability probes".into()))?;
    if probes.len() != tools.len() {
        return Err(bad("sanitizer passed without a negative probe for each requested tool".into()));
    }
    for tool in tools {
        let matching: Vec<_> = probes.iter().filter(|p| p["tool"] == *tool).collect();
        if matching.len() != 1 { return Err(bad(format!("missing or duplicated {tool} capability probe"))); }
        let probe = matching[0];
        let run = &probe["run"];
        let stderr = run["stderr"].as_str().unwrap_or("");
        let detected = match *tool {
            "address" => stderr.contains("AddressSanitizer") && stderr.contains("heap-use-after-free"),
            "undefined" => stderr.contains("signed integer overflow") && (stderr.contains("runtime error:") || stderr.to_lowercase().contains("ubsan")),
            "thread" => stderr.contains("ThreadSanitizer") && stderr.contains("data race"),
            _ => false,
        };
        let nonzero = run["exit_code"].as_i64().is_some_and(|code| code != 0)
            || run["signal"].as_i64().is_some_and(|signal| signal > 0);
        if probe["status"] != "passed" || probe["build"]["status"] != "passed"
            || run["expected_defect_detected"] != true || run["success"] != false
            || run["timed_out"] != false || run["stderr_truncated"] != false
            || run["ran_on"] != "x86_64-linux-gnu" || !nonzero || !detected
            || run["artifact"] != probe["build"]["artifact"]
        {
            return Err(bad(format!("sanitizer passed without actual {tool} defect detection: {probe}")));
        }
        report_artifact(&probe["build"])?;
        report_artifact(run)?;
    }
    Ok(())
}

fn check_report(entry: &Value, path: &Path, source: &[u8]) -> Result<(), Outcome> {
    let bad = |s| outcome(Status::Wrong, s);
    let fixture: Value = serde_json::from_slice(source).map_err(|e| bad(format!("invalid report fixture: {e}")))?;
    if fixture["setup"] != entry["setup"] || fixture["expect"] != entry["expected_report"] {
        return Err(bad("report fixture and manifest disagree".into()));
    }
    let program = fixture["setup"]["program"].as_str().ok_or_else(|| bad("report fixture has no program".into()))?;
    let program = path.parent().unwrap().join(program);
    let before = std::fs::read(&program).map_err(|e| bad(format!("cannot read report input: {e}")))?;
    let id = entry["id"].as_str().unwrap_or("");
    let is_allocation = matches!(id, "T-GA1" | "T-GA2");
    if !is_allocation && !matches!(id, "T-R1" | "T-R2") {
        return Err(outcome(Status::NotRun, format!("no harness for report scenario {id}")));
    }
    // The fixture describes a controlled Linux-only runner. Do not turn its metadata into
    // an assertion of platform availability, or simulate a Windows run on another host.
    if !is_allocation && !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err(outcome(Status::NotRun, "report scenario requires the x86_64 Linux runner"));
    }
    let scratch = common::Scratch::new("conformance-report").map_err(|e| bad(e.to_string()))?;
    let mut cmd = yavo();
    if is_allocation {
        cmd.arg("allocations").arg("demo").arg("--file").arg(&program).arg("--json");
    } else {
        cmd.arg("verify").arg(&program).args(["--target", "x86_64-windows-gnu", "--json"])
            .arg("--output-dir").arg(&scratch.0).env_remove("YAVO_CC");
        if id == "T-R2" { cmd.arg("--sanitize"); }
    }
    let timeout = if id == "T-R2" { Duration::from_secs(360) } else { common::BUILD_TIMEOUT };
    let captured = common::capture(cmd.stdin(Stdio::null()), timeout)
        .map_err(|e| outcome(Status::NotRun, format!("could not run report command: {e}")))?;
    if captured.timed_out { return Err(bad(format!("report command exceeded its {} second deadline", timeout.as_secs()))); }
    let output = captured.output;
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|e| bad(format!(
        "report command returned invalid JSON: {e}; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))))?;
    if std::fs::read(&program).map_err(|e| bad(e.to_string()))? != before {
        return Err(bad("report command modified its source input".into()));
    }
    if is_allocation {
        if output.status.code() != Some(0) || report["schema"] != "yavo-allocations/1"
            || report["source_sha256"] != format!("{:x}", Sha256::digest(&before))
            || report["subject"] != fixture["expect"]["subject"]
            || report["status"] != fixture["expect"]["status"]
            || !report["operations"].is_array()
        {
            return Err(bad(format!("allocation evidence does not match report fixture: {report}")));
        }
        return Ok(());
    }
    if report["schema"] != "yavo-verify/1"
        || report["inputs"]["source"] != format!("sha256:{:x}", Sha256::digest(&before))
    {
        return Err(bad("verification receipt has the wrong schema or source identity".into()));
    }
    let saved_path = report["report_path"].as_str().ok_or_else(|| bad("report has no saved path".into()))?;
    let saved: Value = serde_json::from_slice(&std::fs::read(saved_path).map_err(|e| bad(e.to_string()))?)
        .map_err(|e| bad(e.to_string()))?;
    if saved != report { return Err(bad("saved verification receipt differs from stdout".into())); }
    let build = report_check(&report, "build", "x86_64-windows-gnu")?;
    if matches!(build["status"].as_str(), Some("unsupported" | "not_run")) {
        return Err(outcome(Status::NotRun, format!("Windows build unavailable: {}", build["reason"])));
    }
    if build["status"] != "passed" || build["ran_on"] != "x86_64-linux-gnu" {
        return Err(bad(format!("expected an actual Windows cross-build: {build}")));
    }
    let windows = report_artifact(build)?;
    if !windows.starts_with(b"MZ") { return Err(bad("Windows artifact is not a PE executable".into())); }
    let tests = report_check(&report, "tests", "x86_64-windows-gnu")?;
    if tests["status"] != "not_run" || !tests["ran_on"].is_null() || tests["artifact"] != build["artifact"] {
        return Err(bad(format!("unavailable Windows execution was misreported: {tests}")));
    }
    if output.status.code() != Some(5) || report["overall"] != "not_verified" {
        return Err(bad(format!("missing Windows execution must prevent verification: {report}")));
    }
    if id == "T-R2" {
        let checks: Vec<_> = report["checks"].as_array().into_iter().flatten()
            .filter(|c| c["name"] == "sanitizers").collect();
        let profiles: BTreeSet<_> = checks.iter().filter_map(|c| c["id"].as_str()).collect();
        if checks.len() != 2 || profiles != BTreeSet::from(["address-undefined", "thread"]) {
            return Err(bad("sanitizer profiles are missing or merged".into()));
        }
        let mut unavailable = Vec::new();
        for check in checks {
            if check["target"] != "x86_64-linux-gnu" || check["required"] != true {
                return Err(bad(format!("sanitizer did not target the actual Linux runner: {check}")));
            }
            match check["status"].as_str() {
                Some("passed") => {
                    report_sanitizer_capability(check)?;
                    if check["ran_on"] != "x86_64-linux-gnu" || check["exit_code"] != 0
                        || check["artifact"] == build["artifact"] || report_artifact(check)? == windows
                    {
                        return Err(bad("sanitizer evidence is not an independently executed Linux artifact".into()));
                    }
                }
                Some("unsupported") => {
                    if !check["artifact"].is_null() {
                        return Err(bad("unsupported sanitizer fabricated an artifact".into()));
                    }
                    unavailable.push(format!("{}: {}", check["id"], check["reason"]));
                }
                Some("not_run") => return Err(outcome(Status::NotRun, format!("sanitizer was not run: {}", check["reason"]))),
                _ => return Err(bad(format!("sanitizer failed for a valid report fixture: {check}"))),
            }
        }
        if !unavailable.is_empty() {
            // `null != windows_hash` is not evidence that a sanitizer artifact ran.
            return Err(outcome(Status::Unsupported, unavailable.join("; ")));
        }
    }
    Ok(())
}

// Use the production lexer, not regexes: "//" can occur inside a string.
// Interpolation offsets refer to each source separately, so compare expressions.
fn token_signature(src: &str) -> Result<Vec<Value>, String> {
    fn signature(src: &str, tokens: Vec<lexer::Token>, diags: &mut diag::Diags) -> Vec<Value> {
        tokens.into_iter().map(|t| match t.tok {
            lexer::Tok::Str(pieces) => json!({"string": pieces.into_iter().map(|p| match p {
                lexer::StrPiece::Text(s) => json!({"text": s}),
                lexer::StrPiece::Interp { start, end, line, col, spec } => {
                    let tokens = lexer::lex_range(src, start, end, line, col, false, diags);
                    json!({"interpolation": signature(src, tokens, diags), "format": spec})
                }
            }).collect::<Vec<_>>()}),
            tok => json!(format!("{tok:?}")),
        }).collect()
    }
    let mut diags = diag::Diags::default();
    let tokens = lexer::lex_file(src, &mut diags);
    let tokens = signature(src, tokens, &mut diags);
    if diags.has_errors() { Err(diag::render_human("fix comparison", src, &diags)) }
    else { Ok(tokens) }
}

fn apply_fix_edits(src: &str, fixes: &[Value]) -> Result<String, String> {
    let mut edits = Vec::new();
    for fix in fixes {
        if fix["recheck"] != "passed" { return Err("fix was not rechecked".into()); }
        let parts = fix["edits"].as_array().ok_or("fix has no edits array")?;
        if parts.is_empty() { return Err("fix has no edits".into()); }
        for edit in parts {
            let start = edit["start"].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("invalid edit start")?;
            let end = edit["end"].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or("invalid edit end")?;
            let replacement = edit["replacement"].as_str().ok_or("invalid edit replacement")?;
            if start > end || end > src.len() || !src.is_char_boundary(start) || !src.is_char_boundary(end) {
                return Err(format!("edit {start}..{end} is not a valid UTF-8 byte range"));
            }
            edits.push((start, end, replacement));
        }
    }
    edits.sort_by_key(|e| (e.0, e.1));
    for pair in edits.windows(2) {
        if pair[1].0 < pair[0].1 || (pair[0].0 == pair[0].1 && pair[0].0 == pair[1].0) {
            return Err("fix edits overlap".into());
        }
    }
    let mut result = src.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, replacement);
    }
    Ok(result)
}

fn run_fix_json(path: &Path) -> Result<Value, Outcome> {
    let captured = common::capture(yavo().arg("fix").arg(path).arg("--json-diagnostics")
        .stdin(Stdio::null()), common::RUN_TIMEOUT)
        .map_err(|e| outcome(Status::NotRun, format!("could not start yavo fix: {e}")))?;
    if captured.timed_out { return Err(outcome(Status::Wrong, "yavo fix exceeded its 10 second deadline")); }
    let out = captured.output;
    let report: Value = serde_json::from_slice(&out.stdout).map_err(|e|
        outcome(Status::Wrong, format!("fix output is not JSON: {e}; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))))?;
    if report["schema"] != "yavo-fix/1" {
        return Err(outcome(Status::Wrong, format!("unexpected fix schema: {}", report["schema"])));
    }
    let diags = report["diagnostics"].as_array().ok_or_else(|| outcome(Status::Wrong, "fix report has no diagnostics"))?;
    let errors = diags.iter().filter(|d| d["severity"] == "ERROR").count();
    let expected_exit = if errors == 0 { 0 } else { 1 };
    if out.status.code() != Some(expected_exit) || report["error_count"].as_u64() != Some(errors as u64) {
        return Err(outcome(Status::Wrong, format!("fix exit/error count inconsistent with {errors} errors: {:?}", out.status.code())));
    }
    Ok(report)
}

fn diagnostic_keys(value: &Value) -> BTreeSet<(String, String, u64)> {
    value.as_array().into_iter().flatten().map(|d| (
        d["severity"].as_str().unwrap_or("").to_string(),
        d["code"].as_str().unwrap_or("").to_string(),
        d["line"].as_u64().unwrap_or(0),
    )).collect()
}

fn classify_fix(entry: &Value, path: &Path, source: &[u8]) -> Outcome {
    match check_fix(entry, path, source) {
        Ok(()) => outcome(Status::Pass, ""),
        Err(o) => o,
    }
}

fn check_fix(entry: &Value, path: &Path, source: &[u8]) -> Result<(), Outcome> {
    let bad = |s| outcome(Status::Wrong, s);
    let original = run_check_json(path)?;
    let diagnostics = original["diagnostics"].as_array().unwrap();
    if let Some(d) = diagnostics.iter().find(|d| d["code"] == "E0020") {
        return Err(bad(format!("syntax error on valid v0.5 fix fixture: {}", d["message"])));
    }
    // T-V3 needs unsupported mutable-reference returns and a complex mutable
    // receiver. Its candidate requirement must not count as implemented.
    if let Some(d) = diagnostics.iter().find(|d| d["code"].as_str().is_some_and(|c| c.starts_with('U'))) {
        return Err(outcome(Status::Unsupported, d["message"].as_str().unwrap_or("unsupported fix input")));
    }
    if diagnostic_keys(&original["diagnostics"]) != diagnostic_keys(&entry["diagnostics"]) {
        return Err(bad(format!("initial fix diagnostics differ: {}", original["diagnostics"])));
    }
    let source = std::str::from_utf8(source).map_err(|e| bad(e.to_string()))?;
    let scratch = common::Scratch::new("conformance-fix").map_err(|e| bad(e.to_string()))?;
    let input = scratch.0.join(path.file_name().unwrap());
    std::fs::write(&input, source).map_err(|e| bad(e.to_string()))?;
    let report = run_fix_json(&input)?;
    let fixed = std::fs::read_to_string(&input).map_err(|e| bad(e.to_string()))?;
    if report["source_sha256"] != format!("{:x}", Sha256::digest(source.as_bytes()))
        || report["result_sha256"] != format!("{:x}", Sha256::digest(fixed.as_bytes()))
        || report["dry_run"] != false
        || report["changed"] != (fixed != source)
        || diagnostic_keys(&report["original_diagnostics"]) != diagnostic_keys(&original["diagnostics"])
    {
        return Err(bad("fix receipt does not match its source or actual result".into()));
    }
    let applied = report["applied"].as_array().ok_or_else(|| bad("fix report has no applied array".into()))?;
    let candidates = report["candidates"].as_array().ok_or_else(|| bad("fix report has no candidates array".into()))?;
    if entry["kind"] == "fix-auto" {
        if applied.is_empty() || report["automatic"] != report["applied"]
            || applied.iter().any(|f| f["applicability"] != "automatic")
        {
            return Err(bad("automatic fixture was not automatically fixed".into()));
        }
        if apply_fix_edits(source, applied).map_err(bad)? != fixed {
            return Err(bad("reported applied edits do not produce the actual file".into()));
        }
        let expected_path = path.parent().unwrap().join(entry["expect_fixed"].as_str().unwrap());
        let expected = std::fs::read_to_string(expected_path).map_err(|e| bad(e.to_string()))?;
        if token_signature(&fixed).map_err(bad)? != token_signature(&expected).map_err(bad)? {
            return Err(bad(format!("fixed tokens differ from expect_fixed: {fixed}")));
        }
        let errors = run_check(&input)?;
        if !errors.is_empty() { return Err(bad(format!("fixed file does not check: {}", describe(&errors)))); }
        let second = run_fix_json(&input)?;
        if second["changed"] != false || second["applied"] != json!([])
            || std::fs::read_to_string(&input).map_err(|e| bad(e.to_string()))? != fixed
        {
            return Err(bad("automatic repair is not idempotent".into()));
        }
    } else {
        if fixed != source || !applied.is_empty() {
            return Err(bad("candidate-only fixture was changed automatically".into()));
        }
        let want: BTreeSet<&str> = entry["expect_candidates"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let got: BTreeSet<&str> = candidates.iter().filter_map(|f| f["id"].as_str()).collect();
        if want != got { return Err(bad(format!("expected candidates {want:?}, got {got:?}"))); }
        for (index, candidate) in candidates.iter().enumerate() {
            if candidate["applicability"] != "candidate" { return Err(bad("candidate has incorrect applicability".into())); }
            let text = apply_fix_edits(source, std::slice::from_ref(candidate)).map_err(bad)?;
            let candidate_path = scratch.0.join(format!("candidate-{index}.yv"));
            std::fs::write(&candidate_path, text).map_err(|e| bad(e.to_string()))?;
            let errors = run_check(&candidate_path)?;
            if !errors.is_empty() {
                return Err(bad(format!("candidate {} does not independently check: {}", candidate["id"], describe(&errors))));
            }
        }
    }
    Ok(())
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
fn fixed_source_comparison_uses_lexical_strings_and_interpolations() {
    let a = "fn main() {\n    let n = 1\n    println(\"https://a/${n + 1}\")\n}\n";
    let b = "// metadata\n\nfn main() { // retained\n    let n = 1 // value\n    println(\"https://a/${n + 1}\") // output\n}\n";
    assert_eq!(token_signature(a).unwrap(), token_signature(b).unwrap());
    assert_ne!(token_signature(a).unwrap(), token_signature(&a.replace("https://a/", "https://b/")).unwrap());
    assert_ne!(token_signature(a).unwrap(), token_signature(&a.replace("n + 1", "n + 2")).unwrap());
    assert_ne!(token_signature("return\n1").unwrap(), token_signature("return 1").unwrap());
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
