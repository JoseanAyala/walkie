use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Pipe `input` through a user-configured shell command (`claude -p '…'`,
/// `codex exec '…'`, an ollama call, …) and return its stdout.
///
/// stdin is written and stdout/stderr are read concurrently on background
/// threads. A command that echoes or transforms its input while producing
/// output (`cat`, `tee`, a real CLI that streams its response, …) reads
/// stdin and writes stdout at the same time; once combined traffic exceeds
/// the OS pipe buffer (~16KB on macOS, ~64KB on Linux) a sequential
/// write-then-read would deadlock — the child blocks writing to its full,
/// undrained stdout pipe, which stops it draining stdin, which blocks our
/// write forever, before the timeout loop is ever reached. Draining both
/// pipes in parallel removes that deadlock regardless of transcript size.
///
/// The child also runs in its own process group (`process_group(0)`), so a
/// timeout kill can take down the whole group (`kill(-pid)`), not just the
/// immediate `sh`. For a compound command like `sleep 5 && echo hi`, `sh`
/// forks `sleep` as a real child process; killing only `sh` would leave
/// `sleep` alive, reparented, running for its full remaining duration in
/// the background.
pub fn run_polish(command: &str, input: &str, timeout: Duration) -> Result<String> {
    anyhow::ensure!(!command.trim().is_empty(), "no polish command configured");

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group (pgid == child pid) so a timeout kill can
        // target the whole group, taking any forked descendants with it.
        .process_group(0)
        .spawn()
        .context("spawning polish command")?;

    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let pid = child.id() as i32;

    // Rust ignores SIGPIPE, so a command that exits without reading stdin
    // (or is killed mid-write) surfaces here as a write error — ignored,
    // since the exit-status/timeout check below is authoritative.
    let input = input.to_owned();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
        // `stdin` drops here, closing the pipe so the child sees EOF.
    });
    let stdout_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if Instant::now() >= deadline {
            // SIGKILL the whole process group (negative pid), not just the
            // immediate `sh`, so forked descendants die too instead of
            // being reparented and running to completion in the
            // background. This also guarantees our reader/writer threads
            // unblock promptly: every process that could hold a pipe end
            // open is gone, so pending reads see EOF and pending writes
            // see EPIPE almost immediately.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = child.wait();
            let _ = writer.join();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            anyhow::bail!("polish command timed out after {timeout:?}");
        }
        thread::sleep(Duration::from_millis(25));
    };

    let _ = writer.join();
    let out = stdout_reader.join().unwrap_or_default();

    if !status.success() {
        let err = stderr_reader.join().unwrap_or_default();
        let err = String::from_utf8_lossy(&err);
        let code_desc = match status.code() {
            Some(code) => format!("exit code {code}"),
            None => match status.signal() {
                Some(sig) => format!("killed by signal {sig}"),
                None => "terminated abnormally".to_string(),
            },
        };
        anyhow::bail!("polish command failed ({code_desc}): {}", err.trim());
    }

    let out = String::from_utf8_lossy(&out).trim().to_string();
    anyhow::ensure!(!out.is_empty(), "polish command returned empty output");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::run_polish;
    use std::time::{Duration, Instant};

    #[test]
    fn identity_command_roundtrips() {
        assert_eq!(run_polish("cat", "hello world", Duration::from_secs(5)).unwrap(), "hello world");
    }

    #[test]
    fn command_transforms_stdin() {
        assert_eq!(run_polish("tr 'a-z' 'A-Z'", "hola", Duration::from_secs(5)).unwrap(), "HOLA");
    }

    #[test]
    fn empty_command_errors() {
        assert!(run_polish("  ", "x", Duration::from_secs(1)).is_err());
    }

    #[test]
    fn failing_command_errors() {
        assert!(run_polish("false", "x", Duration::from_secs(5)).is_err());
    }

    #[test]
    fn empty_output_errors() {
        assert!(run_polish("true", "x", Duration::from_secs(5)).is_err());
    }

    #[test]
    fn timeout_kills_child_quickly() {
        let t0 = Instant::now();
        let r = run_polish("sleep 5 && echo hi", "x", Duration::from_millis(300));
        assert!(r.is_err());
        assert!(t0.elapsed() < Duration::from_secs(2), "took {:?}", t0.elapsed());
    }

    /// Regression test for a deadlock: writing all of `input` to the
    /// child's stdin before reading any of its stdout would hang forever
    /// once combined traffic exceeded the OS pipe buffer (~16-64KB),
    /// because `cat` blocks writing to its full stdout pipe, which stops
    /// it draining stdin, which blocks our write. Concurrent read/write
    /// must resolve this regardless of size.
    #[test]
    fn large_input_does_not_deadlock_on_full_pipe_buffer() {
        let input = "x".repeat(200_000);
        let t0 = Instant::now();
        let out = run_polish("cat", &input, Duration::from_secs(10)).unwrap();
        assert_eq!(out, input);
        assert!(t0.elapsed() < Duration::from_secs(5), "took {:?}", t0.elapsed());
    }
}
