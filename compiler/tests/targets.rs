//! `yavo build --target`: every supported target must produce a binary of the right format and
//! architecture, Linux targets must be statically linked, and a target this machine can execute must
//! also run correctly. Windows and macOS binaries are only checked structurally here; running them
//! needs those machines (or CI runners).

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

const TARGETS: &[(&str, &str, &str)] = &[
    ("x86_64-linux-musl", "linux", "x86_64"),
    ("aarch64-linux-musl", "linux", "aarch64"),
    ("x86_64-windows-gnu", "windows", "x86_64"),
    ("aarch64-windows-gnu", "windows", "aarch64"),
    ("x86_64-macos", "macos", "x86_64"),
    ("aarch64-macos", "macos", "aarch64"),
];

fn yavo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yavo"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/p0").join(name)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("targets-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// (os, arch) read from the executable header: ELF, Mach-O 64 or PE.
fn format_of(b: &[u8]) -> Option<(&'static str, &'static str)> {
    if b.len() < 64 {
        return None;
    }
    if b.starts_with(b"\x7fELF") {
        return match u16le(b, 18) {
            0x3e => Some(("linux", "x86_64")),
            0xb7 => Some(("linux", "aarch64")),
            _ => None,
        };
    }
    if b.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
        return match u32le(b, 4) {
            0x0100_0007 => Some(("macos", "x86_64")),
            0x0100_000c => Some(("macos", "aarch64")),
            _ => None,
        };
    }
    if b.starts_with(b"MZ") {
        let pe = u32le(b, 0x3c) as usize;
        if b.len() < pe + 6 || &b[pe..pe + 4] != b"PE\0\0" {
            return None;
        }
        return match u16le(b, pe + 4) {
            0x8664 => Some(("windows", "x86_64")),
            0xaa64 => Some(("windows", "aarch64")),
            _ => None,
        };
    }
    None
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn expected_stdout(src: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in src.lines() {
        let Some(c) = line.strip_prefix("//") else { break };
        let c = c.strip_prefix(' ').unwrap_or(c);
        if c == "expect-stdout:" {
            inside = true;
        } else if inside {
            match c.strip_prefix('|') {
                Some(t) => {
                    out.push_str(t.strip_prefix(' ').unwrap_or(t));
                    out.push('\n');
                }
                None => inside = false,
            }
        }
    }
    out
}

#[test]
fn every_target_produces_the_right_binary_format() {
    let dir = scratch("format");
    for (name, os, arch) in TARGETS {
        let out = dir.join(name);
        let res = yavo()
            .arg("build")
            .arg(fixture("run_arith.yv"))
            .args(["--release", "--target", name, "-o"])
            .arg(&out)
            .output()
            .unwrap();
        assert!(res.status.success(), "{name}: build failed\n{}", String::from_utf8_lossy(&res.stderr));
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(format_of(&bytes), Some((*os, *arch)), "{name}: wrong executable format");
        if *os == "linux" {
            assert!(
                !contains(&bytes, b"ld-linux") && !contains(&bytes, b"ld-musl"),
                "{name}: expected a static binary without a dynamic loader"
            );
        }
    }
}

#[test]
fn host_runnable_target_runs_correctly() {
    let host = TARGETS
        .iter()
        .find(|(_, os, arch)| *os == std::env::consts::OS && *arch == std::env::consts::ARCH && *os == "linux");
    let Some((name, _, _)) = host else {
        eprintln!("no statically linked target runs on this host; skipped");
        return;
    };
    let path = fixture("run_arith.yv");
    let want = expected_stdout(&std::fs::read_to_string(&path).unwrap());
    let res = yavo().arg("run").arg(&path).args(["--target", name]).output().unwrap();
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    assert_eq!(String::from_utf8_lossy(&res.stdout), want);
}

#[test]
fn collections_and_mutable_returns_compile_for_all_targets() {
    let scratch = common::Scratch::new("completion-targets").unwrap();
    let path = fixture("run_collection_returns.yv");
    let want = expected_stdout(&std::fs::read_to_string(&path).unwrap());
    for (name, os, arch) in TARGETS {
        let artifact = scratch.0.join(format!("{name}{}", if *os == "windows" { ".exe" } else { "" }));
        let built = common::output(yavo().arg("build").arg(&path)
            .args(["--release", "--target", name, "-o"]).arg(&artifact), common::BUILD_TIMEOUT).unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        assert!(built.stderr.is_empty(), "{name}: unexpected compiler diagnostics: {}", String::from_utf8_lossy(&built.stderr));
        let bytes = std::fs::read(&artifact).unwrap();
        assert_eq!(format_of(&bytes), Some((*os, *arch)), "{name}: wrong executable format");
        if *os == "linux" {
            assert!(!contains(&bytes, b"ld-linux") && !contains(&bytes, b"ld-musl"), "{name}: expected static linking");
        }
        if *os == std::env::consts::OS && *arch == std::env::consts::ARCH {
            let run = common::output(&mut Command::new(&artifact), common::RUN_TIMEOUT).unwrap();
            assert_eq!(run.status.code(), Some(0), "{name}: {}", String::from_utf8_lossy(&run.stderr));
            assert!(run.stderr.is_empty(), "{name}: {}", String::from_utf8_lossy(&run.stderr));
            assert_eq!(run.stdout, want.as_bytes(), "{name}");
        }
    }
}

#[test]
fn target_errors_and_default_output_name() {
    let path = fixture("run_arith.yv");

    let res = yavo().arg("build").arg(&path).args(["--target", "riscv64-linux"]).output().unwrap();
    assert_eq!(res.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&res.stderr).contains("supported targets"));

    let foreign = TARGETS.iter().find(|(_, os, _)| *os != std::env::consts::OS).unwrap().0;
    let res = yavo().arg("run").arg(&path).args(["--target", foreign]).output().unwrap();
    assert_eq!(res.status.code(), Some(2), "running a foreign target must be a usage error");

    let dir = scratch("yavo-cc");
    let res = yavo()
        .env("YAVO_CC", "cc")
        .arg("build")
        .arg(&path)
        .args(["--target", "x86_64-linux-musl", "-o"])
        .arg(dir.join("x"))
        .output()
        .unwrap();
    assert_eq!(res.status.code(), Some(3), "cross-compiling must require the pinned zig toolchain");

    let dir = scratch("default-name");
    let res =
        yavo().current_dir(&dir).arg("build").arg(&path).args(["--target", "x86_64-windows-gnu"]).output().unwrap();
    assert!(res.status.success(), "{}", String::from_utf8_lossy(&res.stderr));
    assert!(dir.join("run_arith.exe").is_file(), "a Windows target defaults to a `.exe` output name");
}
