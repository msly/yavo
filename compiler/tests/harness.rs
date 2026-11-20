//! Regression tests for subprocess deadlines and the saved cold-start verifier.
mod common;

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn subprocess_fixture() {
    match std::env::var("YAVO_HARNESS_CHILD").as_deref() {
        Ok("large") => {
            std::io::stdout().write_all(&vec![b'O'; 2 * 1024 * 1024]).unwrap();
            std::io::stderr().write_all(&vec![b'E'; 2 * 1024 * 1024]).unwrap();
        }
        Ok("sleep") => std::thread::sleep(Duration::from_secs(5)),
        Ok("exit") => std::process::exit(17),
        Ok("input") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            println!("received:{input}");
            eprintln!("input-received");
        }
        _ => {}
    }
}

fn child(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "subprocess_fixture", "--nocapture"])
        .env("YAVO_HARNESS_CHILD", mode).stdin(Stdio::null());
    command
}

#[test]
fn capture_drains_both_pipes_while_the_program_runs() {
    let out = common::output(&mut child("large"), Duration::from_secs(5)).unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout.iter().filter(|b| **b == b'O').count(), 2 * 1024 * 1024);
    assert_eq!(out.stderr.iter().filter(|b| **b == b'E').count(), 2 * 1024 * 1024);
}

#[test]
fn capture_has_a_deadline_and_preserves_unsuccessful_exit_status() {
    let start = Instant::now();
    let result = common::capture(&mut child("sleep"), Duration::from_millis(100)).unwrap();
    assert!(result.timed_out);
    assert!(!result.output.status.success());
    assert!(start.elapsed() < Duration::from_secs(3));
    let out = common::output(&mut child("exit"), Duration::from_secs(3)).unwrap();
    assert_eq!(out.status.code(), Some(17));
}

#[test]
fn capture_stdio_preserves_redirected_output_and_file_input() {
    let scratch = common::Scratch::new("capture-stdio").unwrap();
    let input = scratch.0.join("input");
    let output = scratch.0.join("output");
    std::fs::write(&input, b"file-input").unwrap();
    let mut command = child("input");
    command.stdin(std::fs::File::open(&input).unwrap())
        .stdout(std::fs::File::create(&output).unwrap()).stderr(Stdio::piped());
    let captured = common::capture_stdio(&mut command, Duration::from_secs(3)).unwrap();
    assert!(!captured.timed_out);
    assert!(captured.output.status.success());
    assert!(captured.output.stdout.is_empty());
    assert!(String::from_utf8(captured.output.stderr).unwrap().contains("input-received"));
    assert!(std::fs::read_to_string(output).unwrap().contains("received:file-input"));
}

#[cfg(unix)]
fn assert_not_running(pid: &str) {
    let out = common::output(Command::new("ps").args(["-p", pid, "-o", "stat="]),
        Duration::from_secs(2)).unwrap();
    let state = String::from_utf8_lossy(&out.stdout);
    assert!(state.trim().is_empty() || state.trim().starts_with('Z'),
        "descendant {pid} is still running: {state}");
}

#[cfg(unix)]
#[test]
fn timeout_kills_descendants_even_after_the_parent_has_exited() {
    let start = Instant::now();
    // The parent exits immediately, but its child retains both captured pipes.
    let result = common::capture(Command::new("sh")
        .args(["-c", "sleep 5 & printf '%s\\n' \"$!\"; exit 0"])
        .stdin(Stdio::null()), Duration::from_millis(200)).unwrap();
    assert!(result.timed_out);
    assert!(start.elapsed() < Duration::from_secs(3));
    let pid = String::from_utf8(result.output.stdout).unwrap();
    assert!(pid.trim().parse::<u32>().is_ok(), "{pid:?}");
    assert_not_running(pid.trim());
}

#[cfg(unix)]
mod verifier {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    const COMPILER: &str = r#"#!/usr/bin/env bash
set -eu
src=
out=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o) out=$2; shift 2;;
        *.yv|*.rs) src=$1; shift;;
        *) shift;;
    esac
done
case "$src" in "$MOCK_SOURCES"/T??.*) ;; *) echo "wrong source: $src" >&2; exit 8;; esac
test -f "$src"
printf '%s\n' "$src" >> "$MOCK_SOURCE_LOG"
if [ "$MOCK_MODE" = build-fail ]; then
    case "$src" in */T01.*) echo "mock build failed" >&2; exit 9;; esac
fi
cp "$MOCK_TEMPLATE" "$out"
chmod +x "$out"
"#;

    const PROGRAM: &str = r#"#!/usr/bin/env bash
set -eu
name=$(basename "$0")
input=$(cat)
if [ "$name" = T01 ]; then
    case "$MOCK_MODE" in
        exit-fail) cat "$MOCK_TASKS/T01.out"; exit 102;;
        stderr-fail) echo unexpected >&2;;
        stdout-fail) echo wrong; exit 0;;
        hang) sleep 20 & printf '%s\n' "$!" > "$MOCK_PID"; wait;;
    esac
fi
case "$name" in
    T04)
        if [ "$input" = "$(cat "$MOCK_TASKS/T04b.in")" ]; then cat "$MOCK_TASKS/T04b.err" >&2; exit 1; fi
        if [ "$input" = "$(cat "$MOCK_TASKS/T04c.in")" ]; then cat "$MOCK_TASKS/T04c.err" >&2; exit 1; fi
        cat "$MOCK_TASKS/T04a.out";;
    T05)
        if [ "$input" = "$(cat "$MOCK_TASKS/T05b.in")" ]; then cat "$MOCK_TASKS/T05b.out"; exit 1; fi
        cat "$MOCK_TASKS/T05a.out";;
    T06)
        if [ "$1" = encode ]; then cat "$MOCK_TASKS/T06.enc";
        elif [ "$input" = "$(cat "$MOCK_TASKS/T06bad.in")" ]; then cat "$MOCK_TASKS/T06bad.err" >&2; exit 1;
        else cat "$MOCK_TASKS/T06.in"; fi;;
    *) cat "$MOCK_TASKS/$name.out";;
esac
"#;

    struct Fixture {
        scratch: common::Scratch,
        experiment: PathBuf,
        compiler: PathBuf,
        template: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let scratch = common::Scratch::new("verifier checkout with spaces").unwrap();
            let experiment = scratch.0.join("experiments/coldstart-2026-11-20");
            fs::create_dir_all(experiment.join("tasks")).unwrap();
            let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../experiments/coldstart-2026-11-20");
            fs::copy(original.join("verify.sh"), experiment.join("verify.sh")).unwrap();
            for file in fs::read_dir(original.join("tasks")).unwrap() {
                let file = file.unwrap();
                if file.path().is_file() {
                    fs::copy(file.path(), experiment.join("tasks").join(file.file_name())).unwrap();
                }
            }
            for (arm, ext) in [("yavo", "yv"), ("rust", "rs")] {
                fs::create_dir(experiment.join(arm)).unwrap();
                for n in 1..=10 {
                    fs::copy(original.join(arm).join(format!("T{n:02}.{ext}")),
                        experiment.join(arm).join(format!("T{n:02}.{ext}"))).unwrap();
                }
            }
            let compiler = scratch.0.join("mock compiler");
            let template = scratch.0.join("mock program");
            fs::write(&compiler, COMPILER).unwrap();
            fs::write(&template, PROGRAM).unwrap();
            fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
            fs::create_dir(scratch.0.join("tmp")).unwrap();
            fs::write(scratch.0.join("tmp/keep"), b"unrelated user file").unwrap();
            Self { scratch, experiment, compiler, template }
        }

        fn run(&self, arm: &str, mode: &str) -> std::process::Output {
            let mut command = Command::new("bash");
            command.arg(self.experiment.join("verify.sh")).arg(arm)
                .current_dir(&self.scratch.0).stdin(Stdio::null())
                .env("YAVO", &self.compiler).env("RUSTC", &self.compiler)
                .env_remove("RUST_LINKER")
                .env("TMPDIR", self.scratch.0.join("tmp"))
                .env("YAVO_VERIFY_BUILD_TIMEOUT", "5").env("YAVO_VERIFY_RUN_TIMEOUT", "1")
                .env("MOCK_MODE", mode).env("MOCK_SOURCES", self.experiment.join(arm))
                .env("MOCK_TASKS", self.experiment.join("tasks")).env("MOCK_TEMPLATE", &self.template)
                .env("MOCK_SOURCE_LOG", self.scratch.0.join("sources.log"))
                .env("MOCK_PID", self.scratch.0.join("pid"));
            let out = common::output(&mut command, Duration::from_secs(20)).unwrap();
            assert_eq!(fs::read(self.scratch.0.join("tmp/keep")).unwrap(), b"unrelated user file");
            assert_eq!(fs::read_dir(self.scratch.0.join("tmp")).unwrap().count(), 1,
                "verifier must remove only its own temporary directory");
            out
        }
    }

    #[test]
    fn verifier_rebuilds_checkout_sources_and_checks_success_for_both_arms() {
        for arm in ["yavo", "rust"] {
            let fixture = Fixture::new();
            let out = fixture.run(arm, "ok");
            assert!(out.status.success(), "{out:?}");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("15 passed, 0 failed, 0 skipped"), "{stdout}");
            let sources = fs::read_to_string(fixture.scratch.0.join("sources.log")).unwrap();
            assert_eq!(sources.lines().count(), 10);
            assert!(sources.lines().all(|s| Path::new(s).starts_with(fixture.experiment.join(arm))));
        }
    }

    #[test]
    fn verifier_rejects_wrong_exit_streams_and_build_failures() {
        for mode in ["exit-fail", "stdout-fail", "stderr-fail", "build-fail"] {
            let fixture = Fixture::new();
            let out = fixture.run("yavo", mode);
            assert_eq!(out.status.code(), Some(1), "{mode}: {out:?}");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("FAIL"), "{mode}: {stdout}");
            assert!(stdout.contains("T10 PASS"), "failure must not skip later checks: {stdout}");
            if mode == "exit-fail" { assert!(stdout.contains("exit 102, expected 0")); }
            if mode == "build-fail" { assert!(stdout.contains("BUILD FAIL")); }
        }
    }

    #[test]
    fn verifier_timeout_is_a_failure_and_kills_the_program_tree() {
        let fixture = Fixture::new();
        let out = fixture.run("yavo", "hang");
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("T01 FAIL (exit 124, expected 0)"), "{stdout}");
        assert!(stdout.contains("T10 PASS"), "{stdout}");
        let pid = fs::read_to_string(fixture.scratch.0.join("pid")).unwrap();
        assert_not_running(pid.trim());
    }
}
