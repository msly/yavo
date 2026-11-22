//! Indexed writes check the same evaluation order that their generated C executes.
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};

mod common;

fn yavo() -> Command { Command::new(env!("CARGO_BIN_EXE_yavo")) }

fn reject(source: &str, code: &str) {
    let scratch = common::Scratch::new("index-order-reject").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    let out = common::output(yavo().arg("check").arg(&path).arg("--json").stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{report:#}");
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|d| d["code"] == code), "{report:#}");
    assert!(!diagnostics.iter().any(|d| d["code"] == "E0020" || d["code"] == "U0001"), "{report:#}");
}

fn run(source: &str, expected: &str) {
    let scratch = common::Scratch::new("index-order-run").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    for release in [false, true] {
        let exe = scratch.exe(if release { "release" } else { "debug" });
        let mut cmd = yavo();
        cmd.arg("build").arg(&path).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
        if release { cmd.arg("--release"); }
        let built = common::output(&mut cmd, common::BUILD_TIMEOUT).unwrap();
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let out = common::output(Command::new(&exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(out.stdout, expected.as_bytes());
        assert!(out.stderr.is_empty(), "{out:?}");
    }
}

#[test]
fn moving_the_assignment_value_precedes_all_target_uses() {
    reject(r#"
struct Node { children: Vec[Node] }
fn main() {
    var nodes = [Node{children: []}]
    nodes[0].children = nodes
}
"#, "E0402");
    reject(r#"
fn index(text: String) -> int { println(text)
    return 0 }
fn main() {
    var text = "new".toString()
    var values = ["old".toString()]
    values[index(text)] = text
    println(values[0])
}
"#, "E0402");
    reject(r#"
fn index(text: String) -> int { println(text)
    return 0 }
fn main() {
    var text = "new".toString()
    var values: Vec[?String] = [none]
    values[index(text)] = text
    println(values.len())
}
"#, "E0402");
}

#[test]
fn assignment_views_survive_target_evaluation() {
    reject(r#"
fn index(text: inout String) -> int { text = "replacement".toString()
    return 0 }
fn main() {
    var text = "hello".toString()
    var views: Vec[str] = [""]
    views[index(&text)] = text.trim()
    println(views[0])
}
"#, "E0102");
    reject(r#"
fn index(text: sink String) -> int { println(text)
    return 0 }
fn main() {
    var text = "hello".toString()
    var views: Vec[str] = [""]
    views[index(text)] = text.trim()
    println(views[0])
}
"#, "E0102");
    reject(r#"
fn index(text: inout String) -> int { text = "replacement".toString()
    return 0 }
fn main() {
    var text = "hello".toString()
    var views: Vec[?str] = [none]
    views[index(&text)] = text.trim()
    println(views.len())
}
"#, "E0102");
    reject(r#"
fn index(values: inout Vec[int]) -> int { values.push(3)
    return 0 }
fn main() {
    var values = [1, 2]
    var views: Vec[[]int] = []
    views.push(values[0..1])
    views[index(&values)] = values
    println(views.len())
}
"#, "E0102");
}

#[test]
fn coercions_and_borrowed_descriptors_are_kept_without_extra_moves() {
    run(r#"
fn bump(value: inout int) -> int { value += 1
    return 0 }
fn main() {
    var n = 7
    let p: &inout int = &n
    var copied = [0]
    copied[bump(&p)] = p
    println("${copied[0]} ${*p}")
    var values = [11, 12]
    var slices: Vec[[]int] = [values[0..1]]
    slices[0] = values
    println("${slices[0][1]} ${values.len()}")
    let text = "hello world".toString()
    var words: Vec[str] = ["", ""]
    let split = text.fields()
    words[0] = split[1]
    words[1] = split[0]
    println(words.join(" "))
    var optional: Vec[?str] = [none]
    optional[0] = text.trim()
    if let view = optional[0] { println(view) }
    var owned: Vec[?String] = [none]
    let newValue = "owned".toString()
    owned[0] = newValue
    if let value = owned[0] { println(value) }
}
"#, "7 8\n12 2\nworld hello\nhello world\nowned\n");
}

#[test]
fn target_signatures_and_indices_run_once_after_the_value() {
    run(r#"
fn pick[T](values: inout Vec[T]) -> &inout Vec[T] of values {
    println("target")
    return &values
}
fn value() -> int { println("value")
    return 7 }
fn index() -> int { println("index")
    return 0 }
struct Store[T] {
    values: Vec[T]
    fn data(inout self) -> &inout Vec[T] of self {
        println("method")
        return &self.values
    }
}
fn main() {
    var numbers = [1]
    pick(&numbers)[index()] = value()
    pick[int](&numbers)[index()] += value()
    var store = Store[int]{values: [0]}
    store.data()[index()] = value()
    var boxed = Box([3])
    boxed[0] = 4
    let writable: &inout Vec[int] = &numbers
    writable[0] += 1
    println("${writable[0]} ${store.values[0]} ${boxed[0]}")
}
"#, "value\ntarget\nindex\nvalue\ntarget\nindex\nvalue\nmethod\nindex\n15 7 4\n");
}

#[test]
fn generic_target_preparation_preserves_definition_diagnostics() {
    reject(r#"
fn missingSource[T](values: inout Vec[T]) -> &inout Vec[T] {
    return &values
}
fn main() {
    var numbers = [1]
    missingSource(&numbers)[0] = 2
    println(numbers[0])
}
"#, "E0302");
}

#[test]
fn match_bound_reference_targets_keep_their_contextual_type() {
    run(r#"
fn identity[T](values: inout Vec[T]) -> &inout Vec[T] of values { return &values }
fn main() {
    var rows: Map[int, Vec[int]] = [1: [0]]
    (match rows.getMut(1) {
        some(row) => identity(&row)
        none => return
    })[0] = 2
    if let row = rows.get(1) { println(row[0]) }
}
"#, "2\n");
}
