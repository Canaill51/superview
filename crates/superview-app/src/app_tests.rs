//! The real window, wired to a simulated engine and desktop: what a click does from the
//! callback to the property it changes, with the threads in between. Neither ffmpeg nor a
//! picker is needed: [`FakeEngine`] and [`FakeHost`] answer, and a conversion is a closure.
//!
//! One `#[test]` only (the testing backend can be initialised once per process): every
//! scenario is a function called from [`run`], then the layout scenarios of `window_tests`.
//! Each scenario prints its name first, so a failure names where it was.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};
use superview_core::config::Config;
use superview_core::encode::cascade::EncodeSummary;
use superview_core::error::Error;
use superview_core::metrics::EncodingMetrics;
use superview_core::pipeline::{EncodeOutcome, PipelineUi};
use superview_core::probe::{EncoderProbe, EncoderProbeReport};
use superview_core::thumbnail::Thumbnail;
use superview_core::video::{VideoSpecs, VideoStream};

use crate::app::{App, EncodeJob, Engine, Host, INSTALL_URL, Setup};
use crate::dialogs::Pick;
use crate::format::resolve_start_settings;
use crate::prefs::Preferences;
use crate::state::VerificationRequest;
use crate::window_tests::{drive, is_shown, only};
use crate::{MainWindow, Popup};

const MISSING: &str = "cannot find ffmpeg/ffprobe on your system";
const SQUEEZE: &str = "Source already stretched to 16:9 (un-squeeze)";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A door a thread waits at until the test opens it.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    fn open(&self) {
        *lock(&self.0.0) = true;
        self.0.1.notify_all();
    }

    /// Waits for the door; 10 s at most, so that a broken test cannot hang the suite.
    fn wait(&self) {
        let guard = lock(&self.0.0);
        let _ = self
            .0
            .1
            .wait_timeout_while(guard, Duration::from_secs(10), |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// Waits until `cancel` is raised; 10 s at most.
fn wait_for_cancel(cancel: &AtomicBool) {
    let start = Instant::now();
    while !cancel.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(2));
    }
}

type ThumbnailFn = dyn Fn(&VideoSpecs) -> Result<Thumbnail, Error> + Send + Sync;

/// A picture of one grey level, so that a test can tell which one the card shows.
fn solid_thumbnail(grey: u8) -> Thumbnail {
    Thumbnail {
        width: 192,
        height: 144,
        rgba: [grey, grey, grey, 255].repeat(192 * 144),
    }
}

fn video_for(path: &str) -> VideoSpecs {
    VideoSpecs {
        file: path.to_string(),
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

type CheckVideoFn = dyn Fn(&str) -> Result<VideoSpecs, Error> + Send + Sync;

type EncodeFn =
    dyn Fn(&EncodeJob, &dyn PipelineUi, &AtomicBool) -> Result<EncodeOutcome, Error> + Send + Sync;

/// An engine that answers from closures.
struct FakeEngine {
    ffmpeg_present: AtomicBool,
    forgotten: AtomicU32,
    check_video: Box<CheckVideoFn>,
    check_video_threads: Mutex<Vec<ThreadId>>,
    verify: Box<dyn Fn(&VerificationRequest) -> String + Send + Sync>,
    probe: Box<dyn Fn() -> EncoderProbeReport + Send + Sync>,
    encode: Box<EncodeFn>,
    jobs: Mutex<Vec<EncodeJob>>,
    health: Box<dyn Fn(&Path) -> String + Send + Sync>,
    thumbnail: Box<ThumbnailFn>,
    /// When set, a thumbnail waits for the cancel flag, like ffmpeg waiting on a slow file.
    thumbnail_blocks: AtomicBool,
    thumbnails_ended: AtomicU32,
}

impl FakeEngine {
    fn new() -> Self {
        Self {
            ffmpeg_present: AtomicBool::new(true),
            forgotten: AtomicU32::new(0),
            check_video: Box::new(|path| {
                if path.to_lowercase().ends_with(".mp4") {
                    Ok(video_for(path))
                } else {
                    Err(Error::Plain(format!("not an MP4 file: {path}")))
                }
            }),
            check_video_threads: Mutex::new(Vec::new()),
            verify: Box::new(|_| "Hardware: verified".to_string()),
            probe: Box::new(EncoderProbeReport::default),
            encode: Box::new(|_, _, _| Ok(outcome("Hardware: the CPU did it"))),
            jobs: Mutex::new(Vec::new()),
            thumbnail: Box::new(|_| Ok(solid_thumbnail(200))),
            thumbnail_blocks: AtomicBool::new(false),
            thumbnails_ended: AtomicU32::new(0),
            health: Box::new(|temp_dir| {
                format!(
                    "=== Health ===\ntemp: {}\n✅ FFMPEG: ok\n❌ DISK: low\nnote: ✅ inside\n",
                    temp_dir.display()
                )
            }),
        }
    }

    fn thumbnailing(
        mut self,
        thumbnail: impl Fn(&VideoSpecs) -> Result<Thumbnail, Error> + Send + Sync + 'static,
    ) -> Self {
        self.thumbnail = Box::new(thumbnail);
        self
    }

    fn encoding(
        mut self,
        encode: impl Fn(&EncodeJob, &dyn PipelineUi, &AtomicBool) -> Result<EncodeOutcome, Error>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.encode = Box::new(encode);
        self
    }

    fn verifying(
        mut self,
        verify: impl Fn(&VerificationRequest) -> String + Send + Sync + 'static,
    ) -> Self {
        self.verify = Box::new(verify);
        self
    }

    fn probing(mut self, probe: impl Fn() -> EncoderProbeReport + Send + Sync + 'static) -> Self {
        self.probe = Box::new(probe);
        self
    }

    fn without_ffmpeg(self) -> Self {
        self.ffmpeg_present.store(false, Ordering::Release);
        self
    }
}

impl Engine for FakeEngine {
    fn check_ffmpeg(&self) -> Result<HashMap<String, String>, Error> {
        if self.ffmpeg_present.load(Ordering::Acquire) {
            Ok(HashMap::from([
                ("version".to_string(), "test".to_string()),
                ("accels".to_string(), String::new()),
                ("encoders".to_string(), "libx264,libx265".to_string()),
            ]))
        } else {
            Err(Error::Plain(MISSING.to_string()))
        }
    }

    fn forget_tools(&self) {
        self.forgotten.fetch_add(1, Ordering::SeqCst);
    }

    fn probe_hardware(&self, _: &HashMap<String, String>, _: &AtomicBool) -> EncoderProbeReport {
        (self.probe)()
    }

    fn check_video(&self, path: &str) -> Result<VideoSpecs, Error> {
        lock(&self.check_video_threads).push(std::thread::current().id());
        (self.check_video)(path)
    }

    fn thumbnail(&self, video: &VideoSpecs, cancel: &AtomicBool) -> Result<Thumbnail, Error> {
        if self.thumbnail_blocks.load(Ordering::SeqCst) {
            wait_for_cancel(cancel);
            // A little later than the flag, as ffmpeg's kill and the directory's removal are.
            std::thread::sleep(Duration::from_millis(100));
            self.thumbnails_ended.fetch_add(1, Ordering::SeqCst);
            return Err(Error::Cancelled);
        }
        (self.thumbnail)(video)
    }

    fn verify_hardware(&self, request: &VerificationRequest, _: &AtomicBool) -> String {
        (self.verify)(request)
    }

    fn encode(
        &self,
        job: &EncodeJob,
        ui: &dyn PipelineUi,
        cancel: &AtomicBool,
    ) -> Result<EncodeOutcome, Error> {
        lock(&self.jobs).push(job.clone());
        (self.encode)(job, ui, cancel)
    }

    fn health_report(&self, temp_dir: &Path, _: &AtomicBool) -> String {
        (self.health)(temp_dir)
    }
}

type Picker = Box<dyn Fn() -> Pick + Send + Sync>;

/// A cancel is `None`, a file is `Some`.
fn picked(answer: Option<PathBuf>) -> Pick {
    answer.map_or(Pick::Cancelled, Pick::Chosen)
}

/// A desktop whose pickers answer from closures.
struct FakeHost {
    pick_input: Picker,
    pick_output: Picker,
    links: Mutex<Vec<String>>,
    quits: AtomicU32,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            pick_input: Box::new(|| Pick::Cancelled),
            pick_output: Box::new(|| Pick::Cancelled),
            links: Mutex::new(Vec::new()),
            quits: AtomicU32::new(0),
        }
    }

    /// No picker can be opened: both ask for the path in the window instead.
    fn without_pickers(mut self) -> Self {
        self.pick_input = Box::new(|| Pick::NoPicker);
        self.pick_output = Box::new(|| Pick::NoPicker);
        self
    }

    /// The input picker answers these in turn (`None` is a cancel).
    fn picking_inputs(mut self, answers: Vec<Option<PathBuf>>) -> Self {
        let queue = Mutex::new(VecDeque::from(answers));
        self.pick_input = Box::new(move || picked(lock(&queue).pop_front().flatten()));
        self
    }

    fn picking_input_with(
        mut self,
        picker: impl Fn() -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        self.pick_input = Box::new(move || picked(picker()));
        self
    }

    fn picking_output_with(
        mut self,
        picker: impl Fn() -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        self.pick_output = Box::new(move || picked(picker()));
        self
    }
}

impl Host for FakeHost {
    fn pick_input(&self) -> Pick {
        (self.pick_input)()
    }

    fn pick_output(&self) -> Pick {
        (self.pick_output)()
    }

    fn open_link(&self, url: &str) {
        lock(&self.links).push(url.to_string());
    }

    fn quit(&self) {
        self.quits.fetch_add(1, Ordering::SeqCst);
    }
}

/// A window, wired.
struct Rig {
    window: MainWindow,
    app: App,
    engine: Arc<FakeEngine>,
    host: Arc<FakeHost>,
    dir: tempfile::TempDir,
}

fn rig(engine: FakeEngine, host: FakeHost) -> Rig {
    rig_with(engine, host, Preferences::default())
}

fn rig_with(engine: FakeEngine, host: FakeHost, prefs: Preferences) -> Rig {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = MainWindow::new().expect("window");
    window.show().expect("show");
    let engine = Arc::new(engine);
    let host = Arc::new(host);
    let app = App::wire(
        &window,
        Setup {
            cfg: Config::default(),
            engine: engine.clone(),
            host: host.clone(),
            prefs,
            prefs_path: Some(dir.path().join("preferences.json")),
            log_path: Some(dir.path().join("superview.log")),
            identity: "dev-test".into(),
            temp_dir: dir.path().to_path_buf(),
        },
    );
    app.set_shutdown_grace(Duration::from_secs(5));
    Rig {
        window,
        app,
        engine,
        host,
        dir,
    }
}

/// Runs the event loop until `cond` holds; fails after 5 s.
fn wait_until(what: &str, cond: impl Fn() -> bool + 'static) {
    if cond() {
        return;
    }
    let poll = slint::Timer::default();
    let check = Arc::new(cond);
    let for_timer = Arc::clone(&check);
    poll.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(5),
        move || {
            if for_timer() {
                slint::quit_event_loop().expect("quit");
            }
        },
    );
    let give_up = slint::Timer::default();
    give_up.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
        slint::quit_event_loop().expect("quit");
    });
    slint::run_event_loop_until_quit().expect("event loop");
    assert!(check(), "timed out waiting for {what}");
}

/// Runs the event loop for a fixed time, to let late events land.
fn pump(duration: Duration) {
    let end = slint::Timer::default();
    end.start(slint::TimerMode::SingleShot, duration, || {
        slint::quit_event_loop().expect("quit");
    });
    slint::run_event_loop_until_quit().expect("event loop");
}

impl Rig {
    fn status(&self) -> String {
        self.window.get_status().to_string()
    }

    fn popup(&self) -> Popup {
        self.window.get_popup()
    }

    fn click(&self, label: &str) {
        only(&self.window, label).invoke_accessible_default_action();
    }

    /// Clicks the enabled one of several elements with the same label: "Cancel" is both the
    /// button's (greyed out behind a popup) and the text field's.
    fn click_enabled(&self, label: &str) {
        let mut found: Vec<_> = ElementHandle::find_by_accessible_label(&self.window, label)
            .filter(|e| e.accessible_enabled() == Some(true))
            .collect();
        assert_eq!(found.len(), 1, "expected one enabled {label:?}");
        found.remove(0).invoke_accessible_default_action();
    }

    /// Types into the focused element, key by key, as a keyboard would.
    fn type_keys(&self, text: &str) {
        for c in text.chars() {
            let key: slint::SharedString = c.into();
            self.window
                .window()
                .dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
            self.window
                .window()
                .dispatch_event(WindowEvent::KeyReleased { text: key });
        }
    }

    fn enabled(&self, label: &str) -> bool {
        only(&self.window, label).accessible_enabled() == Some(true)
    }

    fn wait(&self, what: &str, cond: impl Fn(&MainWindow) -> bool + 'static) {
        let weak = self.window.as_weak();
        wait_until(what, move || weak.upgrade().is_some_and(|w| cond(&w)));
    }

    /// The window once the startup probe has landed: the codec list is filled.
    fn ready(&self) {
        self.wait("the startup probe", |w| {
            w.get_encoder_options().row_count() > 1
        });
    }

    fn pick_input_and_wait(&self, name: &str) {
        let expected = name.to_string();
        self.click("Choose input file");
        self.wait("the input", move |w| w.get_input_name() == expected);
    }

    fn pick_output_and_wait(&self, name: &str) {
        let expected = name.to_string();
        self.click("Choose output file");
        self.wait("the output", move |w| w.get_output_name() == expected);
    }

    fn output_path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Input and output chosen, ready to start.
    fn loaded(engine: FakeEngine, output: &str) -> Self {
        Self::loaded_with(engine, output, true)
    }

    /// The same, without waiting for the startup probe: the engine decides when it answers.
    fn loaded_before_probe(engine: FakeEngine, output: &str) -> Self {
        Self::loaded_with(engine, output, false)
    }

    fn loaded_with(engine: FakeEngine, output: &str, wait_for_probe: bool) -> Self {
        let dir_output = Arc::new(Mutex::new(PathBuf::new()));
        let shared = Arc::clone(&dir_output);
        let host = FakeHost::new()
            .picking_input_with(|| Some(PathBuf::from("/videos/trip/clip.mp4")))
            .picking_output_with(move || Some(lock(&shared).clone()));
        let rig = rig(engine, host);
        *lock(&dir_output) = rig.output_path(output);
        if wait_for_probe {
            rig.ready();
        }
        rig.pick_input_and_wait("clip.mp4");
        rig.pick_output_and_wait(output);
        rig
    }
}

pub fn run() {
    let scenarios: [(&str, fn()); 45] = [
        (
            "verify thread fails",
            a_verification_thread_that_cannot_start_does_not_freeze,
        ),
        (
            "restored codec",
            the_saved_codec_survives_the_list_and_reaches_the_conversion,
        ),
        (
            "stale quit",
            a_quit_question_that_waited_is_dropped_when_nothing_runs,
        ),
        ("startup", startup_fills_the_codec_list),
        (
            "input chosen",
            an_input_is_chosen_and_read_off_the_ui_thread,
        ),
        (
            "input cancelled or refused",
            an_input_is_cancelled_or_refused,
        ),
        ("input picker", the_picker_is_held_and_released),
        ("input not utf-8", an_input_that_is_not_utf8_is_refused),
        ("output", an_output_is_chosen),
        ("start gates", start_asks_for_what_is_missing),
        ("conversion", a_conversion_runs_and_then_another),
        ("cancel", cancel_stops_a_conversion_at_once),
        ("close", closing_during_a_conversion_asks_and_waits),
        ("close idle", closing_when_idle_just_closes),
        ("panic", a_panicking_encoder_does_not_freeze_the_window),
        (
            "failure",
            a_failed_conversion_shows_the_error_and_empties_the_hardware_line,
        ),
        ("signal", a_system_stop_quits_without_a_dialog),
        ("prerequisite", ffmpeg_missing_at_startup_offers_retry),
        (
            "prerequisite after run",
            ffmpeg_missing_after_a_run_offers_no_retry,
        ),
        ("diagnostic", the_diagnostic_reports_and_restores_the_status),
        ("title", the_title_bar_carries_the_build_identity),
        (
            "diagnostic queue",
            a_dialog_queued_behind_the_report_leaves_it_alone,
        ),
        (
            "verification",
            a_slow_verification_cannot_overwrite_a_newer_line,
        ),
        ("popups", popups_wait_their_turn),
        (
            "real error",
            the_real_missing_ffmpeg_error_reaches_the_popup,
        ),
        ("preferences", the_quality_choice_reaches_the_conversion),
        (
            "text field input",
            the_text_field_stands_in_for_the_input_picker,
        ),
        (
            "text field output",
            the_text_field_stands_in_for_the_output_picker,
        ),
        ("text field cancel", a_cancelled_text_field_changes_nothing),
        ("text field queue", the_text_field_keeps_the_popup_queue),
        (
            "text field twice",
            a_second_text_field_for_the_other_file_starts_empty,
        ),
        (
            "text field no thread",
            a_video_check_that_cannot_start_reports_and_frees_the_button,
        ),
        (
            "probe during a run",
            a_startup_probe_that_lands_during_a_conversion_waits_for_its_end,
        ),
        (
            "diagnostic and start",
            the_diagnostic_and_a_conversion_do_not_overlap,
        ),
        (
            "overwrite frozen",
            the_overwrite_question_is_about_the_files_it_was_asked_on,
        ),
        ("thumbnail", the_input_gets_its_picture_and_its_specs),
        (
            "thumbnail failure",
            a_thumbnail_that_fails_leaves_the_card_empty_and_quiet,
        ),
        (
            "thumbnail replaced by a failure",
            the_card_is_emptied_when_the_next_picture_fails,
        ),
        (
            "thumbnail shutdown",
            closing_waits_for_a_thumbnail_in_flight,
        ),
        (
            "thumbnail stale",
            a_slow_thumbnail_cannot_land_on_the_next_input,
        ),
        (
            "late progress",
            a_late_progress_does_not_overwrite_cancelling,
        ),
        (
            "quit question stale",
            the_quit_question_goes_when_the_conversion_ends,
        ),
        ("quality list", the_quality_menu_is_the_rust_list),
        ("modal keys", modals_answer_to_escape_and_enter),
        (
            "destructive keys",
            destructive_questions_ignore_enter_and_escape_means_no,
        ),
    ];
    for (name, scenario) in scenarios {
        println!("scenario: {name}");
        scenario();
    }
    println!("scenario: layout");
    crate::window_tests::run();
}

// One test only: see the header.
#[test]
fn window_scenarios() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    run();
}

/// The window is titled "Superview " + the build identity; a user reporting a bug reads the
/// version off the title bar.
fn the_title_bar_carries_the_build_identity() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    assert_eq!(r.window.get_window_title(), "Superview dev-test");
}

fn startup_fills_the_codec_list() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    assert_eq!(r.status(), "Status: Ready");
    r.ready();
    let options: Vec<String> = r
        .window
        .get_encoder_options()
        .iter()
        .map(|o| o.to_string())
        .collect();
    assert_eq!(
        options,
        [
            "Use same video codec as input file",
            "libx264 encoder",
            "libx265 encoder"
        ]
    );
    assert_eq!(r.window.get_encoder_selected(), options[0]);
    assert_eq!(r.popup(), Popup::None);
    assert!(r.window.get_open_enabled());
    assert!(!r.window.get_start_enabled());
}

fn an_input_is_chosen_and_read_off_the_ui_thread() {
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/trip/clip.mp4".into())]);
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    assert_eq!(r.status(), "Status: Input loaded");
    assert_eq!(r.window.get_bar_state(), crate::BarState::Idle);
    assert!(r.window.get_open_enabled(), "the button must come back");
    let threads = lock(&r.engine.check_video_threads).clone();
    assert_eq!(threads.len(), 1, "ffprobe ran {} times", threads.len());
    assert_ne!(
        threads[0],
        std::thread::current().id(),
        "ffprobe ran on the UI thread"
    );
}

/// The picture and the specs line of the chosen input reach the card, off the UI thread.
fn the_input_gets_its_picture_and_its_specs() {
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/trip/clip.mp4".into())]);
    let r = rig(FakeEngine::new(), host);
    r.ready();
    assert!(!r.window.get_has_thumbnail());
    assert_eq!(r.window.get_input_specs(), "");
    r.pick_input_and_wait("clip.mp4");
    r.wait("the picture", |w| w.get_has_thumbnail());
    assert_eq!(r.window.get_input_specs(), "h264 · 1440×1080 · 10s");
    let size = r.window.get_thumbnail().size();
    assert_eq!((size.width, size.height), (192, 144));
}

/// No picture is not an error: no popup, the status says the input is loaded, the card is empty.
fn a_thumbnail_that_fails_leaves_the_card_empty_and_quiet() {
    let asked = Arc::new(AtomicU32::new(0));
    let for_engine = Arc::clone(&asked);
    let engine = FakeEngine::new().thumbnailing(move |_| {
        for_engine.fetch_add(1, Ordering::SeqCst);
        Err(Error::Plain("no frame".into()))
    });
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    let seen = Arc::clone(&asked);
    r.wait("the decoder to be asked", move |_| {
        seen.load(Ordering::SeqCst) == 1
    });
    pump(Duration::from_millis(150));
    assert!(!r.window.get_has_thumbnail());
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(r.status(), "Status: Input loaded");
}

/// A file whose picture fails must not keep the previous file's picture under its name.
fn the_card_is_emptied_when_the_next_picture_fails() {
    let engine = FakeEngine::new().thumbnailing(|video| {
        if video.file.ends_with("first.mp4") {
            Ok(solid_thumbnail(200))
        } else {
            Err(Error::Plain("no frame".into()))
        }
    });
    let host = FakeHost::new().picking_inputs(vec![
        Some("/videos/first.mp4".into()),
        Some("/videos/second.mp4".into()),
    ]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("first.mp4");
    r.wait("the first picture", |w| w.get_has_thumbnail());
    r.pick_input_and_wait("second.mp4");
    pump(Duration::from_millis(150));
    assert!(
        !r.window.get_has_thumbnail(),
        "the previous file's picture stayed under the new name"
    );
}

/// Leaving while a picture is being decoded waits for it to unwind (its ffmpeg and its
/// directory), instead of ending the process over them.
fn closing_waits_for_a_thumbnail_in_flight() {
    let engine = FakeEngine::new();
    engine.thumbnail_blocks.store(true, Ordering::SeqCst);
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    assert_eq!(r.engine.thumbnails_ended.load(Ordering::SeqCst), 0);
    r.app.shutdown("test");
    assert_eq!(
        r.engine.thumbnails_ended.load(Ordering::SeqCst),
        1,
        "the shutdown returned before the thumbnail thread had unwound"
    );
}

/// The first file's picture arrives after the second was chosen: it is dropped, and the card is
/// emptied while the second one is awaited.
fn a_slow_thumbnail_cannot_land_on_the_next_input() {
    let slow = Gate::default();
    let for_slow = slow.clone();
    let engine = FakeEngine::new().thumbnailing(move |video| {
        if video.file.ends_with("first.mp4") {
            for_slow.wait();
            Ok(solid_thumbnail(10))
        } else {
            Ok(solid_thumbnail(240))
        }
    });
    let host = FakeHost::new().picking_inputs(vec![
        Some("/videos/first.mp4".into()),
        Some("/videos/second.mp4".into()),
    ]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("first.mp4"); // its picture waits at the gate
    r.pick_input_and_wait("second.mp4");
    r.wait("the second picture", |w| w.get_has_thumbnail());
    let grey = |r: &Rig| {
        let image = r.window.get_thumbnail();
        let pixels = image.to_rgba8().expect("pixels");
        pixels.as_slice()[0].r
    };
    assert_eq!(grey(&r), 240, "the second picture is not on the card");
    slow.open(); // the first one answers now, late
    pump(Duration::from_millis(150));
    assert_eq!(grey(&r), 240, "a late picture replaced the newer one");
}

fn an_input_is_cancelled_or_refused() {
    // Good, cancelled, refused: the refused file does not replace the good one, and says why.
    let host = FakeHost::new().picking_inputs(vec![
        Some("/videos/trip/clip.mp4".into()),
        None,
        Some("/videos/other.mkv".into()),
    ]);
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");

    r.click("Choose input file"); // cancelled
    r.wait("the button back", |w| w.get_open_enabled());
    assert_eq!(r.window.get_input_name(), "clip.mp4");
    assert_eq!(r.status(), "Status: Input loaded");
    assert_eq!(r.popup(), Popup::None, "a cancel is not an error");

    r.click("Choose input file"); // refused
    r.wait("the error", |w| w.get_popup() == Popup::Error);
    assert_eq!(r.status(), "Status: Invalid input");
    assert_eq!(
        r.window.get_error_message(),
        "not an MP4 file: /videos/other.mkv"
    );
    assert_eq!(
        r.window.get_input_name(),
        "clip.mp4",
        "a refused file replaced the input"
    );
    assert!(r.window.get_open_enabled());
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);

    // A picker that panics counts as a cancel: the button comes back.
    let host = FakeHost::new().picking_input_with(|| panic!("picker blew up"));
    let r = rig(FakeEngine::new(), host);
    r.click("Choose input file");
    r.wait("the button back", |w| w.get_open_enabled());
    assert_eq!(r.status(), "Status: Ready");
    assert_eq!(r.popup(), Popup::None);
}

fn the_picker_is_held_and_released() {
    let gate = Gate::default();
    let opened = Arc::new(AtomicU32::new(0));
    let (for_picker, counted) = (gate.clone(), Arc::clone(&opened));
    let host = FakeHost::new().picking_input_with(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        for_picker.wait();
        None
    });
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.click("Choose input file");
    assert!(
        !r.enabled("Choose input file"),
        "the button stays usable while its picker is open"
    );
    // Nothing else is blocked by the picker.
    assert!(r.enabled("Choose output file"));
    // A second click reaches nothing.
    r.window.invoke_choose_input();
    gate.open();
    r.wait("the button back", |w| w.get_open_enabled());
    assert_eq!(opened.load(Ordering::SeqCst), 1, "a second picker opened");
}

#[cfg(unix)]
fn an_input_that_is_not_utf8_is_refused() {
    use std::os::unix::ffi::OsStrExt;
    let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/videos/\xff.mp4"));
    let host = FakeHost::new().picking_inputs(vec![Some(bad)]);
    let r = rig(FakeEngine::new(), host);
    r.click("Choose input file");
    r.wait("the error", |w| w.get_popup() == Popup::Error);
    assert!(
        r.window
            .get_error_message()
            .contains("not valid UTF-8 and cannot be used"),
        "{}",
        r.window.get_error_message()
    );
    assert_eq!(r.status(), "Status: Invalid input");
    assert_eq!(r.window.get_input_name(), "No file selected");
    assert!(
        lock(&r.engine.check_video_threads).is_empty(),
        "ffprobe was given a path that is not the one chosen"
    );
}

#[cfg(not(unix))]
fn an_input_that_is_not_utf8_is_refused() {}

fn an_output_is_chosen() {
    let host = FakeHost::new().picking_output_with(|| Some(PathBuf::from("/tmp/videos/result")));
    let r = rig(FakeEngine::new(), host);
    r.click("Choose output file");
    r.wait("the output", |w| w.get_output_name() == "result.mp4");
    assert_eq!(r.status(), "Status: Output selected");
    assert!(r.window.get_output_enabled());

    let host = FakeHost::new().picking_output_with(|| panic!("picker blew up"));
    let r = rig(FakeEngine::new(), host);
    r.click("Choose output file");
    r.wait("the button back", |w| w.get_output_enabled());
    assert_eq!(r.window.get_output_name(), "No file selected");
}

fn start_asks_for_what_is_missing() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    r.window.invoke_start();
    assert_eq!(r.popup(), Popup::NoInput);
    assert!(is_shown(&r.window, "Please open an input video first."));
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);

    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    r.window.invoke_start();
    assert_eq!(r.popup(), Popup::NoOutput);
    assert!(lock(&r.engine.jobs).is_empty(), "a conversion started");
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);
}

fn a_conversion_runs_and_then_another() {
    let gate = Gate::default();
    let for_encode = gate.clone();
    let engine = FakeEngine::new().encoding(move |job, ui, _| {
        assert_eq!(
            ui.get_encoder(),
            "",
            "the first entry means the source's codec"
        );
        assert!(!ui.get_squeeze());
        let (expected, _) = resolve_start_settings(&Config::default(), "Balanced", 5_000_000);
        assert_eq!(ui.get_bitrate().expect("bitrate"), expected);
        ui.show_progress(40.0);
        ui.show_progress_detail(40.0, Duration::from_secs(30));
        for_encode.wait();
        // A real conversion leaves its output behind.
        std::fs::write(&job.output, b"video").expect("write the output");
        Ok(outcome("Hardware: the CPU did it"))
    });
    let r = Rig::loaded(engine, "result.mp4");
    assert!(r.window.get_start_enabled());
    r.click("Start transformation");

    // While it runs: progress and time left arrive, the window is locked, Cancel is live.
    r.wait("the progress", |w| {
        w.get_status() == "Status: Transforming... 40% - about 30s left"
    });
    assert!((r.window.get_progress() - 0.4).abs() < 0.001);
    assert!(r.window.get_cancel_enabled());
    assert!(!r.window.get_start_enabled());
    assert_eq!(r.window.get_bar_state(), crate::BarState::Running);
    for locked in ["Choose input file", "Choose output file", "Diagnostic"] {
        assert!(
            !r.enabled(locked),
            "{locked:?} is usable during a conversion"
        );
    }
    assert!(r.enabled("Quit"), "Quit must stay usable");
    gate.open();

    r.wait("the Done dialog", |w| w.get_popup() == Popup::Done);
    let output = r.output_path("result.mp4");
    assert_eq!(
        r.window.get_done_message(),
        format!("Transform complete. Output file:\n{}", output.display())
    );
    assert_eq!(r.status(), "Status: Completed");
    assert!((r.window.get_progress() - 1.0).abs() < 0.001);
    assert_eq!(r.window.get_bar_state(), crate::BarState::Done);
    assert_eq!(r.window.get_hardware(), "Hardware: the CPU did it");
    assert!(r.window.get_results().starts_with("Results:"));
    assert!(!r.window.get_cancel_enabled());
    assert!(
        r.window.get_start_enabled(),
        "a second conversion must be possible"
    );
    assert!(r.window.get_open_enabled());
    {
        let jobs = lock(&r.engine.jobs);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].output, output);
        assert_eq!(jobs[0].input, PathBuf::from("/videos/trip/clip.mp4"));
        assert_eq!(jobs[0].cfg.video_preset, "medium");
    }
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);

    // The output exists now: Start asks before overwriting. No does nothing...
    r.click("Start transformation");
    assert_eq!(r.popup(), Popup::Overwrite);
    r.click("No");
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(lock(&r.engine.jobs).len(), 1, "No started a conversion");
    // ... and Yes runs the second conversion, to its Done dialog.
    r.click("Start transformation");
    r.click("Yes");
    r.wait("the second Done dialog", |w| w.get_popup() == Popup::Done);
    assert_eq!(lock(&r.engine.jobs).len(), 2);
    assert_eq!(r.status(), "Status: Completed");
    r.click("OK");
}

fn cancel_stops_a_conversion_at_once() {
    let started = Gate::default();
    let for_encode = started.clone();
    let engine = FakeEngine::new().encoding(move |_, _, cancel| {
        for_encode.open();
        wait_for_cancel(cancel);
        Err(Error::Cancelled)
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    // Cancel at once, before the first frame.
    assert!(r.window.get_cancel_enabled());
    r.click("Cancel");
    assert_eq!(r.status(), "Status: Cancelling...");
    r.wait("the cancellation", |w| {
        w.get_status() == "Status: Cancelled"
    });
    assert_eq!(r.window.get_results(), "Results: last run cancelled");
    assert_eq!(r.window.get_bar_state(), crate::BarState::Stopped);
    assert_eq!(r.popup(), Popup::None, "a cancel is not an error");
    assert_eq!(
        r.window.get_hardware(),
        "",
        "the line describes a plan nobody ran"
    );
    assert!(r.window.get_start_enabled());
    assert!(!r.window.get_cancel_enabled());
    assert!(r.window.get_open_enabled());
}

fn closing_during_a_conversion_asks_and_waits() {
    let started = Gate::default();
    let unwound = Arc::new(AtomicBool::new(false));
    let (for_encode, flag) = (started.clone(), Arc::clone(&unwound));
    let engine = FakeEngine::new().encoding(move |_, _, cancel| {
        for_encode.open();
        wait_for_cancel(cancel);
        // Killing ffmpeg and removing the partial file take time: the exit must wait for it.
        std::thread::sleep(Duration::from_millis(150));
        flag.store(true, Ordering::Release);
        Err(Error::Cancelled)
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    started.wait();

    // The system's close request is held back and asks.
    r.window
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert_eq!(r.popup(), Popup::Quit);
    assert!(
        r.window.window().is_visible(),
        "the window closed under a conversion"
    );
    assert!(is_shown(
        &r.window,
        "An encoding is in progress. Do you want to cancel and quit?"
    ));
    // No: the user withdrew, the conversion goes on.
    r.click("No");
    assert_eq!(r.popup(), Popup::None);
    assert!(!unwound.load(Ordering::Acquire));
    assert!(r.window.get_cancel_enabled());
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 0);

    // The Quit button asks too, and Yes cancels, waits for the thread, then leaves.
    r.click("Quit");
    assert_eq!(r.popup(), Popup::Quit);
    r.click("Yes");
    assert!(
        unwound.load(Ordering::Acquire),
        "the window left before the conversion had unwound"
    );
    assert!(!r.window.window().is_visible());
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 1);
    // The completion that follows touches nothing: no dialog in a window on its way out.
    pump(Duration::from_millis(100));
    assert_eq!(r.popup(), Popup::None);
    assert!(r.app.shutdown("again"), "a second exit waits again");
}

fn closing_when_idle_just_closes() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    r.window
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert_eq!(r.popup(), Popup::None);
    assert!(!r.window.window().is_visible());
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 1);

    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    r.click("Quit");
    assert_eq!(r.popup(), Popup::None, "nothing to ask when nothing runs");
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 1);
}

fn a_panicking_encoder_does_not_freeze_the_window() {
    let engine = FakeEngine::new().encoding(|_, _, _| panic!("encoder blew up"));
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    r.wait("the error", |w| w.get_popup() == Popup::Error);
    assert_eq!(
        r.window.get_error_message(),
        "the conversion stopped unexpectedly: encoder blew up"
    );
    assert_eq!(r.status(), "Status: Failed");
    assert_eq!(r.window.get_results(), "Results: last run failed");
    assert_eq!(r.window.get_bar_state(), crate::BarState::Stopped);
    assert!(!r.window.get_cancel_enabled(), "the window is still locked");
    assert!(r.window.get_open_enabled());
    assert!(r.window.get_start_enabled());
    assert_eq!(r.window.get_hardware(), "");
    // Nothing is left to wait for: an exit does not spend its grace period.
    let before = Instant::now();
    assert!(r.app.shutdown("test"));
    assert!(before.elapsed() < Duration::from_secs(1));
}

fn a_failed_conversion_shows_the_error_and_empties_the_hardware_line() {
    let engine = FakeEngine::new().encoding(|_, _, _| Err(Error::Plain("disk exploded".into())));
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    r.wait("the error", |w| w.get_popup() == Popup::Error);
    assert_eq!(r.window.get_error_message(), "disk exploded");
    assert_eq!(r.status(), "Status: Failed");
    assert_eq!(r.window.get_hardware(), "");
    assert!(r.window.get_start_enabled(), "a new try must be possible");
}

fn a_system_stop_quits_without_a_dialog() {
    let engine =
        FakeEngine::new().encoding(|_, _, _| Err(Error::StoppedBySignal("terminated".into())));
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    let host = Arc::clone(&r.host);
    wait_until("the exit", move || host.quits.load(Ordering::SeqCst) == 1);
    pump(Duration::from_millis(50));
    assert_eq!(
        r.popup(),
        Popup::None,
        "a system stop is an order, not an error"
    );
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 1);
}

fn ffmpeg_missing_at_startup_offers_retry() {
    let r = rig(FakeEngine::new().without_ffmpeg(), FakeHost::new());
    assert_eq!(r.popup(), Popup::Prerequisite);
    assert!(r.window.get_prerequisite_retry());
    assert_eq!(r.window.get_install_url(), INSTALL_URL);
    assert_eq!(r.status(), "Status: ffmpeg unavailable");
    assert_eq!(r.window.get_hardware(), "Hardware: ffmpeg unavailable");
    assert!(!r.window.get_open_enabled());
    assert!(!r.window.get_output_enabled());
    assert!(
        r.window.get_diagnostic_enabled(),
        "Diagnostic says what is wrong"
    );
    assert!(!r.window.get_start_enabled());

    // The link opens the program's own address, and nothing else.
    r.window
        .invoke_open_install_link("https://example.com/other".into());
    assert!(
        lock(&r.host.links).is_empty(),
        "an address the program did not set was opened"
    );
    r.window.invoke_open_install_link(INSTALL_URL.into());
    assert_eq!(*lock(&r.host.links), [INSTALL_URL]);

    // Retry with ffmpeg still missing: the lookup is forgotten, the dialog comes back.
    r.click("Retry");
    assert_eq!(r.engine.forgotten.load(Ordering::SeqCst), 1);
    assert_eq!(r.popup(), Popup::Prerequisite);
    assert!(!r.window.get_open_enabled());

    // ffmpeg gets installed: the next Retry unlocks everything.
    r.engine.ffmpeg_present.store(true, Ordering::Release);
    r.click("Retry");
    assert_eq!(r.engine.forgotten.load(Ordering::SeqCst), 2);
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(r.status(), "Status: Ready");
    assert!(r.window.get_open_enabled());
    assert!(r.window.get_output_enabled());
    r.ready();
    assert_ne!(r.window.get_hardware(), "Hardware: ffmpeg unavailable");

    // Close leaves the window as it is.
    let r = rig(FakeEngine::new().without_ffmpeg(), FakeHost::new());
    r.click("Close");
    assert_eq!(r.popup(), Popup::None);
    assert!(!r.window.get_open_enabled());
}

fn ffmpeg_missing_after_a_run_offers_no_retry() {
    let engine = FakeEngine::new().encoding(|_, _, _| Err(Error::Plain(MISSING.into())));
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    r.wait("the prerequisite dialog", |w| {
        w.get_popup() == Popup::Prerequisite
    });
    assert!(
        !r.window.get_prerequisite_retry(),
        "nothing to retry after a run"
    );
    assert!(is_shown(&r.window, "OK"));
    assert!(!is_shown(&r.window, "Retry"));
    assert_eq!(r.status(), "Status: Failed");
}

fn the_diagnostic_reports_and_restores_the_status() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    r.click("Diagnostic");
    assert_eq!(r.status(), "Status: Running diagnostic...");
    r.wait("the report", |w| w.get_popup() == Popup::Diagnostic);
    let report = r.window.get_diagnostic_report().to_string();
    assert!(
        report.starts_with("Superview build: dev-test\n\n=== Health ===\n"),
        "{report}"
    );
    assert!(
        report.contains(&format!("temp: {}", r.dir.path().display())),
        "{report}"
    );
    assert!(
        report.ends_with(&format!(
            "\nLog file: {}\n",
            r.dir.path().join("superview.log").display()
        )),
        "{report}"
    );
    assert!(
        report.contains("[OK] FFMPEG: ok\n[FAIL] DISK: low\n"),
        "{report}"
    );
    assert!(!report.contains('✅') && !report.contains('❌'), "{report}");
    // The same report, line by line, for the list with drawn icons.
    let rows: Vec<crate::DiagnosticRow> = r.window.get_diagnostic_rows().iter().collect();
    let verdict = |text: &str| {
        let row = rows
            .iter()
            .find(|row| row.text == text)
            .unwrap_or_else(|| panic!("no row {text:?} in {rows:?}"));
        (row.kind, row.ok)
    };
    assert_eq!(verdict("FFMPEG: ok"), (crate::ReportRowKind::Check, true));
    assert_eq!(verdict("DISK: low"), (crate::ReportRowKind::Check, false));
    // An emoji inside a line, not in the mark's place, is made plain text too.
    assert_eq!(verdict("note: [OK] inside").0, crate::ReportRowKind::Plain);
    assert!(
        rows.iter().all(|row| !row.text.contains(['✅', '❌'])),
        "an emoji reached the list"
    );
    assert!(
        !r.window.get_diagnostic_as_text(),
        "the list is the default view"
    );
    assert_eq!(r.status(), "Status: Ready", "the status is not restored");
    // Left on the text view, a closed report does not make the next one open on the text.
    r.click("Show as text");
    assert!(r.window.get_diagnostic_as_text());
    r.click("Close");
    assert_eq!(r.popup(), Popup::None);
    r.click("Diagnostic");
    r.wait("the second report", |w| w.get_popup() == Popup::Diagnostic);
    assert!(
        !r.window.get_diagnostic_as_text(),
        "a new report opened on the view the last one was left on"
    );
    r.click("Close");
    assert_eq!(r.popup(), Popup::None);

    // Something else wrote the status meanwhile: it keeps it.
    let gate = Gate::default();
    let for_health = gate.clone();
    let mut engine = FakeEngine::new();
    engine.health = Box::new(move |_| {
        for_health.wait();
        "late report\n".to_string()
    });
    let host = FakeHost::new().picking_output_with(|| Some(PathBuf::from("/tmp/x/out.mp4")));
    let r = rig(engine, host);
    r.ready();
    r.click("Diagnostic");
    r.pick_output_and_wait("out.mp4");
    assert_eq!(r.status(), "Status: Output selected");
    gate.open();
    r.wait("the report", |w| w.get_popup() == Popup::Diagnostic);
    assert_eq!(
        r.status(),
        "Status: Output selected",
        "the diagnostic overwrote a newer status"
    );
}

fn a_slow_verification_cannot_overwrite_a_newer_line() {
    let slow = Gate::default();
    let for_verify = slow.clone();
    let engine = FakeEngine::new().verifying(move |request| {
        if !request.squeeze {
            for_verify.wait();
        }
        format!("Hardware: verified squeeze={}", request.squeeze)
    });
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("clip.mp4"); // verification 1 (squeeze off) waits at the gate
    let checkbox = only(&r.window, SQUEEZE);
    drive(async move { checkbox.single_click(PointerEventButton::Left).await });
    r.wait("the second verification", |w| {
        w.get_hardware() == "Hardware: verified squeeze=true"
    });
    slow.open(); // the first one answers now, late
    pump(Duration::from_millis(150));
    assert_eq!(
        r.window.get_hardware(),
        "Hardware: verified squeeze=true",
        "a late answer replaced a newer line"
    );

    // A verification that panics gives a neutral text, not a stale plan.
    let engine = FakeEngine::new().verifying(|_| panic!("probe blew up"));
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(engine, host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    r.wait("the neutral line", |w| {
        w.get_hardware() == "Hardware: could not verify the encoder on this machine"
    });
}

fn popups_wait_their_turn() {
    let gate = Gate::default();
    let started = Gate::default();
    let (for_encode, for_start) = (gate.clone(), started.clone());
    let engine = FakeEngine::new().encoding(move |_, _, _| {
        for_start.open();
        for_encode.wait();
        Ok(outcome("Hardware: the CPU did it"))
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    started.wait();
    r.window
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert_eq!(r.popup(), Popup::Quit);
    // The conversion ends while the question is open: the question is stale (F-24), and the
    // Done dialog takes its place.
    gate.open();
    r.wait("the completion", |w| w.get_status() == "Status: Completed");
    assert_eq!(
        r.popup(),
        Popup::Done,
        "the stale question stayed on screen"
    );
    assert!(
        r.window
            .get_done_message()
            .starts_with("Transform complete. Output file:\n")
    );
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);
}

#[cfg(unix)]
fn the_real_missing_ffmpeg_error_reaches_the_popup() {
    use std::sync::atomic::AtomicI32;
    use superview_core::probe::Prober;
    use superview_core::tools::ToolResolver;

    use crate::engine::SystemEngine;

    // The tools live for the rest of the process, as in the application; a test may leak them.
    let resolver: &'static ToolResolver = Box::leak(Box::new(ToolResolver::new(|_| None, None)));
    let prober: &'static Prober<'static> = Box::leak(Box::new(Prober::new(resolver)));
    let signal: &'static AtomicI32 = Box::leak(Box::new(AtomicI32::new(0)));
    let engine = SystemEngine::with_tools(Config::default(), resolver, prober, signal);

    let window = MainWindow::new().expect("window");
    window.show().expect("show");
    let _app = App::wire(
        &window,
        Setup {
            cfg: Config::default(),
            engine: Arc::new(engine),
            host: Arc::new(FakeHost::new()),
            prefs: Preferences::default(),
            prefs_path: None,
            log_path: None,
            identity: "dev-test".into(),
            temp_dir: std::env::temp_dir(),
        },
    );
    assert_eq!(
        window.get_popup(),
        Popup::Prerequisite,
        "{}",
        window.get_error_message()
    );
    assert!(is_shown(
        &window,
        "cannot find ffmpeg/ffprobe on your system"
    ));
    assert_eq!(window.get_status(), "Status: ffmpeg unavailable");
}

#[cfg(not(unix))]
fn the_real_missing_ffmpeg_error_reaches_the_popup() {}

fn the_quality_choice_reaches_the_conversion() {
    let engine = FakeEngine::new();
    let r = Rig::loaded(engine, "result.mp4");
    r.window.invoke_quality_changed("Fast".into());
    r.click("Start transformation");
    r.wait("the Done dialog", |w| w.get_popup() == Popup::Done);
    assert_eq!(lock(&r.engine.jobs)[0].cfg.video_preset, "fast");
    let saved = crate::prefs::load(&r.dir.path().join("preferences.json"));
    assert_eq!(saved.quality_profile, "Fast", "the choice was not saved");
    r.click("OK");
}

fn a_verification_thread_that_cannot_start_does_not_freeze() {
    lock(&crate::app::FAILING_SPAWNS).push("verify-hardware");
    let host = FakeHost::new().picking_inputs(vec![Some("/videos/clip.mp4".into())]);
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.click("Choose input file");
    r.wait("the neutral line", |w| {
        w.get_hardware() == "Hardware: could not verify the encoder on this machine"
    });
    lock(&crate::app::FAILING_SPAWNS).clear();
    assert_eq!(r.window.get_input_name(), "clip.mp4");
}

fn the_saved_codec_survives_the_list_and_reaches_the_conversion() {
    let seen = Arc::new(Mutex::new(String::new()));
    let for_encode = Arc::clone(&seen);
    let engine = FakeEngine::new().encoding(move |_, ui, _| {
        *lock(&for_encode) = ui.get_encoder();
        Ok(outcome("Hardware: the CPU did it"))
    });
    let dir_output = Arc::new(Mutex::new(PathBuf::new()));
    let shared = Arc::clone(&dir_output);
    let host = FakeHost::new()
        .picking_input_with(|| Some(PathBuf::from("/videos/clip.mp4")))
        .picking_output_with(move || Some(lock(&shared).clone()));
    let prefs = Preferences {
        encoder_selection: "libx265 encoder".into(),
        ..Preferences::default()
    };
    let r = rig_with(engine, host, prefs);
    *lock(&dir_output) = r.output_path("result.mp4");
    r.ready();
    pump(Duration::from_millis(100));
    assert_eq!(
        r.window.get_encoder_selected(),
        "libx265 encoder",
        "the list rebuild reset the saved codec"
    );
    r.pick_input_and_wait("clip.mp4");
    r.pick_output_and_wait("result.mp4");
    r.click("Start transformation");
    r.wait("the Done dialog", |w| w.get_popup() == Popup::Done);
    assert_eq!(*lock(&seen), "libx265");
    r.click("OK");
}

fn a_quit_question_that_waited_is_dropped_when_nothing_runs() {
    let encode_gate = Gate::default();
    let health_gate = Gate::default();
    let (for_encode, for_health) = (encode_gate.clone(), health_gate.clone());
    let mut engine = FakeEngine::new().encoding(move |_, _, _| {
        for_encode.wait();
        Ok(outcome("Hardware: the CPU did it"))
    });
    engine.health = Box::new(move |_| {
        for_health.wait();
        "report\n".to_string()
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Diagnostic");
    health_gate.open();
    r.wait("the report", |w| w.get_popup() == Popup::Diagnostic);
    r.click("Start transformation");
    // The cross asks, but the Diagnostic is on screen: the question waits.
    r.window
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert_eq!(r.popup(), Popup::Diagnostic);
    encode_gate.open();
    r.wait("the completion", |w| w.get_status() == "Status: Completed");
    r.click("Close");
    assert_eq!(
        r.popup(),
        Popup::Done,
        "a question about a conversion that has ended came back"
    );
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);
}

/// A dialog that waits behind the report asks the window to apply the popup again: the list is
/// not rebuilt (that would send it back to the top) and the view the user chose stays.
fn a_dialog_queued_behind_the_report_leaves_it_alone() {
    let encode_gate = Gate::default();
    let for_encode = encode_gate.clone();
    let engine = FakeEngine::new().encoding(move |_, _, _| {
        for_encode.wait();
        Ok(outcome("Hardware: the CPU did it"))
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Diagnostic");
    r.wait("the report", |w| w.get_popup() == Popup::Diagnostic);
    r.click("Start transformation");
    r.click("Show as text");
    // A marker that a rebuild would wipe.
    let marker = crate::DiagnosticRow {
        kind: crate::ReportRowKind::Plain,
        ok: false,
        text: "marker".into(),
    };
    r.window
        .set_diagnostic_rows(slint::ModelRc::new(slint::VecModel::from(vec![marker])));
    r.window
        .window()
        .dispatch_event(WindowEvent::CloseRequested);
    assert_eq!(r.popup(), Popup::Diagnostic);
    assert_eq!(
        r.window.get_diagnostic_rows().row_count(),
        1,
        "the list was rebuilt under the reader"
    );
    assert!(
        r.window.get_diagnostic_as_text(),
        "the view went back to the list"
    );
    encode_gate.open();
    r.wait("the completion", |w| w.get_status() == "Status: Completed");
    r.click("Close");
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);
}

/// A typed path made absolute on the platform the test runs on: Windows has no absolute path
/// without a drive letter, and the text field refuses a relative one. Forward slashes stay
/// (Windows takes them, `file_name` and `ensure_mp4_extension` split on them).
fn abs(path: &str) -> String {
    if cfg!(windows) {
        format!("C:{path}")
    } else {
        path.to_string()
    }
}

/// The window with no picker to open and the text field up for `button`.
fn rig_asking_for_a_path(button: &str, engine: FakeEngine) -> Rig {
    let r = rig(engine, FakeHost::new().without_pickers());
    r.ready();
    r.click(button);
    r.wait("the text field", |w| w.get_popup() == Popup::PathEntry);
    r
}

fn the_text_field_stands_in_for_the_input_picker() {
    let r = rig_asking_for_a_path("Choose input file", FakeEngine::new());
    assert!(is_shown(&r.window, "Select input video"));
    assert!(!r.window.get_path_entry_output());
    assert!(
        !r.enabled("Choose input file"),
        "the button is held while the field is open"
    );
    assert!(lock(&r.engine.check_video_threads).is_empty());

    // Empty, then relative: the field stays open and says why; nothing was read.
    r.click("OK");
    assert_eq!(r.popup(), Popup::PathEntry);
    assert_eq!(
        r.window.get_path_entry_error(),
        "Please type the path of a file."
    );
    r.window.set_path_entry_text("clip.mp4".into());
    r.click("OK");
    assert_eq!(r.popup(), Popup::PathEntry);
    assert!(
        r.window.get_path_entry_error().contains("full path"),
        "{}",
        r.window.get_path_entry_error()
    );
    assert!(lock(&r.engine.check_video_threads).is_empty());
    assert_eq!(r.window.get_input_name(), "No file selected");

    // A file ffprobe refuses: the error dialog, the input untouched, the button back.
    r.window
        .set_path_entry_text(abs("/videos/other.mkv").into());
    r.click("OK");
    r.wait("the error", |w| w.get_popup() == Popup::Error);
    assert_eq!(
        r.window.get_error_message(),
        format!("not an MP4 file: {}", abs("/videos/other.mkv")).as_str()
    );
    assert_eq!(r.status(), "Status: Invalid input");
    assert_eq!(r.window.get_input_name(), "No file selected");
    assert!(r.window.get_open_enabled());
    r.click("OK");
    assert_eq!(r.popup(), Popup::None);

    // A valid path, typed key by key and validated with Enter, read off the UI thread.
    r.click("Choose input file");
    r.wait("the text field", |w| w.get_popup() == Popup::PathEntry);
    assert_eq!(
        r.window.get_path_entry_text(),
        "",
        "the field kept old text"
    );
    assert_eq!(r.window.get_path_entry_error(), "");
    r.type_keys(&abs("/videos/trip/clip.mp4"));
    assert_eq!(
        r.window.get_path_entry_text(),
        abs("/videos/trip/clip.mp4").as_str()
    );
    r.type_keys("\n");
    r.wait("the input", |w| w.get_input_name() == "clip.mp4");
    assert_eq!(r.status(), "Status: Input loaded");
    assert_eq!(r.popup(), Popup::None);
    assert!(r.window.get_open_enabled());
    let threads = lock(&r.engine.check_video_threads).clone();
    assert_eq!(threads.len(), 2, "one refused file and one good one");
    assert!(threads.iter().all(|t| *t != std::thread::current().id()));
}

fn the_text_field_stands_in_for_the_output_picker() {
    let r = rig_asking_for_a_path("Choose output file", FakeEngine::new());
    assert!(is_shown(&r.window, "Save output video"));
    assert!(r.window.get_path_entry_output());
    assert!(!r.enabled("Choose output file"));

    // No extension: .mp4 is added, as for a picked file.
    r.window
        .set_path_entry_text(abs("/tmp/videos/result").into());
    r.click("OK");
    r.wait("the output", |w| w.get_output_name() == "result.mp4");
    assert_eq!(r.status(), "Status: Output selected");
    assert_eq!(r.popup(), Popup::None);
    assert!(r.window.get_output_enabled());
    assert!(
        lock(&r.engine.check_video_threads).is_empty(),
        "an output is not read by ffprobe"
    );

    // Another extension gets .mp4 on top of it, an .MP4 is kept.
    r.click("Choose output file");
    r.wait("the text field", |w| w.get_popup() == Popup::PathEntry);
    r.window
        .set_path_entry_text(abs("/tmp/videos/clip.mov").into());
    r.click("OK");
    r.wait("the output", |w| w.get_output_name() == "clip.mov.mp4");
    r.click("Choose output file");
    r.wait("the text field", |w| w.get_popup() == Popup::PathEntry);
    r.window
        .set_path_entry_text(abs("/tmp/videos/CLIP.MP4").into());
    r.click("OK");
    r.wait("the output", |w| w.get_output_name() == "CLIP.MP4");
}

fn a_cancelled_text_field_changes_nothing() {
    let r = rig_asking_for_a_path("Choose input file", FakeEngine::new());
    r.window
        .set_path_entry_text(abs("/videos/trip/other.mp4").into());
    r.click_enabled("Cancel");
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(r.window.get_input_name(), "No file selected");
    assert_eq!(r.status(), "Status: Ready");
    assert!(r.window.get_open_enabled(), "the button must come back");
    assert!(lock(&r.engine.check_video_threads).is_empty());

    let r = rig_asking_for_a_path("Choose output file", FakeEngine::new());
    r.window
        .set_path_entry_text(abs("/tmp/videos/other.mp4").into());
    r.click_enabled("Cancel");
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(r.window.get_output_name(), "No file selected");
    assert_eq!(r.status(), "Status: Ready");
    assert!(r.window.get_output_enabled());

    // A field that was cancelled opens empty the next time.
    r.click("Choose output file");
    r.wait("the text field", |w| w.get_popup() == Popup::PathEntry);
    assert_eq!(r.window.get_path_entry_text(), "");
}

fn the_text_field_keeps_the_popup_queue() {
    let r = rig_asking_for_a_path("Choose input file", FakeEngine::new());
    r.window.set_path_entry_text(abs("/videos/par").into());
    // The Diagnostic's report lands while the field is open: it waits its turn and the field
    // keeps what was typed.
    r.window.invoke_diagnostic();
    // The status goes back to what it was in the same step that queues the report.
    r.wait("the report to be queued", |w| {
        w.get_status() == "Status: Ready"
    });
    assert_eq!(r.popup(), Popup::PathEntry, "the report jumped the queue");
    assert_eq!(r.window.get_path_entry_text(), abs("/videos/par").as_str());
    r.click_enabled("Cancel");
    assert_eq!(r.popup(), Popup::Diagnostic, "the queued report was lost");
    assert!(r.window.get_open_enabled());
    r.click("Close");
    assert_eq!(r.popup(), Popup::None);
}

fn a_second_text_field_for_the_other_file_starts_empty() {
    let r = rig(FakeEngine::new(), FakeHost::new().without_pickers());
    r.ready();
    // Both pickers are asked before the event loop runs: two fields, one after the other.
    r.click("Choose input file");
    r.click("Choose output file");
    pump(Duration::from_millis(300));
    assert_eq!(r.popup(), Popup::PathEntry);
    let first_is_output = r.window.get_path_entry_output();
    r.window
        .set_path_entry_text(abs("/videos/typed.mp4").into());
    r.window
        .set_path_entry_error("Please type the path of a file.".into());
    r.click_enabled("Cancel");
    assert_eq!(r.popup(), Popup::PathEntry, "the second field is not there");
    assert_ne!(r.window.get_path_entry_output(), first_is_output);
    assert_eq!(r.window.get_path_entry_text(), "", "the first field's text");
    assert_eq!(
        r.window.get_path_entry_error(),
        "",
        "the first field's error"
    );
}

fn a_video_check_that_cannot_start_reports_and_frees_the_button() {
    let r = rig_asking_for_a_path("Choose input file", FakeEngine::new());
    lock(&crate::app::FAILING_SPAWNS).push("check-input");
    r.window
        .set_path_entry_text(abs("/videos/trip/clip.mp4").into());
    r.click("OK");
    lock(&crate::app::FAILING_SPAWNS).clear();
    assert_eq!(r.popup(), Popup::Error);
    assert!(
        r.window
            .get_error_message()
            .starts_with("cannot read the video: "),
        "{}",
        r.window.get_error_message()
    );
    assert!(r.window.get_open_enabled(), "the button stays held");
    assert_eq!(r.window.get_input_name(), "No file selected");
    assert!(lock(&r.engine.check_video_threads).is_empty());
}

/// The startup probe is a GPU session: one that lands while a conversion runs would start a
/// verification beside a hardware encode (E-14) and rebuild the codec list under it. It waits
/// for the end of the conversion, which does not lose it.
fn a_startup_probe_that_lands_during_a_conversion_waits_for_its_end() {
    let probe_gate = Gate::default();
    let encode_gate = Gate::default();
    let (for_probe, for_encode) = (probe_gate.clone(), encode_gate.clone());
    let verifications = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&verifications);
    let probed = Arc::new(AtomicU32::new(0));
    let probe_done = Arc::clone(&probed);
    let engine = FakeEngine::new()
        .probing(move || {
            for_probe.wait();
            probe_done.fetch_add(1, Ordering::SeqCst);
            // libx265 refuses: the list goes from three entries to two.
            EncoderProbeReport {
                results: vec![EncoderProbe {
                    encoder: "libx265".into(),
                    ..EncoderProbe::default()
                }],
                ..EncoderProbeReport::default()
            }
        })
        .verifying(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            "Hardware: verified".to_string()
        })
        .encoding(move |_, _, _| {
            for_encode.wait();
            Ok(outcome("Hardware: the CPU did it"))
        });
    let r = Rig::loaded_before_probe(engine, "result.mp4");
    r.click("Start transformation");
    probe_gate.open();
    wait_until("the probe to return", {
        let probed = Arc::clone(&probed);
        move || probed.load(Ordering::SeqCst) == 1
    });
    // The answer is posted to the window just after; give the loop time to take it.
    pump(Duration::from_millis(300));
    assert_eq!(
        verifications.load(Ordering::SeqCst),
        0,
        "a hardware verification ran beside the conversion"
    );
    assert_eq!(
        r.window.get_encoder_options().row_count(),
        3,
        "the codec list was rebuilt during the conversion"
    );
    encode_gate.open();
    r.wait("the completion", |w| w.get_status() == "Status: Completed");
    r.wait("the deferred probe", |w| {
        w.get_encoder_options().row_count() == 2
    });
    assert_eq!(
        r.window.get_hardware(),
        "Hardware: the CPU did it",
        "the late probe wrote over the summary of the run"
    );
    pump(Duration::from_millis(100));
    assert_eq!(
        verifications.load(Ordering::SeqCst),
        0,
        "the late probe started a verification when the conversion ended"
    );
    r.click("OK");
}

/// Diagnostic probes every hardware encoder, and a conversion does the same work: neither may
/// start while the other runs. A second click on Diagnostic does not start a second report.
fn the_diagnostic_and_a_conversion_do_not_overlap() {
    let gate = Gate::default();
    let for_health = gate.clone();
    let reports = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&reports);
    let mut engine = FakeEngine::new();
    engine.health = Box::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        for_health.wait();
        "report\n".to_string()
    });
    let r = Rig::loaded(engine, "result.mp4");
    assert!(r.enabled("Start transformation"));
    r.click("Diagnostic");
    assert!(!r.enabled("Start transformation"), "Start stayed available");
    assert!(!r.enabled("Diagnostic"), "Diagnostic stayed available");
    r.click("Diagnostic");
    r.click("Start transformation");
    gate.open();
    r.wait("the report", |w| w.get_popup() == Popup::Diagnostic);
    pump(Duration::from_millis(100));
    assert_eq!(reports.load(Ordering::SeqCst), 1, "two reports were run");
    assert!(
        lock(&r.engine.jobs).is_empty(),
        "a conversion started beside the Diagnostic"
    );
    r.click("Close");
    assert!(
        r.enabled("Start transformation"),
        "Start was not given back"
    );
    assert!(r.enabled("Diagnostic"), "Diagnostic was not given back");
}

/// "Overwrite?" is asked about one pair of files. A picker that was open meanwhile may change
/// the destination under the question: "Yes" then answers a question nobody asked.
fn the_overwrite_question_is_about_the_files_it_was_asked_on() {
    overwrite_question_after_a_change(false);
    overwrite_question_after_a_change(true);
}

fn overwrite_question_after_a_change(change_input: bool) {
    let gate = Gate::default();
    let (for_input, for_output) = (gate.clone(), gate.clone());
    let dir = tempfile::tempdir().expect("tempdir");
    let (first, second) = (dir.path().join("a.mp4"), dir.path().join("b.mp4"));
    std::fs::write(&first, b"precious").expect("write");
    std::fs::write(&second, b"precious too").expect("write");
    let (inputs, outputs) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
    let (kept, moved) = (first.clone(), second.clone());
    let host = FakeHost::new()
        .picking_input_with(move || {
            if !change_input || inputs.fetch_add(1, Ordering::SeqCst) == 0 {
                Some(PathBuf::from("/videos/trip/clip.mp4"))
            } else {
                for_input.wait();
                Some(PathBuf::from("/videos/trip/other.mp4"))
            }
        })
        .picking_output_with(move || {
            if change_input || outputs.fetch_add(1, Ordering::SeqCst) == 0 {
                Some(kept.clone())
            } else {
                for_output.wait();
                Some(moved.clone())
            }
        });
    let r = rig(FakeEngine::new(), host);
    r.ready();
    r.pick_input_and_wait("clip.mp4");
    r.pick_output_and_wait("a.mp4");
    r.click("Start transformation");
    assert_eq!(r.popup(), Popup::Overwrite);
    if change_input {
        r.click("Choose input file");
        gate.open();
        r.wait("the other input", |w| w.get_input_name() == "other.mp4");
    } else {
        r.click("Choose output file");
        gate.open();
        r.wait("the other destination", |w| w.get_output_name() == "b.mp4");
    }
    assert_eq!(r.popup(), Popup::Overwrite, "the question went away");
    r.click("Yes");
    pump(Duration::from_millis(150));
    assert!(
        lock(&r.engine.jobs).is_empty(),
        "the conversion started on files nobody was asked about"
    );
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(r.status(), "Status: The files changed, press Start again");
    assert_eq!(std::fs::read(&second).expect("read"), b"precious too");
}

/// A progress report already queued when Cancel is pressed lands after it: it must not put
/// "Transforming..." back over "Cancelling..." (F-23).
fn a_late_progress_does_not_overwrite_cancelling() {
    let engine = FakeEngine::new().encoding(move |_, ui, cancel| {
        wait_for_cancel(cancel);
        ui.show_progress(60.0);
        ui.show_progress_detail(60.0, Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(300));
        Err(Error::Cancelled)
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    r.click("Cancel");
    pump(Duration::from_millis(150));
    assert_eq!(r.status(), "Status: Cancelling...");
    assert!(r.window.get_progress() < 0.01, "the bar moved after Cancel");
    r.wait("the cancellation", |w| {
        w.get_status() == "Status: Cancelled"
    });
}

/// "Cancel and quit" on screen when the conversion ends is stale: it says an encoding is in
/// progress, and the outcome would wait behind it (F-24).
fn the_quit_question_goes_when_the_conversion_ends() {
    let started = Gate::default();
    let release = Gate::default();
    let (for_encode, hold) = (started.clone(), release.clone());
    let engine = FakeEngine::new().encoding(move |job, _, _| {
        for_encode.open();
        hold.wait();
        std::fs::write(&job.output, b"video").expect("write the output");
        Ok(outcome("Hardware: the CPU did it"))
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    started.wait();
    r.click("Quit");
    assert_eq!(r.popup(), Popup::Quit);
    release.open();
    r.wait("the Done dialog", |w| w.get_popup() == Popup::Done);
    r.click("OK");
    assert_eq!(r.popup(), Popup::None, "the stale question came back");
}

/// The quality menu is the list `prefs` validates against (F-25).
fn the_quality_menu_is_the_rust_list() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    let shown: Vec<String> = r
        .window
        .get_quality_profiles()
        .iter()
        .map(|p| p.to_string())
        .collect();
    assert_eq!(shown, crate::prefs::QUALITY_PROFILES);
}

/// Escape and Enter answer a dialog; a destructive question answers only to Escape, which
/// is its "No" (F-26).
fn modals_answer_to_escape_and_enter() {
    let r = rig(FakeEngine::new(), FakeHost::new());
    r.ready();
    // Start with nothing chosen: the "No input" dialog.
    r.click("Start transformation");
    assert_eq!(r.popup(), Popup::NoInput);
    r.type_keys("\n");
    assert_eq!(r.popup(), Popup::None, "Enter did not close the dialog");
    r.click("Start transformation");
    assert_eq!(r.popup(), Popup::NoInput);
    r.type_keys("\u{1b}");
    assert_eq!(r.popup(), Popup::None, "Escape did not close the dialog");
}

/// A stray Enter never overwrites a file or quits; Escape is the "No" (F-26).
fn destructive_questions_ignore_enter_and_escape_means_no() {
    let started = Gate::default();
    let release = Gate::default();
    let (for_encode, hold) = (started.clone(), release.clone());
    let engine = FakeEngine::new().encoding(move |job, _, _| {
        for_encode.open();
        hold.wait();
        std::fs::write(&job.output, b"video").expect("write the output");
        Ok(outcome("Hardware: the CPU did it"))
    });
    let r = Rig::loaded(engine, "result.mp4");
    r.click("Start transformation");
    started.wait();
    // Quit during the run: Enter leaves the question, Escape answers No.
    r.click("Quit");
    assert_eq!(r.popup(), Popup::Quit);
    r.type_keys("\n");
    assert_eq!(r.popup(), Popup::Quit, "Enter answered the quit question");
    assert_eq!(r.host.quits.load(Ordering::SeqCst), 0);
    r.type_keys("\u{1b}");
    assert_eq!(r.popup(), Popup::None);
    assert!(r.window.get_cancel_enabled(), "Escape stopped the run");
    release.open();
    r.wait("the Done dialog", |w| w.get_popup() == Popup::Done);
    r.click("OK");

    // The output exists now: Start asks before overwriting.
    r.click("Start transformation");
    assert_eq!(r.popup(), Popup::Overwrite);
    r.type_keys("\n");
    assert_eq!(
        r.popup(),
        Popup::Overwrite,
        "Enter answered the overwrite question"
    );
    assert_eq!(lock(&r.engine.jobs).len(), 1);
    r.type_keys("\u{1b}");
    assert_eq!(r.popup(), Popup::None);
    assert_eq!(lock(&r.engine.jobs).len(), 1, "Escape started a conversion");
}
