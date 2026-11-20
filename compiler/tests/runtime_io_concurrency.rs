//! Shared runtime I/O must be serialized across print, stdin and fatal paths.
//! Every build and executed program has a deadline; stdout/stderr are drained
//! concurrently so the stress fixture can exceed pipe-buffer capacity.

mod common;

#[allow(dead_code)]
#[path = "../src/toolchain.rs"]
mod toolchain;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        loop {
            let dir = std::env::temp_dir().join(format!(
                "yavo-runtime-io-{}-{}", std::process::id(), NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&dir) {
                Ok(()) => return Self(dir),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create {}: {e}", dir.display()),
            }
        }
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    fn exe(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(command: &mut Command) -> Output {
    common::output(command, common::RUN_TIMEOUT).unwrap_or_else(|e| panic!("{e}"))
}

fn build_c(scratch: &Scratch, release: bool) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tc = toolchain::find().expect("install the pinned C toolchain");
    let exe = scratch.exe(if release { "runtime-release" } else { "runtime-debug" });
    let mut command = Command::new(&tc.program);
    command.args(&tc.prefix_args).args(["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK"])
        .arg(if release { "-O2" } else { "-O0" })
        .arg("-I").arg(root.join("runtime"))
        .arg(root.join("tests/runtime_io_concurrency.c")).arg("-o").arg(&exe);
    let result = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    exe
}

#[test]
fn flushes_and_fatal_paths_respect_output_lock_and_are_reentrant() {
    let scratch = Scratch::new();
    let input = scratch.file("stdin.txt", "input\n");
    for release in [false, true] {
        let exe = build_c(&scratch, release);
        for (mode, code, stdout, stderr) in [
            ("panic", 101, "beforeafter", "yavo panic [P-PANIC] at io-lock.yv:41:9: other thread\n"),
            ("exit", 23, "beforeafter", ""),
            ("input", 0, "beforeafter", ""),
            ("nested-panic", 101, "nested", "yavo panic [P-PANIC] at io-lock.yv:31:7: nested panic\n"),
            ("nested-exit", 23, "nested", ""),
            ("nested-oom", 101, "nested", "yavo panic [P-OOM] at io-lock.yv:32:8: out of memory\n"),
            ("input-oom", 101, "", "yavo panic [P-OOM] at io-lock.yv:33:9: out of memory\n"),
            ("location", 101, "", "yavo panic [P-SYSTEM] at io-lock.yv:11:4: output write failed\n"),
        ] {
            let result = run(Command::new(&exe).arg(mode).stdin(Stdio::from(File::open(&input).unwrap())));
            assert_eq!(result.status.code(), Some(code), "release={release}, mode={mode}: {result:?}");
            assert_eq!(result.stdout, stdout.as_bytes(), "release={release}, mode={mode}");
            assert_eq!(result.stderr, stderr.as_bytes(), "release={release}, mode={mode}");
        }
    }
}

fn concurrent_io_source(read_all: bool) -> String {
    let source = r#"
use std/io

fn read_lines() {
    while let line = io.readLine() catch e { panic("${e}") } {
        println(line)
    }
}

fn read_rest() {
    let text = io.readAll() catch e { panic("${e}") }
    print(text)
}

fn write_records(id: int) {
    for i in 0..200 {
        let record = format("write:${id}:${i}")
        println(record)
    }
}

fn main() {
    scope {
        let _r0 = spawn read_lines()
        let _r1 = spawn read_lines()
        let _r2 = spawn read_lines()
        let _r3 = spawn read_lines()
        // READ_REST
        let _w0 = spawn write_records(0)
        let _w1 = spawn write_records(1)
        let _w2 = spawn write_records(2)
        let _w3 = spawn write_records(3)
    }
}
"#;
    source.replace("// READ_REST", if read_all { "let _rest = spawn read_rest()" } else { "" })
}

#[test]
fn concurrent_stdin_consumers_and_writers_preserve_every_record() {
    let scratch = Scratch::new();
    let mut input = String::new();
    let mut expected = Vec::new();
    // More than one input buffer, UTF-8, CRLF, an empty line, and a record larger
    // than either runtime buffer. Output must preserve the multiset exactly.
    for i in 0..2048 {
        let record = format!("input:{i}:中文:abcdefghijklmnopqrstuvwxyz");
        input.push_str(&record);
        input.push_str(if i % 2 == 0 { "\r\n" } else { "\n" });
        expected.push(record);
    }
    let long = format!("long:{}:終", "x".repeat(70000));
    input.push_str(&long);
    input.push_str("\n\n");
    expected.push(long);
    expected.push(String::new());
    for id in 0..4 {
        for i in 0..200 {
            expected.push(format!("write:{id}:{i}"));
        }
    }
    expected.sort();
    let input_path = scratch.file("records.txt", &input);
    for read_all in [false, true] {
        let source = scratch.file("readers.yv", &concurrent_io_source(read_all));
        for release in [false, true] {
            let exe = scratch.exe("readers");
            let mut command = Command::new(env!("CARGO_BIN_EXE_yavo"));
            command.arg("build").arg(&source).arg("-o").arg(&exe).env("YAVO_ALLOC_CHECK", "1");
            if release { command.arg("--release"); }
            let result = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
            let result = run(Command::new(&exe).stdin(Stdio::from(File::open(&input_path).unwrap())));
            assert_eq!(result.status.code(), Some(0), "read_all={read_all}, release={release}: {}", String::from_utf8_lossy(&result.stderr));
            assert!(result.stderr.is_empty(), "{}", String::from_utf8_lossy(&result.stderr));
            let stdout = String::from_utf8(result.stdout).expect("output must remain valid UTF-8");
            let mut records: Vec<String> = stdout.lines().map(str::to_owned).collect();
            records.sort();
            assert!(records == expected, "lost, duplicated or interleaved records: read_all={read_all}, release={release}, got {}, wanted {}", records.len(), expected.len());
        }
    }
}
