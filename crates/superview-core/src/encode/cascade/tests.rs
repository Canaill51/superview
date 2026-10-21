use super::*;
use crate::config::Config;
use crate::error::EncoderError;
use crate::pgm::{generate_pgm, remap_output_size};
use crate::process::tests::spawn_lock;
use crate::tools::check_ffmpeg;
use crate::video::VideoStream;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
#[cfg(unix)]
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const GIB: u64 = 1 << 30;

// ---- fixtures ----

fn caps(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A hardware encoder with a matching decode accel opens all three
/// levels of the cascade.
fn nvenc_machine() -> HashMap<String, String> {
    caps(&[("accels", "cuda"), ("encoders", "h264_nvenc,libx264")])
}

/// A VAAPI machine.
fn vaapi_machine() -> HashMap<String, String> {
    caps(&[
        ("encoders", "h264_vaapi,libx264,libx265"),
        ("accels", "vaapi"),
    ])
}

fn cpu_machine(encoders: &str) -> HashMap<String, String> {
    caps(&[("accels", ""), ("encoders", encoders)])
}

/// A plain 1080p H.264 source.
fn h264_video() -> VideoSpecs {
    VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width: 1920,
            height: 1080,
            duration: "60".into(),
            duration_float: 60.0,
            bitrate: "5000000".into(),
            bitrate_int: 5_000_000,
            ..VideoStream::default()
        }],
    }
}

/// An HEVC source.
fn hevc_source(pix_fmt: &str) -> VideoSpecs {
    VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![VideoStream {
            codec: "hevc".into(),
            width: 3840,
            height: 2880,
            pix_fmt: pix_fmt.into(),
            duration_float: 43.0,
            bitrate_int: 120_000_000,
            ..VideoStream::default()
        }],
    }
}

/// The audio is always re-encoded, so one pass is one launch.
fn safe_config() -> Config {
    Config {
        performance_mode: "safe".into(),
        ..Config::default()
    }
}

/// The shipped configuration: copy the audio, retry as AAC.
#[cfg(unix)]
fn safe_performance_config() -> Config {
    Config {
        performance_mode: "safe_performance".into(),
        ..Config::default()
    }
}

/// A session whose first map holds only the header `generate_pgm` writes: that is all
/// `output_size` reads, and it spares writing 30 MB of map for a 5K frame.
fn session_with_header(video: &VideoSpecs) -> EncodingSession {
    let session = EncodingSession::new(&Config::default()).unwrap();
    let (w, h) = remap_output_size(video, false);
    std::fs::write(session.x_path(), format!("P5 {w} {h} 65535\n")).unwrap();
    session
}

fn bare_session() -> EncodingSession {
    EncodingSession::new(&Config::default()).unwrap()
}

/// A resolver that finds nothing. On Unix a launch is then a "failed to start" error. On
/// Windows `resolve` gives back the bare name `ffmpeg` and std looks it up in the
/// parent's PATH, so the CI's real ffmpeg may start (and fail fast on the missing input):
/// the assertions below hold either way, but they cannot tell "nothing launched".
fn no_tools() -> ToolResolver {
    ToolResolver::new(|_| None, None)
}

fn plenty() -> Option<u64> {
    Some(64 * GIB)
}

fn stop_flags() -> (AtomicBool, AtomicI32) {
    (AtomicBool::new(false), AtomicI32::new(0))
}

/// One `encode_video_with_memory` with nothing asked to stop and no interest in progress.
fn encode(
    video: &VideoSpecs,
    encoder: &str,
    machine: &HashMap<String, String>,
    cfg: &Config,
    session: &EncodingSession,
    resolver: &ToolResolver,
    available: &dyn Fn() -> Option<u64>,
) -> Result<EncodeSummary, Error> {
    encode_at(
        RateControl::Bitrate,
        video,
        encoder,
        machine,
        cfg,
        session,
        resolver,
        available,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "`encode` plus the mode under test"
)]
fn encode_at(
    rate_control: RateControl,
    video: &VideoSpecs,
    encoder: &str,
    machine: &HashMap<String, String>,
    cfg: &Config,
    session: &EncodingSession,
    resolver: &ToolResolver,
    available: &dyn Fn() -> Option<u64>,
) -> Result<EncodeSummary, Error> {
    let (cancel, signal) = stop_flags();
    encode_video_with_memory(
        Some(cfg),
        video,
        encoder,
        rate_control,
        2_000_000,
        &session.dir().join("output.mp4"),
        machine,
        session,
        resolver,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        available,
        &mut |_| {},
    )
}

fn assert_memory_refusal(result: &Result<EncodeSummary, Error>) {
    match result {
        Err(Error::Encoder(EncoderError { msg })) => {
            assert!(msg.contains("not enough memory"), "{msg}");
            assert!(msg.contains("libx265"), "{msg}");
        }
        other => panic!("expected the memory refusal, got {other:?}"),
    }
}

// ---- the stand-in ffmpeg (Unix: a shell script) ----

/// What the stand-in does for a call.
#[cfg(unix)]
mod act {
    pub const OK: &str = "echo out_time_ms=1000000; echo progress=end; exit 0";
    pub const REFUSE: &str = "echo 'Hardware does not support encoding at size' 1>&2; exit 1";
    /// Frames are written, then the disk is full.
    pub const LATE: &str = "echo out_time_ms=500000; echo 'No space left on device' 1>&2; exit 1";
    pub const NO_AUDIO_COPY: &str =
        "echo 'Could not find tag for codec pcm_s16le in stream #1' 1>&2; exit 1";
    /// Reports a frame, then runs until it is killed.
    pub const HANG: &str = "echo out_time_ms=1000000; exec sleep 30";
    /// Runs without a word until it is killed.
    pub const SILENT_HANG: &str = "exec sleep 30";
}

#[cfg(unix)]
struct Fake {
    tools: crate::probe::tests::FakeTools,
    ledger: std::path::PathBuf,
    pids: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

/// An ffmpeg that refuses, and its relatives, in one: appends the argv of
/// every call to a ledger and its pid to another, then does the action of the first rule
/// whose pattern is a word sequence of the argv (`default` when none is).
#[cfg(unix)]
fn scripted(rules: &[(&str, &str)], default: &str) -> Fake {
    let dir = tempfile::TempDir::new().unwrap();
    let ledger = dir.path().join("calls.txt");
    let pids = dir.path().join("pids.txt");
    let mut body = format!(
        "echo \"call $*\" >> '{}'\necho $$ >> '{}'\ncase \" $* \" in\n",
        ledger.display(),
        pids.display()
    );
    for (pattern, action) in rules {
        body.push_str(&format!("*\" {pattern} \"*) {action} ;;\n"));
    }
    body.push_str(&format!("*) {default} ;;\nesac"));
    Fake {
        tools: crate::probe::tests::fake_tools(&[("ffmpeg", &body)]),
        ledger,
        pids,
        _dir: dir,
    }
}

#[cfg(unix)]
impl Fake {
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.ledger)
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Waits (bounded) until the stand-in has been called `n` times.
    fn wait_for_calls(&self, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.calls().len() < n {
            assert!(Instant::now() < deadline, "the stand-in was never called");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn assert_no_process_left(&self) {
        let text = std::fs::read_to_string(&self.pids).unwrap_or_default();
        for pid in text.lines() {
            assert_no_trace_of(pid);
        }
    }
}

/// Not alive, not a zombie of ours either (on Linux, where `/proc` says it).
#[cfg(unix)]
fn assert_no_trace_of(pid: &str) {
    let alive = crate::process::background_command("kill", ["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "process {pid} is still there");
    #[cfg(target_os = "linux")]
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "process {pid} is a zombie"
    );
}

/// The i-th recorded call, with the leading "call ".
#[cfg(unix)]
fn words(call: &str) -> String {
    format!(" {} ", call.trim_start_matches("call "))
}

#[cfg(unix)]
fn has(call: &str, option: &str) -> bool {
    words(call).contains(&format!(" {option} "))
}

/// A cascade running in a thread of its own, so that a run that hangs fails the test
/// (the answer is awaited with a limit) instead of blocking it.
#[cfg(unix)]
struct Job {
    signal: Arc<AtomicI32>,
    outcome: mpsc::Receiver<(Result<EncodeSummary, Error>, bool)>,
}

#[cfg(unix)]
impl Job {
    /// `cancel_on_progress`: the progress callback raises the cancellation, as the
    /// window's button would while ffmpeg is provably running.
    #[allow(
        clippy::too_many_arguments,
        reason = "a test fixture: each argument is one thing a scenario varies"
    )]
    fn start(
        fake: &Arc<Fake>,
        video: VideoSpecs,
        encoder: &str,
        machine: HashMap<String, String>,
        cfg: Config,
        session: Arc<EncodingSession>,
        available: Option<u64>,
        cancel_on_progress: bool,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::new(AtomicI32::new(0));
        let (tx, outcome) = mpsc::channel();
        let (fake, c, s, encoder) = (
            Arc::clone(fake),
            Arc::clone(&cancel),
            Arc::clone(&signal),
            encoder.to_string(),
        );
        std::thread::spawn(move || {
            let mut seen = false;
            let result = encode_video_with_memory(
                Some(&cfg),
                &video,
                &encoder,
                RateControl::Bitrate,
                2_000_000,
                &session.dir().join("output.mp4"),
                &machine,
                &session,
                &fake.tools.resolver,
                4,
                &StopControl {
                    cancel: &c,
                    signal: &s,
                },
                &move || available,
                &mut |_| {
                    seen = true;
                    if cancel_on_progress {
                        c.store(true, Ordering::Release);
                    }
                },
            );
            let _ = tx.send((result, seen));
        });
        Self { signal, outcome }
    }

    fn finish(&self) -> (Result<EncodeSummary, Error>, bool) {
        self.outcome
            .recv_timeout(Duration::from_secs(30))
            .expect("the cascade did not return in time (or its thread died)")
    }
}

// ---- the order of the attempts, and the summary of each way out ----

// The first level. The ledger shows -hwaccel, and the summary names the
// decoder in capitals.
#[cfg(unix)]
#[test]
fn the_hardware_path_is_tried_first_and_summarised() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::OK);
    let session = bare_session();
    let summary = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_config(),
        &session,
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: used h264_nvenc encode + CUDA decode"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(has(&calls[0], "-hwaccel cuda"), "{}", calls[0]);
    assert!(has(&calls[0], "-c:v h264_nvenc"), "{}", calls[0]);
    assert!(
        calls[0].ends_with(&session.dir().join("output.mp4").display().to_string()),
        "the output path is the last argument: {}",
        calls[0]
    );
}

// The first hardware attempt fails with an ordinary error: the second launched is the
// same encoder without -hwaccel.
#[cfg(unix)]
#[test]
fn a_failed_hardware_decode_is_followed_by_cpu_decode() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-hwaccel", act::REFUSE)], act::OK);
    let summary = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: used h264_nvenc encode + CPU decode fallback"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(has(&calls[0], "-hwaccel cuda"), "{}", calls[0]);
    assert!(!has(&calls[1], "-hwaccel"), "{}", calls[1]);
    assert!(has(&calls[1], "-c:v h264_nvenc"), "{}", calls[1]);
}

// hwaccel, then CPU decode, then the software encoder, in that order.
#[cfg(unix)]
#[test]
fn a_failed_hardware_encoder_is_followed_by_the_software_one() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-c:v h264_nvenc", act::REFUSE)], act::OK);
    let summary = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: h264_nvenc failed; used CPU encode (libx264) + CPU decode"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(
        has(&calls[0], "-hwaccel cuda") && has(&calls[0], "-c:v h264_nvenc"),
        "{}",
        calls[0]
    );
    assert!(
        !has(&calls[1], "-hwaccel") && has(&calls[1], "-c:v h264_nvenc"),
        "{}",
        calls[1]
    );
    assert!(
        !has(&calls[2], "-hwaccel") && has(&calls[2], "-c:v libx264"),
        "{}",
        calls[2]
    );
}

// DEC-48: a driver that only knows CQP is asked for a quantiser on every pass that uses
// it, and the CPU encoder that replaces it is asked for a bit rate again.
#[cfg(unix)]
#[test]
fn constant_quality_follows_the_hardware_encoder_and_stops_at_the_cpu_one() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-c:v h264_vaapi", act::REFUSE)], act::OK);
    let summary = encode_at(
        RateControl::ConstantQuality,
        &h264_video(),
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert!(
        summary.hardware_acceleration.contains("libx264"),
        "{summary:?}"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "{calls:?}");
    for call in &calls[..2] {
        assert!(
            has(call, "-c:v h264_vaapi") && has(call, "-rc_mode CQP") && has(call, "-qp 23"),
            "{call}"
        );
        assert!(
            !has(call, "-b:v"),
            "no bit rate in constant quality: {call}"
        );
    }
    assert!(
        has(&calls[2], "-c:v libx264") && has(&calls[2], "-b:v 2000000"),
        "{}",
        calls[2]
    );
    assert!(
        !has(&calls[2], "-rc_mode") && !has(&calls[2], "-qp"),
        "{}",
        calls[2]
    );
}

#[cfg(unix)]
#[test]
fn a_software_encoder_is_summarised_as_cpu_encode() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::OK);
    let summary = encode(
        &h264_video(),
        "libx264",
        &cpu_machine("libx264"),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: used CPU encode (libx264) + CPU decode"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(!has(&calls[0], "-hwaccel"), "{}", calls[0]);
}

// The note about a change of codec family is appended to the line (an HEVC source
// encoded in H.264).
#[cfg(unix)]
#[test]
fn a_change_of_codec_family_is_stated_in_the_summary() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::OK);
    let video = hevc_source("yuv420p");
    let summary = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    let line = "Hardware: used h264_vaapi encode + VAAPI decode";
    assert!(
        summary
            .hardware_acceleration
            .starts_with(&format!("{line} -- ")),
        "{}",
        summary.hardware_acceleration
    );
    assert_eq!(
        summary.hardware_acceleration,
        with_codec_switch_note(Some(&video), "h264_vaapi", line)
    );
}

// The note belongs to the encoder that ran: libx265 stays in the HEVC source's family,
// so the line carries none, though the failed h264_vaapi (another family) would have.
#[cfg(unix)]
#[test]
fn the_fallback_summary_carries_the_note_of_the_encoder_that_ran() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-c:v h264_vaapi", act::REFUSE)], act::OK);
    let video = hevc_source("yuv420p");
    assert_ne!(
        with_codec_switch_note(Some(&video), "h264_vaapi", "x"),
        "x",
        "the fixture needs a failed encoder that has a note"
    );
    let summary = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: h264_vaapi failed; used CPU encode (libx265) + CPU decode"
    );
    assert!(has(&fake.calls()[2], "-c:v libx265"), "{:?}", fake.calls());
}

// A software encoder has no encoder fallback: an HEVC source would give libx265 one if
// the guard on the encoder's kind were missing.
#[cfg(unix)]
#[test]
fn a_failing_software_encoder_is_not_swapped_for_another() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::REFUSE);
    let result = encode(
        &hevc_source("yuv420p"),
        "libx264",
        &cpu_machine("libx264,libx265"),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    );
    let err = result.expect_err("every call fails");
    assert!(
        err.to_string().contains("Hardware does not support"),
        "{err}"
    );
    assert_eq!(fake.calls().len(), 1, "{:?}", fake.calls());
}

// A hardware encoder with no software equivalent in this ffmpeg: the error is the CPU
// pass's own, and nothing else is launched.
#[cfg(unix)]
#[test]
fn a_hardware_encoder_without_a_software_fallback_returns_the_cpu_pass_error() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::REFUSE);
    let result = encode(
        &h264_video(),
        "h264_nvenc",
        &cpu_machine("h264_nvenc"),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    );
    let err = result.expect_err("every call fails");
    assert!(matches!(err, Error::Plain(_)), "{err:?}");
    assert!(
        err.to_string().contains("Hardware does not support"),
        "{err}"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "no accel and no fallback: {calls:?}");
    assert!(!has(&calls[0], "-hwaccel"), "{}", calls[0]);
}

// ---- the audio retry ----

// Safe performance retries audio as AAC, first case.
#[cfg(unix)]
#[test]
fn safe_performance_retries_the_audio_as_aac() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-c:a copy", act::NO_AUDIO_COPY)], act::OK);
    let result = encode(
        &h264_video(),
        "libx264",
        &cpu_machine("libx264"),
        &safe_performance_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    );
    result.expect("the AAC retry should have rescued the conversion");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "the copy, then the AAC retry: {calls:?}");
    assert!(has(&calls[0], "-c:a copy"), "{}", calls[0]);
    assert!(has(&calls[1], "-c:a aac"), "{}", calls[1]);
}

// The mode that always re-encodes has no copy to fall back from: one pass, one launch.
#[cfg(unix)]
#[test]
fn safe_mode_asks_for_aac_and_does_not_retry_it() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::REFUSE);
    let result = encode(
        &h264_video(),
        "libx264",
        &cpu_machine("libx264"),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    );
    assert!(result.is_err());
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(has(&calls[0], "-c:a aac"), "{}", calls[0]);
}

// Safe performance retries audio as AAC, second case.
#[cfg(unix)]
#[test]
fn a_copy_that_fails_after_output_is_not_retried_as_aac() {
    let _guard = spawn_lock();
    let fake = scripted(&[("-c:a copy", act::LATE)], act::OK);
    let (cancel, signal) = stop_flags();
    let mut progressed = false;
    let session = bare_session();
    let result = encode_video_with_memory(
        Some(&safe_performance_config()),
        &h264_video(),
        "libx264",
        RateControl::Bitrate,
        2_000_000,
        &session.dir().join("output.mp4"),
        &cpu_machine("libx264"),
        &session,
        &fake.tools.resolver,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        &plenty,
        &mut |_| progressed = true,
    );
    assert!(progressed, "the stand-in never reported progress");
    assert!(
        matches!(result, Err(Error::FailedAfterOutput(_))),
        "{result:?}"
    );
    assert_eq!(fake.calls().len(), 1, "{:?}", fake.calls());
}

// Safe performance retries audio as AAC, third case.
#[cfg(unix)]
#[test]
fn a_cancellation_during_the_copy_run_is_not_retried_as_aac() {
    let _guard = spawn_lock();
    let fake = Arc::new(scripted(&[], act::HANG));
    let job = Job::start(
        &fake,
        h264_video(),
        "libx264",
        cpu_machine("libx264"),
        safe_performance_config(),
        Arc::new(bare_session()),
        plenty(),
        true,
    );
    let (result, _) = job.finish();
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(has(&calls[0], "-c:a copy"), "{}", calls[0]);
    fake.assert_no_process_left();
}

// Each pass has its own audio retry, in the order of the cascade.
#[cfg(unix)]
#[test]
fn every_pass_of_the_cascade_retries_its_own_audio() {
    let _guard = spawn_lock();
    let fake = scripted(
        &[("-c:v h264_nvenc", act::REFUSE), ("-c:a copy", act::REFUSE)],
        act::OK,
    );
    let summary = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_performance_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .unwrap();
    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: h264_nvenc failed; used CPU encode (libx264) + CPU decode"
    );
    // (hardware decode, CPU decode, software encoder) x (copy, AAC): the last pass is
    // the only one whose copy is refused and whose AAC is accepted.
    let calls = fake.calls();
    let shape: Vec<(bool, bool, &str)> = calls
        .iter()
        .map(|c| {
            (
                has(c, "-hwaccel cuda"),
                has(c, "-c:v h264_nvenc"),
                if has(c, "-c:a copy") { "copy" } else { "aac" },
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (true, true, "copy"),
            (true, true, "aac"),
            (false, true, "copy"),
            (false, true, "aac"),
            (false, false, "copy"),
            (false, false, "aac"),
        ],
        "{calls:?}"
    );
}

// ---- the stops ----

// A system signal stops the fallback cascade (E-02). One signal for the
// conversion; under the shipped configuration with a hardware encoder it used to start
// six processes.
#[cfg(unix)]
#[test]
fn a_system_signal_stops_the_fallback_cascade() {
    let _guard = spawn_lock();
    let fake = Arc::new(scripted(&[], act::SILENT_HANG));
    let job = Job::start(
        &fake,
        h264_video(),
        "h264_nvenc",
        nvenc_machine(),
        safe_performance_config(),
        Arc::new(bare_session()),
        plenty(),
        false,
    );
    fake.wait_for_calls(1);
    job.signal.store(15, Ordering::Release);
    let (result, _) = job.finish();
    assert!(
        matches!(&result, Err(Error::StoppedBySignal(name)) if name == "terminated"),
        "{result:?}"
    );
    assert_eq!(
        fake.calls().len(),
        1,
        "ffmpeg was launched again after one system signal: the cascade did not stop"
    );
    fake.assert_no_process_left();
}

// A cancellation stops the fallback cascade (P-03). The cancellation
// lands from the progress callback, so the first ffmpeg is provably running.
#[cfg(unix)]
#[test]
fn a_cancellation_stops_the_fallback_cascade() {
    let _guard = spawn_lock();
    let fake = Arc::new(scripted(&[], act::HANG));
    let job = Job::start(
        &fake,
        h264_video(),
        "h264_nvenc",
        nvenc_machine(),
        Config::default(),
        Arc::new(bare_session()),
        plenty(),
        true,
    );
    let (result, _) = job.finish();
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_eq!(
        fake.calls().len(),
        1,
        "the fallback cascade did not stop on cancellation"
    );
    fake.assert_no_process_left();
}

// A stop that arrives on the CPU-decode pass is returned as it is. With the memory
// guard refusing and the output size known, a missing gate would turn it into a memory
// refusal.
#[cfg(unix)]
#[test]
fn a_stop_on_the_cpu_pass_is_not_turned_into_a_memory_refusal() {
    let _guard = spawn_lock();
    let fake = Arc::new(scripted(&[("-hwaccel", act::REFUSE)], act::HANG));
    let video = hevc_source("yuv420p10le");
    let session = Arc::new(session_with_header(&video));
    let job = Job::start(
        &fake,
        video,
        "h264_vaapi",
        vaapi_machine(),
        safe_config(),
        session,
        Some(GIB),
        true,
    );
    let (result, _) = job.finish();
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_eq!(fake.calls().len(), 2, "{:?}", fake.calls());
    fake.assert_no_process_left();
}

// A failure after output is not retried (E-13). The progress callback must
// have fired, so the failure is a late one and not an open failure the cascade is right
// to retry.
#[cfg(unix)]
#[test]
fn a_failure_after_output_is_not_retried() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::LATE);
    let (cancel, signal) = stop_flags();
    let mut progressed = false;
    let session = bare_session();
    let result = encode_video_with_memory(
        Some(&safe_performance_config()),
        &h264_video(),
        "h264_nvenc",
        RateControl::Bitrate,
        2_000_000,
        &session.dir().join("output.mp4"),
        &nvenc_machine(),
        &session,
        &fake.tools.resolver,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        &plenty,
        &mut |_| progressed = true,
    );
    assert!(
        progressed,
        "the stand-in never reported progress, so this cannot tell a late failure from an early one"
    );
    let err = result.expect_err("the stand-in fails every time");
    assert!(matches!(err, Error::FailedAfterOutput(_)), "{err:?}");
    assert!(
        err.to_string().contains("No space left on device"),
        "ffmpeg's own words must survive in the error: {err}"
    );
    assert_eq!(
        fake.calls().len(),
        1,
        "ffmpeg was launched again after a failure that came after frames were written"
    );
}

// The same on the later levels: the CPU-decode pass and the software encoder's.
#[cfg(unix)]
#[test]
fn a_late_failure_on_the_cpu_pass_or_on_the_fallback_encoder_is_not_retried() {
    let _guard = spawn_lock();
    // CPU-decode pass: the hardware decode was refused, then frames were written.
    let fake = scripted(&[("-hwaccel", act::REFUSE)], act::LATE);
    let err = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .expect_err("fails");
    assert!(matches!(err, Error::FailedAfterOutput(_)), "{err:?}");
    assert_eq!(fake.calls().len(), 2, "{:?}", fake.calls());

    // Software encoder (the recursion): the hardware encoder is refused, libx264 writes
    // frames and then fails; the error comes back as it is.
    let fake = scripted(&[("-c:v h264_nvenc", act::REFUSE)], act::LATE);
    let err = encode(
        &h264_video(),
        "h264_nvenc",
        &nvenc_machine(),
        &safe_config(),
        &bare_session(),
        &fake.tools.resolver,
        &plenty,
    )
    .expect_err("fails");
    assert!(matches!(err, Error::FailedAfterOutput(_)), "{err:?}");
    assert_eq!(fake.calls().len(), 3, "{:?}", fake.calls());
}

// E-02: a stop that was requested before the call launches nothing at all.
#[cfg(unix)]
#[test]
fn a_stop_already_requested_launches_nothing() {
    let _guard = spawn_lock();
    for (cancel, signal) in [(true, 0), (false, 2)] {
        let fake = scripted(&[], act::OK);
        let (c, s) = (AtomicBool::new(cancel), AtomicI32::new(signal));
        let session = bare_session();
        let result = encode_video_with_memory(
            Some(&safe_performance_config()),
            &h264_video(),
            "h264_nvenc",
            RateControl::Bitrate,
            2_000_000,
            &session.dir().join("output.mp4"),
            &nvenc_machine(),
            &session,
            &fake.tools.resolver,
            4,
            &StopControl {
                cancel: &c,
                signal: &s,
            },
            &plenty,
            &mut |_| {},
        );
        assert!(
            matches!(&result, Err(e) if e.is_stop_request()),
            "{result:?}"
        );
        assert!(fake.calls().is_empty(), "{:?}", fake.calls());
    }
}

// The same on both platforms. On Unix, with no ffmpeg to find, a launch would be a "failed
// to start" error, not a stop. On Windows this no longer tells "nothing launched" from
// "launched, then killed" (see `no_tools`): the strict E-02 coverage is
// `a_stop_already_requested_launches_nothing`, on Unix.
#[test]
fn a_stop_already_requested_is_reported_before_any_launch() {
    let _guard = spawn_lock();
    let resolver = no_tools();
    for (cancel, signal) in [(true, 0), (false, 15)] {
        let (c, s) = (AtomicBool::new(cancel), AtomicI32::new(signal));
        let session = bare_session();
        let result = encode_video_with_memory(
            None,
            &h264_video(),
            "h264_nvenc",
            RateControl::Bitrate,
            2_000_000,
            &session.dir().join("output.mp4"),
            &nvenc_machine(),
            &session,
            &resolver,
            4,
            &StopControl {
                cancel: &c,
                signal: &s,
            },
            &plenty,
            &mut |_| {},
        );
        match (cancel, &result) {
            (true, Err(Error::Cancelled)) => {}
            (false, Err(Error::StoppedBySignal(name))) if name == "terminated" => {}
            _ => panic!("{result:?}"),
        }
    }
}

// ---- the memory guard of the CPU-encoder fallback ----

// A fallback that cannot fit in memory is refused.
#[cfg(unix)]
#[test]
fn a_fallback_that_cannot_fit_in_memory_is_refused_before_it_starts() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::REFUSE);
    let video = hevc_source("yuv420p10le");
    let result = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &session_with_header(&video),
        &fake.tools.resolver,
        &|| Some(GIB),
    );
    assert_memory_refusal(&result);
    // Hardware decode, then CPU decode, and then nothing: the third launch is the one
    // that would have been killed.
    assert_eq!(fake.calls().len(), 2, "{:?}", fake.calls());
}

// The fallback proceeds when memory allows.
#[cfg(unix)]
#[test]
fn a_fallback_that_fits_in_memory_proceeds() {
    let _guard = spawn_lock();
    let fake = scripted(&[], act::REFUSE);
    let video = hevc_source("yuv420p10le");
    let result = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &session_with_header(&video),
        &fake.tools.resolver,
        &|| Some(64 * GIB),
    );
    let err = result.expect_err("the stand-in fails every time");
    assert!(!err.to_string().contains("not enough memory"), "{err}");
    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "two hardware attempts and the CPU fallback");
    assert!(has(&calls[2], "-c:v libx265"), "{}", calls[2]);
}

// The fallback prices the threads it will use.
#[cfg(unix)]
#[test]
fn the_fallback_is_priced_with_the_threads_it_will_use() {
    let _guard = spawn_lock();
    let video = hevc_source("yuv420p10le");
    let run = |threads: i64| {
        let fake = scripted(&[], act::REFUSE);
        let cfg = Config {
            encoder_threads: threads,
            ..safe_config()
        };
        let result = encode(
            &video,
            "h264_vaapi",
            &vaapi_machine(),
            &cfg,
            &session_with_header(&video),
            &fake.tools.resolver,
            &|| Some(7 * GIB + GIB / 2),
        );
        (result, fake.calls().len())
    };
    let (result, launched) = run(16);
    assert_memory_refusal(&result);
    assert_eq!(launched, 2, "the fallback must not start");
    let (result, launched) = run(4);
    let err = result.expect_err("the stand-in fails every time");
    assert!(!err.to_string().contains("not enough memory"), "{err}");
    assert_eq!(launched, 3);
}

// libx265 caps at 16 threads: 32 are requested, 16 are used, and it is 16 that are
// priced. The memory holds exactly the 16-thread cost with its headroom, not the
// 32-thread one.
#[cfg(unix)]
#[test]
fn the_fallback_is_priced_with_the_capped_thread_count() {
    let _guard = spawn_lock();
    let video = hevc_source("yuv420p10le");
    let (w, h) = remap_output_size(&video, false);
    let needed16 = crate::memory::memory_needed_for_frame(w, h, Some(&video), "libx265", 16);
    let available = needed16 + needed16 / 4;
    assert!(
        crate::memory::memory_needed_for_frame(w, h, Some(&video), "libx265", 32) > available,
        "the fixture needs 32 threads to cost more than 16"
    );
    let fake = scripted(&[], act::REFUSE);
    let cfg = Config {
        encoder_threads: 32,
        ..safe_config()
    };
    let result = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &cfg,
        &session_with_header(&video),
        &fake.tools.resolver,
        &|| Some(available),
    );
    let err = result.expect_err("the stand-in fails every time");
    assert!(!err.to_string().contains("not enough memory"), "{err}");
    assert_eq!(fake.calls().len(), 3, "{:?}", fake.calls());
}

// A stop that arrives after the CPU pass failed is returned as it is, not as a memory
// refusal; with no stop the same arguments are refused (the control).
#[test]
fn a_stop_is_looked_at_before_the_memory_guard() {
    let video = hevc_source("yuv420p10le");
    let session = session_with_header(&video);
    let guard = |cancel: bool, signal: i32| {
        let (c, s) = (AtomicBool::new(cancel), AtomicI32::new(signal));
        guard_fallback(
            &StopControl {
                cancel: &c,
                signal: &s,
            },
            &session,
            &video,
            &safe_config(),
            "h264_vaapi",
            "libx265",
            4,
            &|| Some(GIB),
        )
    };
    assert!(matches!(guard(true, 0), Err(Error::Cancelled)));
    assert!(matches!(guard(false, 15), Err(Error::StoppedBySignal(n)) if n == "terminated"));
    assert!(matches!(guard(true, 2), Err(Error::StoppedBySignal(n)) if n == "interrupt"));
    assert_memory_refusal(&guard(false, 0).map(|()| EncodeSummary::default()));
}

#[test]
fn map_paths_accepts_text_paths() {
    let (x, y) = map_paths(Path::new("dir/x.pgm"), Path::new("dir/y.pgm")).unwrap();
    assert_eq!((x.as_str(), y.as_str()), ("dir/x.pgm", "dir/y.pgm"));
}

// A path that is not UTF-8 is refused, whichever of the two it is, as a session error.
#[cfg(unix)]
#[test]
fn map_paths_refuses_a_path_that_is_not_utf8() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let bad = Path::new(OsStr::from_bytes(b"/tmp/superview-\xff/x.pgm"));
    let good = Path::new("/tmp/y.pgm");
    for result in [map_paths(bad, good), map_paths(good, bad)] {
        match result {
            Err(Error::Session(e)) => assert!(e.msg.contains("not valid UTF-8"), "{}", e.msg),
            other => panic!("{other:?}"),
        }
    }
}

// Both platforms, no script: the guard's verdict is the error that comes back, whatever
// the passes before it said (here: ffmpeg cannot even be started).
#[test]
fn the_memory_refusal_is_the_error_returned_when_the_fallback_cannot_fit() {
    let _guard = spawn_lock();
    let video = hevc_source("yuv420p10le");
    let resolver = no_tools();
    let refused = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &session_with_header(&video),
        &resolver,
        &|| Some(GIB),
    );
    assert_memory_refusal(&refused);

    let allowed = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &session_with_header(&video),
        &resolver,
        &|| Some(64 * GIB),
    );
    let err = allowed
        .expect_err("no ffmpeg to run (Unix; on Windows the real one fails on the missing input)");
    assert!(!err.to_string().contains("not enough memory"), "{err}");

    // A reading that cannot be taken is not a refusal.
    let unknown = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &session_with_header(&video),
        &resolver,
        &|| None,
    );
    let err = unknown
        .expect_err("no ffmpeg to run (Unix; on Windows the real one fails on the missing input)");
    assert!(!err.to_string().contains("not enough memory"), "{err}");
}

// Without maps there is no size to price: the guard is skipped and the fallback goes on
// (the output size is known only when the session has maps).
#[test]
fn a_session_without_maps_skips_the_guard() {
    let _guard = spawn_lock();
    let video = hevc_source("yuv420p10le");
    let result = encode(
        &video,
        "h264_vaapi",
        &vaapi_machine(),
        &safe_config(),
        &bare_session(),
        &no_tools(),
        &|| Some(1),
    );
    let err = result
        .expect_err("no ffmpeg to run (Unix; on Windows the real one fails on the missing input)");
    assert!(!err.to_string().contains("not enough memory"), "{err}");
}

#[test]
fn a_video_without_a_stream_is_refused() {
    let video = VideoSpecs {
        file: "empty.mp4".into(),
        streams: Vec::new(),
    };
    let err = encode(
        &video,
        "libx264",
        &cpu_machine("libx264"),
        &safe_config(),
        &bare_session(),
        &no_tools(),
        &plenty,
    )
    .expect_err("nothing to encode");
    assert!(matches!(err, Error::InvalidVideo(_)), "{err:?}");
}

// ---- the real ffmpeg ----

/// The machine's ffmpeg, or `None` where there is none -- unless
/// `SUPERVIEW_REQUIRE_FFMPEG=1`, which CI sets, where its absence fails the test.
fn real_ffmpeg() -> Option<(ToolResolver, HashMap<String, String>)> {
    let resolver = ToolResolver::from_process();
    let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
    match check_ffmpeg(&Config::default(), &resolver) {
        Ok(caps) if caps["encoders"].contains("libx264") => Some((resolver, caps)),
        Ok(_) => {
            assert!(
                !require,
                "this ffmpeg has no libx264 although SUPERVIEW_REQUIRE_FFMPEG=1"
            );
            None
        }
        Err(err) => {
            assert!(
                !require,
                "no usable ffmpeg although SUPERVIEW_REQUIRE_FFMPEG=1: {err}"
            );
            None
        }
    }
}

/// A 4:3 test clip (`width`x`height`, `seconds` long at 10 fps) made by ffmpeg itself.
fn lavfi_clip(
    resolver: &ToolResolver,
    dir: &Path,
    width: i64,
    height: i64,
    seconds: i64,
) -> VideoSpecs {
    let file = dir.join("input.mp4");
    let source = format!("testsrc2=size={width}x{height}:rate=10:duration={seconds}");
    let cmd = resolver.ffmpeg_command([
        OsString::from("-hide_banner"),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        source.into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        file.clone().into(),
    ]);
    let (status, text) = crate::process::combined_output(cmd).unwrap();
    assert!(status.success(), "cannot make the test clip: {text}");
    VideoSpecs {
        file: file.to_string_lossy().into_owned(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width,
            height,
            pix_fmt: "yuv420p".into(),
            duration: seconds.to_string(),
            duration_float: seconds as f64,
            bitrate_int: 1_000_000,
            ..VideoStream::default()
        }],
    }
}

// A real encode, with real maps, ends in a non-empty file and
// the summary of a plain CPU conversion.
#[test]
fn a_real_clip_is_encoded_with_libx264() {
    let _guard = spawn_lock();
    let Some((resolver, ffmpeg)) = real_ffmpeg() else {
        return;
    };
    let session = bare_session();
    let out_dir = tempfile::tempdir().unwrap();
    let video = lavfi_clip(&resolver, out_dir.path(), 360, 270, 1);
    generate_pgm(&video, false, session.dir()).unwrap();
    let output = out_dir.path().join("output.mp4");

    let (cancel, signal) = stop_flags();
    let mut last = None;
    let summary = encode_video(
        Some(&Config {
            video_preset: "ultrafast".into(),
            ..Config::default()
        }),
        &video,
        "libx264",
        RateControl::Bitrate,
        1_000_000,
        &output,
        &ffmpeg,
        &session,
        &resolver,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        &mut |p| last = Some(p),
    )
    .expect("a real encode");

    assert_eq!(
        summary.hardware_acceleration,
        "Hardware: used CPU encode (libx264) + CPU decode"
    );
    assert!(
        std::fs::metadata(&output).is_ok_and(|m| m.len() > 0),
        "the output is empty or missing"
    );
    assert!(last.is_some_and(|p| p > 0.0), "no progress was reported");
    let dir = session.dir().to_path_buf();
    session.close().unwrap();
    assert!(!dir.exists());
}

// The proof at the centre of the pull request: a cancellation raised from the progress
// callback of a real encode leaves no ffmpeg behind and no session directory.
#[test]
fn a_real_encode_cancelled_from_the_progress_callback_leaves_nothing_behind() {
    let _guard = spawn_lock();
    let Some((real, ffmpeg)) = real_ffmpeg() else {
        return;
    };
    let session = bare_session();
    let work = tempfile::tempdir().unwrap();
    // 60 seconds at the slowest preset: far longer than the half second before the
    // first progress report, whatever the machine.
    let video = lavfi_clip(&real, work.path(), 480, 360, 60);
    generate_pgm(&video, false, session.dir()).unwrap();
    let output = work.path().join("output.mp4");

    // On Unix the resolver finds a wrapper that writes its pid and becomes the real
    // ffmpeg (exec keeps the pid), so the test can look for the process afterwards.
    #[cfg(unix)]
    let (resolver, pids) = {
        let pids = work.path().join("pids.txt");
        let body = format!(
            "echo $$ >> '{}'\nexec '{}' \"$@\"",
            pids.display(),
            real.resolve("ffmpeg").display()
        );
        (crate::probe::tests::fake_tools(&[("ffmpeg", &body)]), pids)
    };
    #[cfg(unix)]
    let resolver_ref = &resolver.resolver;
    #[cfg(not(unix))]
    let resolver_ref = &real;

    let (cancel, signal) = stop_flags();
    let cfg = Config {
        performance_mode: "safe".into(),
        video_preset: "veryslow".into(),
        ..Config::default()
    };
    let started = Instant::now();
    let mut reports = 0;
    let result = encode_video(
        Some(&cfg),
        &video,
        "libx264",
        RateControl::Bitrate,
        1_000_000,
        &output,
        &ffmpeg,
        &session,
        resolver_ref,
        4,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
        &mut |_| {
            reports += 1;
            cancel.store(true, Ordering::Release);
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(reports >= 1);
    assert!(
        started.elapsed() < Duration::from_secs(50),
        "the cancellation did not end the encode"
    );

    #[cfg(unix)]
    {
        let text = std::fs::read_to_string(&pids).unwrap();
        assert_eq!(text.lines().count(), 1, "one launch, then nothing: {text}");
        for pid in text.lines() {
            assert_no_trace_of(pid);
        }
    }
    let dir = session.dir().to_path_buf();
    session.close().unwrap();
    assert!(!dir.exists(), "the session directory was left behind");
}
