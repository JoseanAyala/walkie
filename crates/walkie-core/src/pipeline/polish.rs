use crate::config::{Polish, PolishProvider};
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Polishes `input` with the configured provider. `helper` is walkie-ai
/// (see [`apple_helper`]); only the Apple provider uses it.
pub fn polish(cfg: &Polish, helper: &Path, input: &str) -> Result<String> {
    let timeout = Duration::from_secs(cfg.timeout_secs);
    match cfg.provider {
        PolishProvider::Command => run_polish(&cfg.command, input, timeout),
        PolishProvider::Apple => {
            anyhow::ensure!(!cfg.prompt.trim().is_empty(), "no polish prompt configured");
            let mut cmd = Command::new(helper);
            cmd.arg("respond").arg(&cfg.prompt);
            run(cmd, "Apple model", input, timeout)
        }
    }
}

/// Pipe `input` through a user-configured shell command (`claude -p '…'`,
/// `codex exec '…'`, an ollama call, …) and return its stdout.
pub fn run_polish(command: &str, input: &str, timeout: Duration) -> Result<String> {
    anyhow::ensure!(!command.trim().is_empty(), "no polish command configured");
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    run(cmd, "polish command", input, timeout)
}

/// walkie-ai: next to the walkie binary, both in Walkie.app and in
/// `target/<profile>` (tauri-build copies it there). Test binaries live one
/// level down, in `target/<profile>/deps`.
pub fn apple_helper() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let beside = |dir: Option<&Path>| dir.map(|d| d.join("walkie-ai"));
    let dir = exe.parent();
    beside(dir)
        .filter(|p| p.exists())
        .or_else(|| beside(dir.and_then(Path::parent)).filter(|p| p.exists()))
        .unwrap_or_else(|| PathBuf::from("walkie-ai"))
}

/// Whether Apple's model can polish right now, as walkie-ai reports it:
/// "available", "off", "not-ready", "not-eligible", "unavailable" or
/// "unsupported"; "missing" when the helper itself can't run.
pub fn apple_status(helper: &Path) -> String {
    let mut cmd = Command::new(helper);
    cmd.arg("status");
    run(cmd, "walkie-ai", "", Duration::from_secs(5)).unwrap_or_else(|_| "missing".into())
}

/// What an [`apple_status`] word means, for the Status and Polish tabs.
pub fn describe_apple_status(status: &str) -> &'static str {
    match status {
        "available" => "ready",
        "off" => "Apple Intelligence is off — turn it on in System Settings",
        "not-ready" => "Apple Intelligence is still downloading its model",
        "not-eligible" => "this Mac can't run Apple Intelligence",
        "unsupported" => "needs macOS 26 with Apple Intelligence",
        "missing" => "walkie-ai is missing from Walkie.app — reinstall walkie",
        _ => "Apple's model is unavailable",
    }
}

/// Runs `cmd` with `input` on stdin and returns its trimmed stdout.
/// `what` names it in errors.
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
fn run(mut cmd: Command, what: &str, input: &str, timeout: Duration) -> Result<String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group (pgid == child pid) so a timeout kill can
        // target the whole group, taking any forked descendants with it.
        .process_group(0)
        .spawn()
        .with_context(|| format!("starting {what}"))?;

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
            anyhow::bail!("{what} timed out after {timeout:?}");
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
        anyhow::bail!("{what} failed ({code_desc}): {}", err.trim());
    }

    let out = String::from_utf8_lossy(&out).trim().to_string();
    anyhow::ensure!(!out.is_empty(), "{what} returned empty output");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A stand-in walkie-ai: reports `status`, and for `respond` prints
    /// its instructions and upper-cases stdin.
    fn fake_helper(dir: &Path, status: &str) -> PathBuf {
        let p = dir.join("walkie-ai");
        let script = format!(
            "#!/bin/sh\ncase $1 in\n  status) echo {status} ;;\n  respond) printf '%s: ' \"$2\"; tr a-z A-Z ;;\nesac\n"
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        p
    }

    fn apple(prompt: &str) -> Polish {
        Polish {
            provider: PolishProvider::Apple,
            prompt: prompt.into(),
            ..Polish::default()
        }
    }

    #[test]
    fn apple_provider_sends_the_prompt_and_text_to_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), "available");
        assert_eq!(
            polish(&apple("tidy"), &helper, "hi there").unwrap(),
            "tidy: HI THERE"
        );
    }

    #[test]
    fn apple_provider_needs_a_prompt() {
        let err = polish(&apple(" "), Path::new("/nope"), "x").unwrap_err();
        assert!(err.to_string().contains("no polish prompt"), "{err}");
    }

    #[test]
    fn apple_provider_reports_a_missing_helper() {
        let err = polish(&apple("tidy"), Path::new("/nope/walkie-ai"), "x").unwrap_err();
        assert!(err.to_string().contains("starting Apple model"), "{err}");
    }

    #[test]
    fn command_provider_runs_the_shell_command() {
        let cfg = Polish {
            provider: PolishProvider::Command,
            command: "tr a-z A-Z".into(),
            ..Polish::default()
        };
        assert_eq!(polish(&cfg, Path::new("/nope"), "hola").unwrap(), "HOLA");
    }

    #[test]
    fn apple_status_reads_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(apple_status(&fake_helper(dir.path(), "off")), "off");
        assert_eq!(apple_status(Path::new("/nope/walkie-ai")), "missing");
    }

    #[test]
    fn every_status_has_a_description() {
        for s in ["off", "not-ready", "not-eligible", "unsupported", "missing"] {
            assert_ne!(describe_apple_status(s), describe_apple_status("?"), "{s}");
        }
        assert_eq!(describe_apple_status("available"), "ready");
    }

    #[test]
    fn identity_command_roundtrips() {
        assert_eq!(
            run_polish("cat", "hello world", Duration::from_secs(5)).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn command_transforms_stdin() {
        assert_eq!(
            run_polish("tr 'a-z' 'A-Z'", "hola", Duration::from_secs(5)).unwrap(),
            "HOLA"
        );
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
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "took {:?}",
            t0.elapsed()
        );
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
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "took {:?}",
            t0.elapsed()
        );
    }
}
