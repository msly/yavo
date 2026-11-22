//! Standard input, files and command-line arguments (stdlib contract §5): programs that need
//! stdin, arguments or a scratch directory, which the `tests/p0` runner does not provide.
//! Every program is also built with the heap-block check (`YAVO_ALLOC_CHECK=1`).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stdlib_io").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds `src` (with the heap-block check) and returns the executable.
fn build(dir: &PathBuf, src: &str) -> PathBuf {
    let file = dir.join("main.yv");
    std::fs::write(&file, src).unwrap();
    let exe = dir.join(format!("main{}", std::env::consts::EXE_SUFFIX));
    let out = Command::new(env!("CARGO_BIN_EXE_yavo"))
        .arg("build")
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .env("YAVO_ALLOC_CHECK", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "build failed:\n{}", String::from_utf8_lossy(&out.stderr));
    exe
}

fn run(exe: &PathBuf, args: &[&std::ffi::OsStr], stdin: &[u8]) -> Output {
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(exe.parent().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

const READ_ALL: &str = r#"use std/{io, os}

fn main() {
    let text = io.readAll() catch e {
        eprintln("read error: ${e}")
        os.exit(1)
    }
    var keys: Vec[String] = []
    var counts: Map[String, int] = [:]
    for w in text.fields() {
        let key = w.toLower()
        let n = counts.getCopy(key) ?? 0
        if n == 0 {
            keys.push(key.clone())
        }
        counts.set(key, n + 1)
    }
    keys.sort()
    for k in keys {
        let n = counts.getCopy(k) ?? 0
        println("${k} ${n}")
    }
}
"#;

#[test]
fn read_all_and_utf8_errors() {
    let dir = scratch("read_all");
    let exe = build(&dir, READ_ALL);
    let out = run(&exe, &[], b"The cat saw the dog\nthe END\n");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "cat 1\ndog 1\nend 1\nsaw 1\nthe 3\n");
    // Invalid UTF-8 is an error value with the byte offset of the bad sequence (S5).
    let out = run(&exe, &[], b"ok\n\xff\n");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stderr), "read error: Utf8(offset: 3)\n");
}

const READ_LINE: &str = r#"use std/io

fn main() {
    var n = 0
    loop {
        let line = io.readLine() catch e {
            println("bad input: ${e}")
            return
        }
        let text = line ?? break
        n += 1
        println("${n}: [${text}]")
    }
    println("lines ${n}")
}
"#;

#[test]
fn read_line_strips_newlines_and_reports_bad_lines() {
    let dir = scratch("read_line");
    let exe = build(&dir, READ_LINE);
    let out = run(&exe, &[], b"10\r\n 20 \n\nlast");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "1: [10]\n2: [ 20 ]\n3: []\n4: [last]\nlines 4\n");
    let out = run(&exe, &[], b"a\n\xc3(\nnot reached\n");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "1: [a]\nbad input: Utf8(offset: 0)\n");
}

const FILES: &str = r#"use std/fs

fn main() {
    var out = String{}
    out.push("line one\n")
    out.push(format("${2 + 1} things\n"))
    fs.writeText("out.txt", out) catch e {
        println("write failed: ${e}")
        return
    }
    let back = fs.readText("out.txt") catch _ => "missing".toString()
    print(back)
    match fs.readText("no-such-file") {
        ok(_) => println("unexpected")
        err(.Io(_)) => println("missing file is an Io error")
        err(e) => println("other: ${e}")
    }
    match fs.readText("bad.txt") {
        ok(_) => println("unexpected")
        err(fs.Error.Utf8(offset)) => println("bad byte at ${offset}")
        err(e) => println("other: ${e}")
    }
}
"#;

#[test]
fn files_round_trip_and_errors() {
    let dir = scratch("files");
    let exe = build(&dir, FILES);
    std::fs::write(dir.join("bad.txt"), b"abc\xe2\x82").unwrap();
    let out = run(&exe, &[], b"");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "line one\n3 things\nmissing file is an Io error\nbad byte at 3\n"
    );
    assert_eq!(std::fs::read(dir.join("out.txt")).unwrap(), b"line one\n3 things\n");
}

const ARGS: &str = r#"use std/os

fn count(args: []str) -> int {
    return args.len()
}

fn main() {
    let args = os.args()
    println("count ${count(args)}")
    for i, a in args {
        if i > 0 {
            println("${i}: ${a}")
        }
    }
}
"#;

#[test]
fn args_are_utf8_or_the_program_does_not_start() {
    let dir = scratch("args");
    let exe = build(&dir, ARGS);
    let out = run(&exe, &["one".as_ref(), "two words".as_ref(), "é".as_ref()], b"");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "count 4\n1: one\n2: two words\n3: é\n");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bad = std::ffi::OsStr::from_bytes(b"\xfe");
        let out = run(&exe, &["ok".as_ref(), bad], b"");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty(), "main must not run");
        assert!(text(&out.stderr).contains("YAVO-STARTUP-UTF8"), "{}", text(&out.stderr));
    }
}
