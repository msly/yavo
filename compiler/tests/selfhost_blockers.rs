//! Name-resolution regressions exposed by the self-hosted frontend comparison.
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};

mod common;

fn check(source: &str, expected_errors: &[&str]) {
    let scratch = common::Scratch::new("selfhost-flow").unwrap();
    let source_path = scratch.0.join("main.yv");
    fs::write(&source_path, source).unwrap();
    let output = common::output(
        Command::new(env!("CARGO_BIN_EXE_yavo"))
            .arg("check").arg(&source_path).arg("--json").stdin(Stdio::null()),
        common::RUN_TIMEOUT,
    ).unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let errors: Vec<&str> = report["diagnostics"].as_array().unwrap().iter()
        .filter(|d| d["severity"] == "ERROR")
        .map(|d| d["code"].as_str().unwrap()).collect();
    if expected_errors.is_empty() {
        assert!(output.status.success(), "{source}\n{report}");
        assert!(errors.is_empty(), "{source}\n{report}");
    } else {
        assert_eq!(output.status.code(), Some(1), "{source}\n{report}");
        for expected in expected_errors {
            assert!(errors.contains(expected), "expected {expected}; {source}\n{report}");
        }
    }
}

#[test]
fn user_panic_function_does_not_supply_a_missing_return() {
    check(r#"
fn panic() -> int { return 7 }
fn missing() -> int { panic() }
fn main() { println(missing()) }
"#, &["E0013"]);
    // Lifted closure bodies use a separate function context and the same rule.
    check(r#"
fn panic() {}
fn main() {
    var xs: Vec[int] = [2, 1]
    xs.sortBy((a, b) => { panic() })
}
"#, &["E0013"]);
}

#[test]
fn local_and_parameter_shadowing_do_not_create_exits() {
    check(r#"
fn missing() -> int {
    let panic = 1
    panic()
}
fn main() {}
"#, &["E0001", "E0013"]);
    check(r#"
fn missing(panic: int) -> int { panic() }
fn main() {}
"#, &["E0001", "E0013"]);
    // The same spelling rule must not mistake an ordinary method for std/os.
    check(r#"
struct Stopper { fn exit(self, _code: int) {} }
fn missing() -> int {
    let os = Stopper{}
    os.exit(0)
}
fn main() {}
"#, &["E0013"]);
}

#[test]
fn shadowed_panic_cannot_end_a_catch_or_match_value_block() {
    check(r#"
fn panic() {}
fn fallible() -> int ! int { return err(2) }
fn main() {
    let n = fallible() catch _ { panic() }
    println(n)
}
"#, &["E0601"]);
    check(r#"
fn panic() {}
fn main() {
    let value: ?int = some(1)
    let n = match value {
        some(_x) => { panic() }
        none => 3
    }
    println(n)
}
"#, &["E0001"]);
}

#[test]
fn a_returning_panic_keeps_moved_values_in_branch_merges() {
    for branch in [
        "if flag {\n consume(s)\n panic()\n}",
        "if let _n = value {\n consume(s)\n panic()\n}",
        "match value {\n some(_n) => {\n consume(s)\n panic()\n}\n none => {}\n}",
    ] {
        check(&format!(r#"
fn consume(_s: sink String) {{}}
fn panic() {{}}
fn probe(flag: bool, value: ?int) {{
    let s = "still owned".toString()
    {branch}
    println(s)
}}
fn main() {{}}
"#), &["E0402"]);
    }
}

#[test]
fn builtin_panic_and_os_exit_still_terminate_all_supported_contexts() {
    check(r#"
use std/os
fn consume(_s: sink String) {}
fn fail() -> int { panic("stop") }
fn quit() -> int { os.exit(9) }
fn branch(flag: bool) -> int {
    if flag { return 1 } else { panic("stop") }
}
fn fallible() -> int ! int { return 1 }
fn owned(flag: bool, value: ?int) {
    let a = "if".toString()
    if flag {
        consume(a)
        panic("stop")
    }
    println(a)
    let b = "optional".toString()
    if let _n = value {
        consume(b)
        os.exit(9)
    }
    println(b)
    let c = "match".toString()
    match value {
        some(_n) => {
            consume(c)
            panic("stop")
        }
        none => {}
    }
    println(c)
}
fn main() {
    let n = fallible() catch _ { panic("stop") }
    let m = fallible() catch _ { os.exit(9) }
    let value: ?int = some(1)
    let k = match value {
        some(x) => x
        none => { panic("stop") }
    }
    var xs: Vec[int] = [2, 1]
    xs.sortBy((a, b) => { panic("stop") })
    println(n + m + k)
}
"#, &[]);
}

#[test]
fn later_local_declarations_do_not_change_an_earlier_builtin_call() {
    check(r#"
fn fail() -> int {
    panic("stop")
    let panic = 1
    println(panic)
}
fn main() {}
"#, &[]);
}
