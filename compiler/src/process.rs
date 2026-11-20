//! Bounded subprocess capture for production verification commands.
//! A deadline includes process exit and pipe EOF. Every stream is continuously
//! drained and hashed, while at most CAPTURE_LIMIT bytes are retained.

use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const CAPTURE_LIMIT: usize = 4 * 1024 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Capture {
    pub output: Output,
    pub timed_out: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    /// A full-stream digest exists only when the reader reached EOF. An escaped
    /// descendant may keep a pipe open past the cleanup deadline; never label
    /// the captured prefix's digest as the complete stream in that case.
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
}

#[derive(Default)]
struct Stream {
    kept: Vec<u8>,
    bytes: u64,
    hash: Sha256,
    eof: bool,
    closed: bool,
}

type SharedStream = Arc<Mutex<Stream>>;

fn reader<R: Read + Send + 'static>(
    name: &'static str,
    pipe: Option<R>,
    stream: SharedStream,
    done: mpsc::Sender<io::Result<()>>,
) -> io::Result<()> {
    let Some(mut pipe) = pipe else {
        stream.lock().unwrap().eof = true;
        let _ = done.send(Ok(()));
        return Ok(());
    };
    thread::Builder::new().name(name.into()).spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        let (result, eof) = loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break (Ok(()), true),
                Ok(n) => {
                    let mut state = stream.lock().unwrap();
                    if state.closed { break (Ok(()), false); }
                    let keep = n.min(CAPTURE_LIMIT.saturating_sub(state.kept.len()));
                    state.kept.extend_from_slice(&buffer[..keep]);
                    state.bytes = state.bytes.saturating_add(n as u64);
                    state.hash.update(&buffer[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => break (Err(e), false),
            }
        };
        stream.lock().unwrap().eof = eof;
        let _ = done.send(result);
    }).map(|_| ())
}

fn kill_tree(child: &mut Child, cleanup_end: Instant) {
    #[cfg(unix)]
    {
        unsafe extern "C" { fn kill(pid: i32, sig: i32) -> i32; }
        // process_group(0) gives the command and its ordinary descendants their
        // own group. The original parent may already have exited.
        unsafe { kill(-(child.id() as i32), 9); }
        let _ = cleanup_end;
    }
    #[cfg(windows)]
    {
        // taskkill is a best-effort fallback, not a Windows Job Object. It cannot
        // recover an already-exited parent's descendants. Pipe capture still
        // returns at the cleanup deadline with absent full-stream hashes.
        if let Ok(mut killer) = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        {
            while Instant::now() < cleanup_end {
                match killer.try_wait() {
                    Ok(None) => thread::sleep(Duration::from_millis(5)),
                    _ => break,
                }
            }
            let _ = killer.kill();
            let _ = killer.try_wait();
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = cleanup_end;
    let _ = child.kill();
}

fn reap_until(child: &mut Child, end: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Err(_) => return None,
            Ok(None) if Instant::now() >= end => return None,
            Ok(None) => thread::sleep(Duration::from_millis(5)),
        }
    }
}

fn close(stream: &SharedStream) {
    stream.lock().unwrap().closed = true;
}

fn snapshot(stream: &SharedStream) -> (Vec<u8>, u64, bool, Option<String>) {
    let mut state = stream.lock().unwrap();
    state.closed = true;
    let hash = state.eof.then(|| format!("{:x}", state.hash.clone().finalize()));
    (std::mem::take(&mut state.kept), state.bytes, state.bytes > CAPTURE_LIMIT as u64, hash)
}

/// Capture both output streams, retaining the caller's stdin configuration.
/// Use a File, Stdio::null(), or inherit() for stdin: a piped stdin with no
/// producer is closed when the subprocess starts. Reader threads are never
/// joined indefinitely if a detached descendant holds a pipe open.
pub fn capture(command: &mut Command, limit: Duration) -> io::Result<Capture> {
    let deadline = Instant::now().checked_add(limit)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "subprocess deadline overflows"))?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    drop(child.stdin.take());
    let stdout = Arc::new(Mutex::new(Stream::default()));
    let stderr = Arc::new(Mutex::new(Stream::default()));
    let (tx, rx) = mpsc::channel();
    let start = reader("yavo-stdout", child.stdout.take(), stdout.clone(), tx.clone())
        .and_then(|_| reader("yavo-stderr", child.stderr.take(), stderr.clone(), tx.clone()));
    drop(tx);
    if let Err(e) = start {
        let end = Instant::now() + CLEANUP_TIMEOUT;
        kill_tree(&mut child, end);
        close(&stdout);
        close(&stderr);
        let _ = reap_until(&mut child, end);
        return Err(e);
    }

    let mut finished = 0;
    let mut read_error = None;
    let mut status = None;
    let mut timed_out = false;
    let mut cleanup_end = None;
    loop {
        while let Ok(result) = rx.try_recv() {
            finished += 1;
            if let Err(e) = result { read_error = Some(e); }
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(s) => status = s,
                Err(e) => {
                    kill_tree(&mut child, Instant::now() + CLEANUP_TIMEOUT);
                    close(&stdout);
                    close(&stderr);
                    return Err(e);
                }
            }
        }
        let now = Instant::now();
        if cleanup_end.is_none() && (now >= deadline || read_error.is_some()) {
            timed_out = now >= deadline;
            let end = now + CLEANUP_TIMEOUT;
            cleanup_end = Some(end);
            kill_tree(&mut child, end);
        }
        if status.is_some() && finished == 2 { break; }
        if cleanup_end.is_some_and(|end| Instant::now() >= end) {
            close(&stdout);
            close(&stderr);
            if status.is_none() {
                // One final nonblocking reap can observe termination that
                // completed while the tree-kill fallback was running.
                status = child.try_wait()?;
            }
            if status.is_none() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "child did not exit after termination"));
            }
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    if let Some(e) = read_error { return Err(e); }
    let (stdout, stdout_bytes, stdout_truncated, stdout_sha256) = snapshot(&stdout);
    let (stderr, stderr_bytes, stderr_truncated, stderr_sha256) = snapshot(&stderr);
    Ok(Capture {
        output: Output { status: status.unwrap(), stdout, stderr },
        timed_out,
        stdout_truncated,
        stderr_truncated,
        stdout_bytes,
        stderr_bytes,
        stdout_sha256,
        stderr_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "process::tests::subprocess_fixture", "--nocapture"])
            .env("YAVO_PROCESS_FIXTURE", mode).stdin(Stdio::null());
        command
    }

    #[test]
    fn subprocess_fixture() {
        let Ok(mode) = std::env::var("YAVO_PROCESS_FIXTURE") else { return };
        match mode.as_str() {
            "dual" | "large" => {
                let count = if mode == "large" { CAPTURE_LIMIT + 65536 } else { 1024 * 1024 };
                let writer = thread::spawn(move || {
                    std::io::stdout().write_all(&vec![b'X'; count]).unwrap();
                    std::io::stdout().flush().unwrap();
                });
                std::io::stderr().write_all(&vec![b'Y'; count]).unwrap();
                std::io::stderr().flush().unwrap();
                writer.join().unwrap();
            }
            "timeout" => {
                println!("ready");
                std::io::stdout().flush().unwrap();
                thread::sleep(Duration::from_secs(30));
            }
            "orphan" => {
                fixture("hold-pipes").stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn().unwrap();
            }
            "hold-pipes" => {
                println!("child ready");
                std::io::stdout().flush().unwrap();
                thread::sleep(Duration::from_secs(30));
            }
            _ => panic!("unknown subprocess fixture"),
        }
        std::process::exit(0);
    }

    #[test]
    fn drains_both_pipes_and_hashes_the_complete_streams() {
        let capture = capture(&mut fixture("dual"), Duration::from_secs(10)).unwrap();
        assert!(!capture.timed_out);
        assert!(capture.output.status.success());
        assert_eq!(capture.output.stdout.iter().filter(|b| **b == b'X').count(), 1024 * 1024);
        assert_eq!(capture.output.stderr, vec![b'Y'; 1024 * 1024]);
        assert_eq!(capture.stdout_bytes, capture.output.stdout.len() as u64);
        assert_eq!(capture.stderr_bytes, capture.output.stderr.len() as u64);
        assert!(!capture.stdout_truncated && !capture.stderr_truncated);
        assert_eq!(capture.stdout_sha256, Some(format!("{:x}", Sha256::digest(&capture.output.stdout))));
        assert_eq!(capture.stderr_sha256, Some(format!("{:x}", Sha256::digest(&capture.output.stderr))));
    }

    #[test]
    fn caps_retained_output_without_stopping_reads_or_hashing() {
        let capture = capture(&mut fixture("large"), Duration::from_secs(10)).unwrap();
        assert!(!capture.timed_out);
        assert!(capture.output.status.success());
        assert!(capture.stdout_truncated && capture.stderr_truncated);
        assert_eq!(capture.output.stdout.len(), CAPTURE_LIMIT);
        assert_eq!(capture.output.stderr.len(), CAPTURE_LIMIT);
        let payload = CAPTURE_LIMIT + 65536;
        let prefix = capture.output.stdout.iter().position(|b| *b == b'X').unwrap();
        assert_eq!(capture.stdout_bytes, (prefix + payload) as u64);
        assert_eq!(capture.stderr_bytes, payload as u64);
        let mut expected = Sha256::new();
        expected.update(&capture.output.stdout[..prefix]);
        expected.update(vec![b'X'; payload]);
        assert_eq!(capture.stdout_sha256, Some(format!("{:x}", expected.finalize())));
        assert_eq!(capture.stderr_sha256, Some(format!("{:x}", Sha256::digest(vec![b'Y'; payload]))));
    }

    #[test]
    fn timeout_kills_a_running_process_and_returns_within_the_cleanup_bound() {
        let start = Instant::now();
        let capture = capture(&mut fixture("timeout"), Duration::from_millis(300)).unwrap();
        assert!(capture.timed_out);
        assert!(!capture.output.status.success());
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_pipe_holding_descendants_after_the_parent_exits() {
        let start = Instant::now();
        let capture = capture(&mut fixture("orphan"), Duration::from_millis(500)).unwrap();
        assert!(capture.timed_out);
        assert!(capture.output.status.success(), "the original parent must already have exited");
        assert!(String::from_utf8_lossy(&capture.output.stdout).contains("child ready"));
        assert!(capture.stdout_sha256.is_some() && capture.stderr_sha256.is_some(), "tree kill must close the inherited pipes");
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
