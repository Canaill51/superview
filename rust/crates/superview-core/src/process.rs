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

use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};

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
}
