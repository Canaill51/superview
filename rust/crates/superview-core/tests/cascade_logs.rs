//! What the fallback cascade says in the log when it is told to stop.
//!
//! `run_ffmpeg` starts nothing once a stop is requested, so a cascade that forgot to stop
//! still returns the right error and launches nothing: the only trace of a missing gate is
//! the log, which would announce a fallback that never happens ("falling back to CPU
//! decode" for a user who pressed Cancel). That log is the one users attach to bug
//! reports. It is read here, in a test binary of its own because the logger is
//! process-wide.
//!
//! Unix only: the stand-in ffmpeg is a shell script.
#![cfg(unix)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use log::{Log, Metadata, Record};
use superview_core::config::Config;
use superview_core::encode::args::RateControl;
use superview_core::encode::cascade::encode_video_with_memory;
use superview_core::encode::run::StopControl;
use superview_core::error::Error;
use superview_core::session::EncodingSession;
use superview_core::tools::ToolResolver;
use superview_core::video::{VideoSpecs, VideoStream};

struct Capture(Mutex<Vec<String>>);

impl Log for Capture {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record.args().to_string());
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

fn take_log() -> String {
    CAPTURE
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Words that announce a path other than the one being run.
const FALLBACK_WORDS: [&str; 5] = [
    "falling back",
    "Using CPU decode path",
    "retrying with AAC",
    "retrying with CPU encoder",
    "does not fit in memory",
];

fn video() -> VideoSpecs {
    VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width: 1920,
            height: 1080,
            duration_float: 60.0,
            ..VideoStream::default()
        }],
    }
}

/// Runs the cascade (nvenc + cuda, so every level is open, the audio copied) against a
/// stand-in ffmpeg that does `action`; `cancel_on_progress` raises the cancellation from
/// the progress callback. Returns the result and what was logged.
fn run(action: &str, cancel_on_progress: bool) -> (Result<(), Error>, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ffmpeg");
    std::fs::write(&file, format!("#!/bin/sh\n{action}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let empty = tempfile::tempdir().unwrap();
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone()),
        Some(empty.path().to_path_buf()),
    );
    let machine: HashMap<String, String> = [("accels", "cuda"), ("encoders", "h264_nvenc,libx264")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let session = EncodingSession::new(&Config::default()).unwrap();
    let (cancel, signal) = (AtomicBool::new(false), AtomicI32::new(0));
    take_log();
    let result = encode_video_with_memory(
        Some(&Config {
            performance_mode: "safe_performance".into(),
            ..Config::default()
        }),
        &video(),
        "h264_nvenc",
        RateControl::Bitrate,
        2_000_000,
        &session.dir().join("output.mp4"),
        &machine,
        &session,
        &resolver,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        &|| Some(0),
        &mut |_| {
            if cancel_on_progress {
                cancel.store(true, Ordering::Release);
            }
        },
    );
    (result.map(|_| ()), take_log())
}

fn assert_no_fallback_announced(log: &str, what: &str) {
    for word in FALLBACK_WORDS {
        assert!(
            !log.contains(word),
            "{what}: the log announces a fallback ({word:?}):\n{log}"
        );
    }
}

// One test, because the scenarios share the logger and must not interleave.
#[test]
fn the_cascade_announces_no_fallback_it_does_not_take() {
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Trace);

    // The control: an ordinary failure on the hardware pass does announce the CPU decode
    // (and the audio retry before it), so the capture sees what the others forbid.
    let (result, log) = run("echo boom 1>&2; exit 1", false);
    assert!(result.is_err());
    for word in [
        "Trying hardware decode+encode path",
        "Audio stream copy failed, retrying with AAC",
        "Hardware decode path failed, falling back to CPU decode",
        "Using CPU decode path",
        "Hardware encoder failed, retrying with CPU encoder",
    ] {
        assert!(
            log.contains(word),
            "the control run never said {word:?}:\n{log}"
        );
    }

    // A cancellation during the first (copy) run of the first pass.
    let (result, log) = run("echo out_time_ms=1000000; exec sleep 30", true);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(log.contains("Trying hardware decode+encode path"), "{log}");
    assert_no_fallback_announced(&log, "cancellation");

    // Frames written, then a failure: no other path would do better.
    let (result, log) = run(
        "echo out_time_ms=500000; echo 'No space left on device' 1>&2; exit 1",
        false,
    );
    assert!(
        matches!(result, Err(Error::FailedAfterOutput(_))),
        "{result:?}"
    );
    assert_no_fallback_announced(&log, "failure after output");
}
