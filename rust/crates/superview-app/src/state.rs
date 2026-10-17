//! The state of the window, with no dependency on Slint: what the user has chosen, whether a
//! conversion runs, and the view of all that (`ViewModel`) that the window copies into its
//! properties. Port of Go's `appState` (`cmd/superview/gui_main.go`), method for method; the
//! Go test that guards each transition is named beside its Rust test.
//!
//! What differs from Go, on purpose:
//!
//! * The widgets Go drives are the fields of [`ViewModel`]; a test reads the view as Go's tests
//!   read `widget.Label.Text`.
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

/// What the hardware line says when a verification could not be completed. Go left the previous
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
/// it: it cancels safely, and a user must always be able to leave. Go: `windowControls.locked`.
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
/// difference is the point, since Diagnostic is the tool that reports "ffmpeg not found". Go:
/// `windowControls.requiresFFmpeg`.
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
    /// File names only (Go's `filepath.Base`); the full paths stay in the state.
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
    /// "Fast" or "Balanced".
    pub quality: String,
    /// The "source already stretched" box.
    pub squeeze: bool,
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
            quality: prefs::DEFAULT_QUALITY_PROFILE.into(),
            squeeze: false,
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

    /// Applies one decision to a whole group (Go: `encodingControls.setEnabled`). Round-tripping
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

/// Released once by the encoding thread when it has nothing left to unwind (Go's `done`
/// channel). An exit waits on it.
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

    prefs: Preferences,
    prefs_path: Option<PathBuf>,

    /// True while the codec dropdown is being set by the program rather than by the user: at
    /// startup, and whenever the list is rebuilt after a probe. The user's saved choice must not
    /// be overwritten by a fallback the program made.
    restoring_selection: bool,

    /// Counts refreshes of the hardware line, so a slow verification cannot overwrite a newer
    /// answer.
    hardware_status_gen: u64,

    encoding: bool,
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

/// The error at the bottom of a `FailedAfterOutput` chain, as `errors.Is` looks through `%w`.
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
            ..ViewModel::default()
        };
        Self {
            view,
            video: None,
            output_path: String::new(),
            ffmpeg: HashMap::new(),
            ffmpeg_available: true,
            encoder_probe: None,
            prefs,
            prefs_path,
            restoring_selection: false,
            hardware_status_gen: 0,
            encoding: false,
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

    /// Enables or disables what needs ffmpeg according to whether the last probe found it: at
    /// startup, and after a successful retry.
    pub fn apply_ffmpeg_availability(&mut self) {
        self.view
            .set_enabled(requires_ffmpeg(), self.ffmpeg_available);
    }

    /// Whether the Start button should be available. All four conditions matter: no input, no
    /// destination, no ffmpeg, or a conversion already running each make starting meaningless.
    pub fn can_start(&self) -> bool {
        self.video.is_some()
            && !self.output_path.is_empty()
            && self.ffmpeg_available
            && !self.encoding
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
    /// computed for the state this call has just replaced. (Go moved the generation on only
    /// when it reached the plan; here every refresh does, which can only drop more stale
    /// answers.)
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
        if !self.ffmpeg_available || self.encoder_probe.is_none() {
            return;
        }
        let request = VerificationRequest {
            generation: self.hardware_status_gen,
            ffmpeg: self.ffmpeg.clone(),
            video,
            squeeze: self.view.squeeze,
            selection: parse_encoder_selection(&self.view.encoder_selected),
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
    pub fn set_encoder_probe(&mut self, report: EncoderProbeReport) {
        self.ffmpeg = apply_encoder_probe(&self.ffmpeg, Some(&report));
        self.encoder_probe = Some(report);
    }

    /// Moves the window between its idle and busy shapes. One list of controls, walked in both
    /// directions: two hand-written sequences had drifted apart (P-01).
    pub fn set_encoding(&mut self, in_progress: bool) {
        self.encoding = in_progress;
        self.view.set_enabled(locked(), !in_progress);
        if in_progress {
            self.view.progress = 0.0;
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
        // (Go: the summary after EncodeVideo), not left on a plan nobody ran.
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
                Completion::Succeeded {
                    message: format!("Transform complete. Output file:\n{output}"),
                }
            }
            Err(err) if matches!(root_cause(&err), Error::Cancelled) => {
                // The user asked for this: nothing to report, and certainly not an error.
                self.set_status_and_results("Status: Cancelled", "Results: last run cancelled");
                Completion::Cancelled
            }
            Err(err) => {
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
/// posting to its event loop (Go: `fyne.Do`).
pub trait ProgressSink: Send + Sync {
    /// The bar, as a fraction in 0..1.
    fn bar(&self, fraction: f64);
    /// The status line.
    fn status(&self, text: &str);
}

/// What the pipeline asks of the window (Go: `GUIHandler`).
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
mod tests {
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

    // TestEncodingControls_RoundTripReEnablesEverything (P-01)
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

    // TestWindowControls_LockDiagnosticDuringAConversion (E-14)
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

    // TestAppState_CanStart
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

    // TestAppState_SetInputAndOutputRefreshStart
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

    // TestAppState_AChosenFileIsIgnoredDuringAConversion
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

    // The invalid-input branch of the same Go test.
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

    // TestAppState_HardwareLineWaitsForTheProbe
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

    // TestAppState_ProbeRefusalRemovesTheEncoderFromThePlan
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

    // TestAppState_RefreshHardwareStatus
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

    // TestAppState_NilWidgetsAreTolerated: the closest thing here is a state fresh from `new`
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

    // TestAppState_SetFFmpegUnblocksTheWindow
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

    // TestAppState_DiagnosticStaysUsableWithoutFFmpeg (E-14, first review finding)
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

    // TestAppState_AStaleVerificationDoesNotOverwriteANewerLine
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

    // TestAppState_RefreshInvalidatesAVerificationInFlight
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

    // Wanted improvement (a): Go left the unverified plan on screen when a verification could
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

        state.ffmpeg_available = false;
        state.encoder_changed("libx264 encoder");
        assert_eq!(count(), 2, "without ffmpeg there is nothing to verify");
    }

    // ---- the lifecycle of a conversion ----

    // TestAppState_EncodingRoundTripRestoresEveryControl (P-01)
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

    // TestAppState_BeginEncodingReturnsTheChannel (P-02)
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

    // TestAppState_RequestCancel
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

    // TestAppState_CompleteEncoding (E-14): the dialogs of Go are the `Completion` returned.
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
        // A text carrying the cancellation message is a failure (Go's errors.Is, not a match
        // on the string).
        let mut state = running(true);
        let text = Error::Plain("encoding failed: encoding interrupted by user".into());
        let completion = state.complete_encoding(Err(text), "out.mp4");
        assert!(
            matches!(completion, Completion::Failed { .. }),
            "{completion:?}"
        );
        assert_eq!(state.view.status, "Status: Failed");

        // A cancellation seen through a failure after output is one, as %w is looked through.
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
        let wrapped =
            Error::FailedAfterOutput(Box::new(Error::StoppedBySignal("interrupt".into())));
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

    // TestAppState_Shutdown (E-03). The thread stands in for the real one: it stops on the
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

    // TestAppState_ShutdownFromTheCompletionPathDoesNotWaitForItself
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

    // TestAppState_RunEncodingReleasesDoneOnPanic
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

    // TestAppState_ShutdownWaitsOnlyOnce
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

    // TestAppState_RefreshEncoderOptionsKeepsTheSavedPreference (E-15)
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

    // TestAppState_RefreshEncoderOptionsPrefersTheSavedChoice (sixth review finding)
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

    // TestGUIHandler_ShowProgressSetsTheBarAsAFraction
    #[test]
    fn show_progress_sets_the_bar_as_a_fraction() {
        let (handler, recorder) = handler_with_sink();
        handler.show_progress(42.0);
        let bars = recorder.bars.lock().unwrap();
        assert_eq!(bars.len(), 1);
        assert!((bars[0] - 0.42).abs() < 1e-9, "bar = {} after 42%", bars[0]);
    }

    // TestGUIHandler_ShowProgressWithoutBarDoesNotPanic, and the same for the detail.
    #[test]
    fn a_handler_without_a_sink_ignores_progress() {
        let handler = GuiHandler::new(1, String::new(), false, None, None);
        handler.show_progress(42.0);
        handler.show_progress_detail(50.0, Duration::from_secs(10));
    }

    // TestGUIHandler_ShowProgressDetailWritesTheStatusLine and
    // TestGUIHandler_ImplementsProgressDetail: reached through the trait object the pipeline
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

    // TestGUIHandler_GetEncoderWithoutVideo and TestGUIHandler_GetEncoderReturnsTheCapturedName
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

    // TestGUIHandler_GetSqueezeAndBitrate (E-19, L-106): both directions, so that a constant
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
}
