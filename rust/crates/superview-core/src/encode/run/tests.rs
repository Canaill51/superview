use super::*;
use crate::process::background_command;
use crate::process::tests::spawn_lock;

/// A worker's answer: the result and the percentages handed to the callback.
type Outcome = (Result<(), Error>, Vec<f64>);

/// A `run_ffmpeg` in a thread of its own, so that a run that hangs fails the test
/// instead of blocking it, with the flags the test raises.
struct Run {
    cancel: Arc<AtomicBool>,
    signal: Arc<AtomicI32>,
    outcome: Receiver<Outcome>,
    started: Instant,
}

impl Run {
    fn start(cmd: Command, duration: f64) -> Self {
        Self::start_with(cmd, duration, |_, _, _| {})
    }

    /// `hook` runs inside the progress callback, with the flags and the percentage.
    fn start_with(
        cmd: Command,
        duration: f64,
        hook: impl Fn(&AtomicBool, &AtomicI32, f64) + Send + 'static,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(AtomicI32::new(0));
        let (tx, outcome) = mpsc::channel();
        let (c, s) = (Arc::clone(&cancel), Arc::clone(&signal));
        thread::spawn(move || {
            let mut seen = Vec::new();
            let result = run_ffmpeg(
                cmd,
                duration,
                &StopControl {
                    cancel: &c,
                    signal: &s,
                },
                &mut |p| {
                    seen.push(p);
                    hook(&c, &s, p);
                },
            );
            let _ = tx.send((result, seen));
        });
        Self {
            cancel,
            signal,
            outcome,
            started: Instant::now(),
        }
    }

    /// The answer, or a failure of the test when the run does not return in time.
    fn finish(&self, limit: Duration) -> Outcome {
        self.outcome
            .recv_timeout(limit)
            .expect("run_ffmpeg did not return in time (or its thread died)")
    }

    fn finish_default(&self) -> Outcome {
        self.finish(Duration::from_secs(30))
    }
}

/// `run_ffmpeg` on a command whose flags nobody raises.
#[cfg(unix)]
fn run_plain(cmd: Command, duration: f64) -> Outcome {
    Run::start(cmd, duration).finish_default()
}

/// `sh -c script sh args...`: the arguments are `$1`, `$2`, ...
#[cfg(unix)]
fn script(body: &str, args: &[&std::path::Path]) -> Command {
    let mut cmd = background_command("sh", ["-c", body, "sh"]);
    cmd.args(args);
    cmd
}

#[cfg(unix)]
fn pid_exists(pid: &str) -> bool {
    // A zombie still answers `kill -0`: this tells "killed" from "killed and waited".
    background_command("kill", ["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// Not alive, not a zombie of ours either (on Linux, where `/proc` says it).
#[cfg(unix)]
fn assert_no_trace_of(pid: &str, what: &str) {
    assert!(!pid_exists(pid), "process {pid} is still there {what}");
    #[cfg(target_os = "linux")]
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "process {pid} is a zombie {what}"
    );
}

/// The pid a script wrote with `echo $$ > file`, once it has written the whole line.
#[cfg(unix)]
fn wait_for_pid(file: &std::path::Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(text) = std::fs::read_to_string(file)
            && text.ends_with('\n')
        {
            return text.trim().to_string();
        }
        assert!(Instant::now() < deadline, "the script never wrote its pid");
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
const SLEEPS: &str = "echo $$ > \"$1\"; exec sleep 30";

#[cfg(windows)]
fn ping_for_a_long_time() -> Command {
    background_command("ping", ["-n", "30", "127.0.0.1"])
}

// ---- stops ----

// Go: TestEncodeVideo_CancelReapsProcess (the zombie is looked for by the pid the
// process wrote, not by counting the zombies of the whole test process: other tests run
// processes in parallel).
#[cfg(unix)]
#[test]
fn cancel_leaves_no_process_behind() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    for round in 0..3 {
        let pidfile = dir.path().join(format!("pid{round}"));
        let run = Run::start(script(SLEEPS, &[&pidfile]), 60.0);
        let pid = wait_for_pid(&pidfile);
        run.cancel.store(true, Ordering::Release);
        let (result, _) = run.finish(Duration::from_secs(15));
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
        assert_no_trace_of(&pid, "after the cancellation");
    }
}

// Go: TestEncodeVideo_CancelReapsProcess, on Windows (an `.exe`, killed directly).
#[cfg(windows)]
#[test]
fn cancel_stops_a_running_exe() {
    let _guard = spawn_lock();
    let run = Run::start(ping_for_a_long_time(), 60.0);
    thread::sleep(Duration::from_millis(1000));
    run.cancel.store(true, Ordering::Release);
    let (result, _) = run.finish(Duration::from_secs(15));
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(run.started.elapsed() < Duration::from_secs(15));
}

// Go: TestEncodeVideo_SystemSignalIsNotReportedAsUserCancellation
#[cfg(unix)]
#[test]
fn a_system_signal_is_not_reported_as_a_user_cancellation() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let run = Run::start(script(SLEEPS, &[&pidfile]), 60.0);
    let pid = wait_for_pid(&pidfile);
    run.signal.store(2, Ordering::Release);
    let (result, _) = run.finish(Duration::from_secs(15));
    let err = result.expect_err("a signal must stop the run");
    assert!(
        matches!(&err, Error::StoppedBySignal(name) if name == "interrupt"),
        "{err:?}"
    );
    assert!(!matches!(err, Error::Cancelled));
    assert!(err.to_string().contains("interrupt"), "{err}");
    assert!(err.is_stop_request() && err.should_not_retry());
    assert_no_trace_of(&pid, "after the signal");
}

#[cfg(windows)]
#[test]
fn a_system_signal_is_not_reported_as_a_user_cancellation_on_windows() {
    let _guard = spawn_lock();
    let run = Run::start(ping_for_a_long_time(), 60.0);
    thread::sleep(Duration::from_millis(1000));
    run.signal.store(15, Ordering::Release);
    let (result, _) = run.finish(Duration::from_secs(15));
    let err = result.expect_err("a signal must stop the run");
    assert!(
        matches!(&err, Error::StoppedBySignal(name) if name == "terminated"),
        "{err:?}"
    );
}

// Go: TestEncodeVideo_CancellationStopsTheFallbackCascade (the part that concerns one
// run: the request lands from the progress callback while ffmpeg is provably running,
// and ends that run. That nothing else is launched is the cascade's, PR 6c).
#[cfg(unix)]
#[test]
fn a_cancellation_raised_from_the_progress_callback_stops_the_run() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let body = "echo $$ > \"$1\"; echo out_time_ms=1000000; exec sleep 30";
    let run = Run::start_with(script(body, &[&pidfile]), 10.0, |cancel, _, _| {
        cancel.store(true, Ordering::Release);
    });
    let (result, seen) = run.finish(Duration::from_secs(15));
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_eq!(seen, vec![10.0]);
    assert_no_trace_of(&wait_for_pid(&pidfile), "after the cancellation");
}

// Go: the already-closed channel; E-02 -- a stop request launches no ffmpeg. The
// script would leave a witness file: Go starts it and kills it, here it never runs.
#[cfg(unix)]
#[test]
fn a_stop_already_requested_starts_nothing() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    for (name, cancel, signal) in [("cancel", true, 0), ("signal", false, 15)] {
        let witness = dir.path().join(name);
        let (c, s) = (AtomicBool::new(cancel), AtomicI32::new(signal));
        let result = run_ffmpeg(
            script("echo ran > \"$1\"; exec sleep 30", &[&witness]),
            60.0,
            &StopControl {
                cancel: &c,
                signal: &s,
            },
            &mut |_| {},
        );
        assert!(
            matches!(&result, Err(e) if e.is_stop_request()),
            "{name}: {result:?}"
        );
        // Time for a process that had been launched to write its witness.
        thread::sleep(Duration::from_millis(300));
        assert!(
            !witness.exists(),
            "{name}: ffmpeg was launched although a stop was requested"
        );
    }
}

// The same, on both platforms: the program does not exist, so a launch would be a
// "failed to start" error, not a stop. A signal outranks a cancellation.
#[test]
fn a_stop_already_requested_is_reported_before_any_launch() {
    let missing = || background_command("superview-definitely-not-a-real-tool", ["x"]);
    let stopped = |cancel: bool, signal: i32| {
        let (c, s) = (AtomicBool::new(cancel), AtomicI32::new(signal));
        run_ffmpeg(
            missing(),
            60.0,
            &StopControl {
                cancel: &c,
                signal: &s,
            },
            &mut |_| {},
        )
    };
    assert!(matches!(stopped(true, 0), Err(Error::Cancelled)));
    assert!(matches!(stopped(false, 15), Err(Error::StoppedBySignal(n)) if n == "terminated"));
    assert!(matches!(stopped(true, 2), Err(Error::StoppedBySignal(n)) if n == "interrupt"));
}

// ---- the end of a run ----

#[test]
fn a_program_that_cannot_be_started_is_reported_as_such() {
    let _guard = spawn_lock();
    let cmd = background_command("superview-definitely-not-a-real-tool", ["x"]);
    let run = Run::start(cmd, 60.0);
    let (result, seen) = run.finish_default();
    let err = result.expect_err("nothing to run");
    assert!(matches!(err, Error::Plain(_)), "{err:?}");
    assert!(
        err.to_string().starts_with("failed to start ffmpeg: "),
        "{err}"
    );
    assert!(seen.is_empty());
}

#[cfg(windows)]
#[test]
fn an_exit_code_decides_the_outcome_on_windows() {
    let _guard = spawn_lock();
    let ok = Run::start(background_command("cmd", ["/C", "exit 0"]), 60.0).finish_default();
    assert!(ok.0.is_ok(), "{:?}", ok.0);
    let failed = Run::start(background_command("cmd", ["/C", "exit 3"]), 60.0).finish_default();
    let err = failed.0.expect_err("exit 3 is a failure");
    assert!(matches!(err, Error::Plain(_)), "{err:?}");
    assert!(err.to_string().starts_with("ffmpeg failed: exit"), "{err}");
}

// Go: a nil Stdin is /dev/null. The command arrives with a pipe as stdin: only a run that
// replaces it can pass, whatever `cargo test` itself reads from.
#[cfg(target_os = "linux")]
#[test]
fn ffmpeg_gets_dev_null_as_stdin() {
    let _guard = spawn_lock();
    let body = "[ \"$(readlink /proc/self/fd/0)\" = /dev/null ]";
    let mut cmd = background_command("sh", ["-c", body]);
    cmd.stdin(Stdio::piped());
    let (result, _) = run_plain(cmd, 10.0);
    assert!(result.is_ok(), "stdin is not /dev/null: {result:?}");
}

// No Go test: the progress arrives in order, as percentages of the duration, and the
// lines that say nothing are skipped (Go: ProgressNotAvailableIsNotAWarning, the
// callback half).
#[cfg(unix)]
#[test]
fn progress_arrives_in_order_as_percentages() {
    let _guard = spawn_lock();
    let body = "printf 'frame=1\\nout_time_ms=1000000\\nout_time_ms=N/A\\nprogress=continue\\n\
                    out_time_ms=2500000\\nout_time_ms=garbage\\nout_time_ms=99999999999\\n'";
    let (result, seen) = run_plain(background_command("sh", ["-c", body]), 10.0);
    assert!(result.is_ok(), "{result:?}");
    // 1e6 / (10 * 1e4) = 10; the share is capped at 100.
    assert_eq!(seen, vec![10.0, 25.0, 100.0]);
}

// The progress is read as bytes: an invalid one does not end it.
#[cfg(unix)]
#[test]
fn invalid_utf8_in_the_progress_does_not_end_it() {
    let _guard = spawn_lock();
    let body = "printf '\\377\\376 noise\\nout_time_ms=1000000\\n'";
    let (result, seen) = run_plain(background_command("sh", ["-c", body]), 10.0);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(seen, vec![10.0]);
}

// No Go test: the progress of a long encode is many lines (about 50 000 over an hour at
// ffmpeg's rate, more with a short period), and none may be lost or block the process.
#[cfg(unix)]
#[test]
fn a_large_progress_output_is_read_to_the_end() {
    let _guard = spawn_lock();
    let body = "yes out_time_ms=1000000 | head -n 200000";
    let (result, seen) = run_plain(background_command("sh", ["-c", body]), 10.0);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(seen.len(), 200_000);
    assert!(seen.iter().all(|p| *p == 10.0));
}

// Go: TestClassifyFfmpegFailure_KeepsStderrForOrdinaryFailures, through a real run.
#[cfg(unix)]
#[test]
fn an_ordinary_failure_carries_ffmpegs_stderr() {
    let _guard = spawn_lock();
    let body = "echo \"Unknown encoder 'nope'\" 1>&2; exit 1";
    let (result, _) = run_plain(background_command("sh", ["-c", body]), 10.0);
    let err = result.expect_err("exit 1 is a failure");
    assert!(matches!(err, Error::Plain(_)), "not wrapped: {err:?}");
    let text = err.to_string();
    assert!(text.starts_with("ffmpeg failed: exit"), "{text}");
    assert!(
        text.ends_with("\nffmpeg stderr:\nUnknown encoder 'nope'\n"),
        "{text}"
    );
    assert!(!err.should_not_retry(), "a failure to open is retried");
}

// Go: errFailedAfterOutput -- a failure after frames have been written is not retried.
#[cfg(unix)]
#[test]
fn a_failure_after_frames_is_marked() {
    let _guard = spawn_lock();
    let body = "echo out_time_ms=5000000; echo 'No space left on device' 1>&2; exit 2";
    let (result, seen) = run_plain(background_command("sh", ["-c", body]), 10.0);
    let err = result.expect_err("exit 2 is a failure");
    assert!(matches!(&err, Error::FailedAfterOutput(_)), "{err:?}");
    assert!(err.should_not_retry() && !err.is_stop_request());
    let text = err.to_string();
    assert!(text.contains("No space left on device"), "{text}");
    assert_eq!(seen, vec![50.0]);
}

// A failure before any frame (no time yet, or a time of zero) stays an ordinary one.
#[cfg(unix)]
#[test]
fn a_failure_before_frames_is_not_marked() {
    let _guard = spawn_lock();
    let body = "echo out_time_ms=N/A; echo out_time_ms=0; echo 'bad option' 1>&2; exit 2";
    let (result, _) = run_plain(background_command("sh", ["-c", body]), 10.0);
    let err = result.expect_err("exit 2 is a failure");
    assert!(matches!(&err, Error::Plain(_)), "{err:?}");
    assert!(!err.should_not_retry());
}

// Go: `stopped` is read only when Wait fails -- a run that succeeded stays a success
// although a stop is requested after it. The flags are raised once the process has
// been reaped (its pid is then gone), so that the run has certainly seen it end; a
// grandchild holds the pipes for the length of the grace, which leaves the test time
// to do it before the run answers.
#[cfg(unix)]
#[test]
fn a_run_that_succeeded_is_not_turned_into_a_stop() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let run = Run::start(
        script(
            "echo $$ > \"$1\"; echo out_time_ms=1000000; sleep 5 & exit 0",
            &[&pidfile],
        ),
        10.0,
    );
    let pid = wait_for_pid(&pidfile);
    let deadline = Instant::now() + Duration::from_secs(20);
    while pid_exists(&pid) {
        assert!(Instant::now() < deadline, "the process never went away");
        thread::sleep(Duration::from_millis(2));
    }
    run.cancel.store(true, Ordering::Release);
    run.signal.store(15, Ordering::Release);
    let (result, seen) = run.finish_default();
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(seen, vec![10.0]);
}

// A failure of ffmpeg itself while a stop is requested (Ctrl+C reaches ffmpeg too) is
// the stop, not a failure to classify or to retry. A grandchild holds the pipes for the
// length of the grace, which leaves the test time to raise the flag once the process has
// been reaped (its pid is gone) and before the run has looked at it.
#[cfg(unix)]
#[test]
fn a_failure_while_a_stop_is_requested_is_the_stop() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let run = Run::start(
        script("echo $$ > \"$1\"; sleep 5 & exit 255", &[&pidfile]),
        10.0,
    );
    let pid = wait_for_pid(&pidfile);
    let deadline = Instant::now() + Duration::from_secs(20);
    while pid_exists(&pid) {
        assert!(Instant::now() < deadline, "the process never went away");
        thread::sleep(Duration::from_millis(2));
    }
    run.signal.store(2, Ordering::Release);
    let (result, _) = run.finish_default();
    let err = result.expect_err("exit 255 is a failure");
    assert!(
        matches!(&err, Error::StoppedBySignal(n) if n == "interrupt"),
        "{err:?}"
    );
}

// U-09: ffmpeg killed from outside (the out-of-memory killer) while the system also
// asks the application to stop (systemd's SIGTERM): the memory diagnosis is kept. The
// signal is raised once the process has been reaped, in the grace a grandchild holding
// the pipes leaves the run.
#[cfg(unix)]
#[test]
fn a_kill_from_outside_keeps_its_diagnosis_although_a_signal_is_raised() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let run = Run::start(
        script("echo $$ > \"$1\"; sleep 5 & kill -9 $$", &[&pidfile]),
        10.0,
    );
    let pid = wait_for_pid(&pidfile);
    let deadline = Instant::now() + Duration::from_secs(20);
    while pid_exists(&pid) {
        assert!(Instant::now() < deadline, "the process never went away");
        thread::sleep(Duration::from_millis(2));
    }
    run.signal.store(15, Ordering::Release);
    let (result, _) = run.finish_default();
    let err = result.expect_err("a killed ffmpeg is a failure");
    assert!(matches!(&err, Error::Encoder(_)), "{err:?}");
    assert!(err.to_string().contains("killed by the system"), "{err}");
}

// No Go test (cmd.StderrPipe deadlock): more than a pipe holds on stderr must neither
// block ffmpeg nor lose bytes. 4 MB: a Linux pipe grows up to 1 MiB.
#[cfg(unix)]
#[test]
fn a_large_stderr_does_not_block_ffmpeg() {
    let _guard = spawn_lock();
    let body = "yes e | head -c 4000000 1>&2; exit 1";
    let (result, _) = run_plain(background_command("sh", ["-c", body]), 10.0);
    let err = result.expect_err("exit 1 is a failure");
    assert!(err.to_string().len() >= 4_000_000, "stderr was cut short");
}

// ---- a grandchild that keeps the pipes (lesson of #110) ----

// The run ends by itself while a child of ffmpeg holds the pipes: the caller must not
// wait for it. Bounded by the grace, with the frames already read kept.
#[cfg(unix)]
#[test]
fn a_child_keeping_the_pipes_does_not_hold_a_finished_run() {
    let _guard = spawn_lock();
    let body = "echo out_time_ms=5000000; sleep 5 & exit 1";
    let run = Run::start(background_command("sh", ["-c", body]), 10.0);
    let (result, seen) = run.finish(Duration::from_secs(4));
    assert!(run.started.elapsed() < Duration::from_secs(4));
    let err = result.expect_err("exit 1 is a failure");
    assert!(matches!(&err, Error::FailedAfterOutput(_)), "{err:?}");
    assert_eq!(seen, vec![50.0], "the lines read before are kept");
}

// The same after a cancellation: the kill reaches the shell, not its `sleep`.
#[cfg(unix)]
#[test]
fn a_child_keeping_the_pipes_does_not_hold_a_cancelled_run() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let body = "echo $$ > \"$1\"; sleep 5 & exec sleep 30";
    let run = Run::start(script(body, &[&pidfile]), 10.0);
    let pid = wait_for_pid(&pidfile);
    let cancelled_at = Instant::now();
    run.cancel.store(true, Ordering::Release);
    let (result, _) = run.finish(Duration::from_secs(4));
    assert!(cancelled_at.elapsed() < Duration::from_secs(4));
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_no_trace_of(&pid, "after the cancellation");
}

// ---- the progress reader ----

/// Yields some bytes, then fails on every read.
struct FailsAfter {
    head: io::Cursor<Vec<u8>>,
}

impl Read for FailsAfter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.head.read(buf)? {
            0 => Err(io::Error::other("pipe closed unexpectedly")),
            n => Ok(n),
        }
    }
}

// Go: TestEncodeVideo_ProgressReaderNonEOFErrorDoesNotHang -- any error ends the
// reader. Run in a thread: a reader that loops on the error would spin for ever.
#[test]
fn a_progress_reader_that_fails_ends_instead_of_hanging() {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = FailsAfter {
            head: io::Cursor::new(b"out_time_ms=5\nout_time_ms=".to_vec()),
        };
        let mut lines = Vec::new();
        let end = read_progress_lines(reader, |line| {
            lines.push(line);
            true
        });
        let _ = tx.send((end, lines));
    });
    let (end, lines) = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the progress reader hung on a read error");
    assert!(matches!(end, ReaderEnd::Failed(_)), "{end:?}");
    assert_eq!(
        lines,
        vec!["out_time_ms=5"],
        "the line cut by the error is dropped"
    );
}

#[test]
fn a_progress_reader_stops_when_nobody_listens() {
    let reader = io::Cursor::new(b"a\nb\nc\n".to_vec());
    let mut taken = 0;
    let end = read_progress_lines(reader, |_| {
        taken += 1;
        false
    });
    assert!(matches!(end, ReaderEnd::Abandoned), "{end:?}");
    assert_eq!(taken, 1);
}

#[test]
fn a_progress_reader_strips_line_ends_and_decodes_lossily() {
    let reader = io::Cursor::new(b"a\r\nb\xff\nlast".to_vec());
    let mut lines = Vec::new();
    let end = read_progress_lines(reader, |line| {
        lines.push(line);
        true
    });
    assert!(matches!(end, ReaderEnd::Eof), "{end:?}");
    assert_eq!(lines, vec!["a", "b\u{fffd}", "last"]);
}
