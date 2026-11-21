//! End-to-end repair safety: these tests inspect actual file bytes and execute
//! resource-bearing programs, independently of the repair planner's predicates.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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

fn invoke(path: &Path, command: &str, flags: &[&str]) -> (Output, Value) {
    let output = common::output(yavo().arg(command).arg(path).args(flags).stdin(Stdio::null()),
        common::RUN_TIMEOUT).unwrap();
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}: {}", String::from_utf8_lossy(&output.stdout)));
    (output, report)
}

fn check(path: &Path) -> Value {
    let (out, report) = invoke(path, "check", &["--json"]);
    assert_eq!(out.status.code(), Some(0), "{report:#}");
    assert_eq!(report["error_count"], 0, "{report:#}");
    report
}

fn fix(path: &Path, dry_run: bool) -> (Output, Value) {
    let before = fs::read(path).unwrap();
    let flags = if dry_run { vec!["--json", "--dry-run"] } else { vec!["--json-diagnostics"] };
    let (out, report) = invoke(path, "fix", &flags);
    let after = fs::read(path).unwrap();
    assert_eq!(report["schema"], "yavo-fix/1");
    assert_eq!(report["source_sha256"], format!("{:x}", Sha256::digest(&before)));
    assert_eq!(report["result_sha256"], format!("{:x}", Sha256::digest(&after)));
    assert_eq!(report["changed"], before != after);
    assert_eq!(report["dry_run"], dry_run);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    let errors = diagnostics.iter().filter(|d| d["severity"] == "ERROR").count();
    let warnings = diagnostics.iter().filter(|d| d["severity"] == "WARN").count();
    assert_eq!(report["error_count"], errors);
    assert_eq!(report["warning_count"], warnings);
    assert_eq!(out.status.code(), Some(if errors == 0 { 0 } else { 1 }), "{report:#}");
    for field in ["original_diagnostics", "automatic", "applied", "candidates"] {
        assert!(report[field].is_array(), "{field}: {report:#}");
    }
    for field in ["automatic", "applied", "candidates"] {
        for proposal in report[field].as_array().unwrap() {
            assert!(!proposal["id"].as_str().unwrap().is_empty());
            assert!(!proposal["patch_id"].as_str().unwrap().is_empty());
            assert!(!proposal["title"].as_str().unwrap().is_empty());
            assert!(!proposal["impact"].as_str().unwrap().is_empty());
            assert!(!proposal["preconditions"].as_array().unwrap().is_empty());
            assert_eq!(proposal["recheck"], "passed");
            assert_eq!(proposal["applicability"], if field == "candidates" { "candidate" } else { "automatic" });
            assert!(!proposal["edits"].as_array().unwrap().is_empty());
            for edit in proposal["edits"].as_array().unwrap() {
                let start = edit["start"].as_u64().unwrap() as usize;
                let end = edit["end"].as_u64().unwrap() as usize;
                let source = std::str::from_utf8(&before).unwrap();
                assert!(start <= end && end <= source.len());
                assert!(source.is_char_boundary(start) && source.is_char_boundary(end));
                assert!(edit["replacement"].is_string());
            }
        }
    }
    if dry_run {
        assert_eq!(before, after);
        assert_eq!(report["applied"], json!([]));
    }
    (out, report)
}

fn build_run(path: &Path, scratch: &common::Scratch, name: &str, release: bool) -> Output {
    let exe = scratch.exe(name);
    let mut command = yavo();
    command.arg("build").arg(path).arg("-o").arg(&exe)
        .env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
    if release { command.arg("--release"); }
    let built = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let run = common::output(Command::new(exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    run
}

#[test]
fn dry_run_and_apply_preserve_utf8_crlf_comments_and_are_idempotent() {
    let source = "// 中文头注释：保留字节\r\nfn main() {\r\n\tlet _count = 1\r\n\tlet count = 2 // 未使用，不能立即析构\r\n\tvar v: Vec[int] = [1, 2]\r\n\tv.push(v.len()) // 保留尾注释\r\n\tprintln(\"https://example/中文 ${_count} ${v.len()}\")\r\n}\r\n";
    let (_scratch, path) = input("fix-crlf", source);
    let (out, preview) = fix(&path, true);
    // Only a warning remains, so the preview exits 0.
    assert_eq!(out.status.code(), Some(0));
    // `v.push(v.len())` is valid since the V2 revision (two-phase receivers): only the rename remains.
    assert_eq!(preview["automatic"].as_array().unwrap().len(), 1, "{preview:#}");
    assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    let (out, applied) = fix(&path, false);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(applied["applied"], preview["automatic"]);
    let expected = source.replace("let count = 2", "let _count2 = 2");
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    check(&path);
    let (_, second) = fix(&path, false);
    assert_eq!(second["changed"], false);
    assert_eq!(second["automatic"], json!([]));
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn fresh_names_avoid_later_bindings_and_function_calls_without_cross_function_pollution() {
    let cases = [
        (
            "fn main() {\n    let count = 1\n    let _count = 2\n    println(_count)\n}\n",
            "let _count2 = 1",
        ),
        (
            "fn _count() -> int { return 7 }\nfn main() {\n    let count = 1\n    println(_count())\n}\n",
            "let _count2 = 1",
        ),
        (
            "fn helper() { let _count = 7 }\nfn main() {\n    let count = 1\n    helper()\n}\n",
            "let _count = 1",
        ),
    ];
    for (source, expected) in cases {
        let (_scratch, path) = input("fix-names", source);
        let (out, report) = fix(&path, false);
        assert_eq!(out.status.code(), Some(0), "{report:#}");
        assert!(report["changed"].as_bool().unwrap(), "{report:#}");
        assert!(fs::read_to_string(&path).unwrap().contains(expected), "{report:#}");
        check(&path);
    }
}

#[test]
fn unused_owned_values_and_catch_errors_keep_their_destruction_times() {
    let source = r#"struct Resource: Drop {
    tag: String
    fn drop(inout self) {
        println("drop ${self.tag}")
    }
}
fn fail() -> int ! Resource {
    return err(Resource{tag: "error".toString()})
}
fn fallback() -> int {
    println("fallback")
    return 7
}
fn main() {
    let unused = Resource{tag: "local".toString()}
    let n = fail() catch failure => fallback()
    println(n)
    println("body")
}
"#;
    let (scratch, path) = input("fix-drop", source);
    let expected = b"fallback\ndrop error\n7\nbody\ndrop local\n";
    for release in [false, true] {
        assert_eq!(build_run(&path, &scratch, "before", release).stdout, expected);
    }
    let (_, report) = fix(&path, false);
    assert_eq!(report["applied"].as_array().unwrap().len(), 2, "{report:#}");
    let fixed = fs::read_to_string(&path).unwrap();
    assert!(fixed.contains("let _unused = Resource"));
    assert!(fixed.contains("catch _failure => fallback()"));
    for release in [false, true] {
        assert_eq!(build_run(&path, &scratch, "after", release).stdout, expected);
    }
}

#[test]
fn renaming_a_parameter_named_static_preserves_the_static_return_contract() {
    let source = "fn literal(static: str) -> str of static {\n    return \"x\"\n}\nfn main() {\n    println(literal(\"arg\"))\n}\n";
    let (_scratch, path) = input("fix-static-contract", source);
    let (_, report) = fix(&path, false);
    assert_eq!(report["applied"].as_array().unwrap().len(), 1, "{report:#}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source.replace("literal(static:", "literal(_static:"));
    check(&path);
}

#[test]
fn a_shadowed_take_function_is_not_advertised_as_builtin_field_replacement() {
    let source = r#"struct Node {
    name: String
}
fn keep(s: sink String) {
    println(s)
}
fn take(s: &String) -> String {
    return s.clone()
}
fn main() {
    let b = Box(Node{name: "n".toString()})
    keep(b.name)
}
"#;
    let (_scratch, path) = input("fix-shadowed-take", source);
    let (out, report) = fix(&path, false);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(report["applied"], json!([]));
    let ids: BTreeSet<&str> = report["candidates"].as_array().unwrap().iter()
        .map(|candidate| candidate["id"].as_str().unwrap()).collect();
    assert_eq!(ids, BTreeSet::from(["borrow", "clone"]), "{report:#}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}


#[test]
fn distinct_source_contract_candidates_have_stable_patch_addresses() {
    let source = "fn literal(a: str, b: str) -> str {\n    return \"x\"\n}\nfn main() {\n    println(literal(\"a\", \"b\"))\n}\n";
    let (_scratch, path) = input("fix-candidate-address", source);
    let (out, first) = fix(&path, false);
    assert_eq!(out.status.code(), Some(1));
    let candidates = first["candidates"].as_array().unwrap();
    assert!(candidates.len() > 1, "{first:#}");
    assert!(candidates.iter().all(|c| c["id"] == "add-of"));
    let ids: BTreeSet<&str> = candidates.iter().map(|c| c["patch_id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), candidates.len());
    let (_, second) = fix(&path, false);
    assert_eq!(first["candidates"], second["candidates"]);
    let (_, checked) = invoke(&path, "check", &["--json"]);
    let fixes = checked["diagnostics"].as_array().unwrap().iter()
        .find(|d| d["code"] == "E0302").unwrap()["fixes"].as_array().unwrap();
    assert_eq!(fixes.iter().map(|f| f["patch_id"].as_str().unwrap()).collect::<BTreeSet<_>>(), ids);
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn writes_and_interpolation_reads_are_not_renamed_as_unused() {
    let source = "fn main() {\n    var count = 0\n    count = 3\n    count += 4\n    let message = \"ok\"\n    println(\"${message}\")\n}\n";
    let (_scratch, path) = input("fix-used", source);
    let (_, report) = fix(&path, false);
    assert_eq!(report["applied"], json!([]));
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    check(&path);
}

#[test]
fn receiver_reads_in_arguments_compile_without_splitting() {
    let source = "fn main() {\n    var v: Vec[int] = [1]\n    if false {\n        v.push(v.len())\n    }\n    while v.len() < 4 {\n        v.push(v.len())\n    }\n    println(v.len())\n}\n";
    let (scratch, path) = input("fix-control-flow", source);
    let (out, report) = fix(&path, false);
    // Two-phase receivers (V2): the argument may read the vector, so nothing is rewritten.
    assert_eq!(out.status.code(), Some(0), "{report:#}");
    assert_eq!(report["applied"], json!([]));
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_eq!(build_run(&path, &scratch, "control", false).stdout, b"4\n");
}

#[test]
fn side_effecting_conditional_and_unsupported_receivers_are_never_automatically_hoisted() {
    let cases = [
        "fn read(v: Vec[int]) -> int {\n    println(\"read\")\n    return v.len()\n}\nfn main() {\n    var v: Vec[int] = [1, 2]\n    v.push(read(v))\n}\n".to_string(),
        "fn main() {\n    var v: Vec[int] = [1, 2]\n    let n: ?int = 1\n    v.push(n ?? v.len())\n}\n".to_string(),
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../yavo-tests/fix/T-V3.yv")).unwrap(),
    ];
    for source in cases {
        let (_scratch, path) = input("fix-not-safe", &source);
        let (_out, report) = fix(&path, false);
        assert_eq!(report["automatic"], json!([]));
        assert_eq!(report["applied"], json!([]));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn an_unrelated_error_prevents_a_partial_automatic_write() {
    let source = "fn main() {\n    let unused = 1\n    missing()\n}\n";
    let (_scratch, path) = input("fix-atomic", source);
    for dry_run in [true, false] {
        let (out, report) = fix(&path, dry_run);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(report["automatic"], json!([]));
        assert_eq!(report["applied"], json!([]));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert!(report["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "W0001"));
    }
}

#[test]
fn imports_used_only_by_uninstantiated_generics_survive_fix_and_later_instantiation() {
    let cases = [
        (
            "math",
            "pub fn rooted[T](_value: T) -> f64 {\n    return math.sqrt(4.0)\n}\n",
        ),
        (
            "sync",
            "pub fn rooted[T](_value: T) -> int {\n    let shared: Arc[String] = Arc(\"x\".toString())\n    return shared.len()\n}\n",
        ),
        (
            "math",
            "pub fn rooted[T](_value: T) -> int {\n    println(\"sqrt=${math.sqrt(4.0)}\")\n    return 2\n}\n",
        ),
    ];
    for (module, generic) in cases {
        let source = format!("use std/{module}\n{generic}fn main() {{\n    println(\"ok\")\n}}\n");
        let (_scratch, path) = input("fix-generic-import", &source);
        let before = check(&path);
        assert!(before["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "W0003"),
            "the uninstantiated generic must exercise the unused-import warning: {before:#}");
        let (_, report) = fix(&path, false);
        assert_eq!(report["applied"], json!([]), "{report:#}");
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        // The public body is checked only when instantiated. Removing its import
        // can pass the original recheck while breaking this otherwise valid call.
        let instantiated = source.replace("println(\"ok\")", "println(rooted(1))");
        fs::write(&path, instantiated).unwrap();
        check(&path);
    }
}


#[test]
fn import_repairs_preserve_used_modules_comments_and_reject_name_guessing() {
    let cases = [
        (
            "use std/{io, math, os} // modules\nfn main() {\n    println(math.sqrt(9.0))\n}\n",
            "use std/{math} // modules\nfn main() {\n    println(math.sqrt(9.0))\n}\n",
        ),
        (
            "// 中文\r\nfn main() {\r\n    println(math.sqrt(9.0))\r\n}\r\n",
            "use std/math\r\n// 中文\r\nfn main() {\r\n    println(math.sqrt(9.0))\r\n}\r\n",
        ),
        (
            "use std/math // keep the comment\nfn main() {\n    println(\"math.sqrt(9.0) // text\")\n}\n",
            " // keep the comment\nfn main() {\n    println(\"math.sqrt(9.0) // text\")\n}\n",
        ),
    ];
    for (source, expected) in cases {
        let (_scratch, path) = input("fix-import", source);
        let (out, report) = fix(&path, false);
        assert_eq!(out.status.code(), Some(0), "{report:#}");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        check(&path);
        let (_, second) = fix(&path, false);
        assert_eq!(second["changed"], false);
    }
    let source = "fn main() {\n    println(maths.sqrt(9.0))\n}\n";
    let (_scratch, path) = input("fix-no-guess", source);
    let (out, report) = fix(&path, false);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(report["automatic"], json!([]));
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn check_json_exposes_actionable_fixes_without_writing_the_source() {
    let source = "fn main() {\n    var v: Vec[int] = [1]\n    v.push(v.pop() ?? 0)\n}\n";
    let (_scratch, path) = input("fix-check-json", source);
    let (out, report) = invoke(&path, "check", &["--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(report["schema"], "yavo-diagnostics/p0");
    let diagnostic = report["diagnostics"].as_array().unwrap().iter().find(|d| d["code"] == "E0403").unwrap();
    assert_eq!(diagnostic["line"], 3);
    assert!(diagnostic["primary"].is_object());
    let fixes = diagnostic["fixes"].as_array().unwrap();
    assert_eq!(fixes.len(), 1, "{report:#}");
    assert_eq!(fixes[0]["id"], "hoist-argument");
    assert_eq!(fixes[0]["applicability"], "candidate");
    assert_eq!(fixes[0]["recheck"], "passed");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}
