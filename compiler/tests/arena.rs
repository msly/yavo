//! Arena provenance, generational identities, cleanup and counter exhaustion.
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
mod common;
#[allow(dead_code)]
#[path = "../src/toolchain.rs"]
mod toolchain;
fn yavo() -> Command { Command::new(env!("CARGO_BIN_EXE_yavo")) }
fn check(source: &str) -> (std::process::Output, Value) {
    let scratch = common::Scratch::new("arena-check").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    let out = common::output(yavo().arg("check").arg(&path).arg("--json").stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    let json = serde_json::from_slice(&out.stdout).unwrap();
    (out, json)
}
fn run_source(source: &str, expected: &str) {
    let scratch = common::Scratch::new("arena-run").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    for release in [false, true] {
        let exe = scratch.exe(if release { "release" } else { "debug" });
        let mut cmd = yavo();
        cmd.arg("build").arg(&path).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
        if release { cmd.arg("--release"); }
        let out = common::output(&mut cmd, common::BUILD_TIMEOUT).unwrap();
        assert!(out.status.success() && out.stderr.is_empty(), "build: {out:?}");
        let out = common::output(Command::new(exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(out.stdout, expected.as_bytes());
        assert!(out.stderr.is_empty(), "{out:?}");
    }
}
#[test]
fn arena_views_keep_external_sources_and_mutable_receivers_keep_real_owners() {
    run_source(r#"
fn views(text: str) -> Arena[str] of text {
    var a = Arena[str]{}
    let _id = a.add(text)
    return a
}
fn pool(a: inout Arena[int]) -> &inout Arena[int] of a { return &a }
fn main() {
    var owner = "hello".toString()
    var a = views(owner)
    let id = a.add(owner.trim())
    let descriptor = a.getCopy(id) ?? ""
    let removed = a.remove(id) ?? ""
    let _ = a
    println("${descriptor} ${removed}")
    owner.push("!")
    var b = Arena[int]{}
    let token = pool(&b).add(7)
    let held = pool(&b).getMut(token)
    println(held.isSome())
    if let n = pool(&b).getMut(token) { *n += 1 }
    let borrowed = pool(&b).get(token)
    if let n = borrowed { println(*n) }
}
"#, "hello hello\ntrue\n8\n");
}
#[test]
fn arena_rejects_alias_escape_bad_payload_and_resource_operations() {
    run_source(r#"
fn before[T: Ord](a: T, b: T) -> bool { return a < b }
fn main() {
    var a = Arena[int]{}
    let first = a.add(1)
    let second = a.add(2)
    assert(before(first, second))
    assert(first <= second)
    assert(second > first)
    assert(second >= first)
}
"#, "");
    let cases = [
        ("get-live", r#"fn main() {
var a = Arena[String]{}
let id = a.add("hi".toString())
let p = a.get(id)
let _new = a.add("bye".toString())
println(p.isSome())
}"#, "E0102"),
        ("mut-live", r#"fn main() {
var a = Arena[int]{}
let id = a.add(1)
let p = a.getMut(id)
let _ = a.remove(id)
println(p.isSome())
}"#, "E0102"),
        ("mut-return-live", r#"fn pool(a: inout Arena[int]) -> &inout Arena[int] of a { return &a }
fn main() { var a = Arena[int]{}
let id = a.add(1)
let p = pool(&a).getMut(id)
let _new = a.add(2)
println(p.isSome())
}"#, "E0102"),
        ("moved", r#"fn main() {
var a = Arena[int]{}
let id = a.add(1)
let b = a
println(a.len())
println(b.has(id))
}"#, "E0402"),
        ("readonly", r#"fn bad(a: Arena[int]) { let _id = a.add(1) }
fn main() {}"#, "E0014"),
        ("wrong-id-type", r#"fn main() {
var a = Arena[int]{}
var b = Arena[String]{}
let id = a.add(1)
println(b.get(id).isSome())
}"#, "E0001"),
        ("noncopy", r#"fn main() {
var a = Arena[String]{}
let id = a.add("a".toString())
let _ = a.getCopy(id)
}"#, "E0404"),
        ("take", r#"fn main() { var a = Arena[int]{}
let _ = take(&a)
}"#, "E0405"),
        ("clone", r#"fn main() { let a = Arena[int]{}
let _ = a.clone()
}"#, "E0001"),
        ("nested-clone", r#"fn main() { let a: Vec[Arena[int]] = [Arena[int]{}]
let _ = a.clone()
}"#, "E0001"),
        ("forge", r#"fn main() { let _id = Id[int]{} }"#, "E0001"),
        ("field-view", r#"struct Bad { a: Arena[str] }
fn main() {}"#, "E0301"),
        ("local-return", r#"fn bad() -> ?&int of static {
var a = Arena[int]{}
let id = a.add(1)
return a.get(id)
}
fn main() {}"#, "E0303"),
        ("view-source", r#"fn main() {
var text = "a".toString()
var a = Arena[str]{}
let id = a.add(text.trim())
text.push("b")
println(a.getCopy(id) ?? "")
}"#, "E0102"),
        ("removed-source", r#"fn main() {
var text = "a".toString()
var a = Arena[str]{}
let id = a.add(text.trim())
let s = a.remove(id) ?? ""
text.push("b")
println(s)
}"#, "E0102"),
        ("write-parameter", r#"fn bad(a: inout Arena[str], text: str) { let _id = a.add(text) }
fn main() {}"#, "E0306"),
        ("arena-not-send", r#"fn consume(a: sink Arena[Rc[int]]) { println(a.len()) }
fn main() {
var a = Arena[Rc[int]]{}
let _id = a.add(Rc(1))
scope { let _h = spawn consume(a) }
}"#, "E0701"),
        ("receiver-argument-alias", r#"fn change(a: inout Arena[int], id: Id[int]) -> Id[int] {
let _new = a.add(2)
return id
}
fn main() {
var a = Arena[int]{}
let id = a.add(1)
println(a.get(change(&a, id)).isSome())
}"#, "E0102"),
    ];
    for (label, source, code) in cases {
        let (out, report) = check(source);
        assert_eq!(out.status.code(), Some(1), "{label}: {report:#}");
        let ds = report["diagnostics"].as_array().unwrap();
        assert!(ds.iter().any(|d| d["code"] == code), "{label}: expected {code}, {report:#}");
        assert!(!ds.iter().any(|d| d["code"] == "E0020" || d["code"] == "U0001"), "{label}: {report:#}");
    }
}
#[test]
fn guard_default_parameters_cannot_gain_exclusive_arena_or_vec_access() {
    let cases = [
        ("vec-live-view", r#"use std/sync
fn mutate(g: Guard[Vec[int]]) { for i in 0..100 { g.push(i) } }
fn main() {
    let mutex = Mutex([1])
    with g = mutex.lock() {
        let p: &int = &g[0]
        mutate(g)
        println(*p)
    }
}"#, "E0101"),
        ("arena-live-view", r#"use std/sync
fn mutate(g: Guard[Arena[int]]) { let _id = g.add(2) }
fn main() {
    let mutex = Mutex(Arena[int]{})
    with g = mutex.lock() {
        let id = g.add(1)
        let p = g.get(id)
        mutate(g)
        println(p.isSome())
    }
}"#, "E0101"),
        ("generic-guard", r#"use std/sync
fn mutate[T](g: T) { g.push(9) }
fn main() { let mutex = Mutex([1])
with g = mutex.lock() { mutate(g) }
}"#, "E0101"),
        ("guard-field-method", r#"use std/sync
struct Store { data: Vec[int], pool: Arena[int] }
fn mutate(g: Guard[Store]) { g.data.push(9)
let _id = g.pool.add(1) }
fn main() {}"#, "E0101"),
        ("guard-field-write", r#"use std/sync
struct Store { data: Vec[int] }
fn mutate(g: Guard[Store]) { g.data = [9] }
fn main() {}"#, "E0101"),
        ("guard-field-take", r#"use std/sync
struct Store { data: Vec[int] }
fn mutate(g: Guard[Store]) -> Vec[int] { return take(&g.data) }
fn main() {}"#, "E0101"),
        ("guard-field-inout", r#"use std/sync
struct Store { data: Vec[int] }
fn grow(data: inout Vec[int]) { data.push(9) }
fn mutate(g: Guard[Store]) { grow(&g.data) }
fn main() {}"#, "E0101"),
        ("guard-index-write", r#"use std/sync
fn mutate(g: Guard[Vec[int]]) { let p: &inout int = &g[0]
*p = 9 }
fn main() {}"#, "E0101"),
        ("guard-index-borrow", r#"use std/sync
fn indexed(g: Guard[Vec[int]]) -> &inout int of g { return &g[0] }
fn main() {}"#, "E0101"),
        ("guard-readonly-pattern", r#"use std/sync
fn mutate(g: ?Guard[Vec[int]]) { if let held = g { held.push(9) } }
fn main() {}"#, "E0101"),
        ("guard-readonly-tuple", r#"use std/sync
fn mutate(g: (Guard[Vec[int]], int)) { g.0.push(9) }
fn main() {}"#, "E0014"),
        ("rc-arena", r#"fn main() { let a = Rc(Arena[int]{})
let _id = a.add(1)
}"#, "E0101"),
        ("arc-arena", r#"fn main() { let a = Arc(Arena[int]{})
let _id = a.add(1)
}"#, "E0101"),
        ("guard-view-escape", r#"use std/sync
fn main() {
let mutex = Mutex(Arena[int]{})
var p: ?&int = none
with g = mutex.lock() {
let id = g.add(1)
p = g.get(id)
}
println(p.isSome())
}"#, "E0703"),
    ];
    for (label, source, code) in cases {
        let (out, report) = check(source);
        assert_eq!(out.status.code(), Some(1), "{label}: {report:#}");
        let ds = report["diagnostics"].as_array().unwrap();
        assert!(ds.iter().any(|d| d["code"] == code), "{label}: expected {code}, {report:#}");
        assert!(!ds.iter().any(|d| d["code"] == "E0020" || d["code"] == "U0001"), "{label}: {report:#}");
    }
    run_source(include_str!("p0/run_arena_sync.yv"), "arena sync ok\n");
}
#[test]
fn foreign_ids_panic_for_all_queries_in_both_modes() {
    for method in ["get", "getCopy", "getMut", "remove", "has"] {
        let scratch = common::Scratch::new("arena-foreign").unwrap();
        let path = scratch.0.join("program.yv");
        let source = format!("fn main() {{\n    var a = Arena[int]{{}}\n    var b = Arena[int]{{}}\n    let id = a.add(1)\n    let _ = b.{method}(id)\n}}\n");
        fs::write(&path, source).unwrap();
        for release in [false, true] {
            let exe = scratch.exe(if release { "release" } else { "debug" });
            let mut cmd = yavo();
            cmd.arg("build").arg(&path).arg("-o").arg(&exe);
            if release { cmd.arg("--release"); }
            let out = common::output(&mut cmd, common::BUILD_TIMEOUT).unwrap();
            assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
            let out = common::output(&mut Command::new(&exe), common::RUN_TIMEOUT).unwrap();
            assert_eq!(out.status.code(), Some(101), "{method}: {out:?}");
            assert!(out.stdout.is_empty(), "{out:?}");
            let expected = format!("yavo panic [P-ARENA-ID] at {}:5:13: Id belongs to another Arena\n", path.display());
            assert_eq!(out.stderr, expected.as_bytes(), "{method}");
        }
    }
}
#[test]
fn arena_runtime_identity_generation_bounds_and_thread_uniqueness() {
    let scratch = common::Scratch::new("arena-runtime").unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tc = toolchain::find().unwrap();
    for release in [false, true] {
        let exe = scratch.exe(if release { "release" } else { "debug" });
        let mut cmd = Command::new(&tc.program);
        cmd.args(&tc.prefix_args).args(["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK"])
            .arg(if release { "-O2" } else { "-O0" })
            .arg("-I").arg(root.join("runtime")).arg(root.join("tests/arena_runtime.c")).arg("-o").arg(&exe);
        let out = common::output(&mut cmd, common::BUILD_TIMEOUT).unwrap();
        assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
        for mode in ["concurrent", "bounds", "identity", "capacity", "generation"] {
            let out = common::output(Command::new(&exe).arg(mode), common::RUN_TIMEOUT).unwrap();
            let panic = matches!(mode, "identity" | "capacity" | "generation");
            assert_eq!(out.status.code(), Some(if panic { 101 } else { 0 }), "{mode}: {out:?}");
            assert!(out.stdout.is_empty(), "{mode}: {out:?}");
            let expected = if panic { "yavo panic [P-CAPACITY] at arena-runtime.yv:31:5: capacity overflow\n" } else { "" };
            assert_eq!(out.stderr, expected.as_bytes(), "{mode}");
        }
    }
}
#[test]
fn arena_allocation_effects_include_element_destructors() {
    let scratch = common::Scratch::new("arena-effects").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, r#"
struct Resource: Drop {
    fn drop(inout self) { let _ = format("drop") }
}
fn empty() -> Arena[int] { return Arena[int]{} }
fn add(a: inout Arena[int]) -> Id[int] { return a.add(1) }
fn read(a: Arena[int], id: Id[int]) -> ?int { return a.getCopy(id) }
fn remove(a: inout Arena[int], id: Id[int]) -> ?int { return a.remove(id) }
fn destroy(a: sink Arena[Resource]) { let _ = a }
fn discarded(a: inout Arena[Resource], id: Id[Resource]) { let _ = a.remove(id) }
fn main() {}
"#).unwrap();
    for (subject, expected) in [("empty", "no_alloc"), ("add", "may_alloc"), ("read", "no_alloc"), ("remove", "no_alloc"), ("destroy", "may_alloc"), ("discarded", "may_alloc")] {
        let out = common::output(yavo().args(["allocations", subject, "--file"]).arg(&path).arg("--json"), common::RUN_TIMEOUT).unwrap();
        assert!(out.status.success(), "{subject}: {out:?}");
        let json: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["status"], expected, "{subject}: {json:#}");
        assert_eq!(json["has_unknown"], false, "{subject}: {json:#}");
    }
}
