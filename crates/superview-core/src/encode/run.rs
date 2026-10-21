//! One ffmpeg pass, from launch to its end: the `run` closure of `EncodeVideo` in
//! Go's `internal/common/encode.go` (removed in phase 8; last release with it: v0.2.9), with the stop reasons of `errors.go`.
//!
//! The command arrives already built (the `-hwaccel` option, [`build_encode_base_args`] and the
//! output path are assembled by the caller, the cascade); this module starts it, follows its
//! `-progress pipe:1` output and says how it ended: success, a user cancellation, a system
//! signal, or a failure (classified, and marked when frames had already been written).
//!
//! [`build_encode_base_args`]: crate::encode::args::build_encode_base_args
//!
//! Deliberate differences from the Go code (exhaustive):
//! - No signal handler is installed here (L-114: no process-wide state). Go registers
//!   `os.Interrupt` and `SIGTERM` inside the run; here the application does it and raises
//!   [`StopControl::signal`] with the signal's number (0 = none), and the cancel button
//!   raises [`StopControl::cancel`]. A signal wins over a cancellation when both are up
//!   (Go's `select` picks one at random).
//! - A stop requested before the call starts nothing: Go starts ffmpeg and kills it at once.
//!   One process less, same result (`Cancelled` / `StoppedBySignal`), and the cascade's rule
//!   "a stop request launches no ffmpeg" (E-02) holds literally.
//! - The child is not shared between a watcher and a waiter. Go has a goroutine kill it; here
//!   the one loop of [`run_ffmpeg`] looks at the process and at both flags every
//!   [`POLL`] (so a stop is seen within that time, as in `process.rs`), reads the progress
//!   lines a thread sends it, and calls `on_progress` itself: the callback needs no `Send`.
//! - A run that fails on its own while a stop is requested returns the stop reason, as Go's
//!   goroutine would have killed it and filled `stopped` first, but only when ffmpeg left with
//!   an exit code. This is the case of Ctrl+C in a terminal, where ffmpeg gets the SIGINT too
//!   and leaves (code 255) before the application has looked at its flag. An ffmpeg killed by
//!   a signal we did not send keeps its diagnosis: the out-of-memory killer takes ffmpeg
//!   (SIGKILL) and systemd stops the application with a SIGTERM a few milliseconds later; if
//!   that SIGTERM fell in the window (a poll, or the grace if a child holds the pipes), the
//!   stop reason would hide the memory diagnosis that `ErrStoppedBySignal` exists to keep
//!   (U-09). Whenever the stop reason replaces a real failure, the classified failure, stderr
//!   included, is logged at warn first. A run that succeeds is a success whatever is
//!   requested after it (Go reads `stopped` only when `Wait` fails). If ffmpeg succeeds between
//!   the last `try_wait` and the stop check, the kill path answers `Cancelled` for a complete
//!   output: a race Go has too.
//! - After a kill Go joins the progress reader without a bound and `Wait`s on the stderr copy.
//!   Here the readers get [`READER_GRACE`] in all, as in `run_with_deadline` (#110), and are
//!   then abandoned: a grandchild of ffmpeg that keeps the pipes open cannot hold the caller. The
//!   same grace applies to the lines still queued when ffmpeg ends by itself. The
//!   known limit shared with Go: the kill reaches the process that was started, not its
//!   children; on Windows a `.bat`/`.cmd` ffmpeg would be killed through `cmd.exe` only, which is
//!   why the `PATH` search refuses them (`tools.rs`).
//! - The process is killed and reaped on every way out of the function, an error or a panic
//!   of the callback included ([`Running`]); Go relies on its goroutine and a `defer`.
//! - The progress pipe is read as bytes and decoded lossily: an invalid byte is a replacement
//!   character, not the end of the progress. The lines are not limited in length (Go cuts them
//!   at 4096 bytes); `-progress pipe:1` always ends its lines, and they are short. After a read error the reader drops its end of
//!   the pipe, so ffmpeg gets a broken pipe instead of blocking on a pipe nobody reads (Go
//!   would leave it blocked). It then dies of SIGPIPE and is classified as killed by the
//!   system, a wrong "ran out of memory" diagnosis; a read error on a pipe is close to
//!   impossible, and the warning above is logged first.
//! - The `wait` after a kill is not bounded, as Go's `cmd.Wait()` is: an ffmpeg stuck in
//!   uninterruptible sleep (a GPU driver, an NFS output) holds the caller.
//! - `TestEncodeVideo_ReturnsStdoutPipeError` has no equivalent: the `commandStdoutPipe`
//!   seam does not exist (a pipe that cannot be opened is a launch failure here).
//! - The logs are `log` records with `key=value` text, where Go uses `slog` attributes.

use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crate::encode::failure::classify_ffmpeg_failure_now;
use crate::encode::progress::{ProgressLine, log_progress_line, parse_progress_line, percent};
use crate::error::{Error, signal_description};
use crate::memory::format_available_memory;
use crate::process::{CAPTURE_CAP, READER_GRACE, collected, killed_by_signal, spawn_collector};

/// How often the loop looks at the process and at the two flags when nothing arrives.
const POLL: Duration = Duration::from_millis(20);

/// Lines the reader may queue before it waits for the loop: a callback that is slow slows
/// ffmpeg down instead of filling memory.
const LINE_QUEUE: usize = 1024;

/// What can stop a run from outside. The application writes them; the run only reads.
#[derive(Clone, Copy)]
pub struct StopControl<'a> {
    /// Raised when the user cancels (Go's `cancel` channel).
    pub cancel: &'a AtomicBool,
    /// 0 while nothing happened, else the number of the signal the system sent the
    /// application (2 = interrupt, 15 = terminated).
    pub signal: &'a AtomicI32,
}

/// The error for a stop that has been requested, if one has. A signal first: it carries
/// the cause, and says it was not the user.
pub(crate) fn stop_requested(stop: &StopControl) -> Option<Error> {
    let signal = stop.signal.load(Ordering::Acquire);
    if signal != 0 {
        let name = signal_description(signal);
        // Error, not Warn: this is the line that has to survive in a log the user sends
        // with a bug report, and the memory reading beside it is what says whether the
        // machine was out of room.
        log::error!(
            "The system signalled the application to stop: signal={name} memory_available={}",
            format_available_memory()
        );
        return Some(Error::StoppedBySignal(name));
    }
    stop.cancel
        .load(Ordering::Acquire)
        .then_some(Error::Cancelled)
}

/// The child process, killed and reaped on every way out unless it was seen ending.
struct Running {
    child: Child,
    reaped: bool,
}

impl Running {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        self.reaped |= status.is_some();
        Ok(status)
    }

    /// Without the wait the killed process stays a zombie for the life of the application.
    /// The errors are discarded on purpose: kill fails only when the process just ended, and
    /// its status after our kill is not news.
    fn kill_and_reap(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.kill_and_reap();
    }
}

/// How the progress reader ended.
#[derive(Debug)]
enum ReaderEnd {
    Eof,
    Failed(io::Error),
    /// Nobody takes the lines any more.
    Abandoned,
}

/// Reads `reader` line by line (bytes, decoded lossily) and hands each line to `send`,
/// which answers whether anyone still listens. Any error ends it: looping on an error would
/// spin forever and freeze the encode with no diagnostic (the pipe is already broken).
fn read_progress_lines<R: Read>(reader: R, mut send: impl FnMut(String) -> bool) -> ReaderEnd {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return ReaderEnd::Eof,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                if !send(text.trim_end_matches(['\n', '\r']).to_string()) {
                    return ReaderEnd::Abandoned;
                }
            }
            Err(e) => return ReaderEnd::Failed(e),
        }
    }
}

/// What one progress line changes: the log, the "frames were written" mark, the callback.
fn handle_line(
    line: &str,
    duration_float: f64,
    produced: &mut bool,
    on_progress: &mut dyn FnMut(f64),
) {
    let parsed = parse_progress_line(line);
    log_progress_line(&parsed);
    if let ProgressLine::Value(time) = &parsed {
        *produced |= parsed.produced_output();
        on_progress(percent(*time, duration_float));
    }
}

/// Waits for `done` until `deadline`; a reader that is not done by then is abandoned.
fn wait_until(done: &Receiver<io::Result<()>>, deadline: Instant) {
    let _ = done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
}

/// Runs `cmd` (ffmpeg with its arguments, `-progress pipe:1` among them) once.
///
/// `duration_float` is the clip's duration in seconds, for the percentage handed to
/// `on_progress` on each progress line. The call returns when ffmpeg has ended, and
/// nothing is left behind it: the process is killed and reaped if a stop was requested.
///
/// - `Ok(())`: ffmpeg exited with success.
/// - [`Error::Cancelled`] / [`Error::StoppedBySignal`]: a stop was requested (nothing is
///   started if it already had been).
/// - [`Error::FailedAfterOutput`]: ffmpeg failed after a frame had been written.
/// - Any other error: the failure, classified (`classify_ffmpeg_failure`), or a process
///   that could not be started (`failed to start ffmpeg: ...`).
pub fn run_ffmpeg(
    mut cmd: Command,
    duration_float: f64,
    stop: &StopControl,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(), Error> {
    if let Some(reason) = stop_requested(stop) {
        return Err(reason);
    }
    let start_failure = |e: io::Error| Error::Plain(format!("failed to start ffmpeg: {e}"));
    // Like Go's nil Stdin: ffmpeg reads end-of-file, never our terminal.
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let child = cmd.spawn().map_err(start_failure)?;
    drop(cmd);
    let mut running = Running {
        child,
        reaped: false,
    };
    let (Some(stdout), Some(stderr)) = (running.child.stdout.take(), running.child.stderr.take())
    else {
        return Err(Error::Plain(
            "failed to start ffmpeg: its output pipes are not available".into(),
        ));
    };

    // Progress: a thread reads the lines and sends them, the loop below handles them.
    let (line_tx, lines) = mpsc::sync_channel::<String>(LINE_QUEUE);
    thread::Builder::new()
        .spawn(
            move || match read_progress_lines(stdout, |line| line_tx.send(line).is_ok()) {
                ReaderEnd::Eof => log::debug!("Encoding complete"),
                ReaderEnd::Failed(e) => {
                    log::warn!("Progress reader stopped on error: error={e}");
                }
                ReaderEnd::Abandoned => {}
            },
        )
        .map_err(start_failure)?;

    // Stderr: read while the process runs, or a full pipe would block it for good. Only
    // the two ends are kept (F-04), and a reader abandoned after the grace still leaves
    // what it had read.
    let (stderr_bytes, stderr_done) =
        spawn_collector(stderr, CAPTURE_CAP).map_err(start_failure)?;

    // Set once ffmpeg has reported a frame leaving the filter graph.
    let mut produced = false;
    let mut lines_open = true;
    let status = loop {
        if lines_open {
            match lines.recv_timeout(POLL) {
                Ok(line) => handle_line(&line, duration_float, &mut produced, on_progress),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => lines_open = false,
            }
        } else {
            thread::sleep(POLL);
        }
        match running.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                // `running` kills and reaps on the way out.
                return Err(Error::Plain(format!("failed to wait for ffmpeg: {e}")));
            }
        }
        if let Some(reason) = stop_requested(stop) {
            running.kill_and_reap();
            let deadline = Instant::now() + READER_GRACE;
            // Discard what is left so that a reader waiting to send can end.
            while lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .is_ok()
                && Instant::now() < deadline
            {}
            wait_until(&stderr_done, deadline);
            return Err(reason);
        }
    };

    // ffmpeg ended by itself: take the lines it left (the "frames were written" mark depends
    // on all of them), then its stderr.
    let deadline = Instant::now() + READER_GRACE;
    // (A reader that has ended has dropped its sender: this returns at once.)
    while let Ok(line) = lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        handle_line(&line, duration_float, &mut produced, on_progress);
        if Instant::now() >= deadline {
            break;
        }
    }
    wait_until(&stderr_done, deadline);

    if status.success() {
        return Ok(());
    }
    let stderr_text = collected(&stderr_bytes);
    let failure = classify_ffmpeg_failure_now(&status, &stderr_text);
    if killed_by_signal(&status).is_none()
        && let Some(reason) = stop_requested(stop)
    {
        log::warn!("ffmpeg failed while a stop was requested: {failure}");
        return Err(reason);
    }
    Err(if produced {
        Error::FailedAfterOutput(Box::new(failure))
    } else {
        failure
    })
}

#[cfg(test)]
mod tests;
