//! Multi-file loading, visibility and evidence must preserve source boundaries.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;

fn yavo() -> Command { Command::new(env!("CARGO_BIN_EXE_yavo")) }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn normalized(path: &Path) -> String { path.to_string_lossy().replace('\\', "/") }

fn input(files: &[(&str, &str)]) -> (common::Scratch, PathBuf) {
    let scratch = common::Scratch::new("modules").unwrap();
    for (name, source) in files {
        let path = scratch.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    let entry = scratch.0.join("main.yv");
    (scratch, entry)
}

fn json(command: &mut Command) -> (Output, Value) {
    let output = common::output(command.stdin(Stdio::null()), common::BUILD_TIMEOUT).unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!(
        "invalid JSON: {e}; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)));
    (output, report)
}

fn check(path: &Path) -> (Output, Value) { json(yavo().arg("check").arg(path).arg("--json")) }

fn rejects(files: &[(&str, &str)], code: &str) {
    let (_scratch, entry) = input(files);
    let (out, report) = check(&entry);
    assert_eq!(out.status.code(), Some(1), "{report:#}; {out:?}");
    assert!(report["diagnostics"].as_array().unwrap().iter().any(|d|
        d["severity"] == "ERROR" && d["code"] == code), "missing {code}: {report:#}");
}

fn execute(path: &Path, release: bool) -> Output {
    let scratch = common::Scratch::new("module-run").unwrap();
    let exe = scratch.exe("program");
    let mut build = yavo();
    build.arg("build").arg(path).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
    if release { build.arg("--release"); }
    let built = common::output(&mut build, common::BUILD_TIMEOUT).unwrap();
    assert!(built.status.success(), "build {path:?}: {}", String::from_utf8_lossy(&built.stderr));
    common::output(Command::new(&exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap()
}

fn runs(path: &Path, expected: &str) {
    for release in [false, true] {
        let out = execute(path, release);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(out.stdout, expected.as_bytes());
        assert!(out.stderr.is_empty(), "{out:?}");
    }
}

#[test]
fn public_types_generics_interfaces_and_borrowed_returns_work_across_files() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/modules/basic/main.yv");
    let (out, report) = check(&entry);
    assert_eq!(out.status.code(), Some(0), "{report:#}");
    assert_eq!(report["error_count"], 0);
    assert_eq!(report["warning_count"], 0);
    assert_eq!(report["sources"].as_array().unwrap().len(), 5);
    runs(&entry, "Ada 10 8\n");
}

#[test]
fn arena_graphs_and_thread_transfers_preserve_identity_across_modules() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/modules/arena/main.yv");
    runs(&entry, "B\n1 1\n");
}

#[test]
fn private_missing_and_transitively_unimported_symbols_are_not_visible() {
    rejects(&[
        ("main.yv", "use lib\nfn main() { println(lib.hidden()) }\n"),
        ("lib.yv", "fn hidden() -> int { return 7 }\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use lib\nfn main() { let x = lib.Secret{value: 7}\nprintln(x.value) }\n"),
        ("lib.yv", "struct Secret { value: int }\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use missing\nfn main() {}\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use bridge\nfn main() { println(bridge.answer() + hidden.answer()) }\n"),
        ("bridge.yv", "use hidden\npub fn answer() -> int { return hidden.answer() }\n"),
        ("hidden.yv", "pub fn answer() -> int { return 7 }\n"),
    ], "E0010");
}

#[test]
fn imported_modules_cannot_implicitly_reach_entry_functions_or_types() {
    rejects(&[
        ("main.yv", "use lib\nfn secret() -> int { return 7 }\nfn main() { println(lib.answer()) }\n"),
        ("lib.yv", "pub fn answer() -> int { return secret() }\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use lib\nstruct Secret { value: int }\nfn main() { println(lib.answer()) }\n"),
        ("lib.yv", "pub fn answer() -> int { let x = Secret{value: 7}\nreturn x.value }\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use lib\nfn main() { lib.callEntry() }\n"),
        ("lib.yv", "pub fn callEntry() { main() }\n"),
    ], "E0010");
}

#[test]
fn standard_imports_are_local_to_each_file_and_duplicates_across_files_are_valid() {
    rejects(&[
        ("main.yv", "use lib\nfn main() { println(math.sqrt(lib.value())) }\n"),
        ("lib.yv", "use std/math\npub fn value() -> f64 { return math.sqrt(16.0) }\n"),
    ], "E0010");
    rejects(&[
        ("main.yv", "use std/math\nuse lib\nfn main() { println(math.sqrt(lib.value())) }\n"),
        ("lib.yv", "pub fn value() -> f64 { return math.sqrt(16.0) }\n"),
    ], "E0010");
    let (_scratch, entry) = input(&[
        ("main.yv", "use std/math\nuse lib\nfn main() { println(math.sqrt(lib.value())) }\n"),
        ("lib.yv", "use std/math\npub fn value() -> f64 { return math.sqrt(16.0) }\n"),
    ]);
    runs(&entry, "2.0\n");
}

#[test]
fn cycles_preserve_file_local_helpers_and_duplicate_declarations_are_rejected() {
    let (_scratch, entry) = input(&[
        ("main.yv", "use alpha\nfn main() { println(alpha.answer()) }\n"),
        ("alpha.yv", "use beta\nfn helper() -> int { return 2 }\npub fn answer() -> int { return helper() + beta.answer() }\n"),
        ("beta.yv", "use alpha\nfn helper() -> int { return 3 }\npub fn answer() -> int { return helper() }\npub fn relay() -> int { return alpha.answer() }\n"),
    ]);
    runs(&entry, "5\n");
    rejects(&[
        ("main.yv", "use lib\nfn helper() {}\nfn helper() {}\nfn main() { lib.ok() }\n"),
        ("lib.yv", "pub fn ok() {}\n"),
    ], "E0011");
    rejects(&[
        ("main.yv", "use left\nuse left\nfn main() { left.ok() }\n"),
        ("left.yv", "pub fn ok() {}\n"),
    ], "E0011");
}

#[test]
fn qualified_types_disambiguate_equal_exported_names() {
    let left = "pub struct Item { value: int }\n";
    let right = "pub struct Item { value: int }\n";
    let (_scratch, entry) = input(&[
        ("main.yv", "use left\nuse right\nfn total(a: left.Item, b: right.Item) -> int { return a.value + b.value }\nfn main() { println(total(left.Item{value: 3}, right.Item{value: 4})) }\n"),
        ("left.yv", left), ("right.yv", right),
    ]);
    runs(&entry, "7\n");
    rejects(&[
        ("main.yv", "use left\nuse right\nfn main() { let x = Item{value: 3}\nprintln(x.value) }\n"),
        ("left.yv", left), ("right.yv", right),
    ], "E0010");
}

#[test]
fn module_aliases_can_be_shadowed_by_locals_and_closure_parameters() {
    let (_scratch, entry) = input(&[
        ("main.yv", r#"use helpers
struct Holder { value: int }
fn main() {
    let before = helpers.answer()
    let helpers = Holder{value: 4}
    var numbers: Vec[int] = [3, 1, 2]
    numbers.sortBy((helpers, b) => helpers < b)
    println("${before} ${helpers.value} ${numbers[0]}")
}
"#),
        ("helpers.yv", "pub fn answer() -> int { return 7 }\n"),
    ]);
    runs(&entry, "7 4 1\n");
}

#[test]
fn files_cannot_complete_each_others_comments_or_strings() {
    rejects(&[
        ("main.yv", "use lib\nfn main() {}\n/*\n"),
        ("lib.yv", "*/\npub fn ok() {}\n"),
    ], "E0020");
    rejects(&[
        ("main.yv", "use lib\nfn main() { println(lib.text()) }\n"),
        ("lib.yv", "pub fn text() -> str of static { return \"unterminated\n}\n"),
    ], "E0020");
}

#[test]
fn reserved_builtin_declarations_are_rejected_before_module_name_mangling() {
    for declaration in [
        "struct String { value: int }\n",
        "interface Copy { fn get(self) -> int }\n",
        "fn println(value: int) { print(value) }\n",
    ] {
        let source = format!("{declaration}pub fn ok() {{}}\n");
        rejects(&[
            ("main.yv", "use lib\nfn main() { lib.ok() }\n"),
            ("lib.yv", &source),
        ], "E0011");
    }
}

#[test]
fn diagnostics_keep_utf8_byte_offsets_and_local_file_coordinates() {
    let dependency = "// 中文 é\npub fn broken() -> int {\n    return missing\n}\n";
    let (scratch, entry) = input(&[
        ("main.yv", "use lib\nfn main() { println(lib.broken()) }\n"),
        ("lib.yv", dependency),
    ]);
    let (out, report) = check(&entry);
    assert_eq!(out.status.code(), Some(1), "{report:#}");
    let diagnostic = report["diagnostics"].as_array().unwrap().iter()
        .find(|d| d["code"] == "E0010" && d["message"].as_str().unwrap().contains("missing")).unwrap();
    let at = dependency.find("missing").unwrap();
    let path = normalized(&scratch.0.join("lib.yv"));
    assert_eq!(diagnostic["file"], path);
    assert_eq!(diagnostic["line"], 3);
    assert_eq!(diagnostic["column"], 12);
    assert_eq!(diagnostic["primary"]["start"], at);
    assert_eq!(diagnostic["primary"]["end"], at + "missing".len());
    assert!(diagnostic["fixes"].as_array().unwrap().is_empty());
}

#[test]
fn runtime_panics_report_the_imported_file_and_its_local_line() {
    let (scratch, entry) = input(&[
        ("main.yv", "use lib\nfn main() { lib.fail() }\n"),
        ("lib.yv", "pub fn fail() {\n    assert(false, \"from module\")\n}\n"),
    ]);
    let expected = format!("yavo panic [P-ASSERT] at {}:2:5: from module\n", normalized(&scratch.0.join("lib.yv")));
    for release in [false, true] {
        let out = execute(&entry, release);
        assert_eq!(out.status.code(), Some(101), "{out:?}");
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, expected.as_bytes());
    }
}

fn objects<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => { found.push(value); for child in fields.values() { objects(child, found); } }
        Value::Array(items) => for child in items { objects(child, found); },
        _ => {}
    }
}

#[test]
fn allocation_reports_bind_dependencies_and_use_local_byte_ranges() {
    let dependency = "// 中文 é\npub fn make() -> String {\n    return \"text\".toString()\n}\n";
    let (scratch, entry) = input(&[
        ("main.yv", "use lib\nfn main() { println(lib.make()) }\n"),
        ("lib.yv", dependency),
    ]);
    let (out, report) = json(yavo().arg("allocations").arg("lib.make").arg("--file").arg(&entry).arg("--json"));
    assert_eq!(out.status.code(), Some(0), "{report:#}; {out:?}");
    assert_eq!(report["status"], "may_alloc");
    assert_eq!(report["subject"], "lib.make");
    let files = report["sources"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let file = files.iter().find(|f| f["relative_path"] == "lib.yv").unwrap();
    assert_eq!(file["sha256"], digest(dependency.as_bytes()));
    let mut entries = Vec::new();
    objects(&report["operations"], &mut entries);
    let locations: Vec<_> = entries.iter().filter_map(|o| {
        let loc = &o["location"];
        (loc["line"] == 3 && loc["file"] == normalized(&scratch.0.join("lib.yv"))).then_some(loc)
    }).collect();
    assert!(!locations.is_empty(), "{report:#}");
    assert!(locations.iter().any(|loc| {
        let start = loc["start"].as_u64().unwrap() as usize;
        let end = loc["end"].as_u64().unwrap() as usize;
        dependency.get(start..end).is_some_and(|text| text.contains("\"text\""))
    }), "{report:#}");
    let changed = dependency.replace("\"text\"", "\"more\"");
    fs::write(scratch.0.join("lib.yv"), &changed).unwrap();
    let (out, next) = json(yavo().arg("allocations").arg("lib.make").arg("--file").arg(&entry).arg("--json"));
    assert_eq!(out.status.code(), Some(0), "{next:#}");
    assert_ne!(report["project_sha256"], next["project_sha256"]);
    assert_eq!(report["source_sha256"], next["source_sha256"]);
}

fn named<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()
}

#[test]
fn verify_records_and_replays_the_complete_immutable_project_snapshot() {
    let original = "pub fn answer() -> int { return 7 }\n";
    let (scratch, entry) = input(&[
        ("main.yv", "use lib\nfn main() { println(lib.answer()) }\n"),
        ("lib.yv", original),
    ]);
    let expected = scratch.0.join("expected.txt");
    fs::write(&expected, "7\n").unwrap();
    let (out, report) = json(yavo().arg("verify").arg(&entry).arg("--json")
        .arg("--output-dir").arg(scratch.0.join("first snapshot 雪"))
        .arg("--expect-stdout").arg(&expected).env_remove("YAVO_CC"));
    assert_eq!(out.status.code(), Some(5), "{report:#}; {out:?}");
    assert_eq!(report["overall"], "not_verified");
    assert_eq!(named(&report, "build")["status"], "passed");
    assert_eq!(named(&report, "tests")["status"], "passed");
    assert_eq!(named(&report, "tests")["stdout"], "7\n");
    assert_eq!(named(&report, "tests")["stderr"], "");
    assert_eq!(named(&report, "tests")["artifact"], named(&report, "build")["artifact"]);
    let artifact = fs::read(named(&report, "build")["artifact_path"].as_str().unwrap()).unwrap();
    assert_eq!(named(&report, "build")["artifact"], format!("sha256:{}", digest(&artifact)));
    let files = report["dependencies"]["source_files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{report:#}");
    for file in files {
        let bytes = fs::read(file["snapshot_path"].as_str().unwrap()).unwrap();
        assert_eq!(file["sha256"], digest(&bytes));
    }
    let snapshot = PathBuf::from(files.iter().find(|f| f["relative_path"] == "main.yv").unwrap()["snapshot_path"].as_str().unwrap());
    fs::write(scratch.0.join("lib.yv"), "pub fn answer() -> int { return 8 }\n").unwrap();
    runs(&snapshot, "7\n");
    let (out, next) = json(yavo().arg("verify").arg(&entry).arg("--json").arg("--no-run")
        .arg("--output-dir").arg(scratch.0.join("second")).env_remove("YAVO_CC"));
    assert_eq!(out.status.code(), Some(5), "{next:#}; {out:?}");
    assert_eq!(named(&next, "build")["status"], "passed");
    assert_eq!(named(&next, "tests")["status"], "not_run");
    assert_ne!(report["inputs"]["project"], next["inputs"]["project"]);
    assert_ne!(report["inputs"]["dependencies"], next["inputs"]["dependencies"]);
    assert_eq!(report["inputs"]["source"], next["inputs"]["source"]);
    let saved: Value = serde_json::from_slice(&fs::read(report["report_path"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(saved, report);
}

#[test]
fn multifile_fix_reports_unsupported_and_does_not_write_any_source() {
    let main = "use lib\nfn main() { let unused = 1\nprintln(lib.answer()) }\n";
    let lib = "pub fn answer() -> int { return 7 }\n";
    let (scratch, entry) = input(&[("main.yv", main), ("lib.yv", lib)]);
    let (out, report) = json(yavo().arg("fix").arg(&entry).arg("--json"));
    assert_eq!(out.status.code(), Some(1), "{report:#}");
    assert!(report["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "U0001"));
    assert_eq!(fs::read_to_string(&entry).unwrap(), main);
    assert_eq!(fs::read_to_string(scratch.0.join("lib.yv")).unwrap(), lib);
}

#[test]
fn allocation_queries_require_full_paths_when_module_leaf_names_are_ambiguous() {
    let (_scratch, entry) = input(&[
        ("main.yv", "use left\nuse right\nfn main() { println(left.answer() + right.answer()) }\n"),
        ("left.yv", "use a/foo\npub fn answer() -> int { return foo.f() }\n"),
        ("right.yv", "use b/foo\npub fn answer() -> int { return foo.f().len() }\n"),
        ("a/foo.yv", "pub fn f() -> int { return 1 }\n"),
        ("b/foo.yv", "pub fn f() -> String { return \"text\".toString() }\n"),
    ]);
    let ambiguous = common::output(yavo().arg("allocations").arg("foo.f").arg("--file").arg(&entry)
        .arg("--json").stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    assert_eq!(ambiguous.status.code(), Some(2), "{ambiguous:?}");
    for (name, expected) in [("a/foo.f", "no_alloc"), ("b/foo.f", "may_alloc")] {
        let (out, report) = json(yavo().arg("allocations").arg(name).arg("--file").arg(&entry).arg("--json"));
        assert_eq!(out.status.code(), Some(0), "{report:#}; {out:?}");
        assert_eq!(report["subject"], name);
        assert_eq!(report["status"], expected);
    }
}

#[cfg(unix)]
#[test]
fn symlink_module_aliases_are_rejected_before_snapshotting() {
    let (scratch, entry) = input(&[
        ("main.yv", "use alias\nfn main() { println(alias.answer()) }\n"),
        ("real.yv", "pub fn answer() -> int { return 7 }\n"),
    ]);
    std::os::unix::fs::symlink("real.yv", scratch.0.join("alias.yv")).unwrap();
    let (out, report) = check(&entry);
    assert_eq!(out.status.code(), Some(1), "{report:#}");
    assert!(report["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "U0001"));
}
