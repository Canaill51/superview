use std::sync::atomic::AtomicUsize;
use std::thread;
use std::time::Instant;

use superview_core::encode::cascade::EncodeSummary;
use superview_core::metrics::EncodingMetrics;
use superview_core::probe::EncoderProbe;
use superview_core::video::VideoStream;

use super::*;

const USE_INPUT: &str = crate::format::USE_INPUT_CODEC;

fn test_video() -> VideoSpecs {
    VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width: 1440,
            height: 1080,
            duration: "10".into(),
            duration_float: 10.0,
            bitrate: "5000000".into(),
            bitrate_int: 5_000_000,
            ..VideoStream::default()
        }],
    }
}

fn probe_of(results: &[(&str, bool, &str)]) -> EncoderProbeReport {
    EncoderProbeReport {
        results: results
            .iter()
            .map(|(encoder, usable, reason)| EncoderProbe {
                encoder: (*encoder).into(),
                usable: *usable,
                reason: (*reason).into(),
                ..EncoderProbe::default()
            })
            .collect(),
        ..EncoderProbeReport::default()
    }
}

fn ffmpeg_map(encoders: &str, accels: &str) -> HashMap<String, String> {
    HashMap::from([
        ("encoders".to_string(), encoders.to_string()),
        ("accels".to_string(), accels.to_string()),
    ])
}

/// The state the way the window builds it: ffmpeg available, preferences persisted in a
/// temporary directory the test can read back.
struct Fixture {
    state: AppState,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn prefs_path(&self) -> PathBuf {
        self.dir.path().join(prefs::FILE_NAME)
    }

    fn saved(&self) -> Preferences {
        prefs::load(&self.prefs_path())
    }
}

impl std::ops::Deref for Fixture {
    type Target = AppState;
    fn deref(&self) -> &AppState {
        &self.state
    }
}

impl std::ops::DerefMut for Fixture {
    fn deref_mut(&mut self) -> &mut AppState {
        &mut self.state
    }
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(prefs::FILE_NAME);
    Fixture {
        state: AppState::new(Preferences::default(), Some(path)),
        dir,
    }
}

fn outcome(summary: &str) -> EncodeOutcome {
    let mut metrics = EncodingMetrics::new_at("in.mp4", "out.mp4", Instant::now());
    metrics.input_file_size = 100 * 1024 * 1024;
    metrics.output_file_size = 150 * 1024 * 1024;
    metrics.end = Some(metrics.start + Duration::from_secs(60));
    EncodeOutcome {
        summary: EncodeSummary {
            hardware_acceleration: summary.into(),
        },
        metrics,
        encoder: "libx264".into(),
        bitrate: 1,
    }
}

// ---- the lists of controls ----

// (P-01)
#[test]
fn locking_and_unlocking_round_trips_every_control() {
    let mut view = ViewModel::default();
    view.set_enabled(locked(), false);
    for control in locked() {
        assert!(!view.is_enabled(*control), "{control:?} still enabled");
    }
    view.set_enabled(locked(), true);
    for control in locked() {
        assert!(view.is_enabled(*control), "{control:?} never re-enabled");
    }
}

// (E-14)
#[test]
fn the_locked_list_holds_diagnostic_and_not_quit() {
    assert!(
        locked().contains(&Control::Diagnostic),
        "its GPU probes would run beside the hardware encode"
    );
    assert!(!locked().contains(&Control::Quit), "Quit must stay usable");
    for control in [
        Control::Open,
        Control::SelectOutput,
        Control::Quality,
        Control::Squeeze,
        Control::Encoder,
    ] {
        assert!(locked().contains(&control), "{control:?} is not locked");
    }
}

#[test]
fn requires_ffmpeg_is_locked_minus_diagnostic_and_never_quit() {
    assert!(!requires_ffmpeg().contains(&Control::Diagnostic));
    assert!(!requires_ffmpeg().contains(&Control::Quit));
    for control in requires_ffmpeg() {
        assert!(locked().contains(control));
    }
    assert_eq!(requires_ffmpeg().len() + 1, locked().len());
}

// ---- can_start and the transitions of the files ----

#[test]
fn can_start_needs_input_output_ffmpeg_and_no_conversion() {
    type Setup = fn(&mut AppState);
    let cases: [(&str, Setup, bool); 6] = [
        ("nothing chosen", |_| {}, false),
        ("input only", |s| s.video = Some(test_video()), false),
        ("output only", |s| s.output_path = "out.mp4".into(), false),
        (
            "input and output",
            |s| {
                s.video = Some(test_video());
                s.output_path = "out.mp4".into();
            },
            true,
        ),
        (
            "everything but ffmpeg",
            |s| {
                s.video = Some(test_video());
                s.output_path = "out.mp4".into();
                s.ffmpeg_available = false;
            },
            false,
        ),
        (
            "already encoding",
            |s| {
                s.video = Some(test_video());
                s.output_path = "out.mp4".into();
                s.encoding = true;
            },
            false,
        ),
    ];
    for (name, setup, want) in cases {
        let mut state = fixture();
        setup(&mut state);
        assert_eq!(state.can_start(), want, "{name}");
        state.refresh_start();
        assert_eq!(state.view.start_enabled, want, "{name}: the button");
    }
}

#[test]
fn setting_input_and_output_refreshes_start() {
    let mut state = fixture();
    state.set_input(test_video());
    assert!(
        !state.view.start_enabled,
        "Start stays unavailable until a destination is chosen"
    );
    state.set_output("out.mp4");
    assert!(state.view.start_enabled);
}

// The trial window's Selection tests: the picker holds its control, once.
#[test]
fn a_second_picker_cannot_open_while_one_is_open() {
    let mut state = fixture();
    assert!(state.begin_pick(Control::Open));
    assert!(!state.view.is_enabled(Control::Open));
    assert!(!state.begin_pick(Control::Open));
    // The other picker is independent.
    assert!(state.begin_pick(Control::SelectOutput));
    state.finish_pick(Control::Open);
    assert!(state.view.is_enabled(Control::Open));
    assert!(
        state.begin_pick(Control::Open),
        "the picker must be reusable after it closed"
    );
}

// An open picker keeps its control down through every unlock, not only the one that
// closes it.
#[test]
fn an_unlock_cannot_re_enable_a_control_whose_picker_is_open() {
    let mut state = fixture();
    assert!(state.begin_pick(Control::Open));

    state.begin_encoding();
    state.finish_encoding(); // unlocks `locked()`
    assert!(
        !state.view.is_enabled(Control::Open),
        "re-enabled by the end of a conversion"
    );
    state.apply_ffmpeg_availability(); // unlocks `requires_ffmpeg()`
    assert!(
        !state.view.is_enabled(Control::Open),
        "re-enabled by the ffmpeg retry"
    );
    assert!(!state.begin_pick(Control::Open), "a second picker opened");

    state.finish_pick(Control::Open);
    assert!(state.view.is_enabled(Control::Open));
    assert!(state.begin_pick(Control::Open));
}

#[test]
fn a_picker_cannot_open_on_a_locked_control() {
    let mut state = fixture();
    state.begin_encoding();
    assert!(!state.begin_pick(Control::Open));
    state.finish_encoding();
    state.set_ffmpeg_unavailable();
    assert!(!state.begin_pick(Control::SelectOutput));
}

// "A refused file keeps the previous input" (the trial's Selection test).
#[test]
fn a_refused_input_keeps_the_previous_one() {
    let mut state = fixture();
    state.apply_chosen_input(Path::new("clip.mp4"), |_| Ok(test_video()));
    state.set_output("out.mp4");

    let outcome = state.apply_chosen_input(Path::new("bad.mp4"), |_| {
        Err(Error::Plain("moov atom not found".into()))
    });

    assert!(matches!(outcome, ChosenOutcome::Rejected(_)));
    assert_eq!(
        state.video,
        Some(test_video()),
        "the previous input was lost"
    );
    assert_eq!(state.view.input_name, "clip.mp4");
    assert!(state.can_start(), "the previous input is still startable");
    assert_eq!(state.view.status, "Status: Invalid input");
}

#[test]
fn the_state_sanitises_an_unknown_quality_profile() {
    let prefs = Preferences {
        quality_profile: "Turbo".into(),
        ..Preferences::default()
    };
    assert_eq!(AppState::new(prefs, None).view.quality, "Balanced");
}

#[test]
fn the_progress_bar_stays_within_zero_and_one_and_ignores_nan() {
    let mut state = fixture();
    for (given, want) in [(0.42, 0.42), (2.0, 1.0), (-1.0, 0.0), (f64::NAN, 0.0)] {
        state.view.progress = 0.5;
        state.set_progress(given);
        assert_eq!(state.view.progress, want, "{given}");
    }
}

#[test]
fn a_picker_closing_during_a_conversion_leaves_the_control_locked() {
    let mut state = fixture();
    assert!(state.begin_pick(Control::Open));
    state.begin_encoding();
    state.finish_pick(Control::Open);
    assert!(!state.view.is_enabled(Control::Open));
    state.finish_encoding();
    assert!(state.view.is_enabled(Control::Open));
}

#[test]
fn a_chosen_input_is_ignored_during_a_conversion() {
    let mut state = fixture();
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();
    state.begin_encoding();
    state.announce_start();

    let mut checked = false;
    let outcome = state.apply_chosen_input(Path::new("other.mp4"), |_| {
        checked = true;
        Ok(test_video())
    });

    assert_eq!(outcome, ChosenOutcome::Ignored);
    assert!(!checked, "the file must not even be checked");
    assert_eq!(state.video, Some(test_video()));
    assert_eq!(state.view.status, STARTING_ENCODER_STATUS);
}

#[test]
fn a_chosen_output_is_ignored_during_a_conversion() {
    let mut state = fixture();
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();
    state.begin_encoding();
    state.announce_start();

    assert_eq!(
        state.apply_chosen_output(Path::new("other")),
        ChosenOutcome::Ignored
    );
    assert_eq!(state.output_path, "out.mp4");
    assert_eq!(state.view.output_name, NO_FILE);
    assert_eq!(state.view.status, STARTING_ENCODER_STATUS);
}

#[test]
fn a_chosen_output_when_idle_gets_the_mp4_extension() {
    let mut state = fixture();
    assert_eq!(
        state.apply_chosen_output(Path::new("other")),
        ChosenOutcome::Applied
    );
    assert_eq!(state.output_path, "other.mp4");
    assert_eq!(state.view.output_name, "other.mp4");
    assert_eq!(state.view.status, "Status: Output selected");
}

#[test]
fn a_chosen_input_when_idle_is_loaded() {
    let mut state = fixture();
    let outcome = state.apply_chosen_input(Path::new("clip.mp4"), |path| {
        assert_eq!(path, "clip.mp4");
        Ok(test_video())
    });
    assert_eq!(outcome, ChosenOutcome::Applied);
    assert_eq!(state.video, Some(test_video()));
    assert_eq!(state.view.input_name, "clip.mp4");
    assert_eq!(state.view.status, "Status: Input loaded");
}

// The invalid-input branch of the same scenario.
#[test]
fn an_invalid_input_when_idle_is_reported_and_not_recorded() {
    let mut state = fixture();
    let outcome = state.apply_chosen_input(Path::new("bad.mp4"), |_| {
        Err(Error::Plain("moov atom not found".into()))
    });
    assert_eq!(
        outcome,
        ChosenOutcome::Rejected("moov atom not found".into())
    );
    assert!(
        state.video.is_none(),
        "an input the check refused was recorded"
    );
    assert_eq!(state.view.status, "Status: Invalid input");
    assert_eq!(state.view.input_name, NO_FILE);
}

// Wanted improvement (b): the video check takes text, so a path that is not UTF-8 is refused
// with a message that says so, instead of checking a lossy copy of another name.
#[test]
fn a_non_utf8_input_path_is_refused_with_a_clear_message() {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(b"clip-\xff.mp4".to_vec()))
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[0x63, 0xD800, 0x2E]))
    };
    let mut state = fixture();
    let mut checked = false;
    let outcome = state.apply_chosen_input(&path, |_| {
        checked = true;
        Ok(test_video())
    });
    let ChosenOutcome::Rejected(message) = outcome else {
        panic!("a non-UTF-8 path was not refused: {outcome:?}");
    };
    assert!(message.contains("not valid UTF-8"), "{message}");
    assert!(!checked, "the check must not run on a lossy copy");
    assert!(state.video.is_none());
    assert_eq!(state.view.status, "Status: Invalid input");

    let outcome = state.apply_chosen_output(&path);
    assert!(
        matches!(&outcome, ChosenOutcome::Rejected(m) if m.contains("not valid UTF-8")),
        "{outcome:?}"
    );
    assert_eq!(state.output_path, "");
}

// ---- the hardware line ----

#[test]
fn the_hardware_line_waits_for_the_probe() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("h264_nvenc,libx264", "cuda");
    state.video = Some(test_video());

    state.refresh_hardware_status();
    let before = state.view.hardware.clone();
    assert!(
        !before.contains("h264_nvenc"),
        "the window named an encoder it has not verified: {before:?}"
    );
    assert!(before.contains("checking"), "{before:?}");

    state.set_encoder_probe(probe_of(&[("h264_nvenc", true, ""), ("libx264", true, "")]));
    state.refresh_hardware_status();
    assert!(
        state.view.hardware.contains("h264_nvenc"),
        "once the probe confirms it, the plan must name the encoder: {:?}",
        state.view.hardware
    );
}

#[test]
fn a_probe_refusal_removes_the_encoder_from_the_plan() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("h264_nvenc,libx264", "cuda");
    state.video = Some(test_video());

    state.set_encoder_probe(probe_of(&[
        (
            "h264_nvenc",
            false,
            "The minimum required Nvidia driver for nvenc is 610.00 or newer",
        ),
        ("libx264", true, ""),
    ]));
    state.refresh_hardware_status();

    assert_eq!(state.ffmpeg["encoders"], "libx264");
    let plan = &state.view.hardware;
    assert!(!plan.contains("nvenc"), "{plan:?}");
    assert!(plan.contains("libx264"), "{plan:?}");
}

#[test]
fn refresh_hardware_status_reports_a_missing_ffmpeg() {
    let mut state = fixture();
    state.ffmpeg_available = false;
    state.refresh_hardware_status();
    assert_eq!(state.view.hardware, "Hardware: ffmpeg unavailable");
}

#[test]
fn refresh_hardware_status_describes_the_planned_path() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("h264_nvenc,libx264", "cuda");
    state.view.encoder_options = encoder_options_for(&state.ffmpeg["encoders"]);
    // Selected before the probe lands: an order the window can see.
    state.encoder_changed("libx264 encoder");
    state.video = Some(test_video());
    state.set_encoder_probe(probe_of(&[("h264_nvenc", true, ""), ("libx264", true, "")]));

    state.refresh_hardware_status();

    let line = &state.view.hardware;
    assert!(line.starts_with("Hardware: "), "{line:?}");
    assert!(
        !line.contains("checking"),
        "the plan was never computed: {line:?}"
    );
    assert!(
        !line.contains("not in approved list"),
        "the dropdown label reached the planner unparsed: {line:?}"
    );
    assert!(
        line.contains("libx264") && !line.contains("nvenc"),
        "want the plan for the selected libx264: {line:?}"
    );
}

// There is no widget to be missing here; the closest thing is a state fresh from `new`
// taking every transition in any order.
#[test]
fn every_transition_is_safe_on_a_fresh_state() {
    let mut state = AppState::default();
    state.refresh_start();
    state.refresh_hardware_status();
    state.verify_hardware_in_background();
    state.set_input(test_video());
    state.set_output("out.mp4");
    state.begin_encoding();
    state.request_cancel();
    state.finish_encoding();
    state.refresh_encoder_options();
    assert!(state.shutdown("test"));
}

#[test]
fn set_ffmpeg_unblocks_the_window() {
    let mut state = fixture();
    state.ffmpeg_available = false;
    state.view.set_enabled(locked(), false);
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();

    state.refresh_start();
    assert!(
        !state.view.start_enabled,
        "Start must stay off without ffmpeg"
    );
    state.refresh_hardware_status();
    assert_eq!(state.view.hardware, "Hardware: ffmpeg unavailable");

    state.set_ffmpeg(ffmpeg_map("h264_nvenc,libx264", "cuda"));
    state.view.set_enabled(locked(), true);
    state.refresh_start();
    state.refresh_hardware_status();

    assert!(state.ffmpeg_available);
    assert!(state.view.start_enabled, "Start should come back");
    assert!(!state.view.hardware.contains("unavailable"));

    // Without a probe the line above is "checking which encoders...", which the check on
    // "unavailable" alone accepts (E-19). Once the probe lands, it must be a plan.
    state.set_encoder_probe(probe_of(&[("h264_nvenc", true, ""), ("libx264", true, "")]));
    state.refresh_hardware_status();
    let line = &state.view.hardware;
    assert!(
        !line.contains("checking") && line.contains("h264_nvenc"),
        "want the plan naming h264_nvenc: {line:?}"
    );
    for control in locked() {
        assert!(state.view.is_enabled(*control), "{control:?} not restored");
    }
}

// (E-14, first review finding)
#[test]
fn diagnostic_stays_usable_without_ffmpeg() {
    let mut state = fixture();
    state.set_ffmpeg_unavailable();

    assert!(
        state.view.is_enabled(Control::Diagnostic),
        "the tool that reports the missing ffmpeg cannot be reached"
    );
    for control in [
        Control::Open,
        Control::SelectOutput,
        Control::Quality,
        Control::Encoder,
        Control::Squeeze,
    ] {
        assert!(
            !state.view.is_enabled(control),
            "{control:?} usable without ffmpeg"
        );
    }
    assert!(
        state.view.is_enabled(Control::Quit),
        "Quit is always usable"
    );

    // The retry finds ffmpeg, then a conversion runs.
    state.set_ffmpeg(ffmpeg_map("libx264", ""));
    state.apply_ffmpeg_availability();
    assert!(state.view.is_enabled(Control::Open));
    state.begin_encoding();
    assert!(
        !state.view.is_enabled(Control::Diagnostic),
        "its GPU probes would run beside the hardware encode"
    );
    assert!(state.view.is_enabled(Control::Quit));
}

#[test]
fn a_stale_verification_does_not_overwrite_a_newer_line() {
    let mut state = fixture();
    state.hardware_status_gen = 7;
    state.view.hardware = "current".into();

    state.apply_verified_hardware_line(6, "an answer for a state that is gone");
    assert_eq!(state.view.hardware, "current");

    state.apply_verified_hardware_line(7, "the answer for the current state");
    assert_eq!(state.view.hardware, "the answer for the current state");
}

#[test]
fn a_refresh_invalidates_a_verification_in_flight() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("libx264", "");
    state.video = Some(test_video());
    state.set_encoder_probe(probe_of(&[("libx264", true, "")]));

    let in_flight = state.hardware_status_gen;
    state.refresh_hardware_status();
    let settled = state.view.hardware.clone();

    state.apply_verified_hardware_line(in_flight, "computed before the refresh");
    assert_eq!(state.view.hardware, settled);
}

// Wanted improvement (a): the unverified plan must not stay on screen when a verification could
// not be completed.
#[test]
fn a_failed_verification_replaces_the_line_with_a_neutral_text() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("libx264", "");
    state.video = Some(test_video());
    state.set_encoder_probe(probe_of(&[("libx264", true, "")]));
    state.refresh_hardware_status();
    let plan = state.view.hardware.clone();
    assert!(plan.contains("libx264"), "{plan:?}");

    let generation = state.hardware_status_gen;
    state.apply_verification_failed(generation);
    assert_eq!(state.view.hardware, HARDWARE_UNVERIFIED);
    assert!(!state.view.hardware.contains("libx264"));

    // A failure that is itself stale leaves a newer line alone.
    state.refresh_hardware_status();
    let newer = state.view.hardware.clone();
    state.apply_verification_failed(generation);
    assert_eq!(state.view.hardware, newer);
}

// The hook behind verifyHardwareInBackground.
#[test]
fn the_verification_is_requested_only_when_it_can_be_answered() {
    let requests = Arc::new(Mutex::new(Vec::<VerificationRequest>::new()));
    let mut state = fixture();
    let sink = Arc::clone(&requests);
    state.set_verify_hook(move |request| sink.lock().unwrap().push(request));
    let count = || requests.lock().unwrap().len();

    // No video, no probe: nothing to ask.
    state.verify_hardware_in_background();
    assert_eq!(count(), 0);
    state.video = Some(test_video());
    state.verify_hardware_in_background();
    assert_eq!(count(), 0, "no probe yet");

    state.ffmpeg = ffmpeg_map("libx264", "");
    state.set_encoder_probe(probe_of(&[("libx264", true, "")]));
    state.squeeze_changed(true);
    assert_eq!(count(), 1, "the squeeze box changes the output frame");
    let request = requests.lock().unwrap()[0].clone();
    assert!(request.squeeze);
    assert_eq!(request.generation, state.hardware_status_gen);
    assert_eq!(request.selection, "");

    state.encoder_changed("libx264 encoder");
    assert_eq!(count(), 2, "the codec changes the plan");
    assert_eq!(requests.lock().unwrap()[1].selection, "libx264");

    // F-07: the profile picks the preset the conversion asks the driver with.
    state.quality_changed("Fast");
    assert_eq!(count(), 3, "the preset changes the question to the driver");
    assert_eq!(requests.lock().unwrap()[2].quality, "Fast");

    state.ffmpeg_available = false;
    state.encoder_changed("libx264 encoder");
    assert_eq!(count(), 3, "without ffmpeg there is nothing to verify");
}

// ---- the lifecycle of a conversion ----

// (P-01)
#[test]
fn the_encoding_round_trip_restores_every_control() {
    let mut state = fixture();
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();
    state.refresh_start();
    assert!(state.view.start_enabled);

    state.begin_encoding();

    assert!(state.is_encoding());
    for control in locked() {
        assert!(
            !state.view.is_enabled(*control),
            "{control:?} enabled while encoding"
        );
    }
    assert!(!state.view.start_enabled);
    assert!(state.view.cancel_enabled);
    assert_eq!(state.view.progress, 0.0);

    state.finish_encoding();

    assert!(!state.is_encoding());
    for control in locked() {
        assert!(
            state.view.is_enabled(*control),
            "{control:?} never re-enabled"
        );
    }
    assert!(!state.view.cancel_enabled);
    assert!(
        state.view.start_enabled,
        "input and output are still selected"
    );
}

// (P-02)
#[test]
fn begin_encoding_returns_a_flag_that_request_cancel_raises() {
    let mut state = fixture();
    let (held, _done) = state.begin_encoding();
    assert!(!held.load(Ordering::Acquire));
    assert!(state.lifecycle.has_cancel());

    state.request_cancel();

    assert!(
        !state.lifecycle.has_cancel(),
        "the field should be cleared once the flag is raised"
    );
    assert!(
        held.load(Ordering::Acquire),
        "the flag handed to the caller was never raised by request_cancel"
    );
}

#[test]
fn request_cancel_does_nothing_when_idle() {
    let mut state = fixture();
    assert!(!state.request_cancel());
    assert_eq!(state.view.status, "Status: Ready");
}

#[test]
fn request_cancel_stops_a_running_conversion() {
    let mut state = fixture();
    let (held, _done) = state.begin_encoding();
    assert!(state.request_cancel());
    assert!(held.load(Ordering::Acquire));
    assert_eq!(state.view.status, "Status: Cancelling...");
}

#[test]
fn request_cancel_is_idempotent() {
    // The Cancel button and the window close can both fire.
    let mut state = fixture();
    state.begin_encoding();
    assert!(state.request_cancel());
    assert!(
        !state.request_cancel(),
        "the second call reported a cancellation it did not perform"
    );
}

#[test]
fn a_cancelled_conversion_still_finishes_cleanly() {
    let mut state = fixture();
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();
    state.begin_encoding();
    state.request_cancel();

    state.finish_encoding();

    assert!(!state.is_encoding());
    for control in locked() {
        assert!(state.view.is_enabled(*control), "{control:?} not restored");
    }
}

// (E-14) The dialogs are the `Completion` returned.
fn running(video_and_output: bool) -> Fixture {
    let mut state = fixture();
    state.view.hardware = "Hardware: planned h264_nvenc".into();
    if video_and_output {
        state.video = Some(test_video());
        state.output_path = "out.mp4".into();
    }
    state.begin_encoding();
    state
}

#[test]
fn a_cancellation_is_not_a_failure() {
    let mut state = running(true);
    state.request_cancel();

    let completion = state.complete_encoding(Err(Error::Cancelled), "out.mp4");

    assert_eq!(
        completion,
        Completion::Cancelled,
        "no dialog for what the user asked"
    );
    assert_eq!(state.view.status, "Status: Cancelled");
    assert_eq!(state.view.results, "Results: last run cancelled");
    assert_eq!(state.view.hardware, "", "the plan nobody ran must not stay");
    assert!(!state.is_encoding(), "not returned to idle");
    assert!(state.view.start_enabled);
}

#[test]
fn only_a_real_cancellation_is_one() {
    // A text carrying the cancellation message is a failure (not a match
    // on the string).
    let mut state = running(true);
    let text = Error::Plain("encoding failed: encoding interrupted by user".into());
    let completion = state.complete_encoding(Err(text), "out.mp4");
    assert!(
        matches!(completion, Completion::Failed { .. }),
        "{completion:?}"
    );
    assert_eq!(state.view.status, "Status: Failed");

    // A cancellation seen through a failure after output is one.
    let mut state = running(true);
    let wrapped = Error::FailedAfterOutput(Box::new(Error::Cancelled));
    let completion = state.complete_encoding(Err(wrapped), "out.mp4");
    assert_eq!(completion, Completion::Cancelled);
    assert_eq!(state.view.status, "Status: Cancelled");
}

#[test]
fn a_failure_reports_and_shows_the_error() {
    let mut state = running(true);

    let completion = state.complete_encoding(
        Err(Error::Plain("ffmpeg exited with code 1".into())),
        "out.mp4",
    );

    assert_eq!(
        completion,
        Completion::Failed {
            message: "ffmpeg exited with code 1".into(),
            missing_ffmpeg: false,
        }
    );
    assert_eq!(state.view.status, "Status: Failed");
    assert_eq!(state.view.results, "Results: last run failed");
    assert_eq!(state.view.hardware, "", "the plan nobody ran must not stay");
    assert!(!state.is_encoding(), "not returned to idle after a failure");
}

// A verification asked before Start must not land during the conversion, nor after the
// completion has written its own line.
#[test]
fn a_verification_in_flight_cannot_overwrite_the_conversions_hardware_line() {
    let mut state = fixture();
    state.video = Some(test_video());
    state.output_path = "out.mp4".into();
    state.view.hardware = "Hardware: planned".into();
    let before_start = state.hardware_status_gen;

    state.begin_encoding();
    state.apply_verified_hardware_line(before_start, "an answer computed before Start");
    assert_eq!(
        state.view.hardware, "Hardware: planned",
        "landed during the conversion"
    );

    let during = state.hardware_status_gen;
    state.complete_encoding(Ok(outcome("Hardware: encoded with libx264")), "out.mp4");
    state.apply_verified_hardware_line(during, "late answer");
    state.apply_verified_hardware_line(before_start, "later answer");
    assert_eq!(state.view.hardware, "Hardware: encoded with libx264");

    // And the same after a failure, which empties the line.
    let mut state = running(true);
    let during = state.hardware_status_gen;
    state.complete_encoding(Err(Error::Plain("boom".into())), "out.mp4");
    state.apply_verified_hardware_line(during, "late answer");
    assert_eq!(state.view.hardware, "");
}

#[test]
fn a_missing_ffmpeg_failure_asks_for_the_install_link() {
    let mut state = running(true);
    let message = "cannot find ffmpeg/ffprobe on your system";
    let completion = state.complete_encoding(Err(Error::Plain(message.into())), "out.mp4");
    assert_eq!(
        completion,
        Completion::Failed {
            message: message.into(),
            missing_ffmpeg: true,
        }
    );
}

#[test]
fn a_success_reports_and_says_where_the_file_is() {
    let mut state = running(true);
    state.view.progress = 0.4;

    let completion =
        state.complete_encoding(Ok(outcome("Hardware: encoded with libx264")), "out.mp4");

    assert_eq!(
        completion,
        Completion::Succeeded {
            message: "Transform complete. Output file:\nout.mp4".into()
        }
    );
    assert_eq!(state.view.status, "Status: Completed");
    assert_eq!(
        state.view.results,
        "Results: in=100.0 MB | out=150.0 MB (150.0%) | total=1m0s | encode=0s"
    );
    assert_eq!(state.view.hardware, "Hardware: encoded with libx264");
    assert_eq!(state.view.progress, 1.0);
    assert!(!state.is_encoding());
    // P-01 again, on the success path: every control came back.
    for control in locked() {
        assert!(state.view.is_enabled(*control), "{control:?} not restored");
    }
}

#[test]
fn a_system_stop_quits_instead_of_reporting() {
    let mut state = running(true);
    let quit_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&quit_calls);
    state.set_quit_hook(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let before = state.view.clone();

    let stopped = Error::StoppedBySignal("terminated".into());
    let completion = state.complete_encoding(Err(stopped), "out.mp4");

    assert_eq!(
        completion,
        Completion::Quit,
        "no dialog: it only delays the SIGKILL"
    );
    assert_eq!(
        quit_calls.load(Ordering::SeqCst),
        1,
        "the application must quit"
    );
    assert_eq!(state.view, before, "nothing is reported on the way out");

    // Through a failure after output too.
    let mut state = running(true);
    let wrapped = Error::FailedAfterOutput(Box::new(Error::StoppedBySignal("interrupt".into())));
    assert_eq!(
        state.complete_encoding(Err(wrapped), "out.mp4"),
        Completion::Quit
    );
}

#[test]
fn complete_encoding_does_nothing_once_the_application_is_shutting_down() {
    let mut state = running(true);
    state.lifecycle.shutting_down.store(true, Ordering::Release);
    let before = state.view.status.clone();

    let completion = state.complete_encoding(
        Err(Error::Plain("ffmpeg exited with code 1".into())),
        "out.mp4",
    );

    assert_eq!(
        completion,
        Completion::Ignored,
        "a dialog on a closing window"
    );
    assert_eq!(state.view.status, before);
    assert!(state.is_encoding(), "the closing window is not touched");
}

// (E-03) The thread stands in for the real one: it stops on the
// cancel flag, takes a moment to "clean up", and only then releases done -- the order the
// pipeline's cleanup imposes.
fn wait_for(flag: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !flag.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn shutdown_cancels_and_waits_for_the_thread_to_unwind() {
    let mut state = fixture();
    state.set_shutdown_grace(Duration::from_secs(5));
    let (cancel, done) = state.begin_encoding();
    let unwound = Arc::new(AtomicBool::new(false));

    let worker = {
        let (cancel, unwound) = (Arc::clone(&cancel), Arc::clone(&unwound));
        thread::spawn(move || {
            wait_for(&cancel);
            // The cleanup: kill, reap, remove, clean up.
            thread::sleep(Duration::from_millis(50));
            unwound.store(true, Ordering::Release);
            done.release();
        })
    };

    assert!(
        state.shutdown("test"),
        "shutdown reported a timeout although the thread unwound"
    );
    assert!(
        unwound.load(Ordering::Acquire),
        "shutdown returned before the thread had unwound: ffmpeg would outlive the window"
    );
    assert!(
        cancel.load(Ordering::Acquire),
        "shutdown did not raise the cancel flag"
    );
    assert!(state.lifecycle.is_shutting_down());
    worker.join().expect("worker");
}

#[test]
fn shutdown_gives_up_after_the_grace_period() {
    let mut state = fixture();
    state.set_shutdown_grace(Duration::from_millis(50));
    state.begin_encoding(); // and nobody will ever release done

    let started = Instant::now();
    assert!(!state.shutdown("test"), "nothing released done");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the bound is not applied"
    );
}

#[test]
fn shutdown_returns_at_once_when_nothing_is_running() {
    let state = fixture();
    state.set_shutdown_grace(Duration::from_secs(3600));
    let started = Instant::now();
    assert!(state.shutdown("test"));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(state.lifecycle.is_shutting_down());
}

#[test]
fn shutdown_is_idempotent() {
    // The window close hook and the line after the event loop both call it.
    let mut state = fixture();
    let (_cancel, done) = state.begin_encoding();
    done.release();
    assert!(state.shutdown("first") && state.shutdown("second"));
}

#[test]
fn shutdown_from_the_completion_path_does_not_wait_for_itself() {
    let mut state = fixture();
    state.set_shutdown_grace(Duration::from_secs(3));
    let (_cancel, done) = state.begin_encoding();

    let unwound = Arc::new(AtomicBool::new(false));
    let lifecycle = state.lifecycle();
    let result = Arc::clone(&unwound);
    state.set_quit_hook(move || {
        // Quit closes the window, whose hook is shutdown.
        result.store(lifecycle.shutdown("window closing"), Ordering::Release);
    });

    let started = Instant::now();
    run_encoding(
        &done,
        || Err(Error::StoppedBySignal("terminated".into())),
        |result| {
            state.complete_encoding(result, "out.mp4");
        },
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the completion path waited {:?} for its own thread to release done",
        started.elapsed()
    );
    assert!(
        unwound.load(Ordering::Acquire),
        "shutdown reported a timeout although the conversion had unwound"
    );
}

#[test]
fn run_encoding_releases_done_on_panic() {
    let mut state = fixture();
    let (_cancel, done) = state.begin_encoding();

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_encoding(&done, || -> () { panic!("pipeline") }, |()| {});
    }));

    assert!(outcome.is_err(), "the panic must still propagate");
    assert!(
        done.is_released(),
        "a panicking conversion left done open: every exit would wait out the grace period"
    );
}

#[test]
fn run_encoding_releases_done_before_it_completes() {
    let done = Done::default();
    let released_at_completion = std::cell::Cell::new(false);
    run_encoding(
        &done,
        || 7,
        |n| {
            assert_eq!(n, 7);
            released_at_completion.set(done.is_released());
        },
    );
    assert!(released_at_completion.get());
}

#[test]
fn shutdown_waits_only_once() {
    let mut state = fixture();
    state.set_shutdown_grace(Duration::from_millis(300));
    state.begin_encoding(); // and nobody will ever release done

    assert!(!state.shutdown("window closing"), "nothing released done");

    let started = Instant::now();
    assert!(!state.shutdown("event loop stopped"));
    assert!(
        started.elapsed() < Duration::from_millis(150),
        "the second shutdown waited {:?} after the first had given up",
        started.elapsed()
    );
}

// ---- the codec dropdown and the preferences ----

// (E-15)
const SAVED: &str = "hevc_nvenc encoder";

fn with_saved_encoder() -> Fixture {
    let mut state = fixture();
    state.prefs.encoder_selection = SAVED.into();
    state.save_prefs();
    state
}

#[test]
fn a_fallback_is_not_persisted_over_the_saved_preference() {
    let mut state = with_saved_encoder();
    state.ffmpeg = ffmpeg_map("libx264", "");

    state.refresh_encoder_options();

    assert_eq!(
        state.view.encoder_selected, USE_INPUT,
        "the first option when the saved one is absent"
    );
    assert_eq!(state.prefs.encoder_selection, SAVED);
    assert_eq!(
        state.saved().encoder_selection,
        SAVED,
        "the fallback was persisted over the saved choice"
    );
    assert!(
        !state.restoring_selection,
        "left set: the next user choice would not be saved"
    );
}

#[test]
fn the_saved_selection_is_restored_when_offered() {
    let mut state = with_saved_encoder();
    state.ffmpeg = ffmpeg_map("hevc_nvenc,libx264", "");
    state.view.encoder_selected = String::new(); // as at startup

    state.refresh_encoder_options();

    assert_eq!(state.view.encoder_selected, SAVED);
    assert_eq!(
        state.view.encoder_options,
        vec![USE_INPUT, "hevc_nvenc encoder", "libx264 encoder"]
    );
}

#[test]
fn a_user_choice_is_still_persisted() {
    let mut state = fixture();
    state.ffmpeg = ffmpeg_map("hevc_nvenc,libx264", "");
    state.refresh_encoder_options();

    state.encoder_changed("libx264 encoder"); // what a click does

    assert_eq!(state.saved().encoder_selection, "libx264 encoder");
}

// (sixth review finding)
#[test]
fn refresh_encoder_options_prefers_the_saved_choice_over_a_fallback() {
    let mut state = with_saved_encoder();

    // Startup without ffmpeg.
    state.ffmpeg = HashMap::new();
    state.refresh_encoder_options();
    assert_eq!(state.view.encoder_selected, USE_INPUT, "the only option");

    // The retry finds ffmpeg.
    state.ffmpeg = ffmpeg_map("hevc_nvenc,libx264", "");
    state.refresh_encoder_options();
    assert_eq!(
        state.view.encoder_selected, SAVED,
        "the retry kept the fallback instead of the saved choice"
    );

    // A saved choice that is not offered gives way to the current selection.
    state.encoder_changed("libx264 encoder"); // a click: persisted
    state.prefs.encoder_selection = "av1_nvenc encoder".into();
    state.refresh_encoder_options();
    assert_eq!(state.view.encoder_selected, "libx264 encoder");
}

#[test]
fn quality_and_squeeze_are_persisted_and_restored() {
    let mut state = fixture();
    assert_eq!(state.view.quality, "Balanced");
    state.quality_changed("Fast");
    state.squeeze_changed(true);
    assert_eq!(state.view.quality, "Fast");
    assert!(state.view.squeeze);

    let saved = state.saved();
    assert_eq!(saved.quality_profile, "Fast");
    assert!(saved.squeeze_source);

    // A new session starts from them.
    let next = AppState::new(saved, None);
    assert_eq!(next.view.quality, "Fast");
    assert!(next.view.squeeze);
}

#[test]
fn the_theme_is_persisted_and_restored() {
    let mut state = fixture();
    assert_eq!(state.view.theme, "Dark");
    state.theme_changed("Light");
    assert_eq!(state.view.theme, "Light");
    assert_eq!(state.saved().theme, "Light");
    // A new session starts from it.
    assert_eq!(AppState::new(state.saved(), None).view.theme, "Light");
}

#[test]
fn a_state_without_a_preferences_path_saves_nothing_and_does_not_fail() {
    let mut state = AppState::default();
    state.quality_changed("Fast");
    state.encoder_changed("libx264 encoder");
    state.squeeze_changed(true);
    assert_eq!(state.view.quality, "Fast");
}

// ---- the handler the pipeline calls ----

#[derive(Default)]
struct Recorder {
    bars: Mutex<Vec<f64>>,
    statuses: Mutex<Vec<String>>,
}

struct RecorderSink(Arc<Recorder>);

impl ProgressSink for RecorderSink {
    fn bar(&self, fraction: f64) {
        self.0.bars.lock().unwrap().push(fraction);
    }
    fn status(&self, text: &str) {
        self.0.statuses.lock().unwrap().push(text.to_string());
    }
}

fn handler_with_sink() -> (GuiHandler, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let handler = GuiHandler::new(
        4_000_000,
        "libx265".into(),
        false,
        Some(&test_video()),
        Some(Box::new(RecorderSink(Arc::clone(&recorder)))),
    );
    (handler, recorder)
}

#[test]
fn show_progress_sets_the_bar_as_a_fraction() {
    let (handler, recorder) = handler_with_sink();
    handler.show_progress(42.0);
    let bars = recorder.bars.lock().unwrap();
    assert_eq!(bars.len(), 1);
    assert!((bars[0] - 0.42).abs() < 1e-9, "bar = {} after 42%", bars[0]);
}

// The same for the detail.
#[test]
fn a_handler_without_a_sink_ignores_progress() {
    let handler = GuiHandler::new(1, String::new(), false, None, None);
    handler.show_progress(42.0);
    handler.show_progress_detail(50.0, Duration::from_secs(10));
}

// Reached through the trait object the pipeline
// holds, so that a method that stopped overriding the trait's no-op default is caught.
#[test]
fn show_progress_detail_writes_the_status_line() {
    let (handler, recorder) = handler_with_sink();
    let ui: &dyn PipelineUi = &handler;
    ui.show_progress_detail(42.0, Duration::from_secs(90));
    assert_eq!(
        *recorder.statuses.lock().unwrap(),
        vec![format_encoding_status(42.0, Duration::from_secs(90))]
    );
    assert_eq!(
        recorder.statuses.lock().unwrap()[0],
        "Status: Transforming... 42% - about 1m30s left"
    );
}

// (E-15)
#[test]
fn get_encoder_returns_the_captured_name_and_nothing_without_a_video() {
    let (handler, _) = handler_with_sink();
    assert_eq!(handler.get_encoder(), "libx265");

    let no_video = GuiHandler::new(1, "libx265".into(), false, None, None);
    assert_eq!(no_video.get_encoder(), "");
    let no_stream = GuiHandler::new(
        1,
        "libx265".into(),
        false,
        Some(&VideoSpecs {
            file: "clip.mp4".into(),
            streams: Vec::new(),
        }),
        None,
    );
    assert_eq!(no_stream.get_encoder(), "");
}

// E-19, L-106: both directions, so that a constant
// answer either way reddens it.
#[test]
fn the_handler_hands_the_pipeline_the_values_it_was_built_with() {
    let handler = GuiHandler::new(4_000_000, String::new(), false, None, None);
    assert_eq!(handler.get_bitrate().expect("bitrate"), 4_000_000);

    for squeeze in [true, false] {
        let handler = GuiHandler::new(1, String::new(), squeeze, None, None);
        assert_eq!(
            handler.get_squeeze(),
            squeeze,
            "the checkbox's state does not reach the pipeline"
        );
    }
}

// ---- the Done signal ----

#[test]
fn done_wait_returns_false_on_timeout_and_true_once_released() {
    let done = Done::default();
    assert!(!done.wait(Duration::from_millis(10)));
    let waiter = {
        let done = done.clone();
        thread::spawn(move || done.wait(Duration::from_secs(10)))
    };
    done.release();
    done.release(); // idempotent
    assert!(waiter.join().expect("waiter"));
    assert!(done.wait(Duration::ZERO));
}
