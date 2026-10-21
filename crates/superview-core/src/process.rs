//! Spawning ffmpeg and ffprobe, and reading how a process ended.
//!
//! Design notes (exhaustive):
//! - Windows hides the console with `CREATE_NO_WINDOW` (rather than
//!   `STARTF_USESHOWWINDOW` + `SW_HIDE`). It keeps a console window from flashing up
//!   and also works for a process that never shows a window of its own.
//! - The window is hidden by [`background_command`] itself, not at each call site: one
//!   place cannot be forgotten.
//! - [`killed_by_signal`] takes the [`ExitStatus`]. The signal names are Linux's; an
//!   unknown number reads `signal N`.
//! - [`combined_output`] interleaves stdout and stderr in the one pipe and decodes
//!   lossily. Captured output is capped at [`CAPTURE_CAP`] bytes at each end (F-04).
//! - The classification of the cancellation sentinels of the fallback cascade is not here:
//!   it concerns the cascade, not a process (`error.rs`).
//! - [`run_with_deadline`] runs a process bounded by a deadline and a cancellation, which
//!   are a [`Duration`] and an optional [`AtomicBool`]; [`RunEnd`] says which one ended the
//!   run. It polls `try_wait` every [`POLL_INTERVAL`] rather than take a crate
//!   (`wait-timeout` would only save the sleep); a cancellation is seen within that
//!   interval. A cancel flag already raised before the call starts nothing.
//! - Known limit: the kill reaches the process that was started, not its
//!   children. On Windows a `.bat`/`.cmd` ffmpeg would make the kill reach only
//!   `cmd.exe`; the `PATH` search therefore refuses them (`tools.rs`).
//!   Rather than wait on the pipe for as long as a child holds it, the run
//!   gives the reader [`READER_GRACE`] after a kill and returns the partial output.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
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

/// How much of a captured stream is kept at each end (F-04). ffmpeg at `-loglevel` verbose
/// can write megabytes of stderr; the first lines say what it was doing and the last ones why
/// it stopped, and the middle is not worth the memory nor a window the size of a screen.
pub const CAPTURE_CAP: usize = 64 * 1024;

/// The first and the last `cap` bytes of a stream, with a count of what fell between.
#[derive(Debug)]
pub struct CappedBuffer {
    cap: usize,
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: u64,
}

impl CappedBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            head: Vec::new(),
            tail: VecDeque::new(),
            dropped: 0,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        let room = self.cap - self.head.len();
        let (first, rest) = bytes.split_at(room.min(bytes.len()));
        self.head.extend_from_slice(first);
        self.tail.extend(rest);
        if self.tail.len() > self.cap {
            let excess = self.tail.len() - self.cap;
            self.tail.drain(..excess);
            self.dropped += excess as u64;
        }
    }

    /// What was kept, decoded lossily (a cut can fall inside a character), with a marker
    /// where bytes were dropped.
    pub fn render(&self) -> String {
        let mut bytes = self.head.clone();
        if self.dropped > 0 {
            bytes.extend_from_slice(
                format!("\n[... {} bytes dropped ...]\n", self.dropped).as_bytes(),
            );
        }
        bytes.extend(self.tail.iter());
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// A [`CappedBuffer`] being filled by a reader thread, and the channel that says it is done.
pub type Collector = (Arc<Mutex<CappedBuffer>>, Receiver<io::Result<()>>);

/// Reads `reader` to its end on a thread of its own, into a shared [`CappedBuffer`]. The
/// bytes go to the buffer as they come, so a reader abandoned after a kill still leaves what
/// it had read. The receiver gets one message when the stream ends: `Ok` at end-of-file,
/// the error that stopped the read otherwise.
pub fn spawn_collector<R: Read + Send + 'static>(reader: R, cap: usize) -> io::Result<Collector> {
    let buffer = Arc::new(Mutex::new(CappedBuffer::new(cap)));
    let mut reader = reader;
    let (done_tx, done_rx) = mpsc::channel::<io::Result<()>>();
    let sink = Arc::clone(&buffer);
    thread::Builder::new().spawn(move || {
        let mut chunk = [0u8; 8192];
        let result = loop {
            match reader.read(&mut chunk) {
                Ok(0) => break Ok(()),
                Ok(n) => sink
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => break Err(e),
            }
        };
        let _ = done_tx.send(result);
    })?;
    Ok((buffer, done_rx))
}

/// `child.wait()`, killing and reaping the child if the wait itself fails, so that no
/// process or reader is left behind.
fn wait_or_kill(child: &mut Child) -> io::Result<ExitStatus> {
    child.wait().inspect_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
    })
}

/// The text a collector has kept so far.
pub fn collected(buffer: &Mutex<CappedBuffer>) -> String {
    buffer.lock().unwrap_or_else(|e| e.into_inner()).render()
}

/// Runs `cmd` to completion and returns its status with stdout and stderr
/// interleaved in the order written, each end capped at
/// [`CAPTURE_CAP`]. The cap suits text for a person; data to parse should not go through
/// here once it can pass 128 KiB (use [`split_output`] and read stdout).
///
/// `Err` only when the process cannot be started or read.
pub fn combined_output(mut cmd: Command) -> io::Result<(ExitStatus, String)> {
    let (reader, writer) = io::pipe()?;
    // The child reads end-of-file, never our terminal.
    cmd.stdin(Stdio::null());
    cmd.stdout(writer.try_clone()?);
    cmd.stderr(writer);
    let mut child = cmd.spawn()?;
    // The command still holds the write ends; the read below only ends once
    // every one of them is closed.
    drop(cmd);
    let collector = spawn_collector(reader, CAPTURE_CAP);
    let (buffer, done) = match collector {
        Ok(pair) => pair,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let status = wait_or_kill(&mut child)?;
    done.recv().unwrap_or(Ok(()))?;
    Ok((status, collected(&buffer)))
}

/// Runs `cmd` to completion and returns its status with stdout and stderr apart, each
/// capped at [`CAPTURE_CAP`]. For a tool whose stdout is data to parse: a warning on stderr
/// must not end up inside it (F-05).
///
/// `Err` only when the process cannot be started or read.
pub fn split_output(cmd: Command) -> io::Result<(ExitStatus, String, String)> {
    split_output_within(cmd, Duration::MAX)
}

/// [`split_output`] that gives up after `deadline` (F-17): the process is killed and reaped
/// and the error is [`io::ErrorKind::TimedOut`]. After a normal end the readers get
/// [`READER_GRACE`], as in [`run_with_deadline`]; after a kill they are abandoned.
pub fn split_output_within(
    mut cmd: Command,
    deadline: Duration,
) -> io::Result<(ExitStatus, String, String)> {
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    drop(cmd);
    let (Some(out), Some(err)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::other("the output pipes are not available"));
    };
    let collectors =
        spawn_collector(out, CAPTURE_CAP).and_then(|o| Ok((o, spawn_collector(err, CAPTURE_CAP)?)));
    let ((stdout, out_done), (stderr, err_done)) = match collectors {
        Ok(pair) => pair,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(timed_out(deadline));
            }
            Ok(None) => {
                thread::sleep(POLL_INTERVAL.min(deadline.saturating_sub(started.elapsed())))
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
    };
    // A child that kept a pipe open must not hold the caller: READER_GRACE for both ends.
    let grace_until = Instant::now() + READER_GRACE;
    for done in [&out_done, &err_done] {
        done.recv_timeout(grace_until.saturating_duration_since(Instant::now()))
            .unwrap_or(Ok(()))?;
    }
    Ok((status, collected(&stdout), collected(&stderr)))
}

fn timed_out(deadline: Duration) -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out after {}s", deadline.as_secs().max(1)),
    )
}

/// [`combined_output`] that gives up after `deadline` (F-17), like [`split_output_within`].
pub fn combined_output_within(
    cmd: Command,
    deadline: Duration,
) -> io::Result<(ExitStatus, String)> {
    let outcome = run_with_deadline(cmd, deadline, None)?;
    match (outcome.end, outcome.status) {
        (RunEnd::Finished, Some(status)) => Ok((status, outcome.output)),
        _ => Err(timed_out(deadline)),
    }
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
    let (reader, writer) = io::pipe()?;
    cmd.stdin(Stdio::null());
    cmd.stdout(writer.try_clone()?);
    cmd.stderr(writer);
    let mut child = cmd.spawn()?;
    // As in combined_output: the read only ends once every write end is closed.
    drop(cmd);
    // Read while the process runs: a full pipe (about 64 KiB) would block it for good.
    let (collected_output, done_rx) = match spawn_collector(reader, CAPTURE_CAP) {
        Ok(pair) => pair,
        Err(e) => {
            // No thread to read with: do not leave the child running unattended.
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };

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
    // A process, killed or not, may have left a grandchild holding the pipe open (a shim, a
    // `.bat`, a `sleep &`), and waiting for that would hang the caller (F-18). So the reader
    // gets READER_GRACE and is then abandoned: it ends by itself whenever the pipe closes,
    // and the output is what it had read so far.
    let read = done_rx.recv_timeout(READER_GRACE).unwrap_or(Ok(()));
    let (status, end, _) = waited?;
    read?;
    Ok(RunOutcome {
        status: Some(status),
        output: collected(&collected_output),
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

/// The name of a signal, from the Linux table. Not Unix-only: [`crate::error::signal_description`]
/// names the signal behind a stop request on every platform. (No effect beyond 2 and 15:
/// only those are ever listened to.)
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
