use crate::config::{Polish, PolishProvider};
use crate::pipeline::style;
use anyhow::{Context as _, Result};
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Polishes `input` with the configured provider. `helper` is walkie-ai
/// (see [`apple_helper`]); only the Apple provider uses it, and only it
/// writes in the configured tone.
pub fn polish(cfg: &Polish, helper: &Path, input: &str) -> Result<String> {
    let timeout = Duration::from_secs(cfg.timeout_secs);
    match cfg.provider {
        PolishProvider::Command => run_polish(&cfg.command, input, timeout),
        PolishProvider::Apple => {
            let mut cmd = Command::new(helper);
            cmd.arg("respond").arg(style::prompt(cfg.tone));
            let out = run(cmd, "Apple model", input, timeout)?;
            anyhow::ensure!(
                !answered(input, &out),
                "Apple's model answered the text instead of cleaning it up"
            );
            Ok(style::finish(&out, cfg.tone))
        }
    }
}

/// Cleaning up never makes text much longer. A reply that is, is the small
/// model answering or acting on what was dictated ("write me a poem…").
fn answered(input: &str, out: &str) -> bool {
    out.chars().count() > input.chars().count() * 3 / 2 + 40
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

/// Starts loading Apple's model in the background and returns at once.
/// After a few idle minutes macOS unloads it, and the first call then takes
/// 4–8s; started when a polish recording begins, loading overlaps speaking.
pub fn prewarm_apple(helper: &Path) {
    let mut cmd = Command::new(helper);
    cmd.arg("prewarm")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(mut child) = spawn_retrying_etxtbsy(&mut cmd) {
        thread::spawn(move || child.wait());
    }
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

/// `Command::spawn`, retrying briefly on ETXTBSY: a file written and chmod
/// +x moments earlier (our own test fixtures below; a freshly-installed
/// walkie-ai) can transiently fail exec on Linux if some unrelated thread's
/// `fork()` duplicated an fd that was open on it for writing — the write
/// closes before the fork's *own* exec, but the kernel's "still open for
/// writing" bookkeeping on the file doesn't clear until that forked child
/// execs or exits. Not a thing on macOS (`Command::spawn` there doesn't
/// fork), so this never retries there.
fn spawn_retrying_etxtbsy(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    for attempt in 0.. {
        match cmd.spawn() {
            Err(e) if attempt < 20 && e.raw_os_error() == Some(libc::ETXTBSY) => {
                thread::sleep(Duration::from_millis(5));
            }
            r => return r,
        }
    }
    unreachable!()
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
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group (pgid == child pid) so a timeout kill can
        // target the whole group, taking any forked descendants with it.
        .process_group(0);
    let mut child = spawn_retrying_etxtbsy(&mut cmd).with_context(|| format!("starting {what}"))?;

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
    use crate::config::Tone;
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

    fn apple() -> Polish {
        Polish {
            provider: PolishProvider::Apple,
            ..Polish::default()
        }
    }

    #[test]
    fn apple_provider_sends_the_tones_prompt_and_text_to_the_helper() {
        let dir = tempfile::tempdir().unwrap();
        let (p, seen) = (dir.path().join("walkie-ai"), dir.path().join("prompt"));
        let script = format!(
            "#!/bin/sh\n[ \"$1\" = respond ] && printf '%s' \"$2\" > {}; tr a-z A-Z\n",
            seen.display()
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let mut cfg = apple();
        cfg.tone = Tone::Excited;
        let out = polish(&cfg, &p, "hi there").unwrap();
        assert_eq!(out, "HI THERE");
        assert_eq!(
            std::fs::read_to_string(seen).unwrap(),
            style::prompt(Tone::Excited)
        );
    }

    #[test]
    fn apple_reply_gets_the_tone() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("walkie-ai");
        std::fs::write(
            &p,
            "#!/bin/sh\ncat >/dev/null\necho 'See you at 5. I will bring it.'\n",
        )
        .unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let said = "see you at five i will bring it";
        let out = |tone| {
            let cfg = Polish { tone, ..apple() };
            polish(&cfg, &p, said).unwrap()
        };
        assert_eq!(out(Tone::VeryCasual), "see you at 5. i will bring it");
        assert_eq!(out(Tone::Excited), "See you at 5. I will bring it!");
        assert_eq!(out(Tone::Formal), "See you at 5. I will bring it.");
    }

    #[test]
    fn apple_reply_much_longer_than_the_dictation_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("walkie-ai");
        let poem = "Cats are graceful creatures. ".repeat(10);
        std::fs::write(&p, format!("#!/bin/sh\ncat >/dev/null\necho '{poem}'\n")).unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let err = polish(&apple(), &p, "write me a poem about cats").unwrap_err();
        assert!(err.to_string().contains("answered"), "{err}");
    }

    #[test]
    fn answered_allows_a_tidied_reply_of_similar_length() {
        let said = "um so i think we should uh ship it on on friday no wait thursday";
        assert!(!answered(said, "I think we should ship it on Thursday."));
        assert!(!answered("hi", "Hi."), "short dictations get slack");
        assert!(answered("hi", &"x".repeat(60)));
    }

    #[test]
    fn apple_provider_reports_a_missing_helper() {
        let err = polish(&apple(), Path::new("/nope/walkie-ai"), "x").unwrap_err();
        assert!(err.to_string().contains("starting Apple model"), "{err}");
    }

    #[test]
    fn command_provider_runs_the_shell_command() {
        let cfg = Polish {
            provider: PolishProvider::Command,
            command: "tr a-z A-Z".into(),
            ..Polish::default()
        };
        let cfg = Polish {
            tone: Tone::VeryCasual,
            ..cfg
        };
        let out = polish(&cfg, Path::new("/nope"), "hola").unwrap();
        assert_eq!(out, "HOLA", "no tone: the command has its own prompt");
    }

    #[test]
    fn prewarm_runs_the_helper_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("warmed");
        let p = dir.path().join("walkie-ai");
        let script = format!(
            "#!/bin/sh\nsleep 1\n[ \"$1\" = prewarm ] && touch {}\n",
            marker.display()
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let t0 = Instant::now();
        prewarm_apple(&p);
        assert!(t0.elapsed() < Duration::from_millis(500), "prewarm blocked");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(marker.exists(), "walkie-ai prewarm never ran");
        prewarm_apple(Path::new("/nope/walkie-ai")); // a missing helper is fine
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
