//! Exclusive file creation must protect existing source files through path aliases.
//! All generated programs run with heap-block checking in Debug and Release modes.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};

mod common;

const PROGRAM: &str = r#"use std/{fs, os}

fn attempt(path: str, mode: str) {
    var ownedPath = path.toString()
    if mode == "nul-path" { ownedPath.push("\0ignored") }
    var text = String{}
    if mode != "empty" {
        text.push("汉🙂\0tail\n")
        text.push(mode)
    }
    match fs.writeNewText(ownedPath, text) {
        ok(_) => println("ok")
        err(.Io(code)) => {
            assert(code != 0)
            println("io")
        }
        err(.InvalidPath) => println("invalid-path")
        err(_) => panic("unexpected file error")
    }
}

fn main() {
    let args = os.args()
    assert(args.len() == 3)
    var times = 1
    if args[2] == "repeat-error" { times = 64 }
    for _i in 0..times { attempt(args[1], args[2]) }
}
"#;

fn build(scratch: &common::Scratch, release: bool) -> PathBuf {
    let source = scratch.0.join("source.yv");
    fs::write(&source, PROGRAM).unwrap();
    let exe = scratch.exe("write-new-text");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yavo"));
    command.arg("build").arg(source).arg("-o").arg(&exe)
        .env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
    if release { command.arg("--release"); }
    let out = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    exe
}

fn run(exe: &Path, path: &Path, mode: &str) -> Output {
    let out = common::output(
        Command::new(exe).arg(path).arg(mode).stdin(Stdio::null()),
        common::RUN_TIMEOUT,
    ).unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty(), "allocation/runtime error: {out:?}");
    out
}

fn expect(exe: &Path, path: &Path, mode: &str, stdout: &str) {
    let out = run(exe, path, mode);
    assert_eq!(out.stdout, stdout.as_bytes(), "{out:?}");
}

fn content(mode: &str) -> Vec<u8> {
    format!("汉🙂\0tail\n{mode}").into_bytes()
}

#[test]
fn creates_utf8_and_nul_text_and_cleans_up_error_paths() {
    for release in [false, true] {
        let scratch = common::Scratch::new("fs-create-new-values").unwrap();
        let exe = build(&scratch, release);
        let path = scratch.0.join("新文件-é.txt");
        expect(&exe, &path, "created", "ok\n");
        assert_eq!(fs::read(&path).unwrap(), content("created"));

        // Existing files are never truncated, including across repeated failures.
        expect(&exe, &path, "replacement", "io\n");
        expect(&exe, &path, "repeat-error", &"io\n".repeat(64));
        assert_eq!(fs::read(&path).unwrap(), content("created"));

        let empty = scratch.0.join("empty.txt");
        expect(&exe, &empty, "empty", "ok\n");
        assert!(fs::read(empty).unwrap().is_empty());
        expect(&exe, &scratch.0, "directory", "io\n");

        let missing = scratch.0.join("missing-parent/file.txt");
        expect(&exe, &missing, "missing", "io\n");
        assert!(!missing.parent().unwrap().exists());

        // An embedded NUL is InvalidPath; it must not create the truncated prefix.
        let invalid = scratch.0.join("nul-prefix.txt");
        expect(&exe, &invalid, "nul-path", "invalid-path\n");
        assert!(!invalid.exists());
    }
}

#[test]
fn existing_source_and_aliases_are_preserved() {
    for release in [false, true] {
        let scratch = common::Scratch::new("fs-create-new-aliases").unwrap();
        let exe = build(&scratch, release);
        let source = scratch.0.join("source.yv");
        expect(&exe, &source, "direct", "io\n");
        fs::create_dir(scratch.0.join("sub")).unwrap();
        expect(&exe, &scratch.0.join("sub/../source.yv"), "dot-dot", "io\n");
        expect(&exe, &scratch.0.join("./source.yv"), "dot", "io\n");

        let hard = scratch.0.join("hard-link.yv");
        fs::hard_link(&source, &hard).unwrap();
        expect(&exe, &hard, "hard-link", "io\n");
        assert_eq!(fs::read(&hard).unwrap(), PROGRAM.as_bytes());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = scratch.0.join("symbolic-link.yv");
            symlink(&source, &link).unwrap();
            expect(&exe, &link, "symlink", "io\n");
            assert_eq!(fs::read_link(&link).unwrap(), source);
            assert_eq!(fs::read(&link).unwrap(), PROGRAM.as_bytes());

            // A dangling symlink is an existing directory entry too.
            let absent = scratch.0.join("absent-target.yv");
            let dangling = scratch.0.join("dangling-link.yv");
            symlink(&absent, &dangling).unwrap();
            expect(&exe, &dangling, "dangling", "io\n");
            assert_eq!(fs::read_link(&dangling).unwrap(), absent);
            assert!(!absent.exists());

            let parent_alias = scratch.0.join("parent-link");
            symlink(&scratch.0, &parent_alias).unwrap();
            expect(&exe, &parent_alias.join("source.yv"), "parent-symlink", "io\n");
        }
        assert_eq!(fs::read(source).unwrap(), PROGRAM.as_bytes());
    }
}

#[test]
fn concurrent_creators_have_exactly_one_winner() {
    for release in [false, true] {
        let scratch = common::Scratch::new("fs-create-new-race").unwrap();
        let exe = build(&scratch, release);
        let path = scratch.0.join("shared.txt");
        let barrier = Arc::new(Barrier::new(8));
        let mut writers = Vec::new();
        for i in 0..8 {
            let exe = exe.clone();
            let path = path.clone();
            let barrier = barrier.clone();
            writers.push(std::thread::spawn(move || {
                let mode = format!("writer-{i}");
                barrier.wait();
                let out = run(&exe, &path, &mode);
                (mode, out.stdout)
            }));
        }
        let mut winners = Vec::new();
        for writer in writers {
            let (mode, stdout) = writer.join().unwrap();
            if stdout == b"ok\n" {
                winners.push(mode);
            } else {
                assert_eq!(stdout, b"io\n");
            }
        }
        assert_eq!(winners.len(), 1, "{winners:?}");
        assert_eq!(fs::read(path).unwrap(), content(&winners[0]));
    }
}
