//! Evidence-oriented tests for allocation queries and verification reports.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;

fn yavo() -> Command { Command::new(env!("CARGO_BIN_EXE_yavo")) }

fn input(label: &str, source: &str) -> (common::Scratch, PathBuf) {
    let scratch = common::Scratch::new(label).unwrap();
    let path = scratch.0.join("source file.yv");
    fs::write(&path, source).unwrap();
    (scratch, path)
}

fn output_json(command: &mut Command) -> (Output, Value) {
    // A sanitizer request has multiple independently bounded compiler stages.
    let timeout = if command.get_args().any(|arg| arg == "--sanitize") {
        std::time::Duration::from_secs(360)
    } else { common::BUILD_TIMEOUT };
    let output = common::output(command.stdin(Stdio::null()), timeout).unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!(
        "invalid report JSON: {e}; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)));
    (output, report)
}

fn allocations(path: &Path, subject: &str) -> Value {
    let before = fs::read(path).unwrap();
    let (out, report) = output_json(yavo().arg("allocations").arg(subject)
        .arg("--file").arg(path).arg("--json"));
    assert_eq!(out.status.code(), Some(0), "{report:#}; {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(report["schema"], "yavo-allocations/1");
    assert_eq!(report["subject"], subject);
    assert_eq!(report["source_sha256"], format!("{:x}", Sha256::digest(&before)));
    assert!(report["operations"].is_array(), "{report:#}");
    assert_eq!(fs::read(path).unwrap(), before);
    report
}

fn objects<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            out.push(value);
            for value in fields.values() { objects(value, out); }
        }
        Value::Array(values) => for value in values { objects(value, out); },
        _ => {}
    }
}

#[test]
fn allocation_queries_distinguish_scalars_known_allocation_and_dynamic_unknown() {
    let cases = [
        ("fn demo(x: int) -> int { return x + 1 }\nfn main() { println(demo(1)) }\n", "no_alloc"),
        ("fn demo() -> String { return format(\"${1}\") }\nfn main() { println(demo()) }\n", "may_alloc"),
        ("interface Inspect { fn inspect(self) -> int }\nfn demo(x: Box[Inspect]) -> int { return x.inspect() }\nfn main() {}\n", "unknown"),
    ];
    for (source, expected) in cases {
        let (_scratch, path) = input("allocation-status", source);
        let report = allocations(&path, "demo");
        assert_eq!(report["status"], expected, "{report:#}");
    }
}

#[test]
fn known_allocation_dominates_unknown_without_erasing_the_unknown_operation() {
    let source = "interface Inspect { fn inspect(self) -> int }\nfn demo(x: Box[Inspect]) -> String {\n    return format(\"${x.inspect()}\")\n}\nfn main() {}\n";
    let (_scratch, path) = input("allocation-mixed", source);
    let report = allocations(&path, "demo");
    assert_eq!(report["status"], "may_alloc", "{report:#}");
    let mut entries = Vec::new();
    objects(&report["operations"], &mut entries);
    assert!(entries.iter().any(|entry| entry["status"] == "unknown"), "{report:#}");
    assert!(entries.iter().any(|entry| entry["callee"]["status"] == "may_alloc"), "{report:#}");
}

#[test]
fn argument_allocations_are_separate_from_the_called_functions_effect() {
    let source = "fn pure(text: String) -> int { return text.len() }\nfn demo() -> int { return pure(format(\"${1}\")) }\nfn main() { println(demo()) }\n";
    let (_scratch, path) = input("allocation-arguments", source);
    let report = allocations(&path, "demo");
    assert_eq!(report["status"], "may_alloc", "{report:#}");
    let mut entries = Vec::new();
    objects(&report["operations"], &mut entries);
    let call = entries.iter().find(|entry| entry["callee"]["name"] == "pure")
        .unwrap_or_else(|| panic!("missing pure call: {report:#}"));
    assert_eq!(call["parameters"]["status"], "may_alloc", "{report:#}");
    assert_eq!(call["callee"]["status"], "no_alloc", "{report:#}");
    assert_eq!(allocations(&path, "pure")["status"], "no_alloc");
}

#[test]
fn allocation_analysis_follows_user_methods_and_implicit_destructors() {
    let method = "struct S {\n    value: int\n    fn len(self) -> int {\n        let values: Vec[int] = [1]\n        return values.len()\n    }\n}\nfn demo() -> int {\n    let s = S{value: 1}\n    return s.len()\n}\nfn main() { println(demo()) }\n";
    let destructor = "struct Resource: Drop {\n    value: int\n    fn drop(inout self) { let _text = \"drop\".toString() }\n}\nfn demo() { let _resource = Resource{value: 1} }\nfn main() { demo() }\n";
    for source in [method, destructor] {
        let (_scratch, path) = input("allocation-effects", source);
        let report = allocations(&path, "demo");
        assert_eq!(report["status"], "may_alloc", "{report:#}");
    }
}

#[test]
fn allocation_analysis_includes_cleanup_of_a_partially_constructed_aggregate() {
    let source = "enum E { Fail }\nstruct Resource: Drop {\n    value: int\n    fn drop(inout self) { let _text = \"drop\".toString() }\n}\nstruct Pair {\n    left: Resource\n    right: int\n}\nfn fail() -> int ! E { return err(.Fail) }\nfn demo() -> Pair ! E {\n    return Pair{left: Resource{value: 1}, right: try fail()}\n}\nfn main() { let _p = demo() }\n";
    let (_scratch, path) = input("allocation-partial-cleanup", source);
    let report = allocations(&path, "demo");
    assert_eq!(report["status"], "may_alloc", "{report:#}");
}

#[test]
fn uninstantiated_generic_body_is_unknown_instead_of_empty_no_alloc() {
    let source = "fn generic[T](value: T) -> T { return value }\nfn main() {}\n";
    let (_scratch, path) = input("allocation-generic", source);
    let report = allocations(&path, "generic");
    assert_eq!(report["status"], "unknown", "{report:#}");
}

fn check_named<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"].as_array().unwrap().iter().find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("missing {name} check: {report:#}"))
}

fn assert_hash(value: &Value) {
    let text = value.as_str().unwrap_or_else(|| panic!("not a hash: {value}"));
    let hex = text.strip_prefix("sha256:").unwrap_or_else(|| panic!("not a SHA-256 identifier: {text}"));
    assert_eq!(hex.len(), 64, "{text}");
    assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()), "{text}");
}

fn assert_artifact(check: &Value) -> Vec<u8> {
    assert_hash(&check["artifact"]);
    let path = check["artifact_path"].as_str().unwrap_or_else(|| panic!("missing artifact path: {check:#}"));
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read artifact {path}: {e}"));
    assert!(!bytes.is_empty());
    assert_eq!(check["artifact"], format!("sha256:{:x}", Sha256::digest(&bytes)), "{check:#}");
    bytes
}

fn verification(path: &Path, dir: &Path, args: &[&str]) -> (Output, Value) {
    let before = fs::read(path).unwrap();
    let (out, report) = output_json(yavo().arg("verify").arg(path).arg("--json")
        .arg("--output-dir").arg(dir).args(args).env_remove("YAVO_CC"));
    assert_eq!(report["schema"], "yavo-verify/1", "{report:#}");
    assert_eq!(report["inputs"]["source"], format!("sha256:{:x}", Sha256::digest(&before)));
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(report["checks"].is_array());
    let expected_exit = match report["overall"].as_str().unwrap() {
        "verified" => 0,
        "failed" => 1,
        "not_verified" => 5,
        other => panic!("unknown overall result: {other}"),
    };
    assert_eq!(out.status.code(), Some(expected_exit), "{report:#}; stderr: {}", String::from_utf8_lossy(&out.stderr));
    let saved = fs::read(report["report_path"].as_str().unwrap()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), report);
    (out, report)
}

#[test]
fn native_verification_runs_the_built_artifact_and_preserves_exact_evidence() {
    let source = "use std/io\nfn main() {\n    let text = io.readAll() catch _ => \"\".toString()\n    print(text)\n}\n";
    let (scratch, path) = input("verify-native", source);
    let data = "hello 中文\r\n";
    let stdin = scratch.0.join("input data.txt");
    let expected = scratch.0.join("expected stdout.txt");
    let stderr = scratch.0.join("expected stderr.txt");
    fs::write(&stdin, data).unwrap();
    fs::write(&expected, data).unwrap();
    fs::write(&stderr, "").unwrap();
    let (_out, report) = verification(&path, &scratch.0.join("report output"), &[
        "--stdin", stdin.to_str().unwrap(), "--expect-stdout", expected.to_str().unwrap(),
        "--expect-stderr", stderr.to_str().unwrap(),
    ]);
    assert_eq!(report["overall"], "not_verified", "{report:#}");
    for key in ["source", "dependencies", "toolchain"] { assert_hash(&report["inputs"][key]); }
    assert_eq!(check_named(&report, "front_end")["status"], "passed");
    assert_eq!(check_named(&report, "toolchain")["status"], "passed");
    let build = check_named(&report, "build");
    assert_eq!(build["status"], "passed");
    assert_artifact(build);
    let tests = check_named(&report, "tests");
    assert_eq!(tests["status"], "passed", "{report:#}");
    assert_eq!(tests["artifact"], build["artifact"]);
    assert_eq!(tests["target"], build["target"]);
    assert!(tests["ran_on"].is_string());
    assert_eq!(tests["exit_code"], 0);
    assert_eq!(tests["stdout"], data);
    assert_eq!(tests["stderr"], "");
    let audit = check_named(&report, "audit");
    assert_eq!(audit["status"], "not_run");
    assert_eq!(audit["required"], true);
    assert_eq!(audit["scope"]["c_runtime"], "trusted_pinned");
    for key in ["app", "dependencies", "stdlib"] { assert_eq!(audit["scope"][key], "not_audited"); }
    let optional: Vec<_> = report["checks"].as_array().unwrap().iter()
        .filter(|c| matches!(c["name"].as_str(), Some("sanitizers" | "reproducible_build"))).collect();
    assert_eq!(optional.len(), 3, "{report:#}");
    assert_eq!(check_named(&report, "reproducible_build")["required"], false);
    for optional in optional {
        assert_eq!(optional["status"], "not_run", "{optional:#}");
        assert_eq!(optional["required"], false);
        assert!(optional["artifact"].is_null());
    }
}

#[test]
fn panic_timeout_and_output_mismatch_cannot_be_reported_as_passed_tests() {
    let cases = [
        ("fn main() { panic(\"expected failure\") }\n", "failed", Some(101)),
        ("fn main() { while true {} }\n", "timeout", None),
        ("fn main() { println(\"wrong\") }\n", "failed", Some(0)),
    ];
    for (source, expected_status, expected_exit) in cases {
        let (scratch, path) = input("verify-failure", source);
        let expected = scratch.0.join("expected.txt");
        fs::write(&expected, "right\n").unwrap();
        let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &[
            "--timeout-ms", "100", "--expect-stdout", expected.to_str().unwrap(),
        ]);
        assert_eq!(report["overall"], "failed");
        assert_eq!(check_named(&report, "build")["status"], "passed");
        let tests = check_named(&report, "tests");
        assert_eq!(tests["status"], expected_status, "{report:#}");
        assert_eq!(tests["timed_out"], expected_status == "timeout");
        if let Some(code) = expected_exit { assert_eq!(tests["exit_code"], code, "{report:#}"); }
        assert_artifact(tests);
    }
}

#[test]
fn truncated_stdout_cannot_equal_an_expected_prefix() {
    const BLOCK: usize = 1024;
    const LIMIT: usize = 4 * 1024 * 1024;
    let source = format!("fn main() {{\n    var i = 0\n    while i < 4097 {{\n        print(\"{}\")\n        i += 1\n    }}\n}}\n", "x".repeat(BLOCK));
    let (scratch, path) = input("verify-truncated-output", &source);
    let expected = scratch.0.join("prefix.txt");
    fs::write(&expected, vec![b'x'; LIMIT]).unwrap();
    let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &[
        "--expect-stdout", expected.to_str().unwrap(),
    ]);
    let tests = check_named(&report, "tests");
    assert_eq!(report["overall"], "failed");
    assert_eq!(tests["status"], "failed");
    assert_eq!(tests["exit_code"], 0);
    assert_eq!(tests["stdout_truncated"], true);
    assert!(tests["stdout_matches"].is_null(), "a retained prefix is not a complete exact match");
    assert_eq!(tests["stdout"].as_str().unwrap().len(), LIMIT);
    assert_eq!(tests["stdout_bytes"], LIMIT + BLOCK);
    assert_eq!(tests["stdout_sha256"], format!("sha256:{:x}", Sha256::digest(vec![b'x'; LIMIT + BLOCK])));
    assert_artifact(tests);
}

#[test]
fn compiler_errors_leave_build_and_execution_unperformed() {
    let (scratch, path) = input("verify-compile-error", "fn main() { missing() }\n");
    let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &[]);
    assert_eq!(report["overall"], "failed");
    assert_eq!(check_named(&report, "front_end")["status"], "failed");
    for name in ["build", "tests"] {
        let check = check_named(&report, name);
        assert_eq!(check["status"], "not_run", "{report:#}");
        assert!(check["artifact"].is_null());
        assert!(check["ran_on"].is_null());
    }
}

#[test]
fn verify_rejects_claimed_runners_and_unbounded_timeouts() {
    let (_scratch, path) = input("verify-options", "fn main() {}\n");
    for args in [
        ["--available-runners", "x86_64-windows-gnu"],
        ["--timeout-ms", "0"],
        ["--build-timeout-ms", "0"],
    ] {
        let out = common::output(yavo().arg("verify").arg(&path).args(args)
            .stdin(Stdio::null()).env_remove("YAVO_CC"), common::RUN_TIMEOUT).unwrap();
        assert_eq!(out.status.code(), Some(2), "accepted {args:?}: {}", String::from_utf8_lossy(&out.stdout));
    }
}

#[test]
fn no_run_restricts_execution_without_claiming_a_test_pass() {
    let (scratch, path) = input("verify-no-run", "fn main() {}\n");
    let marker = scratch.0.join("should-not-exist.txt");
    let literal = serde_json::to_string(marker.to_str().unwrap()).unwrap();
    let source = format!("use std/fs\nfn main() {{\n    fs.writeText({literal}, \"executed\") catch _ {{ return }}\n}}\n");
    fs::write(&path, source).unwrap();
    let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &["--no-run"]);
    assert_eq!(report["overall"], "not_verified");
    assert_eq!(check_named(&report, "build")["status"], "passed");
    assert_artifact(check_named(&report, "build"));
    assert_eq!(check_named(&report, "tests")["status"], "not_run");
    assert!(check_named(&report, "tests")["ran_on"].is_null());
    assert!(!marker.exists(), "--no-run executed the source program");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn cross_builds_do_not_claim_execution_on_an_unavailable_platform() {
    let (scratch, path) = input("verify-cross", "fn main() { println(\"hello\") }\n");
    let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &["--target", "x86_64-windows-gnu"]);
    assert_eq!(report["overall"], "not_verified");
    let build = check_named(&report, "build");
    assert_eq!(build["status"], "passed");
    assert_eq!(build["target"], "x86_64-windows-gnu");
    assert_eq!(build["ran_on"], "x86_64-linux-gnu");
    assert!(assert_artifact(build).starts_with(b"MZ"));
    let tests = check_named(&report, "tests");
    assert_eq!(tests["status"], "not_run");
    assert_eq!(tests["artifact"], build["artifact"]);
    assert!(tests["ran_on"].is_null());
}

#[cfg(unix)]
#[test]
fn verify_rejects_the_development_compiler_override_without_running_it() {
    use std::os::unix::fs::PermissionsExt;
    let (scratch, path) = input("verify-toolchain-policy", "fn main() {}\n");
    let marker = scratch.0.join("compiler-was-executed");
    let fake = scratch.0.join("untrusted-compiler");
    fs::write(&fake, "#!/bin/sh\nprintf ran > \"$YAVO_VERIFY_TEST_MARKER\"\nexit 0\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let (out, report) = output_json(yavo().arg("verify").arg(&path).arg("--json")
        .arg("--output-dir").arg(scratch.0.join("artifacts"))
        .env("YAVO_CC", &fake).env("YAVO_VERIFY_TEST_MARKER", &marker));
    assert_eq!(out.status.code(), Some(5), "{report:#}");
    assert_eq!(report["overall"], "not_verified");
    assert_eq!(check_named(&report, "toolchain")["status"], "unsupported");
    assert_eq!(check_named(&report, "build")["status"], "unsupported");
    assert!(!marker.exists(), "verify invoked YAVO_CC despite requiring the pinned toolchain");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn assert_detected_sanitizer_defect(probe: &Value, tool: &str) {
    assert_eq!(probe["tool"], tool);
    assert_eq!(probe["status"], "passed", "{probe:#}");
    assert_eq!(probe["build"]["status"], "passed");
    assert_artifact(&probe["build"]);
    let run = &probe["run"];
    assert_eq!(run["expected_defect_detected"], true, "{probe:#}");
    assert_eq!(run["success"], false);
    assert_eq!(run["timed_out"], false);
    assert!(run["exit_code"].as_i64().is_some_and(|code| code != 0)
        || run["signal"].as_i64().is_some_and(|signal| signal > 0), "{probe:#}");
    assert_eq!(run["ran_on"], "x86_64-linux-gnu");
    assert_eq!(run["artifact"], probe["build"]["artifact"]);
    assert_artifact(run);
    assert_eq!(run["stderr_truncated"], false);
    let stderr = run["stderr"].as_str().unwrap();
    assert_hash(&run["stderr_sha256"]);
    match tool {
        "address" => assert!(stderr.contains("AddressSanitizer") && stderr.contains("heap-use-after-free"), "{probe:#}"),
        "undefined" => assert!(stderr.contains("signed integer overflow") && (stderr.contains("runtime error:") || stderr.to_lowercase().contains("ubsan")), "{probe:#}"),
        "thread" => assert!(stderr.contains("ThreadSanitizer") && stderr.contains("data race"), "{probe:#}"),
        other => panic!("unexpected sanitizer: {other}"),
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn sanitizer_profiles_are_separate_and_never_supply_windows_execution_evidence() {
    let (scratch, path) = input("verify-sanitizers", "fn main() { println(\"hello\") }\n");
    let (_out, report) = verification(&path, &scratch.0.join("artifacts"), &[
        "--target", "x86_64-windows-gnu", "--sanitize",
    ]);
    let build = check_named(&report, "build");
    assert_eq!(build["status"], "passed");
    let release_bytes = assert_artifact(build);
    let checks = report["checks"].as_array().unwrap();
    let sanitizers: Vec<_> = checks.iter().filter(|check| check["name"] == "sanitizers").collect();
    assert_eq!(sanitizers.len(), 2, "{report:#}");
    assert_eq!(sanitizers.iter().filter_map(|c| c["id"].as_str()).collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["address-undefined", "thread"]));
    for sanitizer in sanitizers {
        assert_eq!(sanitizer["required"], true);
        assert_eq!(sanitizer["target"], "x86_64-linux-gnu");
        let tools = sanitizer["tools"].as_array().unwrap();
        if sanitizer["id"] == "thread" { assert_eq!(tools, &vec![json!("thread")]); }
        else { assert_eq!(tools, &vec![json!("address"), json!("undefined")]); }
        // Starting a clean binary does not establish that requested instrumentation
        // exists. Each component must also detect its own deliberate defect.
        let mut demonstrated = false;
        if sanitizer["probe_run"]["status"] == "passed" {
            let probes = sanitizer["capability_probes"].as_array().unwrap_or_else(||
                panic!("clean probe succeeded but no negative probes were recorded: {sanitizer:#}"));
            assert_eq!(probes.len(), tools.len());
            demonstrated = true;
            for tool in tools {
                let matching: Vec<_> = probes.iter().filter(|probe| probe["tool"] == *tool).collect();
                assert_eq!(matching.len(), 1, "missing or duplicated {tool} negative probe: {sanitizer:#}");
                if matching[0]["status"] == "passed" {
                    assert_detected_sanitizer_defect(matching[0], tool.as_str().unwrap());
                } else {
                    demonstrated = false;
                    assert_ne!(matching[0]["run"]["expected_defect_detected"], true, "{sanitizer:#}");
                }
            }
        }
        match sanitizer["status"].as_str().unwrap() {
            "passed" => {
                assert!(demonstrated, "sanitizer passed without all requested defect detections: {sanitizer:#}");
                assert_eq!(sanitizer["ran_on"], "x86_64-linux-gnu");
                assert_eq!(sanitizer["exit_code"], 0);
                assert_ne!(assert_artifact(sanitizer), release_bytes);
                assert_ne!(sanitizer["artifact"], build["artifact"]);
            }
            "unsupported" => {
                assert!(!demonstrated, "unsupported contradicts successful capability probes: {sanitizer:#}");
                assert!(sanitizer["artifact"].is_null(), "a missing capability cannot invent a source artifact: {sanitizer:#}");
                assert!(sanitizer["build"].is_null(), "application was built despite missing sanitizer capability: {sanitizer:#}");
                assert!(!sanitizer["reason"].as_str().unwrap_or("").is_empty());
            }
            other => panic!("unexpected sanitizer outcome for a valid simple program: {other}: {sanitizer:#}"),
        }
    }
    let tests = check_named(&report, "tests");
    assert_eq!(tests["target"], "x86_64-windows-gnu");
    assert_eq!(tests["artifact"], build["artifact"]);
    assert_eq!(tests["status"], "not_run");
    assert!(tests["ran_on"].is_null());
    assert_eq!(report["overall"], "not_verified");
}
