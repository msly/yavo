//! The erased saved comparator must remain visible to allocation queries.
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};

mod common;

#[test]
fn stored_comparator_and_capture_cleanup_remain_unknown() {
    let scratch = common::Scratch::new("collection-effects").unwrap();
    let path = scratch.0.join("effects.yv");
    fs::write(&path, r#"
use std/collections
fn empty() -> Deque[int] { return Deque[int]{} }
fn size(h: Heap[int]) -> int { return h.len() }
fn pop(h: inout Heap[int]) -> ?int { return h.pop() }
fn push(h: inout Heap[int], n: int) { h.push(n) }
fn destroy(h: sink Heap[int]) { let _ = h }
fn create(s: sink String) -> Heap[int] {
    return Heap[int].by((a, b) => a + s.len() < b)
}
fn queuePop(q: inout Deque[int]) -> ?int { return q.popFront() }
fn main() {}
"#).unwrap();
    let cases = [
        ("empty", "no_alloc", false),
        ("size", "no_alloc", false),
        ("pop", "unknown", true),
        ("push", "may_alloc", true),
        ("destroy", "unknown", true),
        ("create", "may_alloc", false),
        ("queuePop", "no_alloc", false),
    ];
    for (subject, expected, unknown) in cases {
        let out = common::output(Command::new(env!("CARGO_BIN_EXE_yavo"))
            .args(["allocations", subject, "--file"]).arg(&path).arg("--json")
            .stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert!(out.status.success(), "{subject}: {}", String::from_utf8_lossy(&out.stderr));
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["status"], expected, "{subject}: {report:#}");
        assert_eq!(report["has_unknown"], unknown, "{subject}: {report:#}");
    }
}
