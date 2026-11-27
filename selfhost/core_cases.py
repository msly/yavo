"""Small semantic spine plus every changed fixture since the batch baseline."""
import json
from pathlib import Path, PurePosixPath
import subprocess

ROOT = Path(__file__).resolve().parents[1]

# These cases cover parsing, flow, ownership, layouts, cleanup, diagnostics and
# platform-sensitive output without rebuilding all independent applications.
CORE_IDS = """
arithmetic control recursion evaluation_order short_circuit strings integer_limits
newline_rules scopes unit_return while_condition struct_basic vector_basic
aggregate_copy aggregate_nested_forward aggregate_functions enum_nominal_context
owned_string_returns owned_string_utf8_nul owned_struct_move owned_nested_fields
owned_field_defaults owned_return_cleanup owned_loop_cleanup owned_branch_cleanup
owned_temporary_borrows owned_argument_order vec_clone_independence
vec_index_evaluation_order vec_control_flow_cleanup vec_loop_move_reinitialize
owned_vec_string_clone_lifetimes owned_vec_recursive_node owned_vec_literal_temporaries
model_original_source borrowed_index_token_scan borrowed_index_temporary_lifetimes
model_original_traversal byte_conversions byte_vector_lifecycle text_slice_owned_print
inout_params text_slices take_places std_catch_args string_view_binding result_forms
result_owned_string result_owned_vec result_multilevel_try result_argument_try_order
result_catch_return
overflow_add overflow_neg division_zero division_overflow shift_range assertion
vec_upper_index vec_assignment_bounds_order text_slice_boundary modules_panic_in_module
unknown_name immutable_assignment annotation_mismatch missing_return duplicate_function
integer_out_of_range negative_out_of_range unterminated_string invalid_escape
constructor_wrong_nominal recursive_struct_direct owned_use_after_move owned_field_move
owned_parameter_move owned_push_alias owned_branch_move_use owned_loop_move
unsupported_generic unsupported_enum_payload unsupported_optional_match
unsupported_result_vec_element unsupported_optional_field tuple43_pair_vec
tuple43_pair_len sort45_unknown_name
""".split()


def select(cases, baseline=None):
    policy = json.loads((ROOT / "tools/ci/validation-policy.json").read_text())
    baseline = baseline or policy["baseline_commit"]
    prior = json.loads(subprocess.check_output(
        ["git", "show", baseline + ":selfhost/tests/cases.json"], cwd=ROOT))
    previous = {case["id"]: case for case in prior["cases"]}
    changed = set(subprocess.check_output(
        ["git", "diff", "--name-only", baseline, "--", "selfhost"], cwd=ROOT,
        text=True).splitlines())
    by_id = {case["id"]: case for case in cases}
    missing = set(CORE_IDS) - by_id.keys()
    if missing:
        raise ValueError("core cases disappeared: " + ", ".join(sorted(missing)))
    reasons = {name: ["semantic spine"] for name in CORE_IDS}
    for case in cases:
        name = case["id"]
        why = reasons.setdefault(name, [])
        if previous.get(name) != case:
            why.append("new or changed expectation")
        if "selfhost/tests/" + case["file"] in changed:
            why.append("changed fixture")
        if case.get("source_prefix") and "selfhost/" + case["source_prefix"] in changed:
            why.append("changed source prefix")
        if case.get("example") and "selfhost/examples/" + case["example"] in changed:
            why.append("changed example")
        fixture = PurePosixPath(case["file"])
        parent = "selfhost/tests/" + fixture.parent.as_posix() + "/"
        if fixture.parts[0] == "modules" and any(p.startswith(parent) for p in changed):
            why.append("changed sibling module or fixture")
    selected = [case for case in cases if reasons[case["id"]]]
    return selected, {"baseline_commit": baseline, "available_cases": len(cases),
                      "selected_cases": len(selected),
                      "reasons": {c["id"]: reasons[c["id"]] for c in selected}}
