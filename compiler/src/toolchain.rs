//! Locates the C toolchain. Yavo pins one exact `zig cc` release (see `toolchain.lock`)
//! so generated C compiles the same way on every host; `YAVO_CC` overrides it.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../toolchain.lock");

pub struct Lock {
    pub version: String,
    entries: Vec<(String, String, String)>,
}

pub struct Toolchain {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub description: String,
}

pub fn lock() -> Lock {
    let mut version = String::new();
    let mut entries = Vec::new();
    for line in LOCK.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if key == "zig_version" {
            version = value.to_string();
        } else if let Some((url, sha)) = value.split_once(' ') {
            entries.push((key.to_string(), url.trim().to_string(), sha.trim().to_string()));
        }
    }
    Lock { version, entries }
}

pub fn host_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        other => other,
    };
    format!("{}-{}", std::env::consts::ARCH, os)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("YAVO_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|h| PathBuf::from(h).join(".yavo"))
    })
}

fn zig_dir(lock: &Lock) -> Option<PathBuf> {
    Some(home()?.join("toolchains").join(format!("zig-{}-{}", host_key(), lock.version)))
}

fn zig_exe(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) { "zig.exe" } else { "zig" })
}

pub fn find() -> Result<Toolchain, String> {
    if let Ok(cc) = std::env::var("YAVO_CC") {
        let mut parts = cc.split_whitespace().map(str::to_string);
        let program = parts.next().ok_or("YAVO_CC is set but empty")?;
        return Ok(Toolchain {
            program: PathBuf::from(program),
            prefix_args: parts.collect(),
            description: format!("YAVO_CC={cc}"),
        });
    }
    let lock = lock();
    let dir = zig_dir(&lock).ok_or("cannot determine the home directory; set YAVO_HOME")?;
    let exe = zig_exe(&dir);
    if exe.is_file() {
        return Ok(Toolchain {
            program: exe.clone(),
            prefix_args: vec!["cc".into()],
            description: format!("zig {} ({})", lock.version, exe.display()),
        });
    }
    Err(format!(
        "the pinned C toolchain (zig {}) is not installed at {}\n  run `yavo toolchain install`, or set YAVO_CC to a C17 compiler",
        lock.version,
        dir.display()
    ))
}

pub fn install() -> Result<String, String> {
    let lock = lock();
    let key = host_key();
    let (_, url, sha) = lock
        .entries
        .iter()
        .find(|(k, _, _)| *k == key)
        .ok_or_else(|| format!("no pinned toolchain for host `{key}` in toolchain.lock"))?;
    let dir = zig_dir(&lock).ok_or("cannot determine the home directory; set YAVO_HOME")?;
    if zig_exe(&dir).is_file() {
        return Ok(format!("zig {} is already installed at {}", lock.version, dir.display()));
    }
    let root = dir.parent().unwrap().to_path_buf();
    fs::create_dir_all(&root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
    let file_name = url.rsplit('/').next().unwrap_or("zig-archive");
    let archive = root.join(format!("{file_name}.partial"));
    eprintln!("downloading {url}");
    let status = Command::new("curl")
        .args(["-fL", "--retry", "3", "-o"])
        .arg(&archive)
        .arg(url)
        .status()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if !status.success() {
        let _ = fs::remove_file(&archive);
        return Err(format!("download failed: {url}"));
    }
    let actual = sha256_file(&archive)?;
    if !actual.eq_ignore_ascii_case(sha) {
        let _ = fs::remove_file(&archive);
        return Err(format!("checksum mismatch for {url}\n  expected {sha}\n  actual   {actual}"));
    }
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&root)
        .status()
        .map_err(|e| format!("cannot run tar: {e}"))?;
    let _ = fs::remove_file(&archive);
    if !status.success() || !zig_exe(&dir).is_file() {
        return Err(format!("extracting {file_name} did not produce {}", zig_exe(&dir).display()));
    }
    Ok(format!("installed zig {} at {}", lock.version, dir.display()))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

impl Toolchain {
    /// True for the pinned `zig cc`; false for a `YAVO_CC` development override.
    pub fn can_cross_compile(&self) -> bool {
        self.prefix_args.first().map(String::as_str) == Some("cc") && !self.description.starts_with("YAVO_CC=")
    }

    pub fn compile(
        &self,
        c_file: &Path,
        include_dir: &Path,
        output: &Path,
        release: bool,
        target: Option<&Target>,
    ) -> Result<(), String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(["-std=c17", "-fno-strict-aliasing"]);
        if let Some(t) = target {
            cmd.arg("-target").arg(t.zig_triple);
        }
        // Test hook: count heap blocks to catch leaks and double drops (see `yavo_rt.h`).
        if std::env::var_os("YAVO_ALLOC_CHECK").is_some() {
            cmd.arg("-DYV_ALLOC_CHECK");
        }
        if release {
            // Release binaries carry no debug info or symbols (decision 2026-11-16): a static musl
            // build drops from ~1.3 MB to ~12 KB and Windows targets no longer leave a `.pdb` file.
            cmd.args(["-O2", "-s"]);
        } else {
            cmd.args(["-O0", "-g"]);
        }
        cmd.arg("-I").arg(include_dir).arg(c_file).arg("-o").arg(output);
        let out = cmd.output().map_err(|e| format!("cannot run {}: {e}", self.description))?;
        // A successful compiler may still emit warnings. Preserve both streams on
        // stderr so toolchain chatter cannot corrupt frontend JSON on stdout.
        let mut diagnostics = io::stderr().lock();
        diagnostics.write_all(&out.stdout)
            .and_then(|_| diagnostics.write_all(&out.stderr))
            .map_err(|e| format!("cannot report C toolchain diagnostics: {e}"))?;
        drop(diagnostics);
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "the C compiler rejected generated code (this is a Yavo compiler bug)\n  toolchain: {}\n  status: {}",
                self.description,
                out.status
            ))
        }
    }
}

/// A compilation target accepted by `yavo build --target`. All of them are produced by the pinned
/// `zig cc`; Linux targets link musl statically, so the binaries do not depend on the host libc.
pub struct Target {
    pub name: &'static str,
    pub zig_triple: &'static str,
    pub arch: &'static str,
    pub os: &'static str,
}

pub const TARGETS: &[Target] = &[
    Target { name: "x86_64-linux-musl", zig_triple: "x86_64-linux-musl", arch: "x86_64", os: "linux" },
    Target { name: "aarch64-linux-musl", zig_triple: "aarch64-linux-musl", arch: "aarch64", os: "linux" },
    Target { name: "x86_64-windows-gnu", zig_triple: "x86_64-windows-gnu", arch: "x86_64", os: "windows" },
    Target { name: "aarch64-windows-gnu", zig_triple: "aarch64-windows-gnu", arch: "aarch64", os: "windows" },
    Target { name: "x86_64-macos", zig_triple: "x86_64-macos", arch: "x86_64", os: "macos" },
    Target { name: "aarch64-macos", zig_triple: "aarch64-macos", arch: "aarch64", os: "macos" },
];

impl Target {
    pub fn lookup(name: &str) -> Option<&'static Target> {
        TARGETS.iter().find(|t| t.name == name)
    }

    pub fn names() -> String {
        TARGETS.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
    }

    /// Whether a binary for this target can be started on the machine running the compiler.
    pub fn runs_on_host(&self) -> bool {
        self.arch == std::env::consts::ARCH && self.os == std::env::consts::OS
    }
}
