//! Regression tests for toolchain diagnostics and runtime output errors.
//! Compiler child processes receive per-command environment changes, never global ones.

#[allow(dead_code)]
#[path = "../src/toolchain.rs"]
mod toolchain;

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "yavo-p0-regression-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create test directory {}: {e}", path.display()),
            }
        }
    }

    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).unwrap();
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

fn captured(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

fn read_pipe<T: Read + Send + 'static>(pipe: Option<T>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).unwrap();
        }
        bytes
    })
}

fn run(command: &mut Command) -> Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().unwrap();
    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            #[cfg(windows)]
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            let _ = child.kill();
            let _ = child.wait();
            panic!("test child exceeded 60 seconds: {command:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    Output { status, stdout: stdout.join().unwrap(), stderr: stderr.join().unwrap() }
}

fn build_yavo(scratch: &Scratch, name: &str, source: &str, release: bool) -> PathBuf {
    let input = scratch.file(&format!("{name}.yv"), source);
    let exe = scratch.exe(name);
    let mut command = captured(env!("CARGO_BIN_EXE_yavo"));
    command.arg("build").arg(input).arg("-o").arg(&exe);
    if release {
        command.arg("--release");
    }
    let output = run(&mut command);
    assert!(output.status.success(), "build failed: {}", String::from_utf8_lossy(&output.stderr));
    exe
}

fn build_c(scratch: &Scratch, name: &str, source: &str, release: bool) -> PathBuf {
    let input = scratch.file(&format!("{name}.c"), source);
    let exe = scratch.exe(name);
    toolchain::find().expect("install the pinned C toolchain")
        .compile(&input, &scratch.0, &exe, release).expect("build C regression helper");
    exe
}

fn read_only(scratch: &Scratch) -> File {
    let path = scratch.file("read-only.txt", "not writable through this handle");
    File::open(path).unwrap()
}

#[test]
fn c_compiler_diagnostics_are_visible_on_success_and_failure() {
    let scratch = Scratch::new();
    let mock = build_c(&scratch, "mock_cc", r#"
#include <stdio.h>
#include <stdlib.h>
int main(void) {
    fputs("warning: YAVO_MOCK_STDERR\n", stderr);
    fputs("note: YAVO_MOCK_STDOUT\n", stdout);
    const char *status = getenv("YAVO_MOCK_CC_EXIT");
    return status ? atoi(status) : 0;
}
"#, false);
    let source = scratch.file("main.yv", "fn main() {\n    println(7)\n}\n");
    // Work around the existing YAVO_CC whitespace parser: executable name is relative,
    // while current_dir may contain spaces. This test does not change that CLI contract.
    let cc = format!("./{}", mock.file_name().unwrap().to_str().unwrap());
    for cc_status in ["0", "9"] {
        let mut command = captured(env!("CARGO_BIN_EXE_yavo"));
        command.current_dir(&scratch.0)
            .env("YAVO_CC", &cc).env("YAVO_MOCK_CC_EXIT", cc_status)
            .arg("build").arg(&source).arg("-o").arg(scratch.exe("unused"))
            .arg("--json-diagnostics");
        let output = run(&mut command);
        assert_eq!(output.status.code(), Some(if cc_status == "0" { 0 } else { 4 }));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .expect("toolchain output must not corrupt frontend diagnostic JSON");
        assert_eq!(json["error_count"], 0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.matches("YAVO_MOCK_STDERR").count(), 1, "{stderr}");
        assert_eq!(stderr.matches("YAVO_MOCK_STDOUT").count(), 1, "{stderr}");
    }
}

fn stdout_failure_case(source: &str) {
    let scratch = Scratch::new();
    for release in [false, true] {
        let exe = build_yavo(&scratch, if release { "release" } else { "debug" }, source, release);
        let mut command = captured(&exe);
        command.stdout(Stdio::from(read_only(&scratch)));
        let output = run(&mut command);
        assert_eq!(output.status.code(), Some(101), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("yavo panic [P-SYSTEM] at "), "{stderr}");
        assert!(stderr.contains(".yv:2:5: "), "buffered failure must retain its write location: {stderr}");
        assert!(stderr.contains("output write failed"), "{stderr}");
    }
}

#[test]
fn buffered_stdout_failure_is_not_success() {
    stdout_failure_case("fn main() {\n    println(\"buffered\")\n}\n");
}

#[test]
fn direct_stdout_failure_is_not_success() {
    stdout_failure_case(&format!("fn main() {{\n    print(\"{}\")\n}}\n", "x".repeat(9000)));
}

#[test]
fn stdout_flush_before_stderr_detects_failure() {
    stdout_failure_case("fn main() {\n    print(\"buffered\")\n    eprintln(\"later\")\n}\n");
}

#[test]
fn stderr_failure_terminates_without_recursive_reporting() {
    let scratch = Scratch::new();
    for release in [false, true] {
        let exe = build_yavo(&scratch, if release { "release" } else { "debug" },
            "fn main() {\n    print(\"before\")\n    eprintln(\"error\")\n}\n", release);
        let mut command = captured(&exe);
        command.stderr(Stdio::from(read_only(&scratch)));
        let output = run(&mut command);
        assert_eq!(output.status.code(), Some(101));
        assert_eq!(output.stdout, b"before");

        let mut command = captured(&exe);
        command.stdout(Stdio::from(read_only(&scratch))).stderr(Stdio::from(read_only(&scratch)));
        assert_eq!(run(&mut command).status.code(), Some(101));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn full_device_failure_is_not_success() {
    let scratch = Scratch::new();
    for release in [false, true] {
        let exe = build_yavo(&scratch, if release { "release" } else { "debug" },
            "fn main() {\n    println(\"must be written\")\n}\n", release);
        let mut command = captured(&exe);
        command.stdout(Stdio::from(File::options().write(true).open("/dev/full").unwrap()));
        let output = run(&mut command);
        assert_eq!(output.status.code(), Some(101));
        assert!(String::from_utf8_lossy(&output.stderr).contains("[P-SYSTEM]"));
    }
}

#[cfg(unix)]
#[test]
fn runtime_retries_eintr_and_partial_writes_but_rejects_zero_writes() {
    let scratch = Scratch::new();
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/yavo_rt.h"),
        scratch.0.join("yavo_rt.h")).unwrap();
    let source = r#"
#include <unistd.h>
#include <errno.h>
#include <stdlib.h>
static int mode = 1;
static int calls = 0;
static ssize_t controlled_write(int fd, const void *buf, size_t n) {
    if (fd == 1) {
        if (mode == 2) return 0;
        if (calls++ == 0) { errno = EINTR; return -1; }
        if (n > 1) n = 1;
    }
    return write(fd, buf, n);
}
#define write controlled_write
#include "yavo_rt.h"
#undef write
int main(int argc, char **argv) {
    mode = argc > 1 ? atoi(argv[1]) : 1;
    yv_rt_init("runtime_io_probe.yv");
    yv_write(1, "ok\n", 3);
    return yv_rt_exit();
}
"#;
    for release in [false, true] {
        let exe = build_c(&scratch, if release { "release" } else { "debug" }, source, release);
        let output = run(captured(&exe).arg("1"));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"ok\n");
        let output = run(captured(&exe).arg("2"));
        assert_eq!(output.status.code(), Some(101));
        assert!(String::from_utf8_lossy(&output.stderr).contains("[P-SYSTEM]"));
    }
}
