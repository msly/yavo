//! Runs every `tests/p0/run_*.yv` with `YAVO_ALLOC_CHECK=1`, which builds the program with
//! `-DYV_ALLOC_CHECK`: the runtime counts live heap blocks and exits with 102 on a leak (a
//! missed drop) or when more blocks are freed than allocated (a double drop). A double drop
//! can also be stopped first by the C library's own check; any non-zero exit fails this test.
//! Use-after-free is not detected by this check.

use std::path::Path;
use std::process::{Command, Stdio};

mod common;

fn check_all(release: bool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/p0");
    let mut programs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("run_") && n.ends_with(".yv")))
        .collect();
    programs.sort();
    assert!(!programs.is_empty());
    let mut failures = Vec::new();
    for p in &programs {
        let scratch = common::Scratch::new("alloc-check").unwrap();
        let exe = scratch.exe("fixture");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_yavo"));
        cmd.env("YAVO_ALLOC_CHECK", "1").arg("build").arg(p).arg("-o").arg(&exe).stdin(Stdio::null());
        if release {
            cmd.arg("--release");
        }
        match common::output(&mut cmd, common::BUILD_TIMEOUT) {
            Ok(out) if out.status.success() => {},
            Ok(out) => {
                failures.push(format!("{}: build exit {:?}\n{}", p.display(), out.status.code(),
                    String::from_utf8_lossy(&out.stderr)));
                continue;
            }
            Err(e) => { failures.push(format!("{}: build: {e}", p.display())); continue; }
        };
        let out = match common::output(Command::new(&exe).stdin(Stdio::null()), common::RUN_TIMEOUT) {
            Ok(out) => out,
            Err(e) => { failures.push(format!("{}: run: {e}", p.display())); continue; }
        };
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.code() != Some(0) || stderr.contains("alloc-check") {
            failures.push(format!("{}: exit {:?}\n{stderr}", p.display(), out.status.code()));
        }
    }
    assert!(failures.is_empty(), "heap check failed:\n{}", failures.join("\n"));
}

#[test]
fn no_leaks_or_double_drops_debug() {
    check_all(false);
}

#[test]
fn no_leaks_or_double_drops_release() {
    check_all(true);
}
