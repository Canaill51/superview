//! The glue between the Slint window and everything else (DEC-43): it turns
//! the window's callbacks into calls on [`AppState`], runs the slow work (pickers, ffprobe,
//! probes, the conversion, the diagnostic) on threads of its own, and brings every result back
//! to the event loop with `upgrade_in_event_loop` before a property is touched. It holds what
//! `main` runs once the window exists.
//!
//! Nothing here reaches ffmpeg or the desktop directly: [`Engine`] (tools, pipeline) and
//! [`Host`] (pickers, browser, end of the loop) are injected, so the window's tests run the
//! real wiring with a conversion that is only simulated.
//!
//! Design choices:
//!
//! * Dialogs are one [`Popup`] at a time, in a queue: a second one waits for the first to be
//!   answered, not stacked windows. The same dialog is never queued twice.
//! * A panic in the encoder, a picker, ffprobe, a probe or the diagnostic is caught on its
//!   thread: it counts as a failure (an error dialog), a cancel or a neutral text, and the
//!   window is never left locked.
//! * The video check (ffprobe) runs on the picker's thread, after the pick, with the button
//!   still held down, never on the UI thread.
//! * No file picker at all (no portal, no zenity, no kdialog): a text field in a dialog of the
//!   window asks for the path (`dialogs.rs` decides when). What is typed goes through the same
//!   checks as a picked file.
//! * The choice of the output file is applied as it is picked and the overwrite question is
//!   asked at Start (the native picker may ask it too).
//! * The install link opens through `xdg-open` / `explorer.exe`, never a shell string, and only
//!   for the one address the program knows.
//! * The probe, the hardware verification and the Diagnostic threads are not joined at exit:
//!   they watch the `leaving` flag (a probe cut short is not remembered), and the process ends
//!   under them. The conversion is waited for, because it owns files and a process, and so are
//!   the thumbnail threads (2 s at most): each owns a temporary directory and an ffmpeg.
//! * The path from a system signal to the exit (signal handler, end of the event loop,
//!   `App::shutdown`) is not covered by a test here: the window's tests simulate the encoder's
//!   answer to a signal (`StoppedBySignal`) but cannot send one. `superview-harness` covers the
//!   signal's effect on a real conversion.
//! * A file that cannot be opened for the log, or a log line that cannot be written, never
//!   stops anything.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
#[cfg(test)]
use std::time::Duration;

use slint::{CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, VecModel};
use superview_core::config::Config;
use superview_core::error::Error;
use superview_core::pipeline::{EncodeOutcome, PipelineUi};
use superview_core::probe::EncoderProbeReport;
use superview_core::thumbnail::Thumbnail;
use superview_core::video::VideoSpecs;

use crate::dialogs::{Kind, Pick, parse_typed_path};
use crate::format::{is_missing_ffmpeg_error, parse_encoder_selection, resolve_start_settings};
use crate::prefs::{Preferences, QUALITY_PROFILES, THEMES};
use crate::report::{Row, RowKind, parse_report};
use crate::state::{
    AppState, BarState, ChosenOutcome, Completion, Control, GuiHandler, Lifecycle, ProgressSink,
    ViewModel, run_encoding,
};
use crate::{DiagnosticRow, MainWindow, Popup, ReportRowKind};

/// Where the prerequisite dialog sends the user.
pub const INSTALL_URL: &str =
    "https://github.com/Canaill51/superview?tab=readme-ov-file#download-and-install";

/// The status line while the diagnostic runs.
const RUNNING_DIAGNOSTIC: &str = "Status: Running diagnostic...";

/// The core's report marks each check with an emoji (✅ and ❌). The window's
/// software renderer draws no colour emoji, and a machine without a symbols font shows nothing
/// at all, so the window swaps them for plain text. The core's text stays as it is, which a
/// recorded-text test pins; only what is shown changes.
fn plain_marks(report: &str) -> String {
    report.replace('✅', "[OK]").replace('❌', "[FAIL]")
}

/// One parsed line, as the window's list takes it. The text goes through `plain_marks` too: an
/// emoji inside a line (a reason quoting one) would draw as nothing.
fn window_row(row: &Row) -> DiagnosticRow {
    DiagnosticRow {
        kind: match row.kind {
            RowKind::Blank => ReportRowKind::Blank,
            RowKind::Section => ReportRowKind::Section,
            RowKind::Check => ReportRowKind::Check,
            RowKind::Value => ReportRowKind::Value,
            RowKind::Status => ReportRowKind::Status,
            RowKind::Plain => ReportRowKind::Plain,
        },
        ok: row.ok,
        text: plain_marks(&row.text).into(),
    }
}

/// Everything one conversion needs, captured on the UI thread when Start is pressed.
#[derive(Debug, Clone)]
pub struct EncodeJob {
    /// The configuration with the quality profile's preset applied.
    pub cfg: Config,
    pub input: PathBuf,
    pub output: PathBuf,
    /// What `check_ffmpeg` and the probe left in the capability map.
    pub ffmpeg: HashMap<String, String>,
}

/// What does the work: ffmpeg, ffprobe and the pipeline. Every method may block, and none is
/// called on the UI thread, except `check_ffmpeg` and `forget_tools` (a few milliseconds).
pub trait Engine: Send + Sync + 'static {
    fn check_ffmpeg(&self) -> Result<HashMap<String, String>, Error>;
    /// Forgets what was found about the tools, before a retry.
    fn forget_tools(&self);
    /// Asks each advertised encoder to encode a frame. `cancel` ends it early.
    fn probe_hardware(
        &self,
        ffmpeg: &HashMap<String, String>,
        cancel: &AtomicBool,
    ) -> EncoderProbeReport;
    /// ffprobe on a file.
    fn check_video(&self, path: &str) -> Result<VideoSpecs, Error>;
    /// One picture of the input, for the file card. `cancel` ends it early.
    fn thumbnail(&self, video: &VideoSpecs, cancel: &AtomicBool) -> Result<Thumbnail, Error>;
    /// The "Hardware: …" line, with the planned encoder asked about this file's frame.
    fn verify_hardware(
        &self,
        request: &crate::state::VerificationRequest,
        cancel: &AtomicBool,
    ) -> String;
    /// One conversion, from the first check to the rename. `cancel` is the Cancel button.
    fn encode(
        &self,
        job: &EncodeJob,
        ui: &dyn PipelineUi,
        cancel: &AtomicBool,
    ) -> Result<EncodeOutcome, Error>;
    /// The text of the health report (and a mirror of it in the log). `temp_dir` is where the
    /// remap maps would be written.
    fn health_report(&self, temp_dir: &Path, cancel: &AtomicBool) -> String;
}

/// What the window asks of the desktop.
pub trait Host: Send + Sync + 'static {
    /// The input picker. May block: called off the UI thread.
    fn pick_input(&self) -> Pick;
    /// The output picker. May block: called off the UI thread.
    fn pick_output(&self) -> Pick;
    /// Opens an address in the browser.
    fn open_link(&self, url: &str);
    /// Ends the event loop; callable from any thread.
    fn quit(&self);
}

/// What `main` hands to [`App::wire`].
pub struct Setup {
    pub cfg: Config,
    pub engine: Arc<dyn Engine>,
    pub host: Arc<dyn Host>,
    pub prefs: Preferences,
    pub prefs_path: Option<PathBuf>,
    /// The log file, for the Diagnostic report; `None` when it could not be opened.
    pub log_path: Option<PathBuf>,
    /// Which binary this is (`format::build_identity`).
    pub identity: String,
    /// The directory the Diagnostic checks the room of.
    pub temp_dir: PathBuf,
}

/// A dialog, with what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PopupKind {
    Done(String),
    Error(String),
    /// "cannot find ffmpeg/ffprobe": with Retry when something can be retried.
    Prerequisite {
        retry: bool,
    },
    /// The question is about this pair of files; an answer is only good for them.
    Overwrite {
        input: String,
        output: String,
    },
    Quit,
    NoInput,
    NoOutput,
    /// The report as the core wrote it, emoji marks included: the window draws icons from the
    /// marks and shows the plain-text form beside them.
    Diagnostic(String),
    /// The text field that stands in for a picker the system cannot open.
    PathEntry(Kind),
}

/// The dialogs waiting to be answered; the first one is on screen.
#[derive(Default)]
struct Popups {
    queue: VecDeque<PopupKind>,
}

impl Popups {
    /// Queues a dialog, unless the very same one is already waiting.
    fn push(&mut self, kind: PopupKind) {
        if !self.queue.contains(&kind) {
            self.queue.push_back(kind);
        }
    }

    fn current(&self) -> Option<&PopupKind> {
        self.queue.front()
    }

    /// The picker the text field on screen stands for, if it is the one on screen.
    fn path_entry(&self) -> Option<Kind> {
        match self.current() {
            Some(PopupKind::PathEntry(kind)) => Some(*kind),
            _ => None,
        }
    }

    /// The answered dialog goes; the next one, if any, is now the current one. A "Cancel and
    /// quit" question that waited behind it is dropped when nothing runs any more (`encoding`
    /// false): the question is stale.
    fn close(&mut self, encoding: bool) {
        self.queue.pop_front();
        if !encoding {
            self.drop_quit();
        }
    }

    /// Takes the "Cancel and quit" question away, on screen or not.
    fn drop_quit(&mut self) {
        self.queue.retain(|kind| *kind != PopupKind::Quit);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The text of a panic payload.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown cause".to_string())
}

/// The error a caught panic becomes.
fn panic_error(what: &str, payload: &(dyn Any + Send)) -> Error {
    let message = panic_message(payload);
    log::error!("A thread panicked: what={what} panic={message}");
    Error::Plain(format!("{what} stopped unexpectedly: {message}"))
}

/// Everything the callbacks share. The state is touched from the UI thread only; the rest is
/// either immutable or its own synchronisation.
struct Core {
    state: Mutex<AppState>,
    popups: Mutex<Popups>,
    lifecycle: Arc<Lifecycle>,
    engine: Arc<dyn Engine>,
    host: Arc<dyn Host>,
    cfg: Config,
    log_path: Option<PathBuf>,
    identity: String,
    temp_dir: PathBuf,
    window: slint::Weak<MainWindow>,
    /// Raised when the application is leaving: ends the probes (a cancelled probe is not
    /// remembered as a verdict).
    leaving: AtomicBool,
    /// The thumbnail threads: each owns a temporary directory and a process, so they are waited
    /// for (briefly) when the application leaves.
    thumbnails: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl Core {
    fn state(&self) -> MutexGuard<'_, AppState> {
        lock(&self.state)
    }

    /// Copies the view into the window's properties.
    fn refresh(&self, window: &MainWindow) {
        // Cloned, so that no lock is held while the properties are written.
        let view = self.state().view.clone();
        apply_view(window, &view);
    }

    /// Queues a dialog and shows the first one.
    fn show(&self, window: &MainWindow, kind: PopupKind) {
        let mut popups = lock(&self.popups);
        popups.push(kind);
        apply_popup(window, popups.current());
    }

    /// Answers the dialog on screen.
    fn close_popup(&self, window: &MainWindow) {
        let encoding = self.state().is_encoding();
        let mut popups = lock(&self.popups);
        popups.close(encoding);
        apply_popup(window, popups.current());
    }

    /// `leaving` ends a thumbnail's ffmpeg within about 10 ms and its directory goes with the
    /// thread; waiting a little (2 s at most) is what keeps the process and the directory from
    /// outliving the window.
    fn wait_for_thumbnails(&self) {
        let handles = std::mem::take(&mut *lock(&self.thumbnails));
        let give_up = std::time::Instant::now() + std::time::Duration::from_secs(2);
        for handle in handles {
            while !handle.is_finished() && std::time::Instant::now() < give_up {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }

    /// Asks a running conversion to stop and waits, within a bound, for it to have unwound.
    fn shutdown(&self, reason: &str) -> bool {
        self.leaving.store(true, Ordering::Release);
        self.wait_for_thumbnails();
        self.lifecycle.shutdown(reason)
    }
}

/// Gives the quality and Theme menus their entries. Before the first `apply_view`: a menu with no
/// entries empties the choice it is bound to.
pub(crate) fn set_quality_profiles(window: &MainWindow) {
    let profiles: Vec<SharedString> = QUALITY_PROFILES
        .iter()
        .map(|profile| SharedString::from(*profile))
        .collect();
    window.set_quality_profiles(ModelRc::new(VecModel::from(profiles)));
    let themes: Vec<SharedString> = THEMES
        .iter()
        .map(|theme| SharedString::from(*theme))
        .collect();
    window.set_theme_options(ModelRc::new(VecModel::from(themes)));
}

/// Writes the view into the window.
fn apply_view(window: &MainWindow, view: &ViewModel) {
    window.set_status(view.status.as_str().into());
    window.set_results(view.results.as_str().into());
    window.set_hardware(view.hardware.as_str().into());
    window.set_input_name(view.input_name.as_str().into());
    window.set_output_name(view.output_name.as_str().into());
    window.set_progress(view.progress as f32);
    window.set_input_specs(view.input_specs.as_str().into());
    window.set_bar_state(match view.bar {
        BarState::Idle => crate::BarState::Idle,
        BarState::Running => crate::BarState::Running,
        BarState::Done => crate::BarState::Done,
        BarState::Stopped => crate::BarState::Stopped,
    });
    window.set_quality(view.quality.as_str().into());
    window.set_squeeze(view.squeeze);
    window.set_theme(view.theme.as_str().into());
    // The list is replaced only when it changed: a new model would close an open dropdown.
    let shown: Vec<String> = window
        .get_encoder_options()
        .iter()
        .map(|option| option.to_string())
        .collect();
    if shown != view.encoder_options {
        let options: Vec<SharedString> = view
            .encoder_options
            .iter()
            .map(|option| SharedString::from(option.as_str()))
            .collect();
        window.set_encoder_options(ModelRc::new(VecModel::from(options)));
    }
    window.set_encoder_selected(view.encoder_selected.as_str().into());
    window.set_start_enabled(view.start_enabled);
    window.set_cancel_enabled(view.cancel_enabled);
    window.set_open_enabled(view.is_enabled(Control::Open));
    window.set_output_enabled(view.is_enabled(Control::SelectOutput));
    window.set_quality_enabled(view.is_enabled(Control::Quality));
    window.set_squeeze_enabled(view.is_enabled(Control::Squeeze));
    window.set_encoder_enabled(view.is_enabled(Control::Encoder));
    window.set_diagnostic_enabled(view.is_enabled(Control::Diagnostic));
}

/// Shows `current` in the window, or closes the popup when there is none.
fn apply_popup(window: &MainWindow, current: Option<&PopupKind>) {
    let popup = match current {
        None => Popup::None,
        Some(PopupKind::Done(message)) => {
            window.set_done_message(message.as_str().into());
            Popup::Done
        }
        Some(PopupKind::Error(message)) => {
            window.set_error_message(message.as_str().into());
            Popup::Error
        }
        Some(PopupKind::Prerequisite { retry }) => {
            window.set_install_url(INSTALL_URL.into());
            window.set_prerequisite_retry(*retry);
            Popup::Prerequisite
        }
        Some(PopupKind::Overwrite { .. }) => Popup::Overwrite,
        Some(PopupKind::Quit) => Popup::Quit,
        Some(PopupKind::NoInput) => Popup::NoInput,
        Some(PopupKind::NoOutput) => Popup::NoOutput,
        Some(PopupKind::Diagnostic(report)) => {
            let text = plain_marks(report);
            // Called again while the report is on screen (a dialog queued behind it): the list
            // is left as it is, so the scroll position and the chosen view are kept.
            let on_screen = window.get_popup() == Popup::Diagnostic
                && window.get_diagnostic_report() == text.as_str();
            if !on_screen {
                // A report opens on the list, whichever view the last one was left on.
                if window.get_popup() != Popup::Diagnostic {
                    window.set_diagnostic_as_text(false);
                }
                window.set_diagnostic_report(text.into());
                let rows: Vec<DiagnosticRow> =
                    parse_report(report).iter().map(window_row).collect();
                window.set_diagnostic_rows(ModelRc::new(VecModel::from(rows)));
            }
            Popup::Diagnostic
        }
        Some(PopupKind::PathEntry(kind)) => {
            // A field already on screen, for the same file, keeps what is typed in it; another
            // file's field (the input's after the output's) starts empty.
            let same = window.get_popup() == Popup::PathEntry
                && window.get_path_entry_output() == (*kind == Kind::Output);
            if !same {
                window.set_path_entry_text("".into());
                window.set_path_entry_error("".into());
            }
            window.set_path_entry_output(*kind == Kind::Output);
            Popup::PathEntry
        }
    };
    window.set_popup(popup);
}

/// Runs `work` later, on the event loop, with the window. If the loop is gone (the application
/// is leaving) the work is dropped.
fn post(core: &Arc<Core>, work: impl FnOnce(&Arc<Core>, &MainWindow) + Send + 'static) {
    let for_work = Arc::clone(core);
    if let Err(e) = core
        .window
        .upgrade_in_event_loop(move |window| work(&for_work, &window))
    {
        log::debug!("Dropping an update: the event loop is gone: error={e}");
    }
}

/// A thread with a name, for the log and the debugger.
fn spawn(
    name: &str,
    work: impl FnOnce() + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    #[cfg(test)]
    if lock(&FAILING_SPAWNS).contains(&name) {
        return Err(std::io::Error::other("no thread for the test"));
    }
    std::thread::Builder::new().name(name.into()).spawn(work)
}

/// The names of the threads that refuse to start, in tests.
#[cfg(test)]
pub static FAILING_SPAWNS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// The window, wired. Keep it alive for as long as the event loop runs.
pub struct App {
    core: Arc<Core>,
}

impl App {
    /// Connects `window` to the state and the injected services, runs the startup (ffmpeg
    /// check, probe in the background) and registers every callback.
    pub fn wire(window: &MainWindow, setup: Setup) -> Self {
        window.set_window_title(format!("Superview {}", setup.identity).into());
        let state = AppState::new(setup.prefs, setup.prefs_path);
        let lifecycle = state.lifecycle();
        let core = Arc::new(Core {
            state: Mutex::new(state),
            popups: Mutex::new(Popups::default()),
            lifecycle,
            engine: setup.engine,
            host: setup.host,
            cfg: setup.cfg,
            log_path: setup.log_path,
            identity: setup.identity,
            temp_dir: setup.temp_dir,
            window: window.as_weak(),
            leaving: AtomicBool::new(false),
            thumbnails: Mutex::new(Vec::new()),
        });

        {
            let mut state = core.state();
            let host = Arc::clone(&core.host);
            state.set_quit_hook(move || host.quit());
            let weak = Arc::downgrade(&core);
            state.set_verify_hook(move |request| {
                if let Some(core) = weak.upgrade() {
                    verify_in_background(&core, request);
                }
            });
        }

        register(&core, window);
        startup(&core, window);
        Self { core }
    }

    /// What every way out goes through once the loop has stopped: asks a running conversion to
    /// stop and waits for it. Safe to call any number of times.
    pub fn shutdown(&self, reason: &str) -> bool {
        self.core.shutdown(reason)
    }

    /// Shortens the wait of [`App::shutdown`], for tests.
    #[cfg(test)]
    pub fn set_shutdown_grace(&self, grace: Duration) {
        self.core.state().set_shutdown_grace(grace);
    }
}

/// A callback without argument that finds the window again.
fn bind(core: &Arc<Core>, action: fn(&Arc<Core>, &MainWindow)) -> impl Fn() + 'static {
    let core = Arc::clone(core);
    move || {
        if let Some(window) = core.window.upgrade() {
            action(&core, &window);
        }
    }
}

fn register(core: &Arc<Core>, window: &MainWindow) {
    window.on_choose_input(bind(core, choose_input));
    window.on_choose_output(bind(core, choose_output));
    window.on_start(bind(core, start));
    window.on_cancel(bind(core, cancel));
    window.on_diagnostic(bind(core, diagnostic));
    window.on_quit(bind(core, quit_pressed));

    {
        let core = Arc::clone(core);
        window.on_quality_changed(move |profile| core.state().quality_changed(&profile));
    }
    {
        let core = Arc::clone(core);
        window.on_theme_changed(move |theme| core.state().theme_changed(&theme));
    }
    {
        let core = Arc::clone(core);
        window.on_squeeze_toggled(move |checked| {
            core.state().squeeze_changed(checked);
            if let Some(window) = core.window.upgrade() {
                core.refresh(&window);
            }
        });
    }
    {
        let core = Arc::clone(core);
        window.on_encoder_changed(move |selection| {
            core.state().encoder_changed(&selection);
            if let Some(window) = core.window.upgrade() {
                core.refresh(&window);
            }
        });
    }

    // The popups. "OK", "Close" and "No" only answer; the others also act.
    window.on_done_ok(bind(core, |core, window| core.close_popup(window)));
    window.on_error_ok(bind(core, |core, window| core.close_popup(window)));
    window.on_prerequisite_close(bind(core, |core, window| core.close_popup(window)));
    window.on_no_input_ok(bind(core, |core, window| core.close_popup(window)));
    window.on_no_output_ok(bind(core, |core, window| core.close_popup(window)));
    window.on_diagnostic_close(bind(core, |core, window| core.close_popup(window)));
    window.on_overwrite_no(bind(core, |core, window| core.close_popup(window)));
    window.on_quit_no(bind(core, |core, window| core.close_popup(window)));
    window.on_path_entry_ok(bind(core, path_entry_ok));
    window.on_path_entry_cancel(bind(core, path_entry_cancel));
    window.on_prerequisite_retry_clicked(bind(core, retry_prerequisite));
    window.on_overwrite_yes(bind(core, overwrite_yes));
    window.on_quit_yes(bind(core, |core, window| {
        core.close_popup(window);
        leave(core, window, "quit confirmed");
    }));
    {
        let core = Arc::clone(core);
        window.on_open_install_link(move |url| {
            // The window reports the address it was given; only the program's own is opened.
            if url == INSTALL_URL {
                core.host.open_link(INSTALL_URL);
            } else {
                log::warn!("Refusing to open an address the program did not set: url={url}");
            }
        });
    }

    // The system's close request (the title bar's cross, Alt+F4): the same question as the
    // Quit button, and the same wait.
    {
        let core = Arc::clone(core);
        window.window().on_close_requested(move || {
            if core.state().is_encoding() {
                if let Some(window) = core.window.upgrade() {
                    core.show(&window, PopupKind::Quit);
                }
                return CloseRequestResponse::KeepWindowShown;
            }
            core.shutdown("window closing");
            core.host.quit();
            CloseRequestResponse::HideWindow
        });
    }
}

/// The startup: the ffmpeg check, the codec list, then the probe of the encoders.
fn startup(core: &Arc<Core>, window: &MainWindow) {
    match core.engine.check_ffmpeg() {
        Ok(ffmpeg) => core.state().set_ffmpeg(ffmpeg),
        Err(err) => {
            log::error!("ffmpeg is unavailable at startup: error={err}");
            core.state().set_ffmpeg_unavailable();
            show_ffmpeg_failure(core, window, &err, true);
            core.state().view.status = "Status: ffmpeg unavailable".into();
        }
    }
    // The menu offers the profiles the saved choice is validated against.
    set_quality_profiles(window);
    // Fills the options and restores the saved selection, under the flag that keeps a
    // fallback from being saved over it. Without ffmpeg it also writes "Hardware: ffmpeg
    // unavailable".
    core.state().refresh_encoder_options();
    core.refresh(window);
    start_encoder_probe(core);
}

/// The prerequisite dialog for a missing ffmpeg, the error dialog for anything else.
fn show_ffmpeg_failure(core: &Arc<Core>, window: &MainWindow, err: &Error, retry: bool) {
    if is_missing_ffmpeg_error(err) {
        core.show(window, PopupKind::Prerequisite { retry });
    } else {
        core.show(window, PopupKind::Error(err.to_string()));
    }
}

/// Retry, after the user installed ffmpeg. The lookup is not cached when it fails, so this sees
/// an ffmpeg installed while the application was running.
fn retry_prerequisite(core: &Arc<Core>, window: &MainWindow) {
    core.close_popup(window);
    core.engine.forget_tools();
    match core.engine.check_ffmpeg() {
        Err(err) => {
            log::warn!("ffmpeg still unavailable after retry: error={err}");
            show_ffmpeg_failure(core, window, &err, true);
        }
        Ok(ffmpeg) => {
            log::info!(
                "ffmpeg found on retry: version={}",
                ffmpeg.get("version").map_or("", String::as_str)
            );
            {
                let mut state = core.state();
                state.set_ffmpeg(ffmpeg);
                state.refresh_encoder_options();
            }
            start_encoder_probe(core);
            let mut state = core.state();
            // The same list the startup path disabled, computed by the same function.
            state.apply_ffmpeg_availability();
            state.view.status = "Status: Ready".into();
            state.refresh_hardware_status();
            state.refresh_start();
            drop(state);
            core.refresh(window);
        }
    }
}

/// Asks the encoders whether they actually work, off the UI thread: a few hundred milliseconds
/// per encoder, far too long to spend before the window appears, and far too important to skip.
fn start_encoder_probe(core: &Arc<Core>) {
    let ffmpeg = {
        let state = core.state();
        if !state.is_ffmpeg_available() {
            return;
        }
        state.ffmpeg().clone()
    };
    let thread_core = Arc::clone(core);
    let spawned = spawn("probe-encoders", move || {
        let report = catch_unwind(AssertUnwindSafe(|| {
            thread_core
                .engine
                .probe_hardware(&ffmpeg, &thread_core.leaving)
        }))
        .unwrap_or_else(|payload| {
            // No verdict: the encoders stay as ffmpeg advertised them.
            panic_error("the encoder probe", payload.as_ref());
            EncoderProbeReport::default()
        });
        post(&thread_core, move |core, window| {
            {
                let mut state = core.state();
                // Kept for the end of a conversion that is running (see `finish_encoding`).
                if state.set_encoder_probe(report) {
                    state.refresh_encoder_options();
                    state.refresh_hardware_status();
                    state.verify_hardware_in_background();
                    state.refresh_start();
                }
            }
            core.refresh(window);
        });
    });
    if let Err(e) = spawned {
        log::error!("Cannot start the encoder probe: error={e}");
    }
}

/// Asks the planned encoder, off the UI thread, whether it takes this file's frame. The answer
/// carries the generation it was computed for, so a slow one cannot overwrite a newer line.
fn verify_in_background(core: &Arc<Core>, request: crate::state::VerificationRequest) {
    let generation = request.generation;
    let thread_core = Arc::clone(core);
    let spawned = spawn("verify-hardware", move || {
        let line = catch_unwind(AssertUnwindSafe(|| {
            thread_core
                .engine
                .verify_hardware(&request, &thread_core.leaving)
        }));
        post(&thread_core, move |core, window| {
            {
                let mut state = core.state();
                match line {
                    Ok(text) => state.apply_verified_hardware_line(generation, &text),
                    Err(payload) => {
                        panic_error("the hardware verification", payload.as_ref());
                        state.apply_verification_failed(generation);
                    }
                }
            }
            core.refresh(window);
        });
    });
    if let Err(e) = spawned {
        log::error!("Cannot start the hardware verification: error={e}");
        // Not here: the caller (the verify hook) holds the lock on the state, which is not
        // re-entrant. The event loop applies it once the lock is free.
        post(core, move |core, window| {
            core.state().apply_verification_failed(generation);
            core.refresh(window);
        });
    }
}

/// Reads the chosen input with ffprobe (off the UI thread) and applies it on the UI thread;
/// `None` is a cancel. Releases the button either way.
fn finish_input_pick(core: &Arc<Core>, chosen: Option<PathBuf>) {
    // The file is read here, off the UI thread. A path that is not UTF-8 is not read at
    // all: `apply_chosen_input` refuses it with a message of its own.
    let checked = chosen.as_deref().and_then(Path::to_str).map(|text| {
        catch_unwind(AssertUnwindSafe(|| core.engine.check_video(text)))
            .unwrap_or_else(|payload| Err(panic_error("reading the video", payload.as_ref())))
    });
    post(core, move |core, window| {
        let outcome = {
            let mut state = core.state();
            state.finish_pick(Control::Open);
            chosen.map(|path| {
                state.apply_chosen_input(&path, |_| {
                    checked.unwrap_or_else(|| {
                        Err(Error::Plain(
                            "the input file path is not valid UTF-8".to_string(),
                        ))
                    })
                })
            })
        };
        core.refresh(window);
        match outcome {
            Some(ChosenOutcome::Applied) => start_thumbnail(core, window),
            Some(ChosenOutcome::Rejected(message)) => core.show(window, PopupKind::Error(message)),
            _ => {}
        }
    });
}

/// Decodes one frame of the new input off the UI thread and puts it on the file card. The card is
/// emptied first, so the previous file's picture never stays under the new name; a picture that
/// arrives after another input was chosen is dropped. Without one (a decoder refusing, a
/// timeout) the card stays empty: the picture is a courtesy, never an error.
fn start_thumbnail(core: &Arc<Core>, window: &MainWindow) {
    window.set_has_thumbnail(false);
    let Some((generation, video)) = core.state().thumbnail_request() else {
        return;
    };
    let thread_core = Arc::clone(core);
    let spawned = spawn("thumbnail", move || {
        let result = catch_unwind(AssertUnwindSafe(|| {
            thread_core.engine.thumbnail(&video, &thread_core.leaving)
        }));
        post(&thread_core, move |core, window| {
            if !core.state().thumbnail_is_current(generation) {
                return;
            }
            match result {
                Ok(Ok(shot)) => {
                    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        &shot.rgba,
                        shot.width,
                        shot.height,
                    );
                    window.set_thumbnail(slint::Image::from_rgba8(pixels));
                    window.set_has_thumbnail(true);
                }
                Ok(Err(e)) => log::debug!("No thumbnail for the input: error={e}"),
                Err(payload) => {
                    panic_error("the thumbnail", payload.as_ref());
                }
            }
        });
    });
    match spawned {
        Ok(handle) => {
            let mut threads = lock(&core.thumbnails);
            threads.retain(|thread| !thread.is_finished());
            threads.push(handle);
        }
        Err(e) => log::warn!("Cannot start the thumbnail thread: error={e}"),
    }
}

/// Applies the chosen output on the UI thread (`.mp4` enforced by the state); `None` is a
/// cancel. Releases the button either way.
fn finish_output_pick(core: &Arc<Core>, chosen: Option<PathBuf>) {
    post(core, move |core, window| {
        let outcome = {
            let mut state = core.state();
            state.finish_pick(Control::SelectOutput);
            chosen.map(|path| state.apply_chosen_output(&path))
        };
        core.refresh(window);
        if let Some(ChosenOutcome::Rejected(message)) = outcome {
            core.show(window, PopupKind::Error(message));
        }
    });
}

/// What a picker's answer leads to: the file (or the cancel) is applied, or the window asks for
/// the path itself. The button stays held while the text field is open.
fn after_pick(core: &Arc<Core>, kind: Kind, pick: Pick) {
    let chosen = match pick {
        Pick::Chosen(path) => Some(path),
        Pick::Cancelled => None,
        Pick::NoPicker => {
            post(core, move |core, window| {
                core.show(window, PopupKind::PathEntry(kind));
            });
            return;
        }
    };
    match kind {
        Kind::Input => finish_input_pick(core, chosen),
        Kind::Output => finish_output_pick(core, chosen),
    }
}

/// The button a picker of this kind holds down.
fn pick_control(kind: Kind) -> Control {
    match kind {
        Kind::Input => Control::Open,
        Kind::Output => Control::SelectOutput,
    }
}

/// The input and output pickers. The button is held down from the click to the end of the
/// ffprobe check (or of the text field), so that a second click cannot open a second picker.
fn choose(core: &Arc<Core>, window: &MainWindow, kind: Kind) {
    let control = pick_control(kind);
    if !core.state().begin_pick(control) {
        return;
    }
    core.refresh(window);
    let thread_core = Arc::clone(core);
    let name = match kind {
        Kind::Input => "pick-input",
        Kind::Output => "pick-output",
    };
    let spawned = spawn(name, move || {
        // A picker that panics counts as a cancel: the button must never stay greyed out.
        let pick = catch_unwind(AssertUnwindSafe(|| match kind {
            Kind::Input => thread_core.host.pick_input(),
            Kind::Output => thread_core.host.pick_output(),
        }))
        .unwrap_or(Pick::Cancelled);
        after_pick(&thread_core, kind, pick);
    });
    if let Err(e) = spawned {
        log::error!("Cannot start the file picker: error={e}");
        core.state().finish_pick(control);
        core.refresh(window);
        core.show(
            window,
            PopupKind::Error(format!("cannot open the file picker: {e}")),
        );
    }
}

fn choose_input(core: &Arc<Core>, window: &MainWindow) {
    choose(core, window, Kind::Input);
}

fn choose_output(core: &Arc<Core>, window: &MainWindow) {
    choose(core, window, Kind::Output);
}

/// OK in the text field: an empty or relative path keeps the field open with the reason; a path
/// goes through the same checks as a picked file.
fn path_entry_ok(core: &Arc<Core>, window: &MainWindow) {
    let Some(kind) = lock(&core.popups).path_entry() else {
        return;
    };
    let path = match parse_typed_path(&window.get_path_entry_text()) {
        Ok(path) => path,
        Err(reason) => {
            window.set_path_entry_error(reason.into());
            return;
        }
    };
    core.close_popup(window);
    match kind {
        Kind::Output => finish_output_pick(core, Some(path)),
        Kind::Input => {
            // ffprobe off the UI thread, as for a picked file.
            let thread_core = Arc::clone(core);
            let chosen = path.clone();
            let spawned = spawn("check-input", move || {
                finish_input_pick(&thread_core, Some(chosen));
            });
            if let Err(e) = spawned {
                log::error!("Cannot start the video check: error={e}");
                core.state().finish_pick(Control::Open);
                core.refresh(window);
                core.show(
                    window,
                    PopupKind::Error(format!("cannot read the video: {e}")),
                );
            }
        }
    }
}

/// Cancel in the text field: nothing changes, the button is free again.
fn path_entry_cancel(core: &Arc<Core>, window: &MainWindow) {
    let Some(kind) = lock(&core.popups).path_entry() else {
        return;
    };
    core.close_popup(window);
    core.state().finish_pick(pick_control(kind));
    core.refresh(window);
}

/// The Start button.
fn start(core: &Arc<Core>, window: &MainWindow) {
    let (input, output) = {
        let state = core.state();
        (
            state.video().map(|video| video.file.clone()),
            state.output_path().to_string(),
        )
    };
    let Some(input) = input else {
        core.show(window, PopupKind::NoInput);
        return;
    };
    if output.is_empty() {
        core.show(window, PopupKind::NoOutput);
        return;
    }
    if !core.state().can_start() {
        return;
    }
    match std::fs::metadata(&output) {
        Ok(_) => core.show(window, PopupKind::Overwrite { input, output }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => begin_conversion(core, window),
        Err(e) => core.show(
            window,
            PopupKind::Error(format!("cannot access output file: {e}")),
        ),
    }
}

/// "Yes" to the overwrite question, if it still concerns the files on screen. A picker that was
/// open when the question came up can change the input or the destination under it; the answer
/// then belongs to a question about other files, and nothing is overwritten on its strength.
fn overwrite_yes(core: &Arc<Core>, window: &MainWindow) {
    let asked = match lock(&core.popups).current() {
        Some(PopupKind::Overwrite { input, output }) => Some((input.clone(), output.clone())),
        _ => None,
    };
    // Not the question on screen (a second click, a dialog that took its place): not ours.
    let Some((input, output)) = asked else {
        return;
    };
    core.close_popup(window);
    let unchanged = {
        let state = core.state();
        state.video().is_some_and(|video| video.file == input) && state.output_path() == output
    };
    if unchanged {
        begin_conversion(core, window);
        return;
    }
    log::info!("Ignoring the overwrite answer: the files changed since the question");
    core.state().view.status = "Status: The files changed, press Start again".into();
    core.refresh(window);
}

/// Sends what the encoding thread reports to the window.
struct WindowSink {
    core: Arc<Core>,
}

impl ProgressSink for WindowSink {
    fn bar(&self, fraction: f64) {
        post(&self.core, move |core, window| {
            let mut state = core.state();
            // Nothing after the end (the completion has rewritten the bar) nor after a Cancel.
            if !state.shows_progress() {
                return;
            }
            state.set_progress(fraction);
            let progress = state.view.progress;
            drop(state);
            window.set_progress(progress as f32);
        });
    }

    fn status(&self, text: &str) {
        let text = text.to_string();
        post(&self.core, move |core, window| {
            let mut state = core.state();
            if !state.shows_progress() {
                return;
            }
            state.view.status.clone_from(&text);
            drop(state);
            window.set_status(text.as_str().into());
        });
    }
}

/// Reads everything the encoding thread will be asked for, here, on the UI thread, while the
/// widgets are still the user's to change (E-15), then starts it.
fn begin_conversion(core: &Arc<Core>, window: &MainWindow) {
    let (job, video, bitrate, encoder, squeeze, cancel, done) = {
        let mut state = core.state();
        if !state.can_start() {
            return;
        }
        let Some(video) = state.video().cloned() else {
            return;
        };
        // Keep hardware encoders and scale the bitrate by profile.
        let input_bitrate = video.streams.first().map_or(0, |stream| stream.bitrate_int);
        let (bitrate, preset) =
            resolve_start_settings(&core.cfg, &state.view.quality, input_bitrate);
        let mut cfg = core.cfg.clone();
        cfg.video_preset = preset;
        let job = EncodeJob {
            cfg,
            input: PathBuf::from(&video.file),
            output: PathBuf::from(state.output_path()),
            ffmpeg: state.ffmpeg().clone(),
        };
        let encoder = parse_encoder_selection(&state.view.encoder_selected);
        let squeeze = state.view.squeeze;
        // Not "0%": ffmpeg produces no figure until the first frame leaves the filter graph.
        state.announce_start();
        let (cancel, done) = state.begin_encoding();
        (job, video, bitrate, encoder, squeeze, cancel, done)
    };
    core.refresh(window);

    let output = job.output.to_string_lossy().into_owned();
    let handler = GuiHandler::new(
        bitrate,
        encoder,
        squeeze,
        Some(&video),
        Some(Box::new(WindowSink {
            core: Arc::clone(core),
        })),
    );
    let thread_core = Arc::clone(core);
    let thread_done = done.clone();
    let thread_output = output.clone();
    let spawned = spawn("encode", move || {
        run_encoding(
            &thread_done,
            || {
                // A panic is a failed conversion, not a window locked for good.
                catch_unwind(AssertUnwindSafe(|| {
                    thread_core.engine.encode(&job, &handler, &cancel)
                }))
                .unwrap_or_else(|payload| Err(panic_error("the conversion", payload.as_ref())))
            },
            |result| {
                post(&thread_core, move |core, window| {
                    complete(core, window, result, &thread_output);
                });
            },
        );
    });
    if let Err(e) = spawned {
        log::error!("Cannot start the conversion thread: error={e}");
        done.release();
        complete(
            core,
            window,
            Err(Error::Plain(format!("cannot start the conversion: {e}"))),
            &output,
        );
    }
}

/// The single completion path, on the UI thread, once the pipeline has returned.
fn complete(
    core: &Arc<Core>,
    window: &MainWindow,
    result: Result<EncodeOutcome, Error>,
    output: &str,
) {
    let completion = core.state().complete_encoding(result, output);
    core.refresh(window);
    // Nothing runs any more: a "Cancel and quit" question still on screen is stale, and the
    // outcome would wait behind it. (Ignored: the window is leaving, nothing is left to show.)
    if !matches!(completion, Completion::Ignored) {
        let mut popups = lock(&core.popups);
        popups.drop_quit();
        apply_popup(window, popups.current());
    }
    match completion {
        // Shutting down, asked by the system (the loop is ending), or asked by the user.
        Completion::Ignored | Completion::Quit | Completion::Cancelled => {}
        Completion::Succeeded { message } => core.show(window, PopupKind::Done(message)),
        Completion::Failed {
            message,
            missing_ffmpeg,
        } => {
            if missing_ffmpeg {
                core.show(window, PopupKind::Prerequisite { retry: false });
            } else {
                core.show(window, PopupKind::Error(message));
            }
        }
    }
}

/// The Cancel button.
fn cancel(core: &Arc<Core>, window: &MainWindow) {
    core.state().request_cancel();
    core.refresh(window);
}

/// The Quit button: asks first during a conversion.
fn quit_pressed(core: &Arc<Core>, window: &MainWindow) {
    if core.state().is_encoding() {
        core.show(window, PopupKind::Quit);
    } else {
        leave(core, window, "quit button");
    }
}

/// Leaves for good: stops a running conversion and waits for it (the UI thread is held for at
/// most the grace period), then ends the loop.
fn leave(core: &Arc<Core>, window: &MainWindow, reason: &str) {
    // The window goes first, so that it does not stay on screen while the wait runs.
    if let Err(e) = window.hide() {
        log::warn!("Cannot hide the window: error={e}");
    }
    core.host.quit();
    core.shutdown(reason);
}

/// The Diagnostic button: the checks shell out to ffmpeg, so they run off the UI thread.
fn diagnostic(core: &Arc<Core>, window: &MainWindow) {
    let before = {
        let mut state = core.state();
        // Not beside a conversion or another report: both ask the GPU for the same thing.
        if !state.begin_diagnostic() {
            return;
        }
        let before = state.view.status.clone();
        state.view.status = RUNNING_DIAGNOSTIC.into();
        before
    };
    core.refresh(window);
    let thread_core = Arc::clone(core);
    let restore = before.clone();
    let spawned = spawn("diagnostic", move || {
        let health = catch_unwind(AssertUnwindSafe(|| {
            thread_core
                .engine
                .health_report(&thread_core.temp_dir, &thread_core.leaving)
        }))
        .unwrap_or_else(|payload| {
            format!(
                "The diagnostic stopped unexpectedly: {}\n",
                panic_error("the diagnostic", payload.as_ref())
            )
        });
        // The build goes first: it is the line that tells a maintainer which binary produced
        // everything below it. The log path is where a user finds what to attach to a bug
        // report.
        let log_line = match &thread_core.log_path {
            Some(path) => format!("\nLog file: {}\n", path.display()),
            None => "\nLog file: unavailable (diagnostics are being discarded)\n".to_string(),
        };
        let report = format!(
            "Superview build: {}\n\n{}{log_line}",
            thread_core.identity, health
        );
        post(&thread_core, move |core, window| {
            {
                // The line goes back to what it said before, and only if nothing has written
                // over "Running diagnostic..." since: a conversion started meanwhile owns it.
                let mut state = core.state();
                state.finish_diagnostic();
                if state.view.status == RUNNING_DIAGNOSTIC {
                    state.view.status = restore;
                }
            }
            core.refresh(window);
            core.show(window, PopupKind::Diagnostic(report));
        });
    });
    if let Err(e) = spawned {
        log::error!("Cannot start the diagnostic: error={e}");
        {
            let mut state = core.state();
            state.finish_diagnostic();
            if state.view.status == RUNNING_DIAGNOSTIC {
                state.view.status = before;
            }
        }
        core.refresh(window);
        core.show(
            window,
            PopupKind::Error(format!("cannot start the diagnostic: {e}")),
        );
    }
}

#[cfg(test)]
mod tests;
