//! Content-addressed back-end cache (`src/cache.rs`): hits, invalidation, corruption recovery,
//! concurrent publication, eviction and the bypass paths. Every test uses its own cache root.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;

use common::Scratch;

const PROGRAM: &str = "fn main() {\n    println(\"one\")\n}\n";

fn yavo(cache: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yavo"));
    cmd.env_remove("YAVO_CC").env_remove("YAVO_NO_CACHE").env_remove("YAVO_ALLOC_CHECK")
        .env_remove("YAVO_CACHE_MAX_ENTRIES")
        .env("YAVO_CACHE_DIR", cache).env("YAVO_CACHE_TRACE", "1");
    cmd
}

fn run(cmd: &mut Command) -> Output {
    common::output(cmd, common::BUILD_TIMEOUT).unwrap()
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    path
}

/// Builds and returns ("hit" | "miss" | "none", output).
fn build(cache: &Path, src: &Path, out: &Path, extra: &[&str]) -> (&'static str, Output) {
    let o = run(yavo(cache).arg("build").arg(src).arg("-o").arg(out).args(extra));
    assert!(o.status.success(), "build failed: {}", String::from_utf8_lossy(&o.stderr));
    let err = String::from_utf8_lossy(&o.stderr);
    let state = if err.contains("build cache hit") { "hit" } else if err.contains("build cache miss") { "miss" } else { "none" };
    (state, o)
}

fn key_of(o: &Output) -> String {
    let err = String::from_utf8_lossy(&o.stderr);
    err.lines().find_map(|l| l.strip_prefix("yavo: build cache ").map(|r| r.split_whitespace().nth(1).unwrap().to_string()))
        .expect("trace line with key")
}

fn stdout_of(exe: &Path) -> String {
    let o = run(&mut Command::new(exe));
    String::from_utf8(o.stdout).unwrap()
}

fn entries(cache: &Path) -> usize {
    fs::read_dir(cache.join("build")).map(|r| r.count()).unwrap_or(0)
}

#[test]
fn second_build_hits_and_produces_identical_binary() {
    let s = Scratch::new("cache-hit").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", PROGRAM);
    let (a, b) = (s.exe("a"), s.exe("b"));
    let (first, o1) = build(&cache, &src, &a, &["--release"]);
    let (second, o2) = build(&cache, &src, &b, &["--release"]);
    assert_eq!((first, second), ("miss", "hit"));
    assert_eq!(key_of(&o1), key_of(&o2));
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert_eq!(stdout_of(&b), "one\n");
    // The user may modify their output without affecting the cache.
    fs::write(&b, b"clobbered").unwrap();
    let (third, _) = build(&cache, &src, &b, &["--release"]);
    assert_eq!(third, "hit");
    assert_eq!(stdout_of(&b), "one\n");
}

#[test]
fn key_covers_code_mode_target_and_alloc_check() {
    let s = Scratch::new("cache-keys").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", PROGRAM);
    let out = s.exe("p");
    let mut keys = Vec::new();
    let (st, o) = build(&cache, &src, &out, &["--release"]);
    assert_eq!(st, "miss");
    keys.push(key_of(&o));
    let (st, o) = build(&cache, &src, &out, &[]);
    assert_eq!(st, "miss", "debug and release must not share a binary");
    keys.push(key_of(&o));
    let (st, o) = build(&cache, &src, &s.0.join("cross"), &["--release", "--target", "aarch64-linux-musl"]);
    assert_eq!(st, "miss", "a different target must not share a binary");
    keys.push(key_of(&o));
    let o = run(yavo(&cache).env("YAVO_ALLOC_CHECK", "1").arg("build").arg(&src).arg("-o").arg(&out).arg("--release"));
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("build cache miss"), "alloc-check instrumentation is part of the key");
    keys.push(key_of(&o));
    write(&s.0, "main.yv", "fn main() {\n    println(\"two\")\n}\n");
    let (st, o) = build(&cache, &src, &out, &["--release"]);
    assert_eq!(st, "miss");
    keys.push(key_of(&o));
    assert_eq!(stdout_of(&out), "two\n");
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "{keys:?}");
}

#[test]
fn moved_source_lines_rebuild_so_panic_locations_stay_true() {
    let s = Scratch::new("cache-lines").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", "fn main() {\n    panic(\"boom\")\n}\n");
    let out = s.exe("p");
    build(&cache, &src, &out, &["--release"]);
    let o = run(&mut Command::new(&out));
    assert!(String::from_utf8_lossy(&o.stderr).contains("main.yv:2:"), "{}", String::from_utf8_lossy(&o.stderr));
    // A comment line shifts the panic to line 3: the generated C changes, so the cache must miss.
    write(&s.0, "main.yv", "// moved\nfn main() {\n    panic(\"boom\")\n}\n");
    let (st, _) = build(&cache, &src, &out, &["--release"]);
    assert_eq!(st, "miss");
    let o = run(&mut Command::new(&out));
    assert!(String::from_utf8_lossy(&o.stderr).contains("main.yv:3:"), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn module_change_invalidates_entry_build() {
    let s = Scratch::new("cache-modules").unwrap();
    let cache = s.0.join("cache");
    let main = write(&s.0, "main.yv", "use maths\nfn main() {\n    println(\"${maths.value()}\")\n}\n");
    write(&s.0, "maths.yv", "pub fn value() -> int { return 1 }\n");
    let out = s.exe("p");
    assert_eq!(build(&cache, &main, &out, &["--release"]).0, "miss");
    assert_eq!(build(&cache, &main, &out, &["--release"]).0, "hit");
    write(&s.0, "maths.yv", "pub fn value() -> int { return 2 }\n");
    assert_eq!(build(&cache, &main, &out, &["--release"]).0, "miss");
    assert_eq!(stdout_of(&out), "2\n");
}

#[test]
fn corrupted_entry_is_rebuilt_and_healed() {
    let s = Scratch::new("cache-corrupt").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", PROGRAM);
    let out = s.exe("p");
    let (_, o) = build(&cache, &src, &out, &["--release"]);
    let entry = cache.join("build").join(key_of(&o));
    let mut bytes = fs::read(entry.join("out")).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    fs::write(entry.join("out"), &bytes).unwrap();
    assert_eq!(build(&cache, &src, &out, &["--release"]).0, "miss", "a checksum mismatch must not be used");
    assert_eq!(stdout_of(&out), "one\n");
    assert_eq!(build(&cache, &src, &out, &["--release"]).0, "hit");
    // A missing checksum (torn publish) is also a miss.
    fs::remove_file(entry.join("out.sha256")).unwrap();
    assert_eq!(build(&cache, &src, &out, &["--release"]).0, "miss");
    assert_eq!(stdout_of(&out), "one\n");
}

#[test]
fn concurrent_builds_publish_one_consistent_entry() {
    let s = Scratch::new("cache-concurrent").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", PROGRAM);
    let handles: Vec<_> = (0..6).map(|i| {
        let (cache, src, out) = (cache.clone(), src.clone(), s.exe(&format!("p{i}")));
        thread::spawn(move || { build(&cache, &src, &out, &["--release"]); out })
    }).collect();
    let outs: Vec<PathBuf> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let first = fs::read(&outs[0]).unwrap();
    for out in &outs {
        assert_eq!(fs::read(out).unwrap(), first);
        assert_eq!(stdout_of(out), "one\n");
    }
    assert_eq!(entries(&cache), 1);
    let leftovers: Vec<_> = fs::read_dir(fs::read_dir(cache.join("build")).unwrap().next().unwrap().unwrap().path())
        .unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp-")).collect();
    assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
    assert_eq!(build(&cache, &src, &s.exe("again"), &["--release"]).0, "hit");
}

#[test]
fn eviction_bounds_the_number_of_entries() {
    let s = Scratch::new("cache-evict").unwrap();
    let cache = s.0.join("cache");
    let out = s.exe("p");
    for i in 0..4 {
        let src = write(&s.0, "main.yv", &format!("fn main() {{\n    println(\"{i}\")\n}}\n"));
        let o = run(yavo(&cache).env("YAVO_CACHE_MAX_ENTRIES", "2").arg("build").arg(&src).arg("-o").arg(&out).arg("--release"));
        assert!(o.status.success());
        assert_eq!(stdout_of(&out), format!("{i}\n"));
    }
    assert!(entries(&cache) <= 2, "{} entries", entries(&cache));
}

#[test]
fn bypass_paths_leave_no_cache() {
    let s = Scratch::new("cache-bypass").unwrap();
    let src = write(&s.0, "main.yv", PROGRAM);
    let out = s.exe("p");
    let cache = s.0.join("off");
    let o = run(yavo(&cache).env("YAVO_NO_CACHE", "1").arg("build").arg(&src).arg("-o").arg(&out));
    assert!(o.status.success());
    assert!(!cache.exists(), "YAVO_NO_CACHE must not create a cache");
    assert_eq!(stdout_of(&out), "one\n");
    // An unusable cache location falls back to an uncached build instead of failing.
    let blocker = write(&s.0, "not-a-dir", "x");
    let o = run(yavo(&blocker).arg("build").arg(&src).arg("-o").arg(&out));
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("building without it"));
    assert_eq!(stdout_of(&out), "one\n");
}

#[test]
fn stack_overflow_is_a_panic_and_keeps_buffered_output() {
    let s = Scratch::new("cache-stack").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv",
        "fn down(n: int) -> int {\n    if n == 0 { return 0 }\n    return down(n - 1) + 1\n}\nfn main() {\n    println(\"before\")\n    println(\"${down(100000000)}\")\n}\n");
    let o = run(yavo(&cache).arg("run").arg(&src));
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(101), "{err}");
    assert_eq!(String::from_utf8_lossy(&o.stdout), "before\n");
    assert!(err.contains("yavo panic [P-STACK] at "), "{err}");
    assert!(err.contains(":0:0: stack overflow"), "{err}");
}

#[cfg(unix)]
#[test]
fn run_reports_a_program_killed_by_a_signal() {
    let s = Scratch::new("cache-signal").unwrap();
    let cache = s.0.join("cache");
    let src = write(&s.0, "main.yv", "fn main() {\n    var x = 0\n    while true {\n        x = x +% 1\n    }\n}\n");
    // Build first so that only the program runs under the CPU limit, not the C compiler.
    let o = run(yavo(&cache).arg("build").arg(&src).arg("-o").arg(s.exe("spin")));
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let script = "ulimit -S -t 1; exec \"$1\" run \"$2\"";
    let o = run(Command::new("sh").arg("-c").arg(script).arg("sh").arg(env!("CARGO_BIN_EXE_yavo")).arg(&src)
        .env_remove("YAVO_CC").env_remove("YAVO_NO_CACHE").env_remove("YAVO_ALLOC_CHECK").env("YAVO_CACHE_DIR", &cache));
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(128 + 24), "{err}");
    assert!(err.contains("terminated by signal 24 (SIGXCPU)"), "{err}");
}
