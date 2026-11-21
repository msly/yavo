//! Mutable-return references retain pointee provenance and exclusive access.
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod common;

fn yavo() -> Command { Command::new(env!("CARGO_BIN_EXE_yavo")) }

fn check(source: &str) -> (std::process::Output, Value) {
    let scratch = common::Scratch::new("mutref-check").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    let output = common::output(yavo().arg("check").arg(&path).arg("--json").stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap();
    (output, report)
}

fn run_source(source: &str, expected: &str) {
    let scratch = common::Scratch::new("mutref-run").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    for release in [false, true] {
        let exe = scratch.exe(if release { "release" } else { "debug" });
        let mut cmd = yavo();
        cmd.arg("build").arg(&path).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
        if release { cmd.arg("--release"); }
        let built = common::output(&mut cmd, common::BUILD_TIMEOUT).unwrap();
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        assert!(built.stderr.is_empty(), "{}", String::from_utf8_lossy(&built.stderr));
        let output = common::output(Command::new(exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn returned_references_support_index_fields_forwarding_moves_and_rebinding() {
    run_source(r#"
struct Store {
    data: Vec[int]
    fn numbers(inout self) -> &inout Vec[int] of self {
        return &self.data
    }
}
fn identity(v: inout Vec[int]) -> &inout Vec[int] of v {
    let p: &inout Vec[int] = &v
    return p
}
fn first[T](v: inout Vec[T]) -> &inout T of v { return &v[0] }
fn number(v: inout Vec[int], i: int) -> &inout int of v { return &v[i] }
fn main() {
    var a: Vec[int] = [1, 2]
    var b: Vec[int] = [9]
    let p = identity(&a)
    let q = identity(&p)
    q.push(3)
    let r = number(&a, 1)
    let moved = r
    *moved += 5
    var rebound = number(&a, 0)
    *rebound += 1
    rebound = number(&b, 0)
    *rebound += 1
    let f = first(&a)
    *f += 10
    var store = Store{data: [4]}
    store.numbers().push(5)
    var grid: Vec[Vec[int]] = [[1], [2, 3]]
    let cell: &inout int = &grid[1][0]
    *cell = 8
    println("${a[0]} ${a[1]} ${a.len()} ${b[0]} ${store.data.len()} ${grid[1][0]}")
}
"#, "12 7 3 10 2 8\n");
}

#[test]
fn complex_receiver_runs_once_before_argument_and_owned_results_drop() {
    run_source(r#"
fn pick(v: inout Vec[Vec[int]]) -> &inout Vec[int] of v {
    println("receiver")
    return &v[0]
}
fn argument() -> int {
    println("argument")
    return 2
}
fn words(v: inout Vec[String]) -> &inout Vec[String] of v { return &v }
fn main() {
    var lists: Vec[Vec[int]] = [[1]]
    pick(&lists).push(argument())
    var strings: Vec[String] = ["a".toString(), "b".toString()]
    println(words(&strings).pop().isSome())
    let text = words(&strings).pop() ?? "".toString()
    println(text)
    println(lists[0].len())
}
"#, "receiver\nargument\ntrue\na\n2\n");
}

#[test]
fn returned_map_receiver_borrows_the_map_instead_of_the_hidden_descriptor() {
    run_source(r#"
fn table(m: inout Map[int, int]) -> &inout Map[int, int] of m { return &m }
fn main() {
    var m: Map[int, int] = [1: 7]
    let held = table(&m).getMut(1)
    println(held.isSome())
    if let n = table(&m).getMut(1) { *n += 1 }
    println(m.getCopy(1) ?? 0)
}
"#, "true\n8\n");
}

#[test]
fn return_and_alias_violations_are_rejected_semantically() {
    let cases = [
        ("local", r#"fn bad() -> &inout int of static {
    var n = 1
    return &n
}
fn main() {}"#, "E0303"),
        ("readonly", r#"fn bad(n: int) -> &inout int of n { return &n }
fn main() {}"#, "E0014"),
        ("temporary", r#"fn make() -> Vec[int] { return [1] }
fn bad() -> &inout Vec[int] of static { return &make() }
fn main() {}"#, "E0014"),
        ("wrong-source", r#"fn bad(a: inout int, b: inout int) -> &inout int of a {
    println(a)
    return &b
}
fn main() {}"#, "E0303"),
        ("missing-source", r#"fn id(v: inout int) -> &inout int { return &v }
fn main() {}"#, "E0302"),
        ("transitive", r#"fn id(v: inout Vec[int]) -> &inout Vec[int] of v { return &v }
fn main() {
    var v: Vec[int] = [1]
    let p = id(&v)
    let q = id(&p)
    v.push(2)
    q.push(3)
}"#, "E0102"),
        ("moved", r#"fn id(v: inout int) -> &inout int of v { return &v }
fn main() {
    var n = 1
    let p = id(&n)
    let q = p
    *p = 2
    println(*q)
}"#, "E0402"),
        ("source-upper-bound", r#"fn choose(a: inout Vec[int], b: inout Vec[int], first: bool) -> &inout Vec[int] of (a, b) {
    if first { return &a } else { return &b }
}
fn main() {
    var a: Vec[int] = [1]
    var b: Vec[int] = [2]
    let p = choose(&a, &b, true)
    b.push(3)
    p.push(4)
}"#, "E0102"),
        ("call-result-held", r#"fn id(v: inout Vec[int]) -> &inout Vec[int] of v { return &v }
fn change(v: inout Vec[int]) -> int {
    v.push(9)
    return 0
}
fn read(v: Vec[int], n: int) -> int { return v.len() + n }
fn main() {
    var v: Vec[int] = [1]
    println(read(id(&v), change(&v)))
}"#, "E0102"),
        ("rc-field", r#"struct Owner { shared: Rc[Vec[int]] }
fn main() {
    var o = Owner{shared: Rc([1, 2])}
    let p: &inout int = &o.shared[0]
    *p = 2
}"#, "E0101"),
        ("returned-map-owner", r#"fn table(m: inout Map[int, int]) -> &inout Map[int, int] of m { return &m }
fn main() {
    var m: Map[int, int] = [1: 7]
    let held = table(&m).getMut(1)
    m.set(2, 8)
    println(held.isSome())
}"#, "E0102"),
        ("returned-map-guard", r#"use std/sync
struct Holder { entries: Map[int, int] }
fn table(m: inout Map[int, int]) -> &inout Map[int, int] of m { return &m }
fn main() {
    let shared = Mutex(Holder{entries: [1: 7]})
    var outside: Map[int, int] = [1: 0]
    var held = table(&outside).getMut(1)
    with g = shared.lock() {
        held = table(&g.entries).getMut(1)
    }
    println(held.isSome())
}"#, "E0703"),
    ];
    for (name, source, code) in cases {
        let (out, report) = check(source);
        assert_eq!(out.status.code(), Some(1), "{name}: {report}");
        let errors: Vec<_> = report["diagnostics"].as_array().unwrap().iter().filter(|d| d["severity"] == "ERROR").collect();
        assert!(errors.iter().any(|d| d["code"] == code), "{name}: {report}");
        assert!(errors.iter().all(|d| d["code"] != "U0001" && d["code"] != "E0020"), "{name}: {report}");
    }
}

#[test]
fn complex_receiver_fix_remains_candidate_only_and_changes_order_explicitly() {
    let scratch = common::Scratch::new("mutref-fix").unwrap();
    let path = scratch.0.join("program.yv");
    let source = r#"
fn pick(v: inout Vec[Vec[int]]) -> &inout Vec[int] of v {
    println("receiver")
    return &v[0]
}
fn argument(v: Vec[Vec[int]]) -> int {
    println("argument")
    return v.len()
}
fn main() {
    var v: Vec[Vec[int]] = [[1]]
    pick(&v).push(argument(v))
    println(v[0].len())
}
"#;
    fs::write(&path, source).unwrap();
    let output = common::output(yavo().arg("fix").arg(&path).arg("--json").stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["changed"], false);
    assert!(report["automatic"].as_array().unwrap().is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    let candidates = report["candidates"].as_array().unwrap();
    let candidate = candidates.iter().find(|c| c["id"] == "hoist-argument").expect("a checked candidate");
    assert_eq!(candidate["applicability"], "candidate");
    assert_eq!(candidate["recheck"], "passed");
    let mut edited = source.to_string();
    let mut edits: Vec<_> = candidate["edits"].as_array().unwrap().iter().collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e["start"].as_u64().unwrap()));
    for edit in edits {
        edited.replace_range(edit["start"].as_u64().unwrap() as usize..edit["end"].as_u64().unwrap() as usize,
            edit["replacement"].as_str().unwrap());
    }
    run_source(&edited, "argument\nreceiver\n2\n");
}

#[test]
fn frozen_complex_receiver_case_reports_one_conflict() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../yavo-tests/fix/T-V3.yv");
    let (output, report) = check(&fs::read_to_string(path).unwrap());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["error_count"], 1);
    assert_eq!(report["diagnostics"][0]["code"], "E0403");
    assert!(report["diagnostics"][0]["fixes"].as_array().unwrap().iter().any(|f| f["id"] == "hoist-argument"));
}
