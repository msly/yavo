//! Borrow provenance belongs to an evaluated result, including control-flow values.
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};

mod common;

fn check(source: &str) -> (std::process::Output, Value) {
    let scratch = common::Scratch::new("borrow-provenance-check").unwrap();
    let path = scratch.0.join("program.yv");
    fs::write(&path, source).unwrap();
    let output = common::output(
        Command::new(env!("CARGO_BIN_EXE_yavo")).arg("check").arg(&path).arg("--json").stdin(Stdio::null()),
        common::RUN_TIMEOUT,
    ).unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap();
    (output, report)
}

fn rejects(name: &str, source: &str, code: &str) {
    let (output, report) = check(source);
    assert_eq!(output.status.code(), Some(1), "{name}: {report}");
    assert!(report["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == code),
        "{name}: expected {code}, got {report}");
    assert!(!report["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "U0001"),
        "{name}: must be a semantic or syntax rejection, not an unsupported escape: {report}");
}

#[test]
fn local_or_temporary_views_cannot_escape_any_supported_control_flow_form() {
    for (name, source) in [
        ("match-local", r#"fn bad() -> str of static {
    let owner = "x".toString()
    return match owner[0..1] { view => view }
}
fn main() {}"#),
        ("match-owned-arm", r#"enum E { Text(value: String) }
fn bad() -> str of static {
    return match E.Text("x".toString()) { .Text(value) => value[0..value.len()] }
}
fn main() {}"#),
        ("if-return", r#"fn bad(flag: bool) -> str of static {
    if flag { let owner = "x".toString()
        return owner[0..1]
    }
    return ""
}
fn main() {}"#),
        ("block-return", r#"fn bad() -> str of static {
    { let owner = "x".toString()
        return owner[0..1]
    }
}
fn main() {}"#),
        ("coalesce-return", r#"fn bad() -> str of static {
    let owner = "x".toString()
    let view: ?str = owner[0..1]
    return view ?? ""
}
fn main() {}"#),
        ("coalesce-match", r#"fn bad() -> str of static {
    let owner = "x".toString()
    let absent: ?str = none
    return absent ?? match 0 { _ => owner[0..1] }
}
fn main() {}"#),
        ("catch-owned-error", r#"fn failure() -> str ! String of static { return err("x".toString()) }
fn bad() -> str of static { return failure() catch error => error[0..error.len()] }
fn main() {}"#),
        ("catch-match", r#"fn failure() -> str ! String of static { return err("x".toString()) }
fn bad() -> str of static {
    return failure() catch error => match 0 { _ => error[0..error.len()] }
}
fn main() {}"#),
        ("method-chain", r#"fn bad() -> str of static {
    let owner = " x ".toString()
    return owner.trim().trim()
}
fn main() {}"#),
        ("method-chain-through-match", r#"fn bad() -> str of static {
    let owner = " x ".toString()
    return (match owner[0..owner.len()] { view => view }).trim().trim()
}
fn main() {}"#),
        ("temporary-method-chain", r#"fn bad() -> str of static { return " x ".toString().trim().trim() }
fn main() {}"#),
        ("owned-struct-field", r#"struct Owner { text: String }
fn bad() -> str of static {
    let owner = Owner{text: "x".toString()}
    return match 0 { _ => owner.text[0..1] }
}
fn main() {}"#),
        ("slice-result", r#"fn bad() -> []int of static {
    let owner: Vec[int] = [1]
    return match 0 { _ => owner[0..1] }
}
fn main() {}"#),
        ("mutable-reference-result", r#"fn bad() -> &inout int of static {
    var owner = 1
    return match 0 { _ => &owner }
}
fn main() {}"#),
    ] {
        let code = if matches!(name, "match-owned-arm" | "catch-owned-error" | "catch-match") { "E0305" } else { "E0303" };
        rejects(name, source, code);
    }
}

#[test]
fn nested_borrow_carrying_values_and_wrong_parameters_retain_all_sources() {
    for (name, ret, expression) in [
        ("tuple", "(str, int)", "match 0 { _ => (owner[0..1], 1) }"),
        ("optional", "?str", "match 0 { _ => some(owner[0..1]) }"),
        ("nested-tuple-optional", "(?str, int)", "match 0 { _ => (some(owner[0..1]), 1) }"),
        ("vector", "Vec[str]", "match 0 { _ => [owner[0..1]] }"),
        ("map", "Map[int, str]", "match 0 { _ => [1: owner[0..1]] }"),
        ("result", "str ! int", "ok(match 0 { _ => owner[0..1] })"),
    ] {
        rejects(name, &format!("fn bad() -> {ret} of static {{\n    let owner = \"x\".toString()\n    return {expression}\n}}\nfn main() {{}}"), "E0303");
    }
    rejects("wrong-parameter", r#"fn bad(a: str, _b: str) -> str of _b {
    let alias = a
    return match alias { view => view }
}
fn main() {}"#, "E0303");
    rejects("one-wrong-live-arm", r#"enum Pick { A, B }
fn bad(a: str, b: str, pick: Pick) -> str of a {
    return match pick { .A => a, .B => b }
}
fn main() {}"#, "E0303");
    rejects("owned-arm-stored-outside", r#"enum E { Text(value: String) }
fn main() {
    let view = match E.Text("x".toString()) { .Text(value) => value[0..value.len()] }
    println(view)
}"#, "E0305");
}

#[test]
fn supported_parameter_aliases_and_owned_results_execute_without_false_rejections() {
    let source = r#"
enum E { Text(value: String), End }
enum Pick { A, B }
struct Owner { text: String }
fn matchAlias(a: str) -> str of a {
    let alias = a
    return match alias { view => view }
}
fn ifReturn(a: str, flag: bool) -> str of a {
    if flag { let alias = a
        return alias
    }
    return a
}
fn blockReturn(a: str) -> str of a { { let alias = a
    return alias
} }
fn optionalAlias(a: ?str) -> str of a {
    let alias = a
    if let view = alias { return view }
    return ""
}
fn coalesced(a: ?str, b: str) -> str of (a, b) { return a ?? b }
fn outcome(a: str) -> str ! int of a { return ok(a) }
fn caught(a: str, b: str) -> str of (a, b) { return outcome(a) catch _ => b }
fn tried(a: str) -> str ! int of a { return ok(try outcome(a)) }
fn chained(a: str) -> str of a { return (match a { view => view }).trim().trim() }
fn tuple(a: str) -> (?str, int) of a { return match 0 { _ => (some(a), 1) } }
fn vector(a: str) -> Vec[str] of a { return match 0 { _ => [a] } }
fn map(a: str) -> Map[int, str] of a { return match 0 { _ => [1: a] } }
fn enumView(e: E) -> str of e { return match e { .Text(value) => value[0..value.len()], .End => "" } }
fn fieldView(owner: Owner) -> str of owner { return match 0 { _ => owner.text[0..owner.text.len()] } }
fn sliceView(data: Vec[int]) -> []int of data { return match 0 { _ => data[0..data.len()] } }
fn mutableView(data: inout Vec[int]) -> &inout int of data { return match 0 { _ => &data[0] } }
fn firstRef(data: inout Vec[int]) -> &inout int of data { return &data[0] }
fn reborrow(data: inout Vec[int]) -> &inout int of data { return match firstRef(&data) { view => &view } }
fn mutableText(text: inout String) -> str of text { return text.trim() }
fn mutableField(owner: inout Owner) -> str of owner { return mutableText(&owner.text) }
fn failure() -> str ! int of static { return err(1) }
fn owned() -> String { return match E.Text("owned".toString()) { .Text(value) => value, .End => "".toString() } }
fn oldValue(a: str, b: str) -> str of a {
    var slot = a
    return match 0 { _ => replace(&slot, b) }
}
fn main() {
    let owner = " text ".toString()
    println(matchAlias(owner))
    println(ifReturn(owner, true))
    println(blockReturn(owner))
    println(optionalAlias(some("text")))
    println(coalesced(none, "text"))
    println(caught("text", "fallback"))
    println(tried("text") catch _ => "fallback")
    println(chained(owner))
    println(tuple("text").0 ?? "fallback")
    let words = vector("text")
    println(words[0])
    let entries = map("text")
    println(entries.getCopy(1) ?? "fallback")
    let e = E.Text("text".toString())
    println(enumView(e))
    let field = Owner{text: "text".toString()}
    println(fieldView(field))
    var numbers: Vec[int] = [3]
    println(sliceView(numbers)[0])
    let p = mutableView(&numbers)
    *p = 4
    println(numbers[0])
    println(owned())
    println(oldValue("text", "different"))
    println(" temp ".toString().trim())
    println(some(" lhs ".toString().trim()) ?? "fallback")
    println(outcome(" input ".toString().trim()) catch _ => "fallback")
    println(failure() catch _ => owner.trim())
    let q = reborrow(&numbers)
    *q = 5
    println(numbers[0])
    var mutableOwner = Owner{text: " inout ".toString()}
    println(mutableField(&mutableOwner))
}
"#;
    let scratch = common::Scratch::new("borrow-provenance-run").unwrap();
    let input = scratch.0.join("program.yv");
    fs::write(&input, source).unwrap();
    for release in [false, true] {
        let exe = scratch.exe(if release { "release" } else { "debug" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_yavo"));
        command.arg("build").arg(&input).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
        if release { command.arg("--release"); }
        let built = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        assert!(built.stderr.is_empty(), "{}", String::from_utf8_lossy(&built.stderr));
        let ran = common::output(Command::new(exe).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert_eq!(ran.status.code(), Some(0), "{ran:?}");
        assert_eq!(ran.stdout, b" text \n text \n text \ntext\ntext\ntext\ntext\ntext\ntext\ntext\ntext\ntext\ntext\n3\n4\nowned\ntext\ntemp\nlhs\ninput\ntext\n5\ninout\n");
        assert!(ran.stderr.is_empty(), "{ran:?}");
    }
}

#[test]
fn unreachable_or_diverging_arms_do_not_invent_result_origins() {
    for source in [
        r#"fn good(a: str, b: str) -> str of a { return match 0 { _ => a, _ => b } }
fn main() {}"#,
        r#"enum Pick { A, B }
fn good(a: str, b: str, pick: Pick) -> str of a {
    return match pick { .A => a, .B => { println(b)
        return a
    } }
}
fn main() {}"#,
        r#"fn good(a: str) -> str of a { return match 0 { _ => { return a } } }
fn main() {}"#,
    ] {
        let (out, report) = check(source);
        assert!(out.status.success(), "{report}");
    }
    // Copy bindings still pin their source place inside the arm, although the
    // copied view's returned provenance does not include that descriptor slot.
    rejects("copy-binding-access", r#"fn main() {
    var source: ?str = some("x")
    if let view = source {
        source = none
        println(view)
    }
}"#, "E0102");
}

#[test]
fn branch_cleanup_rejects_expired_views_even_for_immediate_consumers() {
    for (name, source) in [
        ("match-owned", r#"enum E { Text(value: String) }
fn main() { println(match E.Text("x".toString()) { .Text(value) => value.trim() }) }"#),
        ("catch-owned-error", r#"fn failure() -> str ! String of static { return err("x".toString()) }
fn main() { println(failure() catch error => error.trim()) }"#),
        ("coalesce-rhs-temporary", r#"fn main() {
    let absent: ?str = none
    println(absent ?? "x".toString().trim())
}"#),
        ("match-arm-temporary", r#"fn main() { println(match 0 { _ => "x".toString().trim() }) }"#),
        ("catch-handler-temporary", r#"fn failure() -> str ! int of static { return err(1) }
fn main() { println(failure() catch _ => "x".toString().trim()) }"#),
        ("nested-carrier", r#"enum E { Text(value: String) }
fn main() { println((match E.Text("x".toString()) { .Text(value) => [value.trim()] }).len()) }"#),
    ] {
        rejects(name, source, "E0305");
    }
    rejects("inout-owned-field", r#"struct Owner { text: String }
fn view(text: inout String) -> str of text { return text.trim() }
fn bad() -> str of static {
    var owner = Owner{text: "x".toString()}
    return view(&owner.text)
}
fn main() {}"#, "E0303");
}

#[test]
fn mutating_builtins_keep_previous_value_sources() {
    for (name, setup, expression) in [
        ("take", "var values: ?str = owner.trim()", "take(&values)"),
        ("replace", "var values = owner.trim()", "replace(&values, \"new\")"),
        ("pop", "var values: Vec[str] = [owner.trim()]", "values.pop()"),
        ("map-remove", "var values: Map[int, str] = [1: owner.trim()]", "values.remove(1)"),
        ("arena-remove", "var values = Arena[str]{}\n    let key = values.add(owner.trim())", "values.remove(key)"),
        ("deque-pop", "var values = Deque[str]{}\n    values.pushBack(owner.trim())", "values.popFront()"),
    ] {
        let ret = if name == "replace" { "str" } else { "?str" };
        let import = if name == "deque-pop" { "use std/collections\n" } else { "" };
        rejects(name, &format!("{import}fn bad() -> {ret} of static {{\n    let owner = \"x\".toString()\n    {setup}\n    return {expression}\n}}\nfn main() {{}}"), "E0303");
        let setup = setup.replace("owner.trim()", "source");
        let source = format!("{import}fn good(source: str) -> {ret} of source {{\n    {setup}\n    return {expression}\n}}\nfn main() {{}}");
        let (out, report) = check(&source);
        assert!(out.status.success(), "{name}: {report}");
    }
    rejects("swap-wrong-source", r#"fn bad(a: str, b: str) -> str of a {
    var first = a
    var second = b
    swap(&first, &second)
    return first
}
fn main() {}"#, "E0303");
    let (out, report) = check(r#"fn good(a: str, b: str) -> str of (a, b) {
    var first = a
    var second = b
    swap(&first, &second)
    return first
}
fn main() {}"#);
    assert!(out.status.success(), "{report}");
}

#[test]
fn syntax_and_storage_boundaries_are_not_counted_as_borrow_escape_checks() {
    rejects("if-is-not-an-expression", r#"fn bad(flag: bool) -> str of static {
    return if flag { "x" } else { "y" }
}
fn main() {}"#, "E0020");
    rejects("block-is-not-a-value", r#"fn bad() -> str of static { return { "x" } }
fn main() {}"#, "E0020");
    rejects("struct-cannot-store-borrow", "struct Bad { value: ?str }\nfn main() {}", "E0301");
    rejects("enum-cannot-store-borrow", "enum Bad { Value(value: (str, int)) }\nfn main() {}", "E0301");
    rejects("box-cannot-store-borrow", "fn bad(value: str) -> Box[str] of value { return Box(value) }\nfn main() {}", "E0301");
}
