use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Pipe `input` through a user-configured shell command (`claude -p '…'`,
/// `codex exec '…'`, an ollama call, …) and return its stdout.
///
/// Transcripts are far below the 64KB pipe buffer, so a sequential
/// write-then-poll is safe (no reader-thread needed).
pub fn run_polish(command: &str, input: &str, timeout: Duration) -> Result<String> {
    anyhow::ensure!(!command.trim().is_empty(), "no polish command configured");

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning polish command")?;

    // Rust ignores SIGPIPE, so a command that exits without reading stdin
    // surfaces here as a write error — fall through to the exit-status check.
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("polish command timed out after {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out)?;
    if !status.success() {
        let mut err = String::new();
        let _ = child.stderr.take().unwrap().read_to_string(&mut err);
        anyhow::bail!("polish command failed: {}", err.trim());
    }
    let out = out.trim().to_string();
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
}
