//! Spawning ffmpeg and ffprobe, and reading how a process ended. Mirrors
//! `command-other.go`, `command-windows.go`, `exitsignal_unix.go` and
//! `exitsignal_windows.go` in `internal/common/`.
//!
//! Deliberate differences from the Go code (exhaustive):
//! - Windows hides the console with `CREATE_NO_WINDOW`, where Go sets
//!   `HideWindow` (`STARTF_USESHOWWINDOW` + `SW_HIDE`). Both keep a console
//!   window from flashing up; the creation flag also works for a process that
//!   never shows a window of its own.
//! - The window is hidden by [`background_command`] itself, where Go makes each
//!   call site call `prepareBackgroundCommand`: one place cannot be forgotten.
//! - [`killed_by_signal`] takes the [`ExitStatus`], not an error: Rust has no
//!   `exec.ExitError` to unwrap. The signal names are Linux's (the table Go
//!   uses on Linux); an unknown number reads `signal N`.
//! - [`combined_output`] interleaves stdout and stderr in the one pipe, as
//!   `CombinedOutput` does, and decodes lossily (Go keeps raw bytes).
//! - `isStopRequest` / `shouldNotRetry` are not here: they classify the
//!   cancellation sentinels of the fallback cascade, not a process, and arrive
//!   with it (phase 4, PR 6).
//! - [`run_with_deadline`] is `exec.CommandContext` + `context.WithTimeout` in one: Go's
//!   context carries both the deadline and the caller's cancellation, here they are a
//!   [`Duration`] and an optional [`AtomicBool`], and [`RunEnd`] says which one ended the
//!   run. It polls `try_wait` every [`POLL_INTERVAL`] rather than take a crate
//!   (`wait-timeout` would only save the sleep); a cancellation is seen within that
//!   interval. A cancel flag already raised before the call starts nothing, as Go's
//!   `Start` refuses a done context.
//! - Known limit, shared with Go: the kill reaches the process that was started, not its
//!   children. On Windows a `.bat`/`.cmd` ffmpeg (accepted through `PATHEXT` as in Go)
//!   would make the kill reach only `cmd.exe`; a real ffmpeg is an `.exe`, so this stays.
//!   Where Go would then wait on the pipe for as long as a child holds it, the run here
//!   gives the reader [`READER_GRACE`] after a kill and returns the partial output.

use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command for `program` with `args`, one argument per element (never a
/// shell string), that opens no console window on Windows.
pub fn background_command<I, S>(program: impl AsRef<OsStr>, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs `cmd` to completion and returns its status with stdout and stderr
/// interleaved in the order written (Go's `CombinedOutput`).
///
/// `Err` only when the process cannot be started or read.
pub fn combined_output(mut cmd: Command) -> io::Result<(ExitStatus, String)> {
    let (mut reader, writer) = io::pipe()?;
    // Like Go's nil Stdin: the child reads end-of-file, never our terminal.
    cmd.stdin(Stdio::null());
    cmd.stdout(writer.try_clone()?);
    cmd.stderr(writer);
    let mut child = cmd.spawn()?;
    // The command still holds the write ends; the read below only ends once
    // every one of them is closed.
    drop(cmd);
    let mut bytes = Vec::new();
    let read = reader.read_to_end(&mut bytes);
    let status = child.wait()?;
    read?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

/// How often [`run_with_deadline`] looks at the process, the deadline and the cancel flag.
pub const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long [`run_with_deadline`] waits for the output of a process it has killed.
pub const READER_GRACE: Duration = Duration::from_secs(1);

/// Why [`run_with_deadline`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// The process ended by itself (with any exit code), including one that ended in the
    /// instant between the last look and the kill.
    Finished,
    /// The deadline passed; the process was killed and reaped.
    TimedOut,
    /// The cancel flag was raised; the process was killed and reaped.
    Cancelled,
}

/// What [`run_with_deadline`] saw.
#[derive(Debug)]
pub struct RunOutcome {
    /// How the process ended; for a killed one, the status of the kill. `None` only when
    /// the flag was already raised and nothing was started.
    pub status: Option<ExitStatus>,
    /// Stdout and stderr interleaved, as [`combined_output`] returns them. After a kill
    /// it is what was read within [`READER_GRACE`]: partial if a child of the killed
    /// process still held the pipe.
    pub output: String,
    pub end: RunEnd,
}

/// Runs `cmd` like [`combined_output`], but stops it when `deadline` passes or when
/// `cancel` is raised: the process is then killed and waited for, so none is left behind
/// (no zombie, no reader thread). Whatever it had written is still returned.
///
/// `Err` only when the process cannot be started or read.
pub fn run_with_deadline(
    mut cmd: Command,
    deadline: Duration,
    cancel: Option<&AtomicBool>,
) -> io::Result<RunOutcome> {
    let cancelled = || cancel.is_some_and(|flag| flag.load(Ordering::Acquire));
    if cancelled() {
        return Ok(RunOutcome {
            status: None,
            output: String::new(),
            end: RunEnd::Cancelled,
        });
    }
    let started = Instant::now();
    let (mut reader, writer) = io::pipe()?;
    cmd.stdin(Stdio::null());
    cmd.stdout(writer.try_clone()?);
    cmd.stderr(writer);
    let mut child = cmd.spawn()?;
    // As in combined_output: the read only ends once every write end is closed.
    drop(cmd);
    // Read while the process runs: a full pipe (about 64 KiB) would block it for good.
    // The bytes go to a shared buffer as they come, so that a reader abandoned after a
    // kill (see below) still leaves what it had read.
    let collected = Arc::new(Mutex::new(Vec::new()));
    let (done_tx, done_rx) = mpsc::channel::<io::Result<()>>();
    let sink = Arc::clone(&collected);
    let spawned = thread::Builder::new().spawn(move || {
        let mut chunk = [0u8; 8192];
        let result = loop {
            match reader.read(&mut chunk) {
                Ok(0) => break Ok(()),
                Ok(n) => sink
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => break Err(e),
            }
        };
        let _ = done_tx.send(result);
    });
    if let Err(e) = spawned {
        // No thread to read with: do not leave the child running unattended.
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }

    let waited = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok((status, RunEnd::Finished, false)),
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        let end = if cancelled() {
            RunEnd::Cancelled
        } else if started.elapsed() >= deadline {
            RunEnd::TimedOut
        } else {
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_sub(started.elapsed())));
            continue;
        };
        // kill() fails only if the process just ended on its own; wait() settles it.
        let _ = child.kill();
        break child
            .wait()
            .map(|status| (status, settle_end(end, &status), true));
    };
    if waited.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // A process that ended by itself closed its pipe; one that was killed may have left a
    // grandchild holding it open (a shim, a `.bat`), and waiting for that would hang the
    // caller. So after a kill the reader gets READER_GRACE and is then abandoned: it ends
    // by itself whenever the pipe closes, and the output is what it had read so far.
    let read = match &waited {
        Ok((_, _, false)) => done_rx.recv().unwrap_or(Ok(())),
        _ => done_rx.recv_timeout(READER_GRACE).unwrap_or(Ok(())),
    };
    let (status, end, _) = waited?;
    read?;
    let bytes = collected.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(RunOutcome {
        status: Some(status),
        output: String::from_utf8_lossy(&bytes).into_owned(),
        end,
    })
}

/// How a run that we killed really ended. The process may have ended on its own between
/// the last `try_wait` and the kill; then it reports a normal exit, not a death by the
/// kill, and it is `Finished` whatever stopped us. Unix tells a kill by its signal;
/// Windows `kill` is `TerminateProcess(.., 1)`, so only another exit code is certain to
/// be a normal exit (code 1 stays attributed to the kill, the safe side).
fn settle_end(wanted: RunEnd, status: &ExitStatus) -> RunEnd {
    #[cfg(unix)]
    let by_the_kill = killed_by_signal(status).is_some();
    #[cfg(not(unix))]
    let by_the_kill = status.code() == Some(1);
    if by_the_kill {
        wanted
    } else {
        RunEnd::Finished
    }
}

/// The name of the signal that ended a process, when one did.
///
/// The consequence for callers is the point: the out-of-memory diagnosis this
/// feeds only means something where a kill can be reported (Unix).
#[cfg(unix)]
pub fn killed_by_signal(status: &ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(signal_name)
}

/// Always `None` on Windows: a process there ends with an exit code and
/// nothing else, so there is no signal to name. Inventing a memory diagnosis
/// from a bare non-zero exit code would attach it to every ordinary failure.
#[cfg(not(unix))]
pub fn killed_by_signal(_status: &ExitStatus) -> Option<String> {
    None
}

/// Go's `syscall.Signal.String()` on Linux.
#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    const NAMES: [&str; 31] = [
        "hangup",
        "interrupt",
        "quit",
        "illegal instruction",
        "trace/breakpoint trap",
        "aborted",
        "bus error",
        "floating point exception",
        "killed",
        "user defined signal 1",
        "segmentation fault",
        "user defined signal 2",
        "broken pipe",
        "alarm clock",
        "terminated",
        "stack fault",
        "child exited",
        "continued",
        "stopped (signal)",
        "stopped",
        "stopped (tty input)",
        "stopped (tty output)",
        "urgent I/O condition",
        "CPU time limit exceeded",
        "file size limit exceeded",
        "virtual timer expired",
        "profiling timer expired",
        "window changed",
        "I/O possible",
        "power failure",
        "bad system call",
    ];
    usize::try_from(signal)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| NAMES.get(i))
        .map_or_else(|| format!("signal {signal}"), |name| (*name).to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialises the tests that write an executable and the ones that spawn
    /// a process. A fork made while another thread still has a script open for
    /// writing keeps that descriptor until it execs, and running the script in
    /// that window fails with "text file busy".
    pub(crate) static SPAWN_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn spawn_lock() -> std::sync::MutexGuard<'static, ()> {
        SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(unix)]
    fn killed_helper_process() -> ExitStatus {
        let _guard = spawn_lock();
        let mut child = background_command("sleep", ["30"])
            .spawn()
            .expect("cannot start a helper process on this machine");
        child.kill().expect("cannot kill the helper process");
        let status = child.wait().unwrap();
        assert!(!status.success(), "a killed process must report a failure");
        status
    }

    // Go: TestKilledBySignal_NamesTheSignal
    #[cfg(unix)]
    #[test]
    fn killed_by_signal_names_the_signal() {
        let signal = killed_by_signal(&killed_helper_process());
        assert_eq!(signal.as_deref(), Some("killed"));
    }

    // Go: TestKilledBySignal_IgnoresAnOrdinaryFailure (the "not an exit error"
    // half has no Rust counterpart: the argument is already an ExitStatus).
    #[cfg(unix)]
    #[test]
    fn killed_by_signal_ignores_an_ordinary_failure() {
        let _guard = spawn_lock();
        let status = background_command("sh", ["-c", "exit 3"]).status().unwrap();
        assert!(!status.success());
        assert_eq!(killed_by_signal(&status), None);
    }

    // No Go test: killedBySignal always reports false on Windows.
    #[cfg(windows)]
    #[test]
    fn killed_by_signal_is_always_none_on_windows() {
        let status = background_command("cmd", ["/C", "exit 3"])
            .status()
            .unwrap();
        assert!(!status.success());
        assert_eq!(killed_by_signal(&status), None);
    }

    #[cfg(unix)]
    #[test]
    fn signal_names_follow_go_and_unknown_numbers_are_numbered() {
        assert_eq!(signal_name(1), "hangup");
        assert_eq!(signal_name(15), "terminated");
        assert_eq!(signal_name(31), "bad system call");
        assert_eq!(signal_name(0), "signal 0");
        assert_eq!(signal_name(64), "signal 64");
        assert_eq!(signal_name(-1), "signal -1");
    }

    // No Go test: exec.Command with arguments, never a shell string (AGENTS.md).
    // The argument holds a space, a `;` and a `$`, which a shell would act on.
    #[cfg(unix)]
    #[test]
    fn arguments_reach_the_program_verbatim() {
        let _guard = spawn_lock();
        let cmd = background_command("sh", ["-c", "printf %s \"$1\"", "sh", "a b;c $HOME"]);
        let (status, output) = combined_output(cmd).unwrap();
        assert!(status.success());
        assert_eq!(output, "a b;c $HOME");
    }

    // No Go test: CombinedOutput keeps both streams.
    #[cfg(unix)]
    #[test]
    fn combined_output_keeps_both_streams_in_order() {
        let _guard = spawn_lock();
        let cmd = background_command("sh", ["-c", "echo out; echo err 1>&2; exit 2"]);
        let (status, output) = combined_output(cmd).unwrap();
        assert_eq!(status.code(), Some(2));
        assert_eq!(output, "out\nerr\n");
    }

    // No Go test: Go's nil Stdin is /dev/null. The child inspects its own
    // descriptor 0, so this cannot pass whatever `cargo test` itself reads from.
    #[cfg(target_os = "linux")]
    #[test]
    fn combined_output_gives_the_child_dev_null_as_stdin() {
        let _guard = spawn_lock();
        let cmd = background_command(
            "sh",
            ["-c", "[ \"$(readlink /proc/self/fd/0)\" = /dev/null ]"],
        );
        let (status, output) = combined_output(cmd).unwrap();
        assert!(status.success(), "stdin is not /dev/null: {output:?}");
    }

    #[test]
    fn combined_output_fails_when_the_program_does_not_exist() {
        let _guard = spawn_lock();
        let cmd = background_command("superview-definitely-not-a-real-tool", ["x"]);
        assert!(combined_output(cmd).is_err());
    }

    // ---- run_with_deadline ----

    /// `sh -c script`, whose first act is to print its own pid; `exec sleep` keeps that
    /// pid for the process that sleeps, with no child holding the pipe after a kill.
    #[cfg(unix)]
    const SLEEPS_FOR_A_LONG_TIME: &str = "echo $$; exec sleep 30";

    #[cfg(unix)]
    fn pid_exists(pid: &str) -> bool {
        // A zombie still answers `kill -0`: this tells "killed" from "killed and waited".
        background_command("kill", ["-0", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    // Go: exec.CommandContext + WithTimeout (TestProbeEncoder_BoundsItselfWithoutACallerDeadline)
    #[cfg(unix)]
    #[test]
    fn run_with_deadline_kills_and_reaps_at_the_deadline() {
        let _guard = spawn_lock();
        let started = Instant::now();
        let out = run_with_deadline(
            background_command("sh", ["-c", SLEEPS_FOR_A_LONG_TIME]),
            Duration::from_millis(1500),
            None,
        )
        .unwrap();
        assert_eq!(out.end, RunEnd::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(
            killed_by_signal(&out.status.unwrap()).as_deref(),
            Some("killed")
        );
        let pid = out.output.trim();
        assert!(
            !pid.is_empty(),
            "the pid was not captured: {:?}",
            out.output
        );
        assert!(
            !pid_exists(pid),
            "process {pid} is still there after the deadline"
        );
    }

    // Go: TestSizeProbeCache_DoesNotRememberACancelledProbe (the process half: the
    // caller's cancellation stops a running process).
    #[cfg(unix)]
    #[test]
    fn run_with_deadline_stops_on_cancellation() {
        let _guard = spawn_lock();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let out = thread::scope(|scope| {
            scope.spawn(|| {
                // Long enough for the shell to have printed its pid, even on a loaded
                // runner: the test needs that pid.
                thread::sleep(Duration::from_millis(1500));
                cancel.store(true, Ordering::Release);
            });
            run_with_deadline(
                background_command("sh", ["-c", SLEEPS_FOR_A_LONG_TIME]),
                Duration::from_secs(20),
                Some(&cancel),
            )
            .unwrap()
        });
        assert_eq!(out.end, RunEnd::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
        let pid = out.output.trim();
        assert!(
            !pid.is_empty(),
            "the pid was not captured: {:?}",
            out.output
        );
        assert!(
            !pid_exists(pid),
            "process {pid} is still there after the cancellation"
        );
    }

    // Go: TestProbeEncoders_StopsOnCancellation (a done context starts nothing).
    #[test]
    fn run_with_deadline_starts_nothing_when_already_cancelled() {
        let cancel = AtomicBool::new(true);
        // A program that does not exist: starting it would be an Err, not an outcome.
        let out = run_with_deadline(
            background_command("superview-definitely-not-a-real-tool", ["x"]),
            Duration::from_secs(1),
            Some(&cancel),
        )
        .unwrap();
        assert_eq!(out.end, RunEnd::Cancelled);
        assert!(out.status.is_none());
    }

    // No Go test: a finished process is reported as such, with its code and both streams.
    #[cfg(unix)]
    #[test]
    fn run_with_deadline_reports_a_process_that_finishes() {
        let _guard = spawn_lock();
        let cancel = AtomicBool::new(false);
        let out = run_with_deadline(
            background_command("sh", ["-c", "echo out; echo err 1>&2; exit 3"]),
            Duration::from_secs(20),
            Some(&cancel),
        )
        .unwrap();
        assert_eq!(out.end, RunEnd::Finished);
        assert_eq!(out.status.and_then(|s| s.code()), Some(3));
        assert_eq!(out.output, "out\nerr\n");
    }

    // No Go test (CombinedOutput reads while it waits too): more than a pipe holds must
    // not block the child, nor lose bytes. 4 MB, not just over 64 KiB: a Linux pipe is
    // 64 KiB by default but grows (up to 1 MiB, `pipe-max-size`), and 300 KB was
    // measured to fit without a reader.
    #[cfg(unix)]
    #[test]
    fn run_with_deadline_reads_more_output_than_a_pipe_holds() {
        let _guard = spawn_lock();
        let out = run_with_deadline(
            background_command("sh", ["-c", "yes | head -c 4000000"]),
            Duration::from_secs(20),
            None,
        )
        .unwrap();
        assert_eq!(out.end, RunEnd::Finished);
        assert_eq!(out.output.len(), 4_000_000);
    }

    #[test]
    fn run_with_deadline_fails_when_the_program_does_not_exist() {
        let _guard = spawn_lock();
        let cmd = background_command("superview-definitely-not-a-real-tool", ["x"]);
        assert!(run_with_deadline(cmd, Duration::from_secs(1), None).is_err());
    }

    // No Go test (CombinedOutput would wait on the pipe for as long as a child holds it):
    // a grandchild keeping the pipe open after the kill must not hold the caller. The run
    // happens in a thread so that the unbounded wait this guards against fails the test
    // instead of hanging it.
    #[cfg(unix)]
    #[test]
    fn run_with_deadline_returns_although_a_child_keeps_the_pipe() {
        let _guard = spawn_lock();
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        thread::spawn(move || {
            // The background sleep inherits stdout; the shell then becomes a sleep too.
            let cmd = background_command("sh", ["-c", "echo hello; sleep 5 & exec sleep 30"]);
            let _ = tx.send(run_with_deadline(cmd, Duration::from_millis(1500), None));
        });
        let out = rx
            .recv_timeout(Duration::from_secs(4))
            .expect("run_with_deadline waited for the grandchild's pipe")
            .unwrap();
        assert_eq!(out.end, RunEnd::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(out.output, "hello\n", "the partial output is kept");
    }

    // No Go test: the outcome of a kill is decided by how the process died.
    #[cfg(unix)]
    #[test]
    fn settle_end_tells_a_kill_from_a_normal_exit() {
        use std::os::unix::process::ExitStatusExt;
        // Raw wait statuses: 9 = killed by SIGKILL, 0 = exit(0), 3 << 8 = exit(3).
        for wanted in [RunEnd::TimedOut, RunEnd::Cancelled] {
            assert_eq!(settle_end(wanted, &ExitStatus::from_raw(9)), wanted);
            assert_eq!(
                settle_end(wanted, &ExitStatus::from_raw(0)),
                RunEnd::Finished
            );
            assert_eq!(
                settle_end(wanted, &ExitStatus::from_raw(3 << 8)),
                RunEnd::Finished
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn settle_end_tells_a_kill_from_a_normal_exit_on_windows() {
        use std::os::windows::process::ExitStatusExt;
        // TerminateProcess(.., 1) is what kill() does; any other code is a normal exit.
        for wanted in [RunEnd::TimedOut, RunEnd::Cancelled] {
            assert_eq!(settle_end(wanted, &ExitStatus::from_raw(1)), wanted);
            assert_eq!(
                settle_end(wanted, &ExitStatus::from_raw(0)),
                RunEnd::Finished
            );
            assert_eq!(
                settle_end(wanted, &ExitStatus::from_raw(3)),
                RunEnd::Finished
            );
        }
    }

    // L-50: the kill, deadline and cancellation paths on Windows. `ping.exe` is run
    // directly (no cmd.exe in between, so the kill reaches the process that waits).
    #[cfg(windows)]
    fn ping_for_a_long_time() -> Command {
        background_command("ping", ["-n", "30", "127.0.0.1"])
    }

    // Go: exec.CommandContext + WithTimeout, on Windows.
    #[cfg(windows)]
    #[test]
    fn run_with_deadline_times_out_a_running_exe() {
        let _guard = spawn_lock();
        let started = Instant::now();
        let out =
            run_with_deadline(ping_for_a_long_time(), Duration::from_millis(1000), None).unwrap();
        assert_eq!(out.end, RunEnd::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(out.status.is_some());
    }

    #[cfg(windows)]
    #[test]
    fn run_with_deadline_cancels_a_running_exe() {
        let _guard = spawn_lock();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let out = thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(1000));
                cancel.store(true, Ordering::Release);
            });
            run_with_deadline(
                ping_for_a_long_time(),
                Duration::from_secs(20),
                Some(&cancel),
            )
            .unwrap()
        });
        assert_eq!(out.end, RunEnd::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
