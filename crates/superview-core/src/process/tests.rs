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

#[cfg(unix)]
#[test]
fn killed_by_signal_names_the_signal() {
    let signal = killed_by_signal(&killed_helper_process());
    assert_eq!(signal.as_deref(), Some("killed"));
}

// An ordinary failure is not a kill (the "not an exit error"
// case has no counterpart: the argument is already an ExitStatus).
#[cfg(unix)]
#[test]
fn killed_by_signal_ignores_an_ordinary_failure() {
    let _guard = spawn_lock();
    let status = background_command("sh", ["-c", "exit 3"]).status().unwrap();
    assert!(!status.success());
    assert_eq!(killed_by_signal(&status), None);
}

// Always reports false on Windows.
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
fn signal_names_follow_v0_2_9_and_unknown_numbers_are_numbered() {
    assert_eq!(signal_name(1), "hangup");
    assert_eq!(signal_name(15), "terminated");
    assert_eq!(signal_name(31), "bad system call");
    assert_eq!(signal_name(0), "signal 0");
    assert_eq!(signal_name(64), "signal 64");
    assert_eq!(signal_name(-1), "signal -1");
}

// Arguments, never a shell string (AGENTS.md).
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

// Both streams are kept.
#[cfg(unix)]
#[test]
fn combined_output_keeps_both_streams_in_order() {
    let _guard = spawn_lock();
    let cmd = background_command("sh", ["-c", "echo out; echo err 1>&2; exit 2"]);
    let (status, output) = combined_output(cmd).unwrap();
    assert_eq!(status.code(), Some(2));
    assert_eq!(output, "out\nerr\n");
}

// A missing stdin is /dev/null. The child inspects its own
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

// A deadline bounds the process.
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

// A cancelled probe is not remembered (the process half: the
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

// A cancel flag already raised starts nothing.
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

// A finished process is reported as such, with its code and both streams.
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

// More than a pipe holds must
// not block the child. 4 MB, not just over 64 KiB: a Linux pipe is
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
    // Read to the end without blocking the process, but kept bounded (F-04): the two ends
    // and the count of what fell between.
    let dropped = 4_000_000 - 2 * CAPTURE_CAP;
    assert!(
        out.output
            .contains(&format!("[... {dropped} bytes dropped ...]")),
        "dropped marker missing"
    );
    assert_eq!(
        out.output.len(),
        2 * CAPTURE_CAP + format!("\n[... {dropped} bytes dropped ...]\n").len()
    );
}

#[test]
fn run_with_deadline_fails_when_the_program_does_not_exist() {
    let _guard = spawn_lock();
    let cmd = background_command("superview-definitely-not-a-real-tool", ["x"]);
    assert!(run_with_deadline(cmd, Duration::from_secs(1), None).is_err());
}

// A grandchild keeping the pipe open after the kill must not hold the caller. The run
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

// The outcome of a kill is decided by how the process died.
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

// A deadline bounds the process, on Windows.
#[cfg(windows)]
#[test]
fn run_with_deadline_times_out_a_running_exe() {
    let _guard = spawn_lock();
    let started = Instant::now();
    let out = run_with_deadline(ping_for_a_long_time(), Duration::from_millis(1000), None).unwrap();
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

#[test]
fn a_capped_buffer_keeps_everything_under_the_cap() {
    let mut buffer = CappedBuffer::new(8);
    buffer.push(b"abc");
    buffer.push(b"defgh");
    assert_eq!(buffer.render(), "abcdefgh");
}

#[test]
fn a_capped_buffer_keeps_both_ends_and_counts_the_middle() {
    let mut buffer = CappedBuffer::new(4);
    // In pieces that straddle the head, the tail and the trimming.
    for piece in ["01", "2345", "6789", "abcdef"] {
        buffer.push(piece.as_bytes());
    }
    assert_eq!(buffer.render(), "0123\n[... 8 bytes dropped ...]\ncdef");
}

#[cfg(unix)]
#[test]
fn split_output_keeps_stdout_and_stderr_apart() {
    let _guard = spawn_lock();
    let cmd = background_command("sh", ["-c", "echo out; echo err 1>&2; exit 3"]);
    let (status, stdout, stderr) = split_output(cmd).unwrap();
    assert_eq!(status.code(), Some(3));
    assert_eq!((stdout.as_str(), stderr.as_str()), ("out\n", "err\n"));
}

// F-18: a process that ends by itself but leaves a child holding the pipe must not hold the
// caller too -- the reader gets READER_GRACE here as well as after a kill.
#[cfg(unix)]
#[test]
fn run_with_deadline_does_not_wait_for_a_child_that_keeps_the_pipe() {
    let _guard = spawn_lock();
    let started = Instant::now();
    let out = run_with_deadline(
        background_command("sh", ["-c", "sleep 6 & echo hello"]),
        Duration::from_secs(30),
        None,
    )
    .unwrap();
    assert_eq!(out.end, RunEnd::Finished);
    assert!(out.output.contains("hello"), "{:?}", out.output);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "waited {:?} for the pipe",
        started.elapsed()
    );
}

// F-17: the bounded variants give up on a process that never ends, and reap it.
#[cfg(unix)]
#[test]
fn combined_output_within_gives_up_at_the_deadline() {
    let _guard = spawn_lock();
    let started = Instant::now();
    let err = combined_output_within(
        background_command("sh", ["-c", SLEEPS_FOR_A_LONG_TIME]),
        Duration::from_millis(500),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn split_output_within_gives_up_at_the_deadline_and_reaps() {
    let _guard = spawn_lock();
    let started = Instant::now();
    let err = split_output_within(
        background_command("sh", ["-c", SLEEPS_FOR_A_LONG_TIME]),
        Duration::from_millis(500),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn split_output_within_keeps_the_streams_apart_when_the_process_is_in_time() {
    let _guard = spawn_lock();
    let (status, stdout, stderr) = split_output_within(
        background_command("sh", ["-c", "echo out; echo err 1>&2"]),
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(status.success());
    assert_eq!((stdout.trim(), stderr.trim()), ("out", "err"));
}
