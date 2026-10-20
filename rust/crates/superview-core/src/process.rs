//! Spawning ffmpeg and ffprobe, and reading how a process ended. Mirrors
//! `command-other.go`, `command-windows.go`, `exitsignal_unix.go` and
//! `exitsignal_windows.go` in Go's `internal/common/` (removed in phase 8; last release with it: v0.2.9).
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
//!   children. On Windows a `.bat`/`.cmd` ffmpeg would make the kill reach only
//!   `cmd.exe`; the `PATH` search therefore refuses them (`tools.rs`), where Go accepts them.
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

/// Go's `syscall.Signal.String()` on Linux. Not Unix-only: [`crate::error::signal_description`]
/// names the signal behind a stop request on every platform. (Go's Windows table names only 1 to
/// 15 and prints "signal 16" beyond; this one keeps Linux's names. No effect: only 2 and 15 are
/// ever listened to.)
pub(crate) fn signal_name(signal: i32) -> String {
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
pub(crate) mod tests;
