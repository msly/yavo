//! Bounded subprocess capture shared by the fixture runners.
//!
//! Both pipes are drained while the child runs. The deadline also covers pipe EOF:
//! a child exiting while its descendants keep the pipes open must not hang a test.
#![allow(dead_code)]

use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const BUILD_TIMEOUT: Duration = Duration::from_secs(120);
pub const RUN_TIMEOUT: Duration = Duration::from_secs(10);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Capture {
    pub output: Output,
    pub timed_out: bool,
}

fn reader<R: Read + Send + 'static>(
    pipe: Option<R>, bytes: Arc<Mutex<Vec<u8>>>, done: mpsc::Sender<io::Result<()>>,
) {
    let Some(mut pipe) = pipe else {
        let _ = done.send(Ok(()));
        return;
    };
    thread::spawn(move || {
        let result = (|| {
            let mut buf = [0u8; 8192];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) => return Ok(()),
                    Ok(n) => bytes.lock().unwrap().extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        })();
        let _ = done.send(result);
    });
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" { fn kill(pid: i32, sig: i32) -> i32; }
        // process_group(0) below creates a private group for this command and all
        // its ordinary descendants, including a generated program or C compiler.
        unsafe { kill(-(child.id() as i32), 9); }
    }
    #[cfg(windows)]
    {
        // On Windows use the native tree-kill utility. It cannot recover children
        // of an already-exited parent, so capture remains bounded if EOF lags.
        if let Ok(mut killer) = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        {
            let end = Instant::now() + CLEANUP_TIMEOUT;
            while matches!(killer.try_wait(), Ok(None)) && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
            let _ = killer.kill();
            let _ = killer.try_wait();
        }
    }
    let _ = child.kill();
}

/// Capture configured pipes without changing stdin/stdout/stderr. Output-error
/// tests can redirect one stream to a special file and capture the other one.
pub fn capture_stdio(command: &mut Command, limit: Duration) -> io::Result<Capture> {
    let deadline = Instant::now().checked_add(limit)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "subprocess deadline overflows"))?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    // Piped stdin without a writer must close. Callers needing input should pass
    // a File, inherited by the child without a writable handle in the parent.
    drop(child.stdin.take());
    let stdout = Arc::new(Mutex::new(Vec::new()));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = mpsc::channel();
    reader(child.stdout.take(), stdout.clone(), tx.clone());
    reader(child.stderr.take(), stderr.clone(), tx.clone());
    drop(tx);
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
                Err(e) => { kill_tree(&mut child); return Err(e); }
            }
        }
        if status.is_some() && finished == 2 { break; }
        let now = Instant::now();
        if !timed_out && now >= deadline {
            timed_out = true;
            kill_tree(&mut child);
            cleanup_end = Some(Instant::now() + CLEANUP_TIMEOUT);
        }
        if cleanup_end.is_some_and(|end| now >= end) {
            // Never join a reader forever if a detached process holds the pipe.
            if status.is_none() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "child did not exit after termination"));
            }
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    if let Some(e) = read_error { return Err(e); }
    let output = Output {
        status: status.unwrap(),
        stdout: stdout.lock().unwrap().clone(),
        stderr: stderr.lock().unwrap().clone(),
    };
    Ok(Capture { output, timed_out })
}

/// Capture stdout/stderr concurrently, preserving any configured stdin File.
pub fn capture(command: &mut Command, limit: Duration) -> io::Result<Capture> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    capture_stdio(command, limit)
}

pub fn output(command: &mut Command, limit: Duration) -> Result<Output, String> {
    let result = capture(command, limit).map_err(|e| format!("{command:?}: {e}"))?;
    if result.timed_out {
        Err(format!("{command:?} exceeded {} ms\nstdout: {}\nstderr: {}", limit.as_millis(),
            String::from_utf8_lossy(&result.output.stdout), String::from_utf8_lossy(&result.output.stderr)))
    } else {
        Ok(result.output)
    }
}

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(label: &str) -> io::Result<Self> {
        loop {
            let path = std::env::temp_dir().join(format!("yavo-{label}-{}-{}", std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }

    pub fn exe(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
