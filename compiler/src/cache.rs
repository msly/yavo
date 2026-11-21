//! Content-addressed cache for the C back end.
//!
//! The front end takes a few milliseconds; nearly all of a build is `zig cc`. The cache key is
//! derived from what the back end actually consumes — the generated C bytes, the runtime header,
//! the exact compiler command line (flags, target triple), the pinned toolchain lock and the
//! compiler executable path, and, for native builds, the host CPU — so any change in the source
//! graph or the Yavo compiler that changes the generated C also changes the key, and changes that
//! leave the C identical correctly reuse the binary.
//!
//! Layout: `<root>/build/<key>/{main.c, yavo_rt.h, out, out.sha256}`. The C inputs live at a stable
//! per-key path so that zig's own cache also hits after a Yavo cache eviction. Publishing is atomic
//! (write to a unique temporary, then rename); a hit is only trusted when the stored SHA-256
//! matches the binary's bytes, so a torn or corrupted entry is rebuilt rather than used.
//!
//! The cache is used only with the pinned zig toolchain (a `YAVO_CC` override has no identity we
//! can hash). `YAVO_NO_CACHE=1` disables it; `YAVO_CACHE_DIR` relocates it. `yavo verify` builds
//! through its own path with fresh caches and never consults this one.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const SCHEMA: &str = "yavo-build-cache/1";
/// Entries kept after a publish; older ones (by last use) are removed.
const MAX_ENTRIES: usize = 256;

pub struct Inputs<'a> {
    pub c_code: &'a str,
    pub runtime_header: &'a str,
    pub toolchain_lock: &'a str,
    pub compiler: &'a Path,
    /// The compiler arguments excluding input/output paths, in order.
    pub args: &'a [OsString],
    /// True when no `-target` is given, so the binary depends on the host CPU.
    pub native: bool,
}

pub fn root() -> Option<PathBuf> {
    if std::env::var_os("YAVO_NO_CACHE").is_some_and(|v| !v.is_empty() && v != "0") {
        return None;
    }
    if let Some(dir) = std::env::var_os("YAVO_CACHE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("yavo").join("cache"));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches/yavo"));
    }
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(|d| PathBuf::from(d).join("yavo"))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache").join("yavo")))
}

fn field(h: &mut Sha256, name: &str, bytes: &[u8]) {
    // Length-prefixed fields: no two different input tuples can hash the same byte stream.
    h.update(name.as_bytes());
    h.update([0]);
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

pub fn key(inputs: &Inputs) -> String {
    let mut h = Sha256::new();
    field(&mut h, "schema", SCHEMA.as_bytes());
    field(&mut h, "c", inputs.c_code.as_bytes());
    field(&mut h, "runtime", inputs.runtime_header.as_bytes());
    field(&mut h, "lock", inputs.toolchain_lock.as_bytes());
    field(&mut h, "compiler", inputs.compiler.to_string_lossy().as_bytes());
    for arg in inputs.args {
        field(&mut h, "arg", arg.to_string_lossy().as_bytes());
    }
    if inputs.native {
        field(&mut h, "host", host_fingerprint().as_bytes());
    }
    format!("{:x}", h.finalize())
}

/// Native builds let zig pick the host CPU's features, so a cache directory shared between
/// machines (e.g. a network home) must not hand one CPU's binary to another.
fn host_fingerprint() -> String {
    let mut s = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    if let Ok(info) = fs::read_to_string("/proc/cpuinfo") {
        for line in info.lines() {
            let name = line.split(':').next().unwrap_or("").trim();
            if matches!(name, "flags" | "Features" | "model name" | "CPU part" | "CPU implementer") {
                s.push('\n');
                s.push_str(line);
            }
            if line.trim().is_empty() {
                break; // the first processor block describes the CPU model
            }
        }
    }
    s
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

/// Writes `bytes` to `path` atomically, so concurrent builds never read a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        unique_suffix()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

pub struct Entry {
    pub dir: PathBuf,
}

impl Entry {
    pub fn open(root: &Path, key: &str) -> io::Result<Entry> {
        let dir = root.join("build").join(key);
        fs::create_dir_all(&dir)?;
        Ok(Entry { dir })
    }

    pub fn c_file(&self) -> PathBuf {
        self.dir.join("main.c")
    }

    fn out(&self) -> PathBuf {
        self.dir.join("out")
    }

    fn sum(&self) -> PathBuf {
        self.dir.join("out.sha256")
    }

    /// Copies a verified cached binary to `output`. Returns false on any miss or mismatch.
    pub fn restore(&self, output: &Path) -> bool {
        let Ok(expected) = fs::read_to_string(self.sum()) else { return false };
        let Ok(actual) = sha256_file(&self.out()) else { return false };
        if expected.trim() != actual {
            // Torn or corrupted entry: drop it so the rebuild republishes a good one.
            let _ = fs::remove_file(self.sum());
            return false;
        }
        if copy_to(&self.out(), output).is_err() {
            return false;
        }
        touch(&self.dir);
        true
    }

    /// Writes the C inputs at their stable paths (atomically; content is identical for every
    /// writer of this key).
    pub fn stage(&self, c_code: &str, runtime_header: &str) -> io::Result<()> {
        write_atomic(&self.c_file(), c_code.as_bytes())?;
        write_atomic(&self.dir.join("yavo_rt.h"), runtime_header.as_bytes())
    }

    /// A unique path for the compiler to write to before publishing.
    pub fn scratch_output(&self) -> PathBuf {
        self.dir.join(format!(".out.tmp-{}", unique_suffix()))
    }

    /// Publishes a freshly built binary and copies it to `output`.
    pub fn publish(&self, built: &Path, output: &Path) -> io::Result<()> {
        let sum = sha256_file(built)?;
        copy_to(built, output)?;
        // Binary first, then its checksum: a reader that sees the checksum also sees a binary
        // (possibly another writer's identical one), and verifies it before use.
        fs::rename(built, self.out())?;
        write_atomic(&self.sum(), sum.as_bytes())?;
        touch(&self.dir);
        Ok(())
    }
}

fn copy_to(from: &Path, to: &Path) -> io::Result<()> {
    // Copy rather than link: the user may modify or strip their output without touching the cache.
    // Copy to a sibling temporary and rename so a running old binary is never overwritten in place.
    let tmp = to.with_file_name(format!(
        ".{}.yavo-tmp-{}",
        to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        unique_suffix()
    ));
    fs::copy(from, &tmp)?;
    fs::rename(&tmp, to).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn touch(dir: &Path) {
    if let Ok(file) = fs::File::open(dir.join("out.sha256")) {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn last_use(dir: &Path) -> SystemTime {
    fs::metadata(dir.join("out.sha256"))
        .or_else(|_| fs::metadata(dir))
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH)
}

/// Keeps at most `MAX_ENTRIES` entries (by last use). Best effort; errors are ignored.
pub fn evict(root: &Path, keep: &str) {
    let limit = std::env::var("YAVO_CACHE_MAX_ENTRIES").ok().and_then(|v| v.parse().ok()).unwrap_or(MAX_ENTRIES);
    let Ok(read) = fs::read_dir(root.join("build")) else { return };
    let mut entries: Vec<(SystemTime, PathBuf)> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| e.file_name().to_string_lossy() != keep)
        .map(|e| (last_use(&e.path()), e.path()))
        .collect();
    if entries.len() < limit {
        return;
    }
    entries.sort();
    let excess = entries.len() + 1 - limit.max(1);
    for (_, path) in entries.into_iter().take(excess) {
        let _ = fs::remove_dir_all(path);
    }
}
