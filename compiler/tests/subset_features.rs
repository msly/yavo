//! Planning reports must inspect all bodies and modules without changing compilation.
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};

mod common;

fn profile(files: &[(&str, &str)]) -> (i32, Value) {
    profile_at(files,"batch52")
}

fn profile_at(files: &[(&str, &str)], selected: &str) -> (i32, Value) {
    let scratch=common::Scratch::new("subset-features").unwrap();
    for (name,source) in files {
        let p=scratch.0.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p,source).unwrap();
    }
    let entry=scratch.0.join("main.yv");
    let before=fs::read(&entry).unwrap();
    let output=common::output(
        Command::new(env!("CARGO_BIN_EXE_yavo")).arg("subset-features").arg(&entry).args(["--profile", selected]).stdin(Stdio::null()),
        common::RUN_TIMEOUT).unwrap();
    assert_eq!(fs::read(entry).unwrap(),before,"analysis edited its source");
    let result=serde_json::from_slice(&output.stdout).unwrap_or_else(|e|panic!("{e}: {output:?}"));
    (output.status.code().unwrap(),result)
}

fn accepted(files:&[(&str,&str)]) -> Value {
    let (code,r)=profile(files);
    assert_eq!(code,0,"{r}");
    assert_eq!(r["ast_complete"],true);
    assert_eq!(r["typed_ir_complete"],true);
    assert_eq!(r["semantic_complete"],false,"planning must not certify future implementations");
    assert!(r["unknown"].as_array().unwrap().iter().any(|x|x=="flow_sensitive_selfhost_restrictions"));
    r
}

#[test]
fn inventories_later_functions_and_uninstantiated_generic_bodies() {
    let r=accepted(&[("main.yv",r#"
enum Shape { Circle(r: int), Empty }
fn generic[T](x: T) { var unused: Vec[(int, int)] = [] }
fn classify(s: Shape) -> int {
    return match s { .Circle(r) => r, .Empty => 0 }
}
fn main() { println(1) }
"#)]);
    let b=r["blockers"].as_object().unwrap();
    for feature in ["enum_payload","generics","match_user_enum","vec_copy_tuple"] {
        assert!(b.contains_key(feature),"missing {feature}: {r}");
    }
}

#[test]
fn loads_modules_and_distinguishes_equal_entry_bytes_with_different_dependencies() {
    let entry="use lib\nfn main() { lib.work() }\n";
    let a=accepted(&[("main.yv",entry),("lib.yv","pub fn work() { let xs: Vec[(int,int)] = []\n println(xs.len()) }\n")]);
    let b=accepted(&[("main.yv",entry),("lib.yv","pub fn work() { println(0) }\n")]);
    assert_eq!(a["sources"].as_array().unwrap().len(),2);
    assert_ne!(a["project_sha256"],b["project_sha256"]);
    assert!(a["blockers"]["vec_copy_tuple"].as_array().unwrap().iter().any(|x|x["file"].as_str().unwrap().ends_with("lib.yv")));
    assert!(b["blockers"].as_object().unwrap().is_empty(),"{b}");
}

#[test]
fn comments_and_strings_are_not_features() {
    let r=accepted(&[("main.yv",r#"
// Vec[(String,int)] enum Shape { Data(value: String) } scope spawn
fn main() { println("match generic float 0xAA sortBy") }
"#)]);
    assert!(r["blockers"].as_object().unwrap().is_empty(),"{r}");
}

#[test]
fn supported_numeric_helpers_and_set_construction_are_not_blockers() {
    let r=accepted(&[("main.yv",r#"
fn main() {
 var s = Set[String]{}
 s.add("one".toString())
 let x = int(byte(2))
 let y = 0xE2
 let optional: ?int = some(2)
 let empty = Vec[int]{}
 println(abs(min(x,max(1,2))))
}
"#)]);
    assert!(r["blockers"].as_object().unwrap().is_empty(),"{r}");
}

#[test]
fn detects_patterns_beyond_first_unsupported_declaration() {
    let r=accepted(&[("main.yv",r#"
enum Marker { First, Second }
fn choose(v: ?int) -> int {
 return match v { some(x) => x, none => 0 }
}
fn main() {
 var xs: Vec[(int,int)] = []
 println(choose(none))
}
"#)]);
    assert!(r["blockers"]["vec_copy_tuple"].is_array(),"{r}");
    assert!(r["blockers"]["patterns_optional_result"].is_array(),"{r}");
}

#[test]
fn standard_error_match_is_not_classified_as_user_enum_match() {
    let r=accepted(&[("main.yv",r#"
use std/fs
fn classify(e: fs.Error) -> int {
 return match e { .Io(n) => n, _ => 0 }
}
fn main() { println(classify(fs.Error.InvalidPath)) }
"#)]);
    assert!(!r["blockers"].as_object().unwrap().contains_key("match_user_enum"),"{r}");
    assert!(!r["blockers"].as_object().unwrap().contains_key("enum_payload"),"{r}");
}

#[test]
fn rejected_stage0_input_cannot_be_a_complete_profile() {
    let (code,r)=profile(&[("main.yv","fn main() { println(missing) }\n")]);
    assert_eq!(code,1);
    assert_eq!(r["stage0_accepted"],false);
    assert_eq!(r["ast_complete"],false);
    assert_eq!(r["unknown"],serde_json::json!(["stage0_rejected"]));
}


fn fixture(relative: &str) -> Value {
    fixture_at(relative,"batch52")
}

fn fixture_at(relative: &str, selected: &str) -> Value {
    let input=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../selfhost/tests").join(relative);
    let output=common::output(Command::new(env!("CARGO_BIN_EXE_yavo"))
        .arg("subset-features").arg(input).args(["--profile",selected]).stdin(Stdio::null()),common::RUN_TIMEOUT).unwrap();
    let report:Value=serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(),"{relative}: {report}");
    report
}

#[test]
fn supported_system_apis_are_not_missing_capabilities() {
    for path in ["run/system49_env.yv","run/system49_temp.yv","errors51/programs/success_file_roundtrip.yv"] {
        let r=fixture(path);
        assert!(r["blockers"].as_object().unwrap().is_empty(),"{path}: {r}");
    }
}

#[test]
fn standard_error_container_boundaries_are_detected() {
    for (suffix,feature) in [("vec","vec_standard_error"),("optional","optional_standard_error"),("tuple","tuple_general")] {
        for name in ["error","utf8"] {
            let path=format!("errors51/programs/unsupported_{name}_{suffix}.yv");
            let r=fixture(&path);
            assert!(r["blockers"][feature].is_array(),"{path}: missing {feature}: {r}");
        }
    }
}

#[test]
fn collection_method_boundaries_survive_typed_lowering() {
    for (path,feature) in [
        ("unsupported/sort45_byte.yv","sort_general"),
        ("unsupported/sort45_str.yv","sort_general"),
        ("unsupported/set47_iterate.yv","set_iteration"),
        ("unsupported/vec_str_result.yv","result_borrow"),
    ] {
        let r=fixture(path);
        assert!(r["blockers"][feature].is_array(),"{path}: missing {feature}: {r}");
    }
}

#[test]
fn heap_comparator_call_is_not_treated_as_supported_pure_expression() {
    let path="unsupported/batch48_heap_user_call_comparator.yv";
    let r=fixture(path);
    assert!(r["blockers"]["heap_comparator_expression"].is_array(),"{r}");
}

#[test]
fn batch53_patterns_remove_only_delivered_structural_flags() {
    for path in ["copy_fields","nested_product","nested_owned","optional_enum","optional_owned_string",
                 "optional_owned_enum","result_utf8","result_standard_nested","result_owned_error",
                 "vector_owned","scalar_bind","many_variants"] {
        let file=format!("patterns53/programs/{path}.yv");
        let before=fixture_at(&file,"batch52");
        let after=fixture_at(&file,"batch53");
        assert!(!before["blockers"].as_object().unwrap().is_empty(),"{path}: legacy baseline changed");
        assert!(after["blockers"].as_object().unwrap().is_empty(),"{path}: {after}");
        assert_eq!(after["semantic_complete"],false);
        assert_eq!(after["profile"],"selfhost-batch53-structural-v1");
    }
}

#[test]
fn batch53_keeps_unsupported_payload_layout_and_optional_composition() {
    let (code,r)=profile_at(&[("main.yv",r#"
enum E { Nested(n: ?int), End }
fn main() {
 let x=E.Nested(some(1))
 let y: ??int=some(some(1))
 match x { .Nested(_) => println(1), .End => println(0) }
}
"#)],"batch53");
    assert_eq!(code,0,"{r}");
    assert!(r["blockers"]["enum_payload_composition"].is_array(),"{r}");
    assert!(r["blockers"]["optional_general"].is_array(),"{r}");
    assert!(r["blockers"].get("enum_payload").is_none(),"{r}");
}

#[test]
fn batch53_does_not_certify_flow_sensitive_match_views() {
    let r=fixture_at("patterns53/programs/escaping_view.yv","batch53");
    assert_eq!(r["semantic_complete"],false);
    assert!(r["unknown"].as_array().unwrap().iter().any(|x|x=="flow_sensitive_selfhost_restrictions"));
}
