//! Public system APIs: real processes, identity, temp-directory races, and argv.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};
mod common;
#[allow(dead_code)]
#[path = "../src/toolchain.rs"]
mod toolchain;

const PROGRAM: &str = r#"use std/{os, fs, process}
fn work() {
    let a = os.args()
    if a[1] == "env" {
        match os.env(a[2], "fallback\0tail") {
            ok(value) => println("value:${value}")
            err(.Utf8(offset)) => println("utf8:${offset}")
            err(.Io(_)) => println("io")
            err(.InvalidArgument) => println("invalid")
        }
        println(os.host())
        return
    }
    if a[1] == "env-nul" {
        match os.env("bad\0name", "fallback") {
            err(.InvalidArgument) => println("invalid")
            _ => panic("expected invalid environment name")
        }
        return
    }
    if a[1] == "temp" {
        match fs.tempDir(a[2], a[3]) {
            ok(value) => println(value)
            err(.InvalidPath) => println("invalid")
            err(.Io(_)) => println("io")
            err(.Utf8(_)) => println("utf8")
        }
        return
    }
    if a[1] == "file" {
        match fs.removeFile(a[2]) {
            ok(_) => println("ok")
            err(.InvalidPath) => println("invalid")
            err(.Io(_)) => println("io")
            err(.Utf8(_)) => println("utf8")
        }
        return
    }
    if a[1] == "dir" {
        match fs.removeDir(a[2]) {
            ok(_) => println("ok")
            err(.InvalidPath) => println("invalid")
            err(.Io(_)) => println("io")
            err(.Utf8(_)) => println("utf8")
        }
        return
    }
    if a[1] == "same" {
        match fs.sameFile(a[2], a[3]) {
            ok(value) => println(value)
            err(.InvalidPath) => println("invalid")
            err(.Io(_)) => println("io")
            err(.Utf8(_)) => println("utf8")
        }
        return
    }
    if a[1] == "nul-paths" {
        let bad = "bad\0path"
        match fs.tempDir(bad, "yavo-") { err(.InvalidPath) => {}, _ => panic("temp") }
        match fs.removeFile(bad) { err(.InvalidPath) => {}, _ => panic("file") }
        match fs.removeDir(bad) { err(.InvalidPath) => {}, _ => panic("dir") }
        match fs.sameFile("missing", bad) { err(.InvalidPath) => {}, _ => panic("same") }
        println("invalid")
        return
    }
    if a[1] == "empty-paths" {
        let empty = String{}
        let same = fs.sameFile(empty, empty) catch _ => true
        println(same)
        return
    }
    var values: Vec[String] = []
    for i in 3..a.len() { values.push(a[i].toString()) }
    if a[1] == "nul-arg" { values.push("nul\0arg".toString()) }
    var program = a[2].toString()
    if a[1] == "nul-program" { program.push("\0suffix") }
    var repeats = 1
    if a[1] == "repeat" { repeats = 64 }
    for _i in 0..repeats {
        match process.run(program, values, a[1] == "diagnostics") {
            ok(value) => println("status:${value}")
            err(.InvalidArgument) => println("invalid")
            err(.Io(_)) => println("io")
            err(.Utf8(_)) => println("utf8")
        }
    }
}
fn main() { work() }
"#;

const HELPER: &str = r#"
#ifndef _WIN32
#define _POSIX_C_SOURCE 200809L
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#ifdef _WIN32
#include <windows.h>
#include <wchar.h>
#include <io.h>
#include <fcntl.h>
int wmain(int argc, wchar_t **argv) {
    /* The oracle writes exact bytes; do not let the CRT translate LF to CRLF. */
    if (_setmode(_fileno(stdout), _O_BINARY) == -1 ||
        _setmode(_fileno(stderr), _O_BINARY) == -1) return 97;
    if (argc > 2 && wcscmp(argv[1], L"slow") == 0) {
        HANDLE file = CreateFileW(argv[2], GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (file == INVALID_HANDLE_VALUE) return 98;
        CloseHandle(file); Sleep(400); printf("slow-child\n"); return 0;
    }
    if (argc > 1 && wcscmp(argv[1], L"fast") == 0) { printf("fast-child\n"); return 0; }
    if (argc > 1 && wcscmp(argv[1], L"exit37") == 0) return 37;
    if (argc > 1 && wcscmp(argv[1], L"exit127") == 0) return 127;
    if (argc > 1 && wcscmp(argv[1], L"wide-status") == 0) ExitProcess(0xf1234567u);
    if (argc > 1 && wcscmp(argv[1], L"stdin") == 0) {
        unsigned char buf[4096]; DWORD got;
        while (ReadFile(GetStdHandle(STD_INPUT_HANDLE), buf, sizeof buf, &got, NULL) && got)
            for (DWORD i = 0; i < got; i++) printf("%02x", buf[i]);
        printf("\n"); return 0;
    }
    for (int i = 1; i < argc; i++) {
        int n = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, argv[i], -1, NULL, 0, NULL, NULL);
        char *p = malloc(n);
        WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, argv[i], -1, p, n, NULL, NULL);
        printf("%d:", n - 1);
        for (int j = 0; j < n - 1; j++) printf("%02x", (unsigned char)p[j]);
        printf("\n"); free(p);
    }
#else
#include <time.h>
int main(int argc, char **argv) {
    if (argc > 2 && strcmp(argv[1], "slow") == 0) {
        FILE *file = fopen(argv[2], "wb"); if (file == NULL) return 98; fclose(file);
        struct timespec delay = {0, 400000000}; nanosleep(&delay, NULL);
        printf("slow-child\n"); return 0;
    }
    if (argc > 1 && strcmp(argv[1], "fast") == 0) { printf("fast-child\n"); return 0; }
    if (argc > 1 && strcmp(argv[1], "exit37") == 0) return 37;
    if (argc > 1 && strcmp(argv[1], "exit127") == 0) return 127;
    if (argc > 1 && strcmp(argv[1], "signal") == 0) { raise(SIGTERM); return 99; }
    if (argc > 1 && strcmp(argv[1], "stdin") == 0) {
        int c; while ((c = getchar()) != EOF) printf("%02x", (unsigned char)c);
        printf("\n"); return 0;
    }
    for (int i = 1; i < argc; i++) {
        size_t n = strlen(argv[i]); printf("%zu:", n);
        for (size_t j = 0; j < n; j++) printf("%02x", (unsigned char)argv[i][j]);
        printf("\n");
    }
#endif
    fflush(stdout); fprintf(stderr, "helper-stderr\n"); return 0;
}
"#;

fn build(scratch: &common::Scratch, release: bool) -> PathBuf {
    build_source(scratch, PROGRAM, release)
}
fn build_source(scratch: &common::Scratch, source_text: &str, release: bool) -> PathBuf {
    let source = scratch.0.join("system.yv"); fs::write(&source, source_text).unwrap();
    let output = scratch.exe("system");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yavo"));
    command.arg("build").arg(&source).arg("-o").arg(&output)
        .env("YAVO_ALLOC_CHECK", "1").stdin(Stdio::null());
    if release { command.arg("--release"); }
    let out = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
    assert!(out.status.success(), "build failed: {out:?}");
    output
}
fn helper(scratch: &common::Scratch) -> PathBuf {
    let source = scratch.0.join("helper.c"); fs::write(&source, HELPER).unwrap();
    let directory = scratch.0.join("native 汉 space"); fs::create_dir(&directory).unwrap();
    let output = directory.join(format!("child{}", std::env::consts::EXE_SUFFIX));
    let tc = toolchain::find_pinned().unwrap();
    let mut command = Command::new(&tc.program);
    command.args(tc.flags(false, None)).arg(&source).arg("-o").arg(&output).stdin(Stdio::null());
    if cfg!(windows) { command.arg("-municode"); }
    let out = common::output(&mut command, common::BUILD_TIMEOUT).unwrap();
    assert!(out.status.success(), "native helper: {out:?}");
    output
}
fn run(exe: &Path, args: &[&std::ffi::OsStr]) -> Output {
    let out = common::output(Command::new(exe).args(args).stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}"); out
}
fn expect(exe: &Path, args: &[&std::ffi::OsStr], text: &str) {
    let out = run(exe, args); assert_eq!(out.stdout, text.as_bytes(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
}

#[test]
fn environment_preserves_missing_empty_unicode_and_native_errors() {
    for release in [false, true] {
        let scratch = common::Scratch::new("system-env").unwrap(); let exe = build(&scratch, release);
        for value in [None, Some(""), Some("汉字 é 🙂"), Some("a=b=c")] {
            let mut command = Command::new(&exe);
            command.args(["env", "YAVO_SYSTEM49_VALUE"]).stdin(Stdio::null());
            match value { Some(value) => { command.env("YAVO_SYSTEM49_VALUE", value); }
                None => { command.env_remove("YAVO_SYSTEM49_VALUE"); } }
            let out = common::output(&mut command, common::RUN_TIMEOUT).unwrap();
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            let expected = format!("value:{}\n{}\n", value.unwrap_or("fallback\0tail"), toolchain::host_key());
            assert_eq!(out.stdout, expected.as_bytes()); assert!(out.stderr.is_empty(), "{out:?}");
        }
        for name in ["", "bad=name"] {
            expect(&exe, &["env".as_ref(), name.as_ref()], &format!("invalid\n{}\n", toolchain::host_key()));
        }
        expect(&exe, &["env-nul".as_ref()], "invalid\n");
        #[cfg(unix)] {
            use std::os::unix::ffi::OsStrExt;
            let out = common::output(Command::new(&exe).args(["env", "YAVO_SYSTEM49_VALUE"])
                .env("YAVO_SYSTEM49_VALUE", std::ffi::OsStr::from_bytes(b"ab\xff")).stdin(Stdio::null()),
                common::RUN_TIMEOUT).unwrap();
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            assert_eq!(out.stdout, format!("utf8:2\n{}\n", toolchain::host_key()).as_bytes());
            assert!(out.stderr.is_empty());
        }
    }
}

#[test]
fn temporary_directories_are_atomic_unique_and_only_explicitly_removed() {
    for release in [false, true] {
        let scratch = common::Scratch::new("system-temp").unwrap(); let exe = build(&scratch, release);
        let parent = scratch.0.join("parent 汉 space"); fs::create_dir(&parent).unwrap();
        let barrier = Arc::new(Barrier::new(8)); let mut workers = Vec::new();
        for _ in 0..8 {
            let exe = exe.clone(); let parent = parent.clone(); let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                let out = run(&exe, &["temp".as_ref(), parent.as_os_str(), "prefix-".as_ref()]);
                assert!(out.stderr.is_empty(), "{out:?}");
                PathBuf::from(String::from_utf8(out.stdout).unwrap().trim_end())
            }));
        }
        let dirs: Vec<PathBuf> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        let unique: std::collections::HashSet<_> = dirs.iter().collect(); assert_eq!(unique.len(), 8);
        for dir in dirs {
            assert_eq!(dir.parent(), Some(parent.as_path())); assert!(dir.is_dir());
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
            }
            let file = dir.join("data.txt"); fs::write(&file, b"keep").unwrap();
            expect(&exe, &["dir".as_ref(), dir.as_os_str()], "io\n"); assert_eq!(fs::read(&file).unwrap(), b"keep");
            expect(&exe, &["file".as_ref(), dir.as_os_str()], "io\n");
            expect(&exe, &["file".as_ref(), file.as_os_str()], "ok\n");
            expect(&exe, &["file".as_ref(), file.as_os_str()], "io\n");
            expect(&exe, &["dir".as_ref(), dir.as_os_str()], "ok\n"); assert!(!dir.exists());
            expect(&exe, &["dir".as_ref(), dir.as_os_str()], "io\n");
        }
        for prefix in ["", "../escape", "space prefix", "汉"] {
            expect(&exe, &["temp".as_ref(), parent.as_os_str(), prefix.as_ref()], "invalid\n");
        }
        expect(&exe, &["temp".as_ref(), "".as_ref(), "x".as_ref()], "invalid\n");
        expect(&exe, &["temp".as_ref(), parent.join("absent").as_os_str(), "x".as_ref()], "io\n");
        expect(&exe, &["nul-paths".as_ref()], "invalid\n");
    }
}

#[test]
fn identity_follows_aliases_and_distinguishes_missing_from_failures() {
    for release in [false, true] {
        let scratch = common::Scratch::new("system-identity").unwrap(); let exe = build(&scratch, release);
        let first = scratch.0.join("file 汉.txt"); let second = scratch.0.join("other.txt"); let hard = scratch.0.join("hard.txt");
        fs::write(&first, b"same").unwrap(); fs::write(&second, b"same").unwrap(); fs::hard_link(&first, &hard).unwrap();
        expect(&exe, &["same".as_ref(), first.as_os_str(), first.as_os_str()], "true\n");
        expect(&exe, &["same".as_ref(), first.as_os_str(), hard.as_os_str()], "true\n");
        expect(&exe, &["same".as_ref(), first.as_os_str(), second.as_os_str()], "false\n");
        expect(&exe, &["same".as_ref(), first.as_os_str(), scratch.0.join("absent").as_os_str()], "false\n");
        expect(&exe, &["same".as_ref(), first.join("not-dir").as_os_str(), first.as_os_str()], "false\n");
        expect(&exe, &["same".as_ref(), scratch.0.as_os_str(), scratch.0.join(".").as_os_str()], "true\n");
        expect(&exe, &["empty-paths".as_ref()], "false\n");
        #[cfg(unix)] {
            use std::os::unix::fs::symlink;
            let link = scratch.0.join("link.txt"); symlink(&first, &link).unwrap();
            expect(&exe, &["same".as_ref(), first.as_os_str(), link.as_os_str()], "true\n");
            expect(&exe, &["file".as_ref(), link.as_os_str()], "ok\n"); assert_eq!(fs::read(&first).unwrap(), b"same");
            let cycle = scratch.0.join("cycle"); symlink(&cycle, &cycle).unwrap();
            expect(&exe, &["same".as_ref(), cycle.as_os_str(), first.as_os_str()], "io\n");
        }
    }
}

#[test]
fn process_argv_stdio_status_and_launch_errors_are_distinct() {
    for release in [false, true] {
        let scratch = common::Scratch::new("system-process").unwrap(); let exe = build(&scratch, release); let child = helper(&scratch);
        let values = ["", "a b", "tab\there", "\"", "\\", "tail\\", "x\\\"y", "汉字🙂", ";$(touch no-shell)", "a\nb"];
        let mut args = vec!["run".as_ref(), child.as_os_str()]; args.extend(values.iter().map(std::ffi::OsStr::new));
        let out = run(&exe, &args);
        let lines: String = values.iter().map(|s| format!("{}:{}\n", s.len(),
            s.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>())).collect();
        assert_eq!(out.stdout, format!("{lines}status:0\n").as_bytes(), "{out:?}"); assert_eq!(out.stderr, b"helper-stderr\n");
        args[0] = "diagnostics".as_ref(); let out = run(&exe, &args); assert_eq!(out.stdout, b"status:0\n");
        assert_eq!(out.stderr, format!("{lines}helper-stderr\n").as_bytes(), "{out:?}");
        for (mode, expected) in [("exit37", "status:37\n"), ("exit127", "status:127\n")] {
            expect(&exe, &["run".as_ref(), child.as_os_str(), mode.as_ref()], expected);
        }
        #[cfg(unix)] expect(&exe, &["run".as_ref(), child.as_os_str(), "signal".as_ref()], "status:-15\n");
        #[cfg(windows)] expect(&exe, &["run".as_ref(), child.as_os_str(), "wide-status".as_ref()], "status:4045620583\n");
        expect(&exe, &["nul-arg".as_ref(), child.as_os_str()], "invalid\n");
        expect(&exe, &["nul-program".as_ref(), child.as_os_str()], "invalid\n");
        expect(&exe, &["run".as_ref(), "".as_ref()], "invalid\n");
        let absent = scratch.0.join("not-installed"); expect(&exe, &["repeat".as_ref(), absent.as_os_str()], &"io\n".repeat(64));
        let mut command = Command::new(&exe);
        command.arg("run").arg(child.file_name().unwrap()).env("PATH", child.parent().unwrap()).stdin(Stdio::null());
        let out = common::output(&mut command, common::RUN_TIMEOUT).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}"); assert_eq!(out.stdout, b"io\n"); assert!(out.stderr.is_empty());
        let input = scratch.0.join("stdin.dat"); fs::write(&input, b"a\0b\n").unwrap();
        let out = common::output(Command::new(&exe).arg("run").arg(&child).arg("stdin")
            .stdin(Stdio::from(fs::File::open(input).unwrap())), common::RUN_TIMEOUT).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}"); assert_eq!(out.stdout, b"6100620a\nstatus:0\n"); assert!(out.stderr.is_empty());
    }
}

#[test]
fn concurrent_process_launch_does_not_hold_a_global_wait_lock() {
    const SOURCE: &str = r#"use std/{os, fs, process}
fn invoke(executable: str, values: []String) -> int {
    return process.run(executable, values, false) catch _ => -999
}
fn main() {
    let a = os.args()
    let slow = ["slow".toString(), a[2].toString()]
    let fast = ["fast".toString()]
    scope {
        let task = spawn invoke(a[1], slow)
        loop {
            let _ = fs.readText(a[2]) catch _ { continue }
            break
        }
        let fastCode = invoke(a[1], fast)
        assert(fastCode == 0)
        let slowCode = task.join()
        assert(slowCode == 0)
    }
    println("joined")
}
"#;
    for release in [false, true] {
        let scratch = common::Scratch::new("system-concurrent").unwrap();
        let exe = build_source(&scratch, SOURCE, release);
        let child = helper(&scratch);
        let marker = scratch.0.join("ready.txt");
        expect(&exe, &[child.as_os_str(), marker.as_os_str()], "fast-child\nslow-child\njoined\n");
    }
}

#[test]
fn system_signatures_keep_borrows_until_all_arguments_are_evaluated() {
    let cases = [
        ("missing-module", "fn main() { let _ = process.run(\"x\", [], false) }", "E0010"),
        ("wrong-arity", "use std/process\nfn main() { let _ = process.run(\"x\") }", "E0012"),
        ("wrong-arg-type", "use std/process\nfn main() { var a: Vec[int] = []\nlet _ = process.run(\"x\", a, false) }", "E0001"),
        ("borrowed-mutation", r#"use std/process
fn alter(a: inout Vec[String]) -> bool { a.push("x".toString())
return false }
fn main() { var a: Vec[String] = []
let _ = process.run("x", a, alter(&a)) }"#, "E0102"),
        ("env-name-mutation", r#"use std/os
fn alter(a: inout String) -> str of static { a.push("x")
return "fallback" }
fn main() { var a = "NAME".toString()
let _ = os.env(a, alter(&a)) }"#, "E0102"),
    ];
    for (name, source, code) in cases {
        let scratch = common::Scratch::new(name).unwrap();
        let source_path = scratch.0.join("main.yv"); fs::write(&source_path, source).unwrap();
        let out = common::output(Command::new(env!("CARGO_BIN_EXE_yavo")).arg("check").arg(&source_path).arg("--json")
            .stdin(Stdio::null()), common::RUN_TIMEOUT).unwrap();
        assert!(!out.status.success(), "{name}: {out:?}");
        let stdout = String::from_utf8(out.stdout).unwrap(); assert!(stdout.contains(code), "{name}: {stdout}");
    }
}
