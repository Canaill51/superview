//! The state of the window, with no dependency on Slint: what the user has chosen, whether a
//! conversion runs, and the view of all that (`ViewModel`) that the window copies into its
//! properties.
//!
//! Design choices:
//!
//! * The widgets are driven through the fields of [`ViewModel`]; a test reads the view there.
//! * Nothing here spawns a thread or opens a dialog. The video check is a closure the caller
//!   passes; the background hardware verification is a hook ([`AppState::set_verify_hook`]);
//!   the dialogs are described by what [`AppState::apply_chosen_input`] and
//!   [`AppState::complete_encoding`] return.
//! * The cancel channel and the `done` channel are an `Arc<AtomicBool>` and a [`Done`]. What an
//!   exit needs (cancel, wait, the shutting-down flag) lives in [`Lifecycle`], which the window
//!   shares: the close hook calls it without taking the lock on the state, which the completion
//!   path may hold.
//! * `GetLastHardwareAccelerationSummary` and `GetLastEncodingMetrics` are gone: the outcome of
//!   the conversion carries them to [`AppState::complete_encoding`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use superview_core::error::Error;
use superview_core::hardware::describe_hardware_acceleration_plan;
use superview_core::pipeline::{EncodeOutcome, PipelineUi};
use superview_core::probe::{EncoderProbeReport, apply_encoder_probe};
use superview_core::video::VideoSpecs;

use crate::format::{
    STARTING_ENCODER_STATUS, encoder_options_for, ensure_mp4_extension, format_encoding_status,
    format_results_panel, is_missing_ffmpeg_error, parse_encoder_selection,
};
use crate::prefs::{self, Preferences};

/// How long an exit waits for the encoding thread to unwind before giving up on it. The
/// slowest thing the thread can be inside is the pre-encode probe, which has a 15 s timeout of
/// its own and does not watch the cancel flag; ffmpeg itself is killed within milliseconds.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(20);

/// What the hardware line says when a verification could not be completed. v0.2.9 left the previous
/// line in place, which described a state the window had moved on from.
pub const HARDWARE_UNVERIFIED: &str = "Hardware: could not verify the encoder on this machine";

/// The controls whose enabled state the transitions drive. `Quit` exists in the tests only,
/// so that "Quit is in no list" is something a test can say: the window never reads it, since
/// the Quit button is never greyed out by a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Open,
    SelectOutput,
    Quality,
    Squeeze,
    Encoder,
    Diagnostic,
    #[cfg(test)]
    Quit,
}

#[cfg(test)]
const CONTROL_COUNT: usize = 7;
#[cfg(not(test))]
const CONTROL_COUNT: usize = 6;

/// Every control that changes what the next conversion will do, or that would run a GPU probe
/// beside a hardware encode (Diagnostic probes every hardware encoder: two probes on one GPU
/// beside a running encode can make a working encoder fail, E-14). Quit is deliberately not in
/// it: it cancels safely, and a user must always be able to leave.
pub fn locked() -> &'static [Control] {
    &[
        Control::Open,
        Control::SelectOutput,
        Control::Quality,
        Control::Squeeze,
        Control::Encoder,
        Control::Diagnostic,
    ]
}

/// Every control that has nothing to do without ffmpeg: [`locked`] minus Diagnostic, and the
/// difference is the point, since Diagnostic is the tool that reports "ffmpeg not found".
pub fn requires_ffmpeg() -> &'static [Control] {
    &[
        Control::Open,
        Control::SelectOutput,
        Control::Quality,
        Control::Squeeze,
        Control::Encoder,
    ]
}

/// What the window shows. The window copies it into its properties and holds no state of its
/// own.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewModel {
    pub status: String,
    pub results: String,
    pub hardware: String,
    /// File names only; the full paths stay in the state.
    pub input_name: String,
    pub output_name: String,
    pub start_enabled: bool,
    pub cancel_enabled: bool,
    enabled: [bool; CONTROL_COUNT],
    /// A file picker is open on this control. Kept apart from `enabled` so that no unlock
    /// (end of a conversion, ffmpeg found) can re-enable a control whose picker is still open.
    picking: [bool; CONTROL_COUNT],
    /// The progress bar, 0 to 1.
    pub progress: f64,
    pub bar: BarState,
    /// "h264 · 1440×1080 · 2m15s" for the input, "" before one is chosen.
    pub input_specs: String,
    /// "Fast" or "Balanced".
    pub quality: String,
    /// The "source already stretched" box.
    pub squeeze: bool,
    /// "Light" or "Dark".
    pub theme: String,
    pub encoder_options: Vec<String>,
    pub encoder_selected: String,
}

impl Default for ViewModel {
    fn default() -> Self {
        Self {
            status: "Status: Ready".into(),
            results: "Results: no completed run yet".into(),
            hardware: "Hardware: waiting for input video".into(),
            input_name: NO_FILE.into(),
            output_name: NO_FILE.into(),
            start_enabled: false,
            cancel_enabled: false,
            enabled: [true; CONTROL_COUNT],
            picking: [false; CONTROL_COUNT],
            progress: 0.0,
            bar: BarState::Idle,
            input_specs: String::new(),
            quality: prefs::DEFAULT_QUALITY_PROFILE.into(),
            squeeze: false,
            theme: prefs::DEFAULT_THEME.into(),
            encoder_options: Vec::new(),
            encoder_selected: String::new(),
        }
    }
}

/// What a file label says before a choice.
pub const NO_FILE: &str = "No file selected";

impl ViewModel {
    /// Whether the control can be used: not locked, and no picker open on it.
    pub fn is_enabled(&self, control: Control) -> bool {
        self.enabled[control as usize] && !self.picking[control as usize]
    }

    /// Applies one decision to a whole group. Round-tripping
    /// it (false then true) leaves every control as it started, which is the property P-01
    /// broke.
    pub fn set_enabled(&mut self, controls: &[Control], enabled: bool) {
        for control in controls {
            self.enabled[*control as usize] = enabled;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Released once by the encoding thread when it has nothing left to unwind. An exit waits on it.
#[derive(Clone, Default)]
pub struct Done(Arc<(Mutex<bool>, Condvar)>);

impl Done {
    /// Idempotent.
    pub fn release(&self) {
        let (flag, condvar) = &*self.0;
        *lock(flag) = true;
        condvar.notify_all();
    }

    pub fn is_released(&self) -> bool {
        *lock(&self.0.0)
    }

    /// Waits until released or until `timeout` has passed; true when released.
    pub fn wait(&self, timeout: Duration) -> bool {
        let (flag, condvar) = &*self.0;
        let guard = lock(flag);
        let (guard, _) = condvar
            .wait_timeout_while(guard, timeout, |released| !*released)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

struct LifecycleInner {
    /// Some exactly while a conversion runs and has not been cancelled yet. `request_cancel`
    /// is the only place that takes it, so a second cancellation is a no-op.
    cancel: Option<Arc<AtomicBool>>,
    /// Some exactly while an encoding thread exists.
    done: Option<Done>,
    /// Zero or `None` means [`SHUTDOWN_GRACE`]; tests set it short.
    grace: Option<Duration>,
}

/// What every exit goes through: it asks a running conversion to stop and waits, within a
/// bound, for its thread to have unwound (ffmpeg killed and reaped, the partial file removed,
/// the session directory gone). Safe to call any number of times, from any thread.
///
/// The wait happens once. After it has run out, every later call returns false at once: the
/// close hook and the line after the event loop would otherwise each spend the whole grace
/// period (40 s instead of the 20 s documented).
pub struct Lifecycle {
    inner: Mutex<LifecycleInner>,
    /// Set by `shutdown`, read by the completion path, which may run after the window is gone.
    shutting_down: AtomicBool,
    gave_up: AtomicBool,
}

impl Lifecycle {
    fn new() -> Self {
        Self {
            inner: Mutex::new(LifecycleInner {
                cancel: None,
                done: None,
                grace: None,
            }),
            shutting_down: AtomicBool::new(false),
            gave_up: AtomicBool::new(false),
        }
    }

    fn begin(&self, cancel: Arc<AtomicBool>, done: Done) {
        let mut inner = lock(&self.inner);
        inner.cancel = Some(cancel);
        inner.done = Some(done);
    }

    fn clear(&self) {
        let mut inner = lock(&self.inner);
        inner.cancel = None;
        inner.done = None;
    }

    /// Raises the cancel flag of the running conversion, once. True when it cancelled anything.
    fn cancel(&self) -> bool {
        match lock(&self.inner).cancel.take() {
            Some(flag) => {
                flag.store(true, Ordering::Release);
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    fn has_cancel(&self) -> bool {
        lock(&self.inner).cancel.is_some()
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    /// Shortens the wait; the window never does, only tests.
    #[cfg(test)]
    pub fn set_grace(&self, grace: Duration) {
        lock(&self.inner).grace = Some(grace);
    }

    /// Asks a running conversion to stop and waits for it. Returns whether it got there: true
    /// when nothing runs or the thread unwound, false when the wait ran out (now or before).
    pub fn shutdown(&self, reason: &str) -> bool {
        self.shutting_down.store(true, Ordering::Release);
        let (done, grace) = {
            let inner = lock(&self.inner);
            let grace = inner
                .grace
                .filter(|g| !g.is_zero())
                .unwrap_or(SHUTDOWN_GRACE);
            (inner.done.clone(), grace)
        };
        let Some(done) = done else { return true };
        if done.is_released() {
            return true;
        }
        if self.gave_up.load(Ordering::Acquire) {
            return false;
        }

        self.cancel();
        log::info!("Waiting for the conversion to unwind before quitting: reason={reason}");
        if done.wait(grace) {
            log::info!("Conversion unwound, quitting: reason={reason}");
            true
        } else {
            self.gave_up.store(true, Ordering::Release);
            log::error!(
                "Quitting without waiting any longer: the conversion did not unwind in time: reason={reason} waited={grace:?}"
            );
            false
        }
    }
}

/// Releases `done` when dropped, so that a panic in the pipeline cannot leave an exit waiting.
struct ReleaseOnDrop<'a>(&'a Done);

impl Drop for ReleaseOnDrop<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// The body of the encoding thread: runs `encode`, releases `done`, then hands the result to
/// `complete` (which the window posts to its event loop).
///
/// `done` is released *before* the completion, not by a drop at the end. Once the event loop
/// has drained, the completion may run inline on this thread; a system stop then goes to quit,
/// to the window's close hook, to `shutdown`, which waits on `done` -- that only this thread
/// would release: twenty seconds of a process waiting for itself. When `encode` returns, the
/// pipeline has killed and reaped ffmpeg and removed the partial file and the session
/// directory, which is all `done` promises. The guard keeps the old guarantee for a panic.
pub fn run_encoding<R>(done: &Done, encode: impl FnOnce() -> R, complete: impl FnOnce(R)) {
    let _release_on_unwind = ReleaseOnDrop(done);
    let result = encode();
    done.release();
    complete(result);
}

/// What the background verification of the hardware path needs, captured on the UI side.
#[derive(Debug, Clone)]
pub struct VerificationRequest {
    /// The line it is computed for; see [`AppState::apply_verified_hardware_line`].
    pub generation: u64,
    pub ffmpeg: HashMap<String, String>,
    pub video: VideoSpecs,
    pub squeeze: bool,
    /// The encoder name ("" = the source's codec).
    pub selection: String,
    /// The quality profile; the preset the conversion will pass comes from it.
    pub quality: String,
}

/// What a file the picker returned came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChosenOutcome {
    /// The file is now the input or the output.
    Applied,
    /// A conversion was running: the file is dropped, nothing changes.
    Ignored,
    /// The file is refused; the text is for an error dialog. The status says so.
    Rejected(String),
}

/// What the progress bar says by its colour (DEC-58): the accent at rest, red while a conversion
/// goes, green once it is done, grey when it was stopped or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarState {
    #[default]
    Idle,
    Running,
    Done,
    Stopped,
}

/// What the window must show once a conversion ends (the dialog, if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// The application is shutting down: nothing was touched.
    Ignored,
    /// The system asked the application to stop: quit, with no dialog.
    Quit,
    /// A "Done" dialog with this message.
    Succeeded { message: String },
    /// The user asked for this: no dialog.
    Cancelled,
    /// An error dialog; the prerequisite dialog with the install link when `missing_ffmpeg`.
    Failed {
        message: String,
        missing_ffmpeg: bool,
    },
}

/// Everything about a conversion session that outlives one callback, with the view those
/// transitions drive. Meant to be used from the UI thread; the one cross-thread part is
/// [`Lifecycle`].
pub struct AppState {
    pub view: ViewModel,

    video: Option<VideoSpecs>,
    output_path: String,

    ffmpeg: HashMap<String, String>,
    ffmpeg_available: bool,

    /// What the advertised encoders answered when actually asked to encode a frame. `None`
    /// until the probe finishes; the hardware line says so rather than claiming a path it has
    /// not verified.
    encoder_probe: Option<EncoderProbeReport>,
    /// A startup probe that landed while a conversion ran, applied when it ends.
    pending_probe: Option<EncoderProbeReport>,

    prefs: Preferences,
    prefs_path: Option<PathBuf>,

    /// True while the codec dropdown is being set by the program rather than by the user: at
    /// startup, and whenever the list is rebuilt after a probe. The user's saved choice must not
    /// be overwritten by a fallback the program made.
    restoring_selection: bool,

    /// Counts refreshes of the hardware line, so a slow verification cannot overwrite a newer
    /// answer.
    hardware_status_gen: u64,
    /// Counts the inputs chosen; the thumbnail of an older one is stale.
    thumbnail_gen: u64,

    encoding: bool,
    /// True from the Cancel of a running conversion until it ends: a progress
    /// report still queued then must not rewrite "Cancelling...".
    cancelling: bool,
    /// True while the Diagnostic report is being computed: it asks every hardware encoder to
    /// encode a frame, which a conversion must not do beside it (E-14), and the other way round.
    diagnosing: bool,
    lifecycle: Arc<Lifecycle>,

    /// What ends the application; called when the system asked the process to stop.
    quit: Option<Box<dyn FnMut() + Send>>,
    /// Receives the request for a background verification of the hardware path.
    on_verify: Option<Box<dyn FnMut(VerificationRequest) + Send>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(Preferences::default(), None)
    }
}

/// The last component of a path as a label, or the whole text when it has none.
fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The error at the bottom of a `FailedAfterOutput` chain.
fn root_cause(err: &Error) -> &Error {
    match err {
        Error::FailedAfterOutput(inner) => root_cause(inner),
        other => other,
    }
}

impl AppState {
    /// A state with the user's saved choices; they are written back to `prefs_path` (when
    /// given) on every change the user makes. ffmpeg is assumed available until told otherwise.
    pub fn new(prefs: Preferences, prefs_path: Option<PathBuf>) -> Self {
        let prefs = prefs.sanitized();
        let view = ViewModel {
            quality: prefs.quality_profile.clone(),
            squeeze: prefs.squeeze_source,
            theme: prefs.theme.clone(),
            ..ViewModel::default()
        };
        Self {
            view,
            video: None,
            output_path: String::new(),
            ffmpeg: HashMap::new(),
            ffmpeg_available: true,
            encoder_probe: None,
            pending_probe: None,
            prefs,
            prefs_path,
            restoring_selection: false,
            hardware_status_gen: 0,
            thumbnail_gen: 0,
            encoding: false,
            cancelling: false,
            diagnosing: false,
            lifecycle: Arc::new(Lifecycle::new()),
            quit: None,
            on_verify: None,
        }
    }

    pub fn set_quit_hook(&mut self, quit: impl FnMut() + Send + 'static) {
        self.quit = Some(Box::new(quit));
    }

    pub fn set_verify_hook(&mut self, hook: impl FnMut(VerificationRequest) + Send + 'static) {
        self.on_verify = Some(Box::new(hook));
    }

    /// The part of the state an exit needs, shareable without the state itself.
    pub fn lifecycle(&self) -> Arc<Lifecycle> {
        Arc::clone(&self.lifecycle)
    }

    pub fn video(&self) -> Option<&VideoSpecs> {
        self.video.as_ref()
    }

    pub fn output_path(&self) -> &str {
        &self.output_path
    }

    pub fn ffmpeg(&self) -> &HashMap<String, String> {
        &self.ffmpeg
    }

    pub fn is_ffmpeg_available(&self) -> bool {
        self.ffmpeg_available
    }

    /// Records that ffmpeg is missing: nothing to start, and what needs ffmpeg is greyed out.
    pub fn set_ffmpeg_unavailable(&mut self) {
        self.ffmpeg_available = false;
        self.apply_ffmpeg_availability();
        self.refresh_start();
    }

    pub fn is_encoding(&self) -> bool {
        self.encoding
    }

    /// Whether progress still means something: a conversion runs and nobody has stopped it.
    pub fn shows_progress(&self) -> bool {
        self.encoding && !self.cancelling
    }

    /// Enables or disables what needs ffmpeg according to whether the last probe found it: at
    /// startup, and after a successful retry.
    pub fn apply_ffmpeg_availability(&mut self) {
        self.view
            .set_enabled(requires_ffmpeg(), self.ffmpeg_available);
    }

    /// Whether the Start button should be available. All five conditions matter: no input, no
    /// destination, no ffmpeg, a conversion already running, or the Diagnostic running each
    /// make starting meaningless.
    pub fn can_start(&self) -> bool {
        self.video.is_some()
            && !self.output_path.is_empty()
            && self.ffmpeg_available
            && !self.encoding
            && !self.diagnosing
    }

    /// Marks the Diagnostic as running: Start and Diagnostic are held until
    /// [`AppState::finish_diagnostic`]. False when a conversion or another report is running,
    /// and nothing is changed.
    pub fn begin_diagnostic(&mut self) -> bool {
        if self.encoding || self.diagnosing {
            return false;
        }
        self.diagnosing = true;
        self.view.set_enabled(&[Control::Diagnostic], false);
        self.refresh_start();
        true
    }

    /// Gives Start and Diagnostic back once the report has been shown, or could not be made.
    pub fn finish_diagnostic(&mut self) {
        self.diagnosing = false;
        self.view
            .set_enabled(&[Control::Diagnostic], !self.encoding);
        self.refresh_start();
        // Changes made while the report ran asked for a verification that was held back.
        self.verify_hardware_in_background();
    }

    pub fn refresh_start(&mut self) {
        self.view.start_enabled = self.can_start();
    }

    /// Records a validated input video and refreshes what depends on it.
    pub fn set_input(&mut self, video: VideoSpecs) {
        self.video = Some(video);
        self.refresh_hardware_status();
        self.refresh_start();
    }

    /// What the thumbnail thread needs: the generation it is for and the file's specs. `None`
    /// before an input is chosen.
    pub fn thumbnail_request(&self) -> Option<(u64, VideoSpecs)> {
        self.video.clone().map(|video| (self.thumbnail_gen, video))
    }

    /// Whether a picture computed for `generation` still belongs to the input: a new input
    /// bumps the generation, so a slow picture of the previous file is dropped.
    pub fn thumbnail_is_current(&self, generation: u64) -> bool {
        self.video.is_some() && generation == self.thumbnail_gen
    }

    /// Records the destination and refreshes what depends on it.
    pub fn set_output(&mut self, path: &str) {
        self.output_path = path.to_string();
        self.refresh_start();
    }

    /// Marks a picker as open on `control` (`Open` or `SelectOutput`): the control is held down
    /// so that a second click cannot open a second picker. False when one is already open or
    /// when the control is locked.
    pub fn begin_pick(&mut self, control: Control) -> bool {
        if !self.view.is_enabled(control) {
            return false;
        }
        self.view.picking[control as usize] = true;
        true
    }

    /// Releases the control a picker held. A lock that came in the meantime (a conversion
    /// started) still holds it.
    pub fn finish_pick(&mut self, control: Control) {
        self.view.picking[control as usize] = false;
    }

    /// Reports, and logs, a picker result that lands while a conversion runs.
    ///
    /// The pickers run off the UI thread and the window stays interactive meanwhile, so a user
    /// can press Start and then validate the picker. Its result used to be applied anyway: the
    /// video replaced under the running encode, the status overwritten, a GPU probe started
    /// beside a hardware encode. It is dropped; the user can choose again after.
    fn chosen_file_arrives_too_late(&self, kind: &str, path: &Path) -> bool {
        if !self.is_encoding() {
            return false;
        }
        log::info!(
            "Ignoring a file chosen during a conversion: kind={kind} path={}",
            path.display()
        );
        true
    }

    /// Validates the file the input picker returned with `check` (ffprobe, in the real window)
    /// and makes it the input, unless a conversion has started since the picker opened.
    ///
    /// A path that is not valid UTF-8 is refused with a message of its own: the video check
    /// takes text, and a lossy conversion would check a different file than the one chosen.
    pub fn apply_chosen_input(
        &mut self,
        path: &Path,
        check: impl FnOnce(&str) -> Result<VideoSpecs, Error>,
    ) -> ChosenOutcome {
        if self.chosen_file_arrives_too_late("input", path) {
            return ChosenOutcome::Ignored;
        }
        let checked = match path.to_str() {
            Some(text) => check(text).map_err(|e| e.to_string()),
            None => Err(format!(
                "the input file path is not valid UTF-8 and cannot be used: {}",
                path.display()
            )),
        };
        match checked {
            Err(message) => {
                self.view.status = "Status: Invalid input".into();
                ChosenOutcome::Rejected(message)
            }
            Ok(video) => {
                self.view.input_name = base_name(&video.file);
                self.view.input_specs = crate::format::input_specs(&video);
                self.thumbnail_gen += 1;
                self.view.bar = BarState::Idle;
                self.view.progress = 0.0;
                self.set_input(video);
                self.verify_hardware_in_background();
                self.view.status = "Status: Input loaded".into();
                ChosenOutcome::Applied
            }
        }
    }

    /// Makes the path the output picker returned the destination, `.mp4` enforced, unless a
    /// conversion has started since the picker opened. A path that is not valid UTF-8 is
    /// refused, as for the input.
    pub fn apply_chosen_output(&mut self, path: &Path) -> ChosenOutcome {
        if self.chosen_file_arrives_too_late("output", path) {
            return ChosenOutcome::Ignored;
        }
        let Some(text) = path.to_str() else {
            self.view.status = "Status: Invalid output".into();
            return ChosenOutcome::Rejected(format!(
                "the output file path is not valid UTF-8 and cannot be used: {}",
                path.display()
            ));
        };
        let path = ensure_mp4_extension(text);
        self.set_output(&path);
        self.view.output_name = base_name(&path);
        self.view.status = "Status: Output selected".into();
        ChosenOutcome::Applied
    }

    /// Records a successful ffmpeg probe.
    pub fn set_ffmpeg(&mut self, ffmpeg: HashMap<String, String>) {
        self.ffmpeg = ffmpeg;
        self.ffmpeg_available = true;
    }

    /// Rewrites the line describing the path the next conversion will take.
    ///
    /// Any refresh invalidates a verification still in flight: the answer it carries was
    /// computed for the state this call has just replaced. (Every refresh moves the generation on,
    /// which can only drop more stale answers.)
    pub fn refresh_hardware_status(&mut self) {
        self.hardware_status_gen += 1;
        if !self.ffmpeg_available {
            self.view.hardware = "Hardware: ffmpeg unavailable".into();
            return;
        }
        if self.encoder_probe.is_none() {
            self.view.hardware = "Hardware: checking which encoders this machine accepts...".into();
            return;
        }
        let selection = parse_encoder_selection(&self.view.encoder_selected);
        self.view.hardware =
            describe_hardware_acceleration_plan(&self.ffmpeg, self.video.as_ref(), &selection);
    }

    /// Asks, through the hook, for the planned hardware encoder to be asked whether it will
    /// take the frame this file will actually produce. The answer comes back to
    /// [`AppState::apply_verified_hardware_line`].
    ///
    /// Separate from the synchronous refresh, deliberately: that one is called from callbacks
    /// that must not wait. This is called where the *inputs* to the verdict change: the file,
    /// the squeeze box, the codec, and the startup probe that decides which encoders are
    /// eligible.
    pub fn verify_hardware_in_background(&mut self) {
        let Some(video) = self.video.clone() else {
            return;
        };
        // A verification asks the GPU to encode a frame: not beside a conversion that uses it
        // (E-14). The inputs are checked again at the end of the conversion by the next change.
        if !self.ffmpeg_available
            || self.encoder_probe.is_none()
            || self.encoding
            || self.diagnosing
        {
            return;
        }
        let request = VerificationRequest {
            generation: self.hardware_status_gen,
            ffmpeg: self.ffmpeg.clone(),
            video,
            squeeze: self.view.squeeze,
            selection: parse_encoder_selection(&self.view.encoder_selected),
            quality: self.view.quality.clone(),
        };
        if let Some(hook) = self.on_verify.as_mut() {
            hook(request);
        }
    }

    /// Writes a verified line, unless the state it was computed for has moved on. The
    /// generation is what makes a slow verdict harmless: a user who ticks the squeeze box twice
    /// in a second starts two verifications, and the first must not land after the second.
    pub fn apply_verified_hardware_line(&mut self, generation: u64, text: &str) {
        if generation != self.hardware_status_gen {
            return;
        }
        self.view.hardware = text.to_string();
    }

    /// A verification that could not be completed (its thread failed). The line goes to a
    /// neutral text instead of staying on a plan nobody verified -- unless the state has moved
    /// on, in which case the newer line is left alone.
    pub fn apply_verification_failed(&mut self, generation: u64) {
        self.apply_verified_hardware_line(generation, HARDWARE_UNVERIFIED);
    }

    /// Records the probe verdicts and drops the encoders that refused the work from the
    /// capability map. From here on every question asked of `ffmpeg` is answered from what this
    /// machine did, not from what the binary was compiled with.
    ///
    /// Returns false when a conversion is running: the verdicts are kept for its end, since
    /// rebuilding the codec list and verifying the hardware path would run beside the encode.
    pub fn set_encoder_probe(&mut self, report: EncoderProbeReport) -> bool {
        if self.encoding {
            self.pending_probe = Some(report);
            return false;
        }
        self.ffmpeg = apply_encoder_probe(&self.ffmpeg, Some(&report));
        self.encoder_probe = Some(report);
        true
    }

    /// Moves the window between its idle and busy shapes. One list of controls, walked in both
    /// directions: two hand-written sequences had drifted apart (P-01).
    pub fn set_encoding(&mut self, in_progress: bool) {
        self.encoding = in_progress;
        self.cancelling = false;
        self.view.set_enabled(locked(), !in_progress);
        if in_progress {
            self.view.progress = 0.0;
            self.view.bar = BarState::Running;
            self.view.start_enabled = false;
            self.view.cancel_enabled = true;
            return;
        }
        self.view.cancel_enabled = false;
        self.refresh_start();
    }

    /// Says the encoder is starting (see [`STARTING_ENCODER_STATUS`]); not "0%", since no
    /// figure exists until the first frame.
    pub fn announce_start(&mut self) {
        self.view.status = STARTING_ENCODER_STATUS.into();
    }

    /// Marks a conversion as started and returns what the encoding thread keeps: the flag that
    /// stops it and the [`Done`] it releases when it has unwound. The flag is *returned* rather
    /// than read back from the state, so a cancellation cannot hand the pipeline a missing one
    /// (P-02).
    pub fn begin_encoding(&mut self) -> (Arc<AtomicBool>, Done) {
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Done::default();
        self.lifecycle.begin(Arc::clone(&cancel), done.clone());
        // A verification computed before Start describes a line the conversion is about to
        // replace: it must not land during or after it.
        self.hardware_status_gen += 1;
        self.set_encoding(true);
        (cancel, done)
    }

    /// Returns the window to its idle shape once a conversion ends, however it ended.
    pub fn finish_encoding(&mut self) {
        self.lifecycle.clear();
        // Still "encoding" while the late probe is applied, so that the refresh below cannot
        // start a verification: the one it would ask for is stale on arrival.
        if let Some(report) = self.pending_probe.take() {
            // The hardware line is the conversion's summary: it stays.
            let summary = std::mem::take(&mut self.view.hardware);
            self.ffmpeg = apply_encoder_probe(&self.ffmpeg, Some(&report));
            self.encoder_probe = Some(report);
            self.refresh_encoder_options();
            self.hardware_status_gen += 1;
            self.view.hardware = summary;
        }
        self.set_encoding(false);
    }

    /// The single completion path, run on the UI thread once the pipeline has returned.
    ///
    /// Four outcomes. A cancellation used to land in the failure branch, so a user who pressed
    /// Cancel read "Status: Failed" and then dismissed an error dialog saying "encoding
    /// interrupted by user".
    pub fn complete_encoding(
        &mut self,
        result: Result<EncodeOutcome, Error>,
        output: &str,
    ) -> Completion {
        // The window is on its way out: the completion may run inline on the encoding thread,
        // against a window with nothing left to show a dialog in.
        if self.lifecycle.is_shutting_down() {
            return Completion::Ignored;
        }

        // A system signal is an order, not a failure to report. Holding the window open would
        // only delay the SIGKILL that follows, which would leave the working file and the
        // session directory behind. The pipeline has already unwound both.
        if let Err(err) = &result
            && matches!(root_cause(err), Error::StoppedBySignal(_))
        {
            log::error!("Quitting: the system asked the application to stop: error={err}");
            if let Some(quit) = self.quit.as_mut() {
                quit();
            }
            return Completion::Quit;
        }

        // The line is rewritten here, so a verification still in flight is stale. After a
        // success it is the path that encoded; after a failure or a cancellation it is emptied
        // not left on a plan nobody ran.
        self.hardware_status_gen += 1;
        self.view.hardware = match &result {
            Ok(outcome) => outcome.summary.hardware_acceleration.clone(),
            Err(_) => String::new(),
        };
        self.finish_encoding();

        match result {
            Ok(outcome) => {
                self.set_status_and_results(
                    "Status: Completed",
                    &format_results_panel(Some(&outcome.metrics)),
                );
                self.view.progress = 1.0;
                self.view.bar = BarState::Done;
                Completion::Succeeded {
                    message: format!("Transform complete. Output file:\n{output}"),
                }
            }
            Err(err) if matches!(root_cause(&err), Error::Cancelled) => {
                // The user asked for this: nothing to report, and certainly not an error.
                self.view.bar = BarState::Stopped;
                self.set_status_and_results("Status: Cancelled", "Results: last run cancelled");
                Completion::Cancelled
            }
            Err(err) => {
                self.view.bar = BarState::Stopped;
                self.set_status_and_results("Status: Failed", "Results: last run failed");
                Completion::Failed {
                    missing_ffmpeg: is_missing_ffmpeg_error(&err),
                    message: err.to_string(),
                }
            }
        }
    }

    fn set_status_and_results(&mut self, status: &str, results: &str) {
        self.view.status = status.into();
        self.view.results = results.into();
    }

    /// What every exit goes through; see [`Lifecycle::shutdown`]. The window calls the
    /// lifecycle itself, without taking the lock on the state; this stays for the tests.
    #[cfg(test)]
    pub fn shutdown(&self, reason: &str) -> bool {
        self.lifecycle.shutdown(reason)
    }

    #[cfg(test)]
    pub fn set_shutdown_grace(&self, grace: Duration) {
        self.lifecycle.set_grace(grace);
    }

    /// Stops a running conversion. The only place that raises the flag, and it takes the flag
    /// in the same breath, so calling it twice -- the Cancel button and then the window close
    /// -- is harmless. Returns whether it actually cancelled anything.
    pub fn request_cancel(&mut self) -> bool {
        if !self.encoding || !self.lifecycle.cancel() {
            return false;
        }
        self.cancelling = true;
        self.view.bar = BarState::Stopped;
        self.view.status = "Status: Cancelling...".into();
        true
    }

    /// Rebuilds the codec dropdown from the current encoder list: at startup, after the probe
    /// has dropped the encoders that refused to work, and after a successful ffmpeg retry.
    ///
    /// The selection it ends on is the saved preference when offered, else the current one when
    /// still offered, else the first option. The preference comes first because it is the
    /// user's last choice by construction, while the current selection may be a fallback the
    /// program made. Whatever it picks is set under `restoring_selection`, so that it is not
    /// persisted as if the user had chosen it.
    pub fn refresh_encoder_options(&mut self) {
        let options = encoder_options_for(self.ffmpeg.get("encoders").map_or("", String::as_str));

        let mut wanted = self.prefs.encoder_selection.clone();
        if !options.contains(&wanted) {
            wanted.clone_from(&self.view.encoder_selected);
        }
        if !options.contains(&wanted) {
            wanted.clone_from(&options[0]);
        }
        self.view.encoder_options = options;

        self.restoring_selection = true;
        self.encoder_changed(&wanted);
        self.restoring_selection = false;
    }

    /// The codec dropdown changed. The preference is saved only for a choice the user made; a
    /// selection the program restored is not one.
    pub fn encoder_changed(&mut self, selection: &str) {
        self.view.encoder_selected = selection.to_string();
        if !self.restoring_selection {
            self.prefs.encoder_selection = selection.to_string();
            self.save_prefs();
        }
        self.refresh_hardware_status();
        self.verify_hardware_in_background();
    }

    /// The quality dropdown changed.
    pub fn quality_changed(&mut self, profile: &str) {
        self.view.quality = profile.to_string();
        self.prefs.quality_profile = profile.to_string();
        self.save_prefs();
        // The profile picks the preset, and the driver is asked with the conversion's own.
        self.verify_hardware_in_background();
    }

    /// The "source already stretched" box changed. The output frame changes width with it, and
    /// a hardware encoder refuses on width: the line is recomputed, not just kept.
    pub fn squeeze_changed(&mut self, checked: bool) {
        self.view.squeeze = checked;
        self.prefs.squeeze_source = checked;
        self.save_prefs();
        self.refresh_hardware_status();
        self.verify_hardware_in_background();
    }

    /// The Theme menu changed. Nothing else depends on it, so nothing is recomputed.
    pub fn theme_changed(&mut self, theme: &str) {
        self.view.theme = theme.to_string();
        self.prefs.theme = theme.to_string();
        self.save_prefs();
    }

    fn save_prefs(&self) {
        if let Some(path) = &self.prefs_path
            && let Err(e) = prefs::save(path, &self.prefs)
        {
            log::warn!(
                "Cannot save the preferences: path={} error={e}",
                path.display()
            );
        }
    }

    /// The progress bar, from a fraction (clamped to 0..1).
    pub fn set_progress(&mut self, fraction: f64) {
        // NaN (a division by a zero duration, say) reads as no progress.
        self.view.progress = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
    }
}

/// Where the encoding thread sends what the window must show. The window implements it by
/// posting to its event loop.
pub trait ProgressSink: Send + Sync {
    /// The bar, as a fraction in 0..1.
    fn bar(&self, fraction: f64);
    /// The status line.
    fn status(&self, text: &str);
}

/// What the pipeline asks of the window.
///
/// Every choice it answers for is a plain value captured on the UI thread when the conversion
/// starts. Reading the codec dropdown from the encoding thread was a data race, and an encoder
/// that could differ from the one on screen when Start was pressed (E-15).
pub struct GuiHandler {
    bitrate: i64,
    encoder_name: String,
    squeeze: bool,
    has_video_stream: bool,
    sink: Option<Box<dyn ProgressSink>>,
}

impl GuiHandler {
    pub fn new(
        bitrate: i64,
        encoder_name: String,
        squeeze: bool,
        video: Option<&VideoSpecs>,
        sink: Option<Box<dyn ProgressSink>>,
    ) -> Self {
        Self {
            bitrate,
            encoder_name,
            squeeze,
            has_video_stream: video.is_some_and(|v| !v.streams.is_empty()),
            sink,
        }
    }
}

impl PipelineUi for GuiHandler {
    fn get_encoder(&self) -> String {
        // Nothing without a loaded video: the pipeline reads that as "no explicit choice".
        if self.has_video_stream {
            self.encoder_name.clone()
        } else {
            String::new()
        }
    }

    fn get_bitrate(&self) -> Result<i64, Error> {
        Ok(self.bitrate)
    }

    fn get_squeeze(&self) -> bool {
        self.squeeze
    }

    fn show_progress(&self, percent: f64) {
        if let Some(sink) = &self.sink {
            sink.bar(percent / 100.0);
        }
    }

    fn show_progress_detail(&self, percent: f64, remaining: Duration) {
        if let Some(sink) = &self.sink {
            sink.status(&format_encoding_status(percent, remaining));
        }
    }
}

#[cfg(test)]
mod tests;
