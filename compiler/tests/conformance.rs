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
#[allow(dead_code)]
#[path = "../src/toolchain.rs"]
mod toolchain;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Status {
    Pass,
    Wrong,
    Unsupported,
    NotRun,
    /// The old expectation is overturned by a dated decision in `tests/conformance_superseded.txt`.
    Superseded,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Wrong => "wrong",
            Status::Unsupported => "unsupported",
            Status::NotRun => "not_run",
            Status::Superseded => "superseded",
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

fn superseded() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance_superseded.txt");
    std::fs::read_to_string(path).expect("read conformance_superseded.txt")
        .lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(' ').map(|(id, why)| (id.to_string(), why.to_string())))
        .collect()
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance_baseline.txt")
}

fn yavo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yavo"))
}

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
        "check-pass" | "check-fail" | "run-pass" | "run-panic" | "run-exit" | "run-allowed" => {}
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
            if let Err(o) = platform_requirement(entry) { return o; }
            // `modes: ["debug", "release"]` means the case must hold in both generated-C optimisation modes.
            let modes = match build_modes(entry) {
                Ok(modes) => modes,
                Err(o) => return o,
            };
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
            if !entry["sanitizer_profiles"].is_null() {
                return run_sanitized(entry, &path, kind);
            }
            if !entry["sanitize_mode"].is_null() {
                return outcome(Status::Wrong, "sanitize_mode requires sanitizer_profiles");
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

fn platform_requirement(entry: &Value) -> Result<(), Outcome> {
    let Some(platforms) = entry.get("platforms") else { return Ok(()); };
    let platforms = platforms.as_array().filter(|v| !v.is_empty())
        .ok_or_else(|| outcome(Status::Wrong, "platforms must be a nonempty array"))?;
    if platforms.iter().any(|p| !p.is_string()) {
        return Err(outcome(Status::Wrong, "platform names must be strings"));
    }
    if platforms.iter().any(|p| p == std::env::consts::OS) { Ok(()) }
    else { Err(outcome(Status::NotRun, format!("requires one of {platforms:?}; host is {}", std::env::consts::OS))) }
}

fn build_modes(entry: &Value) -> Result<Vec<String>, Outcome> {
    let Some(value) = entry.get("modes") else { return Ok(vec!["debug".into()]); };
    let values = value.as_array().filter(|v| !v.is_empty())
        .ok_or_else(|| outcome(Status::Wrong, "modes must be a nonempty array"))?;
    let mut modes = Vec::new();
    for value in values {
        let Some(mode @ ("debug" | "release")) = value.as_str() else {
            return Err(outcome(Status::Wrong, "modes accepts only debug and release"));
        };
        if modes.iter().any(|m| m == mode) {
            return Err(outcome(Status::Wrong, format!("duplicate build mode: {mode}")));
        }
        modes.push(mode.to_string());
    }
    Ok(modes)
}

fn raw_args(entry: &Value) -> Result<Vec<std::ffi::OsString>, Outcome> {
    let Some(args) = entry.get("args_hex") else { return Ok(Vec::new()); };
    let args = args.as_array().ok_or_else(|| outcome(Status::Wrong, "args_hex must be an array"))?;
    let mut result = Vec::new();
    for hex in args {
        let hex = hex.as_str().ok_or_else(|| outcome(Status::Wrong, "args_hex elements must be hex strings"))?;
        let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2)
            .map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect();
        let bytes = bytes.filter(|_| hex.len() % 2 == 0)
            .ok_or_else(|| outcome(Status::Wrong, "args_hex contains invalid hex"))?;
        if bytes.contains(&0) { return Err(outcome(Status::Wrong, "native command-line arguments cannot contain NUL")); }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            result.push(std::ffi::OsString::from_vec(bytes));
        }
        #[cfg(not(unix))]
        {
            result.push(String::from_utf8(bytes).map_err(|_| outcome(Status::NotRun,
                "raw non-UTF-8 argv requires a Unix runner; Windows uses UTF-16 arguments"))?.into());
        }
    }
    Ok(result)
}

fn fault_allocation(entry: &Value) -> Result<Option<u64>, Outcome> {
    if entry["fault_alloc_at"].is_null() && entry["fault_scope"].is_null() { return Ok(None); }
    if entry["fault_scope"] != "after-main-entry" {
        return Err(outcome(Status::Wrong, "allocation injection requires fault_scope: after-main-entry"));
    }
    let n = entry["fault_alloc_at"].as_u64().or_else(|| entry["fault_alloc_at"].as_str()?.parse().ok());
    let n = n.filter(|&n| n > 0 && n <= i64::MAX as u64)
        .ok_or_else(|| outcome(Status::Wrong, "fault_alloc_at must be a positive allocation index"))?;
    Ok(Some(n))
}

/// Judge actual process output, never a compiler failure or timeout. Allowed stdout
/// requires clean stderr; allowed panics require a single diagnostic. Ordinary panic
/// fixtures may explicitly require output emitted before the panic.
fn judge_runtime(entry: &Value, kind: &str, code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Outcome {
    let (Ok(stdout), Ok(stderr)) = (std::str::from_utf8(stdout), std::str::from_utf8(stderr)) else {
        return outcome(Status::Wrong, "program output is not valid UTF-8");
    };
    let stdout = stdout.replace("\r\n", "\n");
    let stderr = stderr.replace("\r\n", "\n");
    let sanitizer_error = ["AddressSanitizer", "ThreadSanitizer", "UndefinedBehaviorSanitizer", "runtime error:", "ubsan_rt", "LeakSanitizer"]
        .iter().any(|marker| stderr.contains(marker));
    let panic_matches = |category: &str| {
        !category.is_empty() && code == Some(101) && !sanitizer_error
            && stderr.lines().any(|line| line.starts_with(&format!("yavo panic [{category}] at ")))
    };
    let accepted = match kind {
        "run-pass" => code == Some(0) && stdout == entry["expected_stdout"].as_str().unwrap_or("") && stderr.is_empty(),
        "run-panic" => panic_matches(entry["expect_panic"].as_str().or(entry["expect"].as_str()).unwrap_or(""))
            && entry["expected_stdout"].as_str().is_none_or(|want| stdout == want)
            && entry["expect_stderr_contains"].as_str().is_none_or(|want| stderr.contains(want)),
        "run-exit" => {
            let expected = entry["expect_exit"].as_str().unwrap_or("");
            let valid_exit = if expected == "nonzero" { code.is_some_and(|c| c != 0) }
                else { entry["expect_exit"].as_i64().or_else(|| expected.parse().ok()) == code.map(i64::from) && code.is_some() };
            let marker = entry["expect_stderr_code"].as_str().unwrap_or("");
            valid_exit && !marker.is_empty() && stderr.starts_with(&format!("yavo: {marker}: "))
                && stderr.lines().count() == 1 && stdout == entry["expected_stdout"].as_str().unwrap_or("")
        }
        "run-allowed" => {
            let Some(allowed) = entry["allowed_outcomes"].as_array().filter(|a| !a.is_empty()) else {
                return outcome(Status::Wrong, "run-allowed requires nonempty allowed_outcomes");
            };
            let mut matched = false;
            for allowed in allowed {
                let Some(allowed) = allowed.as_str() else { return outcome(Status::Wrong, "allowed_outcomes must contain strings"); };
                if let Some(want) = allowed.strip_prefix("stdout:") {
                    matched |= code == Some(0) && stdout == format!("{want}\n") && stderr.is_empty();
                } else if let Some(category) = allowed.strip_prefix("panic:").filter(|c| !c.is_empty()) {
                    matched |= panic_matches(category) && stderr.lines().count() == 1;
                } else { return outcome(Status::Wrong, format!("unknown allowed outcome: {allowed}")); }
            }
            matched
        }
        _ => return outcome(Status::Wrong, format!("unknown runtime kind: {kind}")),
    };
    if accepted { outcome(Status::Pass, "") }
    else { outcome(Status::Wrong, format!("{kind}: exit {code:?}, stdout {stdout:?}, stderr {stderr:?} do not match the manifest")) }
}

fn run_once(entry: &Value, path: &Path, kind: &str, release: bool, dir: Option<&Path>) -> Outcome {
    if let Err(o) = platform_requirement(entry) { return o; }
    let args = match raw_args(entry) { Ok(a) => a, Err(o) => return o };
    let fault = match fault_allocation(entry) { Ok(n) => n, Err(o) => return o };
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
    if fault.is_some() {
        cmd.arg("check").arg(path).arg("--emit-c").arg(scratch.0.join("fixture.c"));
    } else {
        cmd.arg("build").arg(path).arg("-o").arg(&exe);
        if release { cmd.arg("--release"); }
    }
    cmd.stdin(Stdio::null());
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
    if let Some(n) = fault {
        // Interpose allocation calls only in this runner-owned translation unit.
        // The production runtime, code generator and compiler options have no hook.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let wrapper = format!("#define YV_TEST_FAIL_AT {n}ULL\n#include \"fault_alloc.h\"\n#include \"fixture.c\"\n");
        if let Err(e) = std::fs::write(scratch.0.join("fault.c"), wrapper) {
            return outcome(Status::NotRun, format!("cannot write test wrapper: {e}"));
        }
        let tc = match toolchain::find_pinned() { Ok(tc) => tc, Err(e) => return outcome(Status::NotRun, e) };
        let mut cc = Command::new(tc.program);
        cc.args(tc.prefix_args).args(["-std=c17", "-fno-strict-aliasing", if release { "-O2" } else { "-O0" }])
            .arg("-I").arg(root.join("runtime")).arg("-I").arg(root.join("tests/common"))
            .arg(scratch.0.join("fault.c")).arg("-o").arg(&exe).stdin(Stdio::null());
        let output = match common::capture(&mut cc, common::BUILD_TIMEOUT) {
            Ok(o) if o.timed_out => return outcome(Status::Wrong, "fault-injection build timed out"),
            Ok(o) => o.output,
            Err(e) => return outcome(Status::NotRun, format!("cannot run pinned compiler: {e}")),
        };
        if !output.status.success() { return outcome(Status::Wrong,
            format!("fault-injection build failed: {}", String::from_utf8_lossy(&output.stderr))); }
    }
    let mut run = Command::new(&exe);
    run.args(args).stdin(Stdio::null());
    if let Some(d) = dir { run.current_dir(d); }
    let out = match common::capture(&mut run, limit) {
        Ok(o) if o.timed_out => return outcome(Status::Wrong, format!("program exceeded its {} ms deadline", limit.as_millis())),
        Ok(o) => o.output,
        Err(e) => return outcome(Status::NotRun, format!("could not run program: {e}")),
    };
    judge_runtime(entry, kind, out.status.code(), &out.stdout, &out.stderr)
}

/// Reuse verification's negative capability probes and independent instrumented
/// artifacts. The manifest defines acceptable program exits, so an allowed panic
/// is judged here rather than confused with verify's default exit-zero assertion.
fn run_sanitized(entry: &Value, path: &Path, kind: &str) -> Outcome {
    match check_sanitized(entry, path, kind) {
        Ok(()) => outcome(Status::Pass, ""),
        Err(o) => o,
    }
}

fn check_sanitized(entry: &Value, path: &Path, kind: &str) -> Result<(), Outcome> {
    let bad = |s| outcome(Status::Wrong, s);
    let profiles = entry["sanitizer_profiles"].as_array().filter(|p| !p.is_empty())
        .ok_or_else(|| bad("sanitizer_profiles must be a nonempty array".into()))?;
    let mut ids = BTreeSet::new();
    for profile in profiles {
        let id = match profile.as_str() {
            Some("address+undefined") => "address-undefined",
            Some("thread") => "thread",
            _ => return Err(bad(format!("unsupported sanitizer profile syntax: {profile}"))),
        };
        if !ids.insert(id) { return Err(bad("sanitizer profiles must not be duplicated".into())); }
    }
    if !entry["sanitize_mode"].is_null() && entry["sanitize_mode"] != "separate-artifacts" {
        return Err(bad("unknown sanitize_mode".into()));
    }
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return Err(outcome(Status::NotRun, "sanitizer capability assertions currently require the x86_64 Linux runner"));
    }
    for field in ["fixture", "cwd", "args_hex", "fault_alloc_at", "fault_scope"] {
        if !entry[field].is_null() {
            return Err(outcome(Status::NotRun, format!("sanitizer runner does not yet combine with {field}")));
        }
    }
    let source = std::fs::read(path).map_err(|e| bad(e.to_string()))?;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance-sanitizers");
    std::fs::create_dir_all(&dir).map_err(|e| bad(e.to_string()))?;
    let limit = runtime_limit(entry).map_err(bad)?;
    let mut command = yavo();
    command.arg("verify").arg(path).args(["--sanitize", "--json"])
        .arg("--timeout-ms").arg(limit.as_millis().to_string())
        .arg("--output-dir").arg(&dir).env_remove("YAVO_CC").stdin(Stdio::null());
    let captured = common::capture(&mut command, Duration::from_secs(600))
        .map_err(|e| outcome(Status::NotRun, format!("cannot execute sanitizer verification: {e}")))?;
    if captured.timed_out { return Err(bad("sanitizer verification exceeded its 600 second deadline".into())); }
    let output = captured.output;
    if !matches!(output.status.code(), Some(0 | 1 | 5)) {
        return Err(bad(format!("verification failed to produce evidence: exit {:?}, {}", output.status.code(), String::from_utf8_lossy(&output.stderr))));
    }
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|e| bad(format!("invalid verification JSON: {e}")))?;
    if report["schema"] != "yavo-verify/1"
        || report["inputs"]["source"] != format!("sha256:{:x}", Sha256::digest(&source))
        || std::fs::read(path).map_err(|e| bad(e.to_string()))? != source
    { return Err(bad("sanitizer receipt is not bound to the unchanged source".into())); }
    let saved = report["report_path"].as_str().ok_or_else(|| bad("verification report has no saved path".into()))?;
    let disk: Value = serde_json::from_slice(&std::fs::read(saved).map_err(|e| bad(e.to_string()))?)
        .map_err(|e| bad(e.to_string()))?;
    if disk != report { return Err(bad("saved sanitizer receipt differs from stdout".into())); }
    let checks = report["checks"].as_array().ok_or_else(|| bad("verification has no checks".into()))?;
    for name in ["front_end", "toolchain", "build"] {
        let item = checks.iter().find(|c| c["name"] == name).ok_or_else(|| bad(format!("missing {name} evidence")))?;
        if matches!(item["status"].as_str(), Some("not_run" | "unsupported")) {
            return Err(outcome(Status::NotRun, format!("{name}: {}", item["reason"])));
        }
        if item["status"] != "passed" { return Err(bad(format!("{name} failed: {}", item["reason"]))); }
    }
    let mut unavailable = Vec::new();
    let mut artifacts = BTreeSet::new();
    for id in ids {
        let matching: Vec<_> = checks.iter().filter(|c| c["name"] == "sanitizers" && c["id"] == id).collect();
        if matching.len() != 1 { return Err(bad(format!("missing or duplicate sanitizer {id}"))); }
        let check = matching[0];
        if check["target"] != "x86_64-linux-gnu" || check["required"] != true {
            return Err(bad("sanitizer evidence targets the wrong platform or is not required".into()));
        }
        match check["status"].as_str() {
            Some("unsupported" | "not_run") => {
                if !check["artifact"].is_null() { return Err(bad("unavailable sanitizer claims an application artifact".into())); }
                unavailable.push(format!("{id}: {}", check["reason"]));
                continue;
            }
            Some("passed" | "failed") => {}
            _ => return Err(bad(format!("{id}: sanitizer execution failed or timed out: {}", check["reason"]))),
        }
        report_sanitizer_capability(check)?;
        if check["build"]["status"] != "passed" || check["artifact"] != check["build"]["artifact"]
            || check["artifact_unchanged"] != true || check["ran_on"] != "x86_64-linux-gnu"
            || check["tests_run"] != 1 || check["timed_out"] != false
            || check["stdout_truncated"] != false || check["stderr_truncated"] != false
            || check["stdout_valid_utf8"] != true || check["stderr_valid_utf8"] != true
        { return Err(bad(format!("{id}: incomplete or invalid execution evidence"))); }
        report_artifact(check)?;
        report_artifact(&check["build"])?;
        if !artifacts.insert(check["artifact"].as_str().unwrap_or("").to_string()) {
            return Err(bad("different sanitizer profiles reused the same artifact".into()));
        }
        let stdout = check["stdout"].as_str().ok_or_else(|| bad("execution has no stdout".into()))?;
        let stderr = check["stderr"].as_str().ok_or_else(|| bad("execution has no stderr".into()))?;
        for (stream, text) in [("stdout", stdout), ("stderr", stderr)] {
            if check[format!("{stream}_sha256")] != format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
                || check[format!("{stream}_bytes")] != text.len() as u64
            { return Err(bad(format!("{id}: output evidence does not describe the complete {stream}"))); }
        }
        let result = judge_runtime(entry, kind, check["exit_code"].as_i64().and_then(|c| i32::try_from(c).ok()), stdout.as_bytes(), stderr.as_bytes());
        if result.status != Status::Pass { return Err(bad(format!("{id}: {}", result.reason))); }
    }
    if !unavailable.is_empty() {
        // Check every available profile first: unavailable ASan must not hide a TSan defect.
        return Err(outcome(Status::Unsupported, unavailable.join("; ")));
    }
    Ok(())
}

#[test]
fn conformance() {
    let root = tests_root();
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("MANIFEST.json")).expect("read MANIFEST.json"))
            .expect("parse MANIFEST.json");
    let entries = manifest.as_array().expect("MANIFEST.json must be an array");

    let superseded = superseded();
    let rows: Vec<(String, String, String, Outcome)> = entries
        .iter()
        .map(|e| {
            let id = e["id"].as_str().unwrap_or("?").to_string();
            let mut o = classify(e, &root);
            if let Some(why) = superseded.get(&id) {
                o = match o.status {
                    Status::Pass => outcome(Status::Wrong, "listed as superseded but still meets its old expectation; remove it from the list"),
                    _ => outcome(Status::Superseded, format!("{why} (was: {})", o.reason)),
                };
            }
            (id, e["phase"].as_str().unwrap_or("?").to_string(), e["category"].as_str().unwrap_or("?").to_string(), o)
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
    println!("{:<8}{:>7}{:>7}{:>13}{:>9}{:>12}", "phase", "pass", "wrong", "unsupported", "not_run", "superseded");
    let mut phases: Vec<String> = rows.iter().map(|r| r.1.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    phases.push("total".into());
    for p in &phases {
        let n = |s| by_phase.get(&(p.clone(), s)).copied().unwrap_or(0);
        println!("{:<8}{:>7}{:>7}{:>13}{:>9}{:>12}", p, n(Status::Pass), n(Status::Wrong), n(Status::Unsupported), n(Status::NotRun), n(Status::Superseded));
    }
    println!("\nwrong:");
    for (id, _, _, o) in rows.iter().filter(|r| r.3.status == Status::Wrong) {
        println!("  {id}: {}", o.reason);
    }
    println!("\nsuperseded by later decisions:");
    for (id, _, _, o) in rows.iter().filter(|r| r.3.status == Status::Superseded) {
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
    // A superseded case leaves the baseline: its old expectation no longer holds by decision.
    let baseline: BTreeSet<String> = read_baseline(&baseline_path()).expect("read existing conformance baseline")
        .into_iter().filter(|id| !superseded.contains_key(id)).collect();
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
    let superseded = superseded();
    let old: BTreeSet<String> = read_baseline(path)?.into_iter().filter(|id| !superseded.contains_key(id)).collect();
    require_monotonic(&old, passing)?;
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

#[test]
fn allowed_outcomes_reject_wrong_exits_extra_output_and_sanitizer_diagnostics() {
    let entry = json!({"allowed_outcomes": ["stdout:intact", "panic:P-COMPARATOR"]});
    let panic = b"yavo panic [P-COMPARATOR] at test.yv:1:1: invalid comparison\n";
    for (code, stdout, stderr, expected) in [
        (Some(0), &b"intact\n"[..], &b""[..], Status::Pass),
        (Some(0), &b"intact\r\n"[..], &b""[..], Status::Pass),
        (Some(101), &b""[..], &panic[..], Status::Pass),
        (Some(0), &b"intact"[..], &b""[..], Status::Wrong),
        (Some(0), &b"intact\nextra\n"[..], &b""[..], Status::Wrong),
        (Some(17), &b"intact\n"[..], &b""[..], Status::Wrong),
        (None, &b"intact\n"[..], &b""[..], Status::Wrong),
        (Some(0), &b"intact\n"[..], &b"runtime error: signed integer overflow\n"[..], Status::Wrong),
        (Some(101), &b""[..], &b"yavo panic [P-COMPARATOR] at x:1:1: bad\nAddressSanitizer: heap-use-after-free\n"[..], Status::Wrong),
        (Some(101), &b""[..], &b"yavo panic [P-PANIC] at x:1:1: bad\n"[..], Status::Wrong),
        (Some(0), &b"\xff"[..], &b""[..], Status::Wrong),
    ] {
        let got = judge_runtime(&entry, "run-allowed", code, stdout, stderr);
        assert_eq!(got.status, expected, "{}", got.reason);
    }
    for allowed in [json!([]), json!(["stdout:intact", "arbitrary-success"]), json!([1])] {
        let got = judge_runtime(&json!({"allowed_outcomes": allowed}), "run-allowed", Some(0), b"intact\n", b"");
        assert_eq!(got.status, Status::Wrong);
    }
    let conversion = json!({"expect_panic": "P-CONVERT", "expect_stderr_contains": "before"});
    for (stderr, status) in [
        (&b"before\nyavo panic [P-CONVERT] at x:1:1: bad\n"[..], Status::Pass),
        (&b"yavo panic [P-CONVERT] at x:1:1: bad\n"[..], Status::Wrong),
        (&b"before\nyavo panic [P-CONVERT] at x:1:1: bad\nruntime error: overflow\n"[..], Status::Wrong),
    ] {
        assert_eq!(judge_runtime(&conversion, "run-panic", Some(101), b"", stderr).status, status);
    }
}

#[test]
fn raw_argument_and_fault_metadata_are_validated_without_lossy_decoding() {
    for value in [json!([]), json!([17]), json!(["turbo"]), json!(["debug", "debug"]), json!(null), json!("debug")] {
        assert!(build_modes(&json!({"modes": value})).is_err(), "{value}");
    }
    assert_eq!(build_modes(&json!({})).unwrap_or_else(|o| panic!("{}", o.reason)), ["debug"]);
    assert_eq!(build_modes(&json!({"modes": ["debug", "release"]})).unwrap_or_else(|o| panic!("{}", o.reason)), ["debug", "release"]);
    for hex in ["ff0", "zz", "00", "aabb00cc", "中文"] {
        assert!(raw_args(&json!({"args_hex": [hex]})).is_err(), "{hex}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let args = raw_args(&json!({"args_hex": ["66ff", ""]})).unwrap_or_else(|o| panic!("{}", o.reason));
        assert_eq!(args[0].as_bytes(), b"f\xff");
        assert!(args[1].is_empty());
    }
    for entry in [json!({"fault_alloc_at": 1}), json!({"fault_scope": "after-main-entry"}),
        json!({"fault_scope": "startup", "fault_alloc_at": 1}),
        json!({"fault_scope": "after-main-entry", "fault_alloc_at": 0}),
        json!({"fault_scope": "after-main-entry", "fault_alloc_at": -1})] {
        assert!(fault_allocation(&entry).is_err(), "{entry}");
    }
}

#[test]
fn fault_injection_starts_after_startup_and_can_select_a_later_allocation() {
    let scratch = common::Scratch::new("fault-injection").unwrap();
    let path = scratch.0.join("alloc.yv");
    std::fs::write(&path, "fn main() {\n    println(\"entered\")\n    var values: Vec[int] = []\n    values.reserve(4)\n    println(\"first\")\n    values.reserve(8)\n    println(\"second\")\n}\n").unwrap();
    for release in [false, true] {
        for (n, expected) in [(1, "entered\n"), (2, "entered\nfirst\n")] {
            let entry = json!({"fault_scope": "after-main-entry", "fault_alloc_at": n,
                "expect_panic": "P-OOM", "expected_stdout": expected, "args_hex": ["e4b8ade69687"]});
            let got = run_once(&entry, &path, "run-panic", release, None);
            assert_eq!(got.status, Status::Pass, "allocation {n}, release={release}: {}", got.reason);
        }
        for entry in [json!({"expected_stdout": "entered\nfirst\nsecond\n"}),
            json!({"fault_scope": "after-main-entry", "fault_alloc_at": 3, "expected_stdout": "entered\nfirst\nsecond\n"})] {
            let got = run_once(&entry, &path, "run-pass", release, None);
            assert_eq!(got.status, Status::Pass, "normal/later allocation, release={release}: {}", got.reason);
        }
    }
}

#[cfg(unix)]
#[test]
fn invalid_native_argument_stops_before_user_main() {
    let scratch = common::Scratch::new("invalid-argv").unwrap();
    let path = scratch.0.join("startup.yv");
    std::fs::write(&path, "fn main() { println(\"main ran\") }\n").unwrap();
    let entry = json!({"args_hex": ["66ff"], "expect_exit": "nonzero", "expect_stderr_code": "YAVO-STARTUP-UTF8", "expected_stdout": ""});
    for release in [false, true] {
        let got = run_once(&entry, &path, "run-exit", release, None);
        assert_eq!(got.status, Status::Pass, "{}", got.reason);
    }
}
