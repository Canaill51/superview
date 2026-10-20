//! One whole conversion: `PerformEncoding` in `internal/common/pipeline.go` (and the
//! `UIHandler` / `ProgressDetailHandler` interfaces at the top of that file).
//!
//! The order of the steps is Go's, and it is the contract: the cheap refusals come first, so
//! that a run that cannot work leaves nothing behind, and the session (the temporary
//! directory) is created only once every refusal has been given.
//!
//! 1. the input path, the output path, output = input (no process is launched before these);
//! 2. `check_video` (ffprobe), the input's size;
//! 3. the encoder the interface asked for, cleaned against ffmpeg's list;
//! 4. the bitrate (the interface's, else the source's) and its limits;
//! 5. the room at the destination, `find_encoder`, the squeeze flag, the geometry;
//! 6. the encoder asked about the real frame, the machine profile;
//! 7. the room for the maps, the memory guard;
//! 8. **only then** the session, the maps, the working file, the encode, the rename.
//!
//! Whatever happens after the session exists, the working file is removed first (unless it
//! is the finished video that could not be renamed), then the session directory, and the
//! stage timings are logged last, in every case.
//!
//! Deliberate differences from the Go code (exhaustive):
//! - No global. Go stores the metrics and the hardware summary in `lastEncodingMetrics` and
//!   `lastHardwareAccelerationSummary`; [`perform_encoding`] returns them in an
//!   [`EncodeOutcome`]. Only `Ok` returns the metrics: on failure there is only the error,
//!   and `record_error` and `record_stage_durations` work on metrics that are then thrown
//!   away (the log line says the same thing). Go left the previous "Hardware: ..." summary
//!   in place after an error, stale; the caller must clear that label itself on `Err`.
//! - A stop requested while the validations ran (the encoder probe cannot be cancelled,
//!   as in Go) is returned as it is just before the session is created: nothing is
//!   created, no map is generated, no ffmpeg is started. Go goes on to create the session
//!   and the maps, launches ffmpeg and kills it.
//! - The event bus of `observability.go` (`RecordEncodingEvent`, `RecordEncodingError`,
//!   `RecordEncodingProgress`, `RecordEncodingCompletion`) and `LogMetrics` are not ported.
//!   Each failure is one `error` log record naming the stage (the `stage` attribute of Go's
//!   events); the line "Encoding completed successfully" is kept.
//! - The session's parent directory, the core count, the free-space and memory readings,
//!   the tool lookup and the probe cache are [`PipelineServices`] (Go reads `os.TempDir()`,
//!   `runtime.NumCPU()`, package variables and globals).
//! - `ProgressDetailHandler` is optional in Go (a type assertion). Here
//!   [`PipelineUi::show_progress_detail`] has an empty default body, which is the same
//!   thing: an interface that does not override it ignores the estimate.
//!   `ShowError` and `ShowInfo` are never called by `PerformEncoding` and are not in the
//!   trait.
//! - The core installs no signal handler: the stop requests are the [`StopControl`] given to
//!   [`perform_encoding`], passed to the encode as it is.
//! - The input path is a [`Path`]; one that is not valid UTF-8 is refused (ffprobe's and
//!   the metrics' side of the interface is text, as `VideoSpecs::file` is).
//! - The wrapped errors of Go (`%w`) keep their type only where it matters: the error of the
//!   encode is returned as it is (so [`Error::is_stop_request`] and
//!   [`Error::should_not_retry`] still work on it), and so are those Go returns bare. The
//!   ones Go prefixes ("invalid input file: ...", "video validation failed: ...", "invalid
//!   encoder selection: ...", the failed move) become [`Error::Plain`] with Go's text.
//!   Nothing in Go matches on their type.
//! - The cause inside the failed-move message is Rust's `io::Error` text.
//! - Go removes the working file with `os.Remove` in a `defer`; here a guard does, so a
//!   panic cleans up too.

use std::cell::Cell;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::disk::{check_temp_space_for_maps, free_disk_gb};
use crate::encode::args::RateControl;
use crate::encode::args::{effective_encoder_threads, validate_bitrate};
use crate::encode::cascade::{EncodeSummary, encode_video_with_memory};
use crate::encode::run::{StopControl, stop_requested};
use crate::error::{Error, InvalidVideoError};
use crate::hardware::{analyze_machine_profile, find_encoder, is_hardware_encoder};
use crate::memory::{available_memory_bytes, check_memory_for_encode};
use crate::metrics::EncodingMetrics;
use crate::output::{check_output_space, new_partial_output_path, same_output_as_input};
use crate::pgm::{check_geometry, generate_pgm};
use crate::probe::Prober;
use crate::security::{is_valid_input_path, is_valid_output_path, sanitize_encoder_input};
use crate::session::EncodingSession;
use crate::tools::ToolResolver;
use crate::video::check_video;

/// What a stage that fails before ffmpeg ever ran records as ffmpeg's exit code
/// (Go: `exitCodeUnavailable`).
const EXIT_CODE_UNAVAILABLE: i32 = -1;

/// What the pipeline asks of the interface (Go: `UIHandler`, reduced to what
/// `PerformEncoding` calls).
pub trait PipelineUi {
    /// The encoder selection (for example "libx265"); "" means the source's codec.
    fn get_encoder(&self) -> String;
    /// The wanted output bitrate in bits per second; 0 (or an error) means the source's.
    fn get_bitrate(&self) -> Result<i64, Error>;
    /// Whether to squeeze (keep the source's width) instead of widening to 16:9.
    fn get_squeeze(&self) -> bool;
    /// The progress, 0 to 100 percent.
    fn show_progress(&self, percent: f64);
    /// The progress with the estimated time left (Go: `ProgressDetailHandler`). `remaining`
    /// is zero while there is not enough data to estimate, which is "no estimate", not "no
    /// time left". Called after [`PipelineUi::show_progress`], once per update. The default
    /// does nothing: an interface that has no use for the estimate need not know about it.
    fn show_progress_detail(&self, _percent: f64, _remaining: Duration) {}
}

/// What the pipeline reads from the machine, injected so that a test can choose it (the
/// package variables and globals of the Go code).
pub struct PipelineServices<'a> {
    /// Finds ffmpeg and ffprobe.
    pub resolver: &'a ToolResolver,
    /// Asks hardware encoders about the real frame; its cache outlives one conversion.
    pub prober: &'a Prober<'a>,
    /// The host's core count (Go: `runtime.NumCPU()`).
    pub cpu_count: i64,
    /// The directory the session is created in, and whose room the maps are checked against
    /// (Go: `os.TempDir()`).
    pub temp_dir: PathBuf,
    /// The room, in GB, a user may still write at a path (Go: `readFreeDiskGB`).
    pub free_gb: &'a dyn Fn(&Path) -> io::Result<f64>,
    /// The memory available, in bytes; `None` when it cannot be read.
    pub available_memory: &'a dyn Fn() -> Option<u64>,
}

impl<'a> PipelineServices<'a> {
    /// The real machine: the system's temporary directory, core count, disk and memory.
    pub fn system(resolver: &'a ToolResolver, prober: &'a Prober<'a>) -> Self {
        Self {
            resolver,
            prober,
            cpu_count: std::thread::available_parallelism()
                .map_or(1, |n| i64::try_from(n.get()).unwrap_or(1)),
            temp_dir: std::env::temp_dir(),
            free_gb: &free_disk_gb,
            available_memory: &available_memory_bytes,
        }
    }
}

/// What a successful conversion reports (Go: `GetLastEncodingMetrics` and
/// `GetLastHardwareAccelerationSummary`, plus what the log line carries).
#[derive(Debug, Clone)]
pub struct EncodeOutcome {
    /// The line the window shows about the path that encoded.
    pub summary: EncodeSummary,
    pub metrics: EncodingMetrics,
    /// The encoder that really ran (the probe may have stepped back to the CPU).
    pub encoder: String,
    /// The bitrate used, in bits per second.
    pub bitrate: i64,
}

/// The durations of the stages, shared with the guards that measure them on the way out.
#[derive(Default)]
struct Stages {
    video_check: Cell<Duration>,
    pgm_generation: Cell<Duration>,
    encoding: Cell<Duration>,
    cleanup: Cell<Duration>,
}

/// Everything a run reads.
struct Context<'a> {
    cfg: &'a Config,
    input: &'a Path,
    output: &'a Path,
    ui: &'a dyn PipelineUi,
    ffmpeg: &'a HashMap<String, String>,
    services: &'a PipelineServices<'a>,
    stop: &'a StopControl<'a>,
}

/// Owns the session; closing it on the way out is the cleanup stage (Go's deferred
/// `CleanUp`). A failed removal is a warning, never the run's error.
struct SessionGuard<'a> {
    session: Option<EncodingSession>,
    cleanup: &'a Cell<Duration>,
}

impl SessionGuard<'_> {
    fn session(&self) -> &EncodingSession {
        self.session
            .as_ref()
            .expect("the session is held until the guard is dropped")
    }
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let start = Instant::now();
        if let Some(session) = self.session.take()
            && let Err(e) = session.close()
        {
            log::warn!("Failed to cleanup encoding session: error={e}");
        }
        self.cleanup.set(start.elapsed());
    }
}

/// The working file ffmpeg writes into. Removed on the way out unless `keep` is set, which
/// it is once the file has been renamed into place (nothing left to remove) or could not be
/// (the finished video: deleting it would turn a rename error into the loss of the whole
/// conversion).
struct PartialFile {
    path: PathBuf,
    keep: bool,
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        if let Err(e) = fs::remove_file(&self.path)
            && e.kind() != io::ErrorKind::NotFound
        {
            log::warn!(
                "Failed to remove the partial output file: path={} error={e}",
                self.path.display()
            );
        }
    }
}

/// Moves the finished working file onto the destination. Same directory, so this is atomic:
/// the destination is either the previous file or the finished one, never a half-written
/// mixture. When it fails the working file is kept and the error says where it is.
fn move_into_place(partial: &mut PartialFile, output: &Path) -> Result<(), Error> {
    // Either way there is nothing for the guard to delete: the file is the destination now,
    // or it is the finished video that must survive.
    partial.keep = true;
    fs::rename(&partial.path, output).map_err(|e| {
        log::error!(
            "The finished video could not be moved into place; it is kept under its working name: partial={} destination={} error={e}",
            partial.path.display(),
            output.display()
        );
        Error::Plain(format!(
            "failed to move the finished file to {}: {e}. The finished video is kept at {}; move or rename it yourself",
            output.display(),
            partial.path.display()
        ))
    })
}

/// Records a failure the way Go's `RecordError` + `RecordEncodingError` pair does and gives
/// the error back.
fn fail(metrics: &mut EncodingMetrics, stage: &str, metrics_message: &str, err: Error) -> Error {
    metrics.record_error(EXIT_CODE_UNAVAILABLE, metrics_message);
    log::error!("Encoding failed: stage={stage} error={err}");
    err
}

/// [`fail`] for an error Go wraps with a prefix: the metrics and the caller get the same text.
fn fail_prefixed(metrics: &mut EncodingMetrics, stage: &str, prefix: &str, cause: &Error) -> Error {
    let message = format!("{prefix}: {cause}");
    fail(metrics, stage, &message, Error::Plain(message.clone()))
}

/// Converts `input` into `output`, from the first check to the rename (Go:
/// `PerformEncoding`). `cfg` `None` is the default configuration; `ffmpeg` is what
/// `check_ffmpeg` returned. See the module header for the order of the steps and the
/// differences from Go.
///
/// On failure or stop, nothing is left behind: no working file, no session directory, no
/// ffmpeg process. The exception is a destination that could not be written at the end: the
/// finished video then stays under its working name and the error says where.
pub fn perform_encoding(
    cfg: Option<&Config>,
    input: &Path,
    output: &Path,
    ui: &dyn PipelineUi,
    ffmpeg: &HashMap<String, String>,
    services: &PipelineServices,
    stop: &StopControl,
) -> Result<EncodeOutcome, Error> {
    let default_cfg;
    let cfg = match cfg {
        Some(cfg) => cfg,
        None => {
            default_cfg = Config::default();
            &default_cfg
        }
    };
    let mut metrics = EncodingMetrics::new(&input.to_string_lossy(), &output.to_string_lossy());
    let stages = Stages::default();
    let context = Context {
        cfg,
        input,
        output,
        ui,
        ffmpeg,
        services,
        stop,
    };

    // Every guard of `run` is gone when it returns, so the timings below are the last thing
    // said, as with Go's first deferred function.
    let result = run(&context, &mut metrics, &stages);

    metrics.record_stage_durations(
        stages.video_check.get(),
        stages.pgm_generation.get(),
        stages.encoding.get(),
        stages.cleanup.get(),
    );
    log::info!(
        "Encoding stage timings: video_check_ms={} pgm_generation_ms={} encoding_ms={} cleanup_ms={}",
        stages.video_check.get().as_millis(),
        stages.pgm_generation.get().as_millis(),
        stages.encoding.get().as_millis(),
        stages.cleanup.get().as_millis()
    );

    result.map(|(summary, encoder, bitrate)| EncodeOutcome {
        summary,
        metrics,
        encoder,
        bitrate,
    })
}

/// The steps themselves. Returns the summary, the encoder that ran and the bitrate.
fn run(
    c: &Context,
    metrics: &mut EncodingMetrics,
    stages: &Stages,
) -> Result<(EncodeSummary, String, i64), Error> {
    let cfg = c.cfg;
    let services = c.services;

    // ==== SECURITY VALIDATION ====
    is_valid_input_path(c.input)
        .map_err(|e| fail_prefixed(metrics, "input_validation", "invalid input file", &e))?;
    let Some(input_text) = c.input.to_str() else {
        let cause = Error::Plain("path is not valid UTF-8".into());
        return Err(fail_prefixed(
            metrics,
            "input_validation",
            "invalid input file",
            &cause,
        ));
    };
    is_valid_output_path(c.output)
        .map_err(|e| fail_prefixed(metrics, "output_validation", "invalid output file", &e))?;
    if same_output_as_input(c.input, c.output) {
        let err = Error::Plain(
            "the output file is the input file; choose a different destination".into(),
        );
        let message = err.to_string();
        return Err(fail(metrics, "output_validation", &message, err));
    }

    // Load and validate video metadata (includes security checks).
    let video_check_start = Instant::now();
    let video = check_video(services.resolver, input_text);
    stages.video_check.set(video_check_start.elapsed());
    let video =
        video.map_err(|e| fail_prefixed(metrics, "video_check", "video validation failed", &e))?;

    let input_size =
        fs::metadata(c.input).map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX));
    metrics.record_input_metadata(&video, input_size);

    // The encoder selection from the interface, against the whitelist.
    let encoder_sanitized = sanitize_encoder_input(
        &c.ui.get_encoder(),
        c.ffmpeg.get("encoders").map_or("", String::as_str),
    )
    .map_err(|e| {
        fail_prefixed(
            metrics,
            "encoder_sanitization",
            "invalid encoder selection",
            &e,
        )
    })?;

    // The bitrate: the interface's, else the source's.
    let Some(stream) = video.streams.first() else {
        let err = Error::from(InvalidVideoError {
            reason: "no video streams".into(),
        });
        let message = err.to_string();
        return Err(fail(metrics, "bitrate_validation", &message, err));
    };
    let mut bitrate = match c.ui.get_bitrate() {
        Ok(wanted) if wanted > 0 => wanted,
        _ => 0,
    };
    if bitrate == 0 {
        bitrate = stream.bitrate_int;
    }
    // In constant quality (DEC-48) the bit rate is not sent to ffmpeg, and these two checks
    // keep using it as the estimate of the file: the real size follows the content and
    // can differ from it either way. Which mode applies is only known after the probe,
    // further down, and the checks must run before the (slower) probe and the encoder
    // selection they precede.
    validate_bitrate(bitrate, cfg.min_bitrate, cfg.max_bitrate).map_err(|e| {
        let message = format!("bitrate validation failed: {e}");
        fail(metrics, "bitrate_validation", &message, e)
    })?;

    check_output_space(c.output, bitrate, stream.duration_float, services.free_gb).map_err(
        |e| {
            let message = e.to_string();
            fail(metrics, "output_space_check", &message, e)
        },
    )?;

    // The encoder, with full validation (it uses the cleaned selection).
    let encoder = find_encoder(&encoder_sanitized, c.ffmpeg, &video).map_err(|e| {
        let message = format!("encoder selection failed: {e}");
        fail(metrics, "encoder_selection", &message, e)
    })?;

    // Asked once and reused: the size check just below, the space check and the map
    // generation further down must all agree on which geometry is being produced.
    let squeeze = c.ui.get_squeeze();

    check_geometry(&video, squeeze).map_err(|e| {
        let err = Error::from(e);
        let message = err.to_string();
        fail(metrics, "geometry_check", &message, err)
    })?;

    // Ask a hardware encoder whether it will take the frame it is about to be given, and
    // step back to the CPU here if it will not. Here rather than inside the encode, because
    // everything downstream is entitled to know which encoder will really run. The reason
    // for a refusal is dropped, as in Go (`encoder, _ =`); the probe logs it.
    //
    // The same answer says how the encoder it keeps must be asked to spend bits: a VAAPI
    // driver that knows CQP only is used in constant quality instead of being refused
    // (DEC-48), and a CPU encoder stepped back to is asked for a bit rate again.
    let (encoder, _refusal, rate_control) = services.prober.verify_encoder_with_rate_control(
        &encoder,
        Some(&video),
        squeeze,
        c.ffmpeg,
        &cfg.video_preset,
        None,
    );

    if rate_control == RateControl::ConstantQuality {
        log::info!(
            "The driver of {encoder} only knows constant quality; the requested bit rate is not used: bitrate_bits_sec={bitrate}"
        );
    }

    let profile = analyze_machine_profile(
        c.ffmpeg,
        usize::try_from(services.cpu_count).unwrap_or_default(),
    );
    log::info!(
        "Machine profile analyzed: cpu_cores={} hw_accels={} selected_encoder={encoder} hardware_encoder={}",
        profile.cpu_cores,
        profile.hardware_accels.join(","),
        is_hardware_encoder(&encoder)
    );

    metrics.record_output_metadata(bitrate, &encoder);

    check_temp_space_for_maps(&video, squeeze, &services.temp_dir, services.free_gb).map_err(
        |e| {
            let err = Error::from(e);
            let message = err.to_string();
            fail(metrics, "space_check", &message, err)
        },
    )?;

    // After the encoder is known, because what a conversion costs in memory depends on it:
    // a hardware encoder is not measured here, and a 10-bit source only stays 10-bit on an
    // HEVC one.
    check_memory_for_encode(
        Some(&video),
        squeeze,
        &encoder,
        effective_encoder_threads(Some(cfg), &encoder, services.cpu_count),
        services.available_memory,
    )
    .map_err(|e| {
        let err = Error::from(e);
        let message = err.to_string();
        fail(metrics, "memory_check", &message, err)
    })?;

    // The probe above cannot be cancelled: a stop asked meanwhile ends the run here, before
    // the session and the maps (large frames make those expensive) rather than after them.
    // The verification at the output size has no cancellation either: in constant quality it
    // may ask twice, so the wait before the stop is seen can reach two probe timeouts.
    if let Some(stop) = stop_requested(c.stop) {
        let message = stop.to_string();
        return Err(fail(metrics, "stop", &message, stop));
    }

    // ==== Nothing has been created so far. From here on, everything is cleaned up. ====
    let session = EncodingSession::new_in(&services.temp_dir, cfg).map_err(|e| {
        let err = Error::from(e);
        let message = format!("session initialization failed: {err}");
        fail(metrics, "session_init", &message, err)
    })?;
    // Declared before the working file, so dropped after it: the working file goes first.
    let session = SessionGuard {
        session: Some(session),
        cleanup: &stages.cleanup,
    };

    let pgm_start = Instant::now();
    let maps = generate_pgm(&video, squeeze, session.session().dir());
    stages.pgm_generation.set(pgm_start.elapsed());
    maps.map_err(|e| {
        let message = format!("filter generation failed: {e}");
        fail(metrics, "pgm_generation", &message, e)
    })?;

    // Encode into a working file and only move it into place once ffmpeg has finished:
    // a cancellation or a failure must not leave a truncated .mp4 where the user expected
    // a finished one, nor destroy the file that was there before.
    let partial_path = new_partial_output_path(c.output).map_err(|e| {
        let message = e.to_string();
        fail(metrics, "encoding", &message, e)
    })?;
    let mut partial = PartialFile {
        path: partial_path,
        keep: false,
    };

    // Recorded before reporting: `record_progress` is what refreshes the remaining-time
    // estimate, and the interface should see the estimate that goes with the percentage.
    let mut on_progress = |percent: f64| {
        metrics.record_progress(percent);
        c.ui.show_progress(percent);
        c.ui.show_progress_detail(percent, metrics.remaining());
    };
    let encode_start = Instant::now();
    let result = encode_video_with_memory(
        Some(cfg),
        &video,
        &encoder,
        rate_control,
        bitrate,
        &partial.path,
        c.ffmpeg,
        session.session(),
        services.resolver,
        services.cpu_count,
        c.stop,
        services.available_memory,
        &mut on_progress,
    );
    stages.encoding.set(encode_start.elapsed());
    let summary = result.map_err(|e| {
        let message = format!("encoding failed: {e}");
        fail(metrics, "encoding", &message, e)
    })?;

    move_into_place(&mut partial, c.output).map_err(|e| {
        let message = e.to_string();
        fail(metrics, "finalize", &message, e)
    })?;

    let output_size =
        fs::metadata(c.output).map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX));
    metrics.record_completion(output_size);

    log::info!(
        "Encoding completed successfully: output_file={} encoder={encoder} bitrate_bits_sec={bitrate} elapsed_time={:?}",
        c.output
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        metrics.elapsed()
    );

    Ok((summary, encoder, bitrate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::spawn_lock;
    use crate::tools::check_ffmpeg;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use tempfile::TempDir;

    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    const CLIP_TEXT: &str = "pretend this is a video";

    type Events = Rc<RefCell<Vec<String>>>;
    type FreeReading<'a> = Box<dyn Fn(&Path) -> io::Result<f64> + 'a>;

    // ---- the interface ----

    struct TestUi<'a> {
        events: Events,
        encoder: &'a str,
        /// `None`: the interface cannot say (Go: `GetBitrate` returns an error).
        bitrate: Option<i64>,
        squeeze: bool,
        on_progress: Option<Box<dyn Fn(f64) + 'a>>,
        progress: Cell<usize>,
        detail: Cell<usize>,
    }

    impl<'a> TestUi<'a> {
        fn new(events: &Events, encoder: &'a str, bitrate: Option<i64>) -> Self {
            Self {
                events: Rc::clone(events),
                encoder,
                bitrate,
                squeeze: false,
                on_progress: None,
                progress: Cell::new(0),
                detail: Cell::new(0),
            }
        }

        fn quiet(encoder: &'a str, bitrate: i64) -> Self {
            Self::new(&Events::default(), encoder, Some(bitrate))
        }

        fn on_progress(mut self, f: impl Fn(f64) + 'a) -> Self {
            self.on_progress = Some(Box::new(f));
            self
        }
    }

    impl PipelineUi for TestUi<'_> {
        fn get_encoder(&self) -> String {
            self.events.borrow_mut().push("get_encoder".into());
            self.encoder.to_string()
        }
        fn get_bitrate(&self) -> Result<i64, Error> {
            self.events.borrow_mut().push("get_bitrate".into());
            self.bitrate
                .ok_or_else(|| Error::Plain("no bitrate field".into()))
        }
        fn get_squeeze(&self) -> bool {
            self.events.borrow_mut().push("get_squeeze".into());
            self.squeeze
        }
        fn show_progress(&self, percent: f64) {
            self.events.borrow_mut().push("progress".into());
            self.progress.set(self.progress.get() + 1);
            if let Some(f) = &self.on_progress {
                f(percent);
            }
        }
        fn show_progress_detail(&self, _percent: f64, _remaining: Duration) {
            self.events.borrow_mut().push("detail".into());
            self.detail.set(self.detail.get() + 1);
        }
    }

    // ---- the scene: an input, a destination directory, a temporary directory ----

    struct Rig {
        _root: TempDir,
        input: PathBuf,
        out_dir: PathBuf,
        tmp: PathBuf,
    }

    impl Rig {
        fn new() -> Self {
            let root = TempDir::new().unwrap();
            let (out_dir, tmp) = (root.path().join("out"), root.path().join("tmp"));
            fs::create_dir(&out_dir).unwrap();
            fs::create_dir(&tmp).unwrap();
            let input = root.path().join("input.mp4");
            fs::write(&input, CLIP_TEXT).unwrap();
            Self {
                _root: root,
                input,
                out_dir,
                tmp,
            }
        }

        fn output(&self) -> PathBuf {
            self.out_dir.join("output.mp4")
        }

        fn scenario<'a>(
            &self,
            resolver: &'a ToolResolver,
            caps: HashMap<String, String>,
        ) -> Scenario<'a> {
            Scenario {
                resolver,
                caps,
                cfg: Config::default(),
                input: self.input.clone(),
                output: self.output(),
                tmp: self.tmp.clone(),
                free: Box::new(|_| Ok(1000.0)),
                mem: Box::new(|| Some(64 * GIB)),
                cancel: AtomicBool::new(false),
                signal: AtomicI32::new(0),
            }
        }

        /// Nothing at the destination and nothing in the temporary directory: no working
        /// file, no session.
        fn assert_clean(&self) {
            assert_eq!(names(&self.out_dir), Vec::<String>::new(), "destination");
            assert_eq!(names(&self.tmp), Vec::<String>::new(), "temporary dir");
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    fn caps(encoders: &str, accels: &str) -> HashMap<String, String> {
        [("encoders", encoders), ("accels", accels)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn cpu_caps() -> HashMap<String, String> {
        caps("libx264,libx265,aac", "")
    }

    /// A resolver that finds nothing (nothing in these tests may launch a process).
    fn no_tools() -> ToolResolver {
        ToolResolver::new(|_| None, None)
    }

    struct Scenario<'a> {
        resolver: &'a ToolResolver,
        caps: HashMap<String, String>,
        cfg: Config,
        input: PathBuf,
        output: PathBuf,
        tmp: PathBuf,
        free: FreeReading<'a>,
        mem: Box<dyn Fn() -> Option<u64> + 'a>,
        cancel: AtomicBool,
        signal: AtomicI32,
    }

    impl Scenario<'_> {
        fn run(&self, ui: &TestUi) -> Result<EncodeOutcome, Error> {
            let prober = Prober::new(self.resolver);
            let services = PipelineServices {
                resolver: self.resolver,
                prober: &prober,
                cpu_count: 4,
                temp_dir: self.tmp.clone(),
                free_gb: &*self.free,
                available_memory: &*self.mem,
            };
            perform_encoding(
                Some(&self.cfg),
                &self.input,
                &self.output,
                ui,
                &self.caps,
                &services,
                &StopControl {
                    cancel: &self.cancel,
                    signal: &self.signal,
                },
            )
        }
    }

    fn message(result: Result<EncodeOutcome, Error>) -> String {
        match result {
            Ok(_) => panic!("the run was expected to be refused"),
            Err(e) => e.to_string(),
        }
    }

    // ---- refusals that need no process (they run on every platform) ----

    // Go: TestPerformEncoding_RefusesOutputEqualToInput
    #[test]
    fn refuses_an_output_that_is_the_input() {
        let rig = Rig::new();
        let resolver = no_tools();
        let mut scn = rig.scenario(&resolver, HashMap::new());
        scn.output = rig.input.clone();
        let events = Events::default();
        let ui = TestUi::new(&events, "", Some(0));

        let err = scn.run(&ui).expect_err("the output is the input");

        assert!(matches!(err, Error::Plain(_)), "{err:?}");
        assert_eq!(
            err.to_string(),
            "the output file is the input file; choose a different destination"
        );
        assert!(err.to_string().contains("input file"));
        // The guard exists because the encode goes through a working file: nothing else
        // would stop the final rename from destroying the source.
        assert_eq!(fs::read_to_string(&rig.input).unwrap(), CLIP_TEXT);
        assert_eq!(names(&rig.tmp), Vec::<String>::new());
        assert!(events.borrow().is_empty(), "the interface was asked");
    }

    /// Go: the first three cases of `TestPerformEncoding_EarlyFailuresAreRecorded`, and
    /// the order in which they are given.
    fn path_refusals(resolver: &ToolResolver) {
        let rig = Rig::new();
        let missing_dir = rig.out_dir.join("missing");
        let gone = rig.out_dir.join("gone.mp4");
        // (input, output, what the message starts with)
        let cases: [(PathBuf, PathBuf, &str); 4] = [
            (
                PathBuf::from("clip.mp4"),
                rig.output(),
                "invalid input file: ",
            ),
            (gone.clone(), rig.output(), "invalid input file: "),
            (
                rig.input.clone(),
                missing_dir.join("output.mp4"),
                "invalid output file: ",
            ),
            // Both are wrong: the input is looked at first.
            (gone, missing_dir.join("output.mp4"), "invalid input file: "),
        ];
        for (input, output, starts) in cases {
            let events = Events::default();
            let mut scn = rig.scenario(resolver, cpu_caps());
            scn.input = input.clone();
            scn.output = output.clone();
            let ui = TestUi::new(&events, "libx264", Some(2_000_000));

            let text = message(scn.run(&ui));

            assert!(text.starts_with(starts), "{input:?} -> {output:?}: {text}");
            assert!(
                events.borrow().is_empty(),
                "the interface was asked: {text}"
            );
            rig.assert_clean();
        }
    }

    #[test]
    fn a_bad_path_is_refused_before_anything_else() {
        path_refusals(&no_tools());
    }

    // ---- the working file and the move into place ----

    // Go: TestIntegration_RenameFailureKeepsTheFinishedFile (the failing rename itself)
    #[test]
    fn a_failed_move_keeps_the_finished_file_and_says_where_it_is() {
        let dir = TempDir::new().unwrap();
        let partial_path = dir.path().join(".superview-partial-test.mp4");
        fs::write(&partial_path, "the finished video").unwrap();
        // A directory at the destination: renaming a file onto it fails on every platform.
        let destination = dir.path().join("finished.mp4");
        fs::create_dir(&destination).unwrap();

        let mut partial = PartialFile {
            path: partial_path.clone(),
            keep: false,
        };
        let err =
            move_into_place(&mut partial, &destination).expect_err("a directory is in the way");
        drop(partial);

        let text = err.to_string();
        assert!(
            text.starts_with(&format!(
                "failed to move the finished file to {}: ",
                destination.display()
            )),
            "{text}"
        );
        assert!(
            text.ends_with(&format!(
                ". The finished video is kept at {}; move or rename it yourself",
                partial_path.display()
            )),
            "{text}"
        );
        assert_eq!(
            fs::read_to_string(&partial_path).unwrap(),
            "the finished video",
            "the finished video was deleted"
        );
    }

    #[test]
    fn a_successful_move_leaves_only_the_destination() {
        let dir = TempDir::new().unwrap();
        let partial_path = dir.path().join(".superview-partial-test.mp4");
        fs::write(&partial_path, "done").unwrap();
        let destination = dir.path().join("finished.mp4");
        fs::write(&destination, "the previous file").unwrap();

        let mut partial = PartialFile {
            path: partial_path,
            keep: false,
        };
        move_into_place(&mut partial, &destination).unwrap();
        drop(partial);

        assert_eq!(names(dir.path()), ["finished.mp4"]);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "done");
    }

    #[test]
    fn the_working_file_goes_unless_it_is_kept() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".superview-partial-test.mp4");
        fs::write(&path, "x").unwrap();
        drop(PartialFile {
            path: path.clone(),
            keep: false,
        });
        assert!(!path.exists(), "the working file was left behind");

        fs::write(&path, "x").unwrap();
        drop(PartialFile {
            path: path.clone(),
            keep: true,
        });
        assert!(path.exists(), "a kept working file was deleted");

        // Already gone (moved): not worth a warning, and not a panic.
        let gone = dir.path().join("gone.mp4");
        drop(PartialFile {
            path: gone,
            keep: false,
        });
    }

    #[test]
    fn the_guard_of_the_session_removes_its_directory() {
        let tmp = TempDir::new().unwrap();
        let cleanup = Cell::new(Duration::MAX);
        let session = EncodingSession::new_in(tmp.path(), &Config::default()).unwrap();
        let dir = session.dir().to_path_buf();
        assert!(dir.is_dir());
        drop(SessionGuard {
            session: Some(session),
            cleanup: &cleanup,
        });
        assert!(!dir.exists(), "the session directory was left behind");
        assert_ne!(cleanup.get(), Duration::MAX, "the cleanup was not timed");
    }

    #[test]
    fn the_system_services_describe_this_machine() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        let services = PipelineServices::system(&resolver, &prober);
        assert!(services.cpu_count >= 1);
        assert_eq!(services.temp_dir, std::env::temp_dir());
        assert!((services.free_gb)(&services.temp_dir).unwrap() > 0.0);
    }

    struct Quiet;
    impl PipelineUi for Quiet {
        fn get_encoder(&self) -> String {
            String::new()
        }
        fn get_bitrate(&self) -> Result<i64, Error> {
            Ok(0)
        }
        fn get_squeeze(&self) -> bool {
            false
        }
        fn show_progress(&self, _: f64) {}
    }

    // Go: an interface that does not implement ProgressDetailHandler is simply not called.
    #[test]
    fn the_progress_detail_is_optional() {
        Quiet.show_progress_detail(50.0, Duration::from_secs(3));
    }

    // ---- stand-in tools (Unix: shell scripts) ----

    #[cfg(unix)]
    mod act {
        pub const OK: &str = "echo out_time_ms=1000000; echo progress=end; exit 0";
        pub const REFUSE: &str = "echo 'Hardware does not support encoding at size' 1>&2; exit 1";
        /// Frames are written, then the disk is full.
        pub const LATE: &str =
            "echo out_time_ms=500000; echo 'No space left on device' 1>&2; exit 1";
        /// Reports a frame, then runs until it is killed.
        // Truncates its target (the last argument), as ffmpeg does when it opens the output.
        pub const HANG: &str =
            "for a; do last=$a; done; : > \"$last\"; echo out_time_ms=1000000; exec sleep 30";
        pub const PROBE_OK: &str = "cat <<'EOF'\n{\"streams\":[{\"codec_name\":\"h264\",\"width\":640,\"height\":480,\"duration\":\"3.0\",\"bit_rate\":\"2500000\",\"pix_fmt\":\"yuv420p\",\"r_frame_rate\":\"30/1\"}]}\nEOF";
        pub const PROBE_BROKEN: &str = "echo 'moov atom not found' 1>&2; exit 1";
    }

    /// An ffprobe and an ffmpeg that each append their argv to a ledger (the ffmpeg also
    /// its pid) and then do what they are told.
    #[cfg(unix)]
    struct Fake {
        tools: crate::probe::tests::FakeTools,
        probe_ledger: PathBuf,
        ffmpeg_ledger: PathBuf,
        pids: PathBuf,
        _dir: TempDir,
    }

    #[cfg(unix)]
    fn fake(ffprobe: &str, ffmpeg: &str) -> Fake {
        let dir = TempDir::new().unwrap();
        let (probe_ledger, ffmpeg_ledger, pids) = (
            dir.path().join("probe.txt"),
            dir.path().join("ffmpeg.txt"),
            dir.path().join("pids.txt"),
        );
        let probe_body = format!(
            "echo \"call $*\" >> '{}'\n{ffprobe}",
            probe_ledger.display()
        );
        let ffmpeg_body = format!(
            "echo \"call $*\" >> '{}'\necho $$ >> '{}'\n{ffmpeg}",
            ffmpeg_ledger.display(),
            pids.display()
        );
        Fake {
            tools: crate::probe::tests::fake_tools(&[
                ("ffprobe", &probe_body),
                ("ffmpeg", &ffmpeg_body),
            ]),
            probe_ledger,
            ffmpeg_ledger,
            pids,
            _dir: dir,
        }
    }

    #[cfg(unix)]
    impl Fake {
        fn resolver(&self) -> &ToolResolver {
            &self.tools.resolver
        }

        fn calls(ledger: &Path) -> Vec<String> {
            fs::read_to_string(ledger)
                .map(|text| text.lines().map(str::to_string).collect())
                .unwrap_or_default()
        }

        fn probe_calls(&self) -> Vec<String> {
            Self::calls(&self.probe_ledger)
        }

        fn ffmpeg_calls(&self) -> Vec<String> {
            Self::calls(&self.ffmpeg_ledger)
        }

        fn assert_no_process_left(&self) {
            let text = fs::read_to_string(&self.pids).unwrap_or_default();
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

    // ---- the order of the steps ----

    // Go: TestPerformEncoding_EarlyFailuresAreRecorded (the order: no process is launched
    // before the paths are valid)
    #[cfg(unix)]
    #[test]
    fn no_process_is_launched_before_the_paths_are_valid() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        path_refusals(tools.resolver());
        // The same-file refusal comes before ffprobe too.
        let rig = Rig::new();
        let mut scn = rig.scenario(tools.resolver(), cpu_caps());
        scn.output = rig.input.clone();
        let ui = TestUi::quiet("libx264", 2_000_000);
        scn.run(&ui).expect_err("the output is the input");
        assert_eq!(tools.probe_calls(), Vec::<String>::new());
        assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new());
    }

    // The sequence Go follows, seen from the places the interface and the injected readings
    // can see it: encoder, bitrate, room at the destination, squeeze, room for the maps,
    // memory (and no session yet), and only then progress.
    #[cfg(unix)]
    #[test]
    fn the_steps_come_in_the_order_of_the_go_pipeline() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let events = Events::default();
        let mut scn = rig.scenario(tools.resolver(), cpu_caps());
        let (log, out_dir, tmp) = (Rc::clone(&events), rig.out_dir.clone(), rig.tmp.clone());
        scn.free = Box::new({
            let log = Rc::clone(&log);
            let tmp = tmp.clone();
            move |path| {
                let which = if path == tmp { "tmp" } else { "output" };
                assert!(path == tmp || path == out_dir, "{path:?}");
                log.borrow_mut().push(format!("free:{which}"));
                Ok(1000.0)
            }
        });
        scn.mem = Box::new({
            let tmp = tmp.clone();
            move || {
                // The session is created in `tmp`: it must not exist yet.
                let entries = names(&tmp).len();
                log.borrow_mut().push(format!("memory:{entries}"));
                Some(64 * GIB)
            }
        });
        let ui = TestUi::new(&events, "libx264", Some(2_000_000));

        scn.run(&ui).expect("the conversion");

        let seen = events.borrow();
        let first_progress = seen.iter().position(|e| e == "progress").unwrap();
        assert_eq!(
            seen[..first_progress],
            [
                "get_encoder",
                "get_bitrate",
                "free:output",
                "get_squeeze",
                "free:tmp",
                "memory:0",
            ],
            "{seen:?}"
        );
        assert_eq!(seen[first_progress + 1], "detail", "{seen:?}");
    }

    // Go: TestPerformEncoding_EarlyFailuresAreRecorded (the cases after the paths): each
    // refusal is returned, launches no ffmpeg and leaves nothing at the destination or in
    // the temporary directory.
    #[cfg(unix)]
    #[test]
    fn every_early_refusal_is_returned_and_leaves_nothing() {
        let _guard = spawn_lock();
        let min = Config::default().min_bitrate;
        let tmp_full = |_: &Path| Ok(0.001);

        struct Case {
            name: &'static str,
            ffprobe: &'static str,
            encoder: &'static str,
            bitrate: i64,
            // Applied to the scenario.
            tweak: fn(&mut Scenario),
            check: fn(&Error),
        }
        let cases = [
            Case {
                name: "input that is not a video",
                ffprobe: act::PROBE_BROKEN,
                encoder: "libx264",
                bitrate: 2_000_000,
                tweak: |_| {},
                check: |e| {
                    let text = e.to_string();
                    assert!(
                        text.starts_with("video validation failed: ffprobe failed, output:\n"),
                        "{text}"
                    );
                    assert!(text.contains("moov atom not found"), "{text}");
                },
            },
            Case {
                name: "encoder not in the whitelist",
                ffprobe: act::PROBE_OK,
                encoder: "bogus_encoder",
                bitrate: 2_000_000,
                tweak: |_| {},
                check: |e| {
                    let text = e.to_string();
                    assert!(text.starts_with("invalid encoder selection: "), "{text}");
                    assert!(text.contains("bogus_encoder"), "{text}");
                },
            },
            Case {
                name: "bitrate below the configured minimum",
                ffprobe: act::PROBE_OK,
                encoder: "libx264",
                bitrate: min - 1,
                tweak: |_| {},
                check: |e| {
                    let wanted = validate_bitrate(
                        Config::default().min_bitrate - 1,
                        Config::default().min_bitrate,
                        Config::default().max_bitrate,
                    )
                    .unwrap_err();
                    assert!(matches!(e, Error::Plain(_)), "{e:?}");
                    assert_eq!(e.to_string(), wanted.to_string());
                },
            },
            Case {
                name: "no room at the destination",
                ffprobe: act::PROBE_OK,
                encoder: "libx264",
                bitrate: 2_000_000,
                tweak: |s| s.free = Box::new(|_| Ok(0.0)),
                check: |e| {
                    assert!(e.to_string().starts_with("not enough space in "), "{e}");
                },
            },
            Case {
                name: "no room for the maps",
                ffprobe: act::PROBE_OK,
                encoder: "libx264",
                bitrate: 2_000_000,
                tweak: |_| {},
                check: |e| {
                    assert!(matches!(e, Error::Session(_)), "{e:?}");
                    assert!(e.to_string().contains("TMPDIR"), "{e}");
                },
            },
            Case {
                name: "not enough memory",
                ffprobe: act::PROBE_OK,
                encoder: "libx264",
                bitrate: 2_000_000,
                tweak: |s| s.mem = Box::new(|| Some(64 * MIB)),
                check: |e| {
                    assert!(matches!(e, Error::Encoder(_)), "{e:?}");
                    assert!(e.to_string().contains("not enough memory"), "{e}");
                },
            },
        ];

        for case in cases {
            let tools = fake(case.ffprobe, act::OK);
            let rig = Rig::new();
            let mut scn = rig.scenario(tools.resolver(), cpu_caps());
            (case.tweak)(&mut scn);
            if case.name == "no room for the maps" {
                // Room at the destination, none in the temporary directory.
                let tmp = rig.tmp.clone();
                scn.free = Box::new(move |p| if p == tmp { tmp_full(p) } else { Ok(1000.0) });
            }
            let ui = TestUi::quiet(case.encoder, case.bitrate);

            let err = scn.run(&ui).expect_err(case.name);

            (case.check)(&err);
            assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new(), "{}", case.name);
            rig.assert_clean();
        }
    }

    // Go: TestIntegration_PortraitSourceIsRefusedUpFront. The geometry check is wired into
    // the pipeline ahead of the memory check, not merely left to the defensive copy in
    // `generate_pgm`: the memory reading must never be taken.
    #[cfg(unix)]
    #[test]
    fn a_portrait_source_is_refused_before_the_memory_check_and_ffmpeg() {
        let _guard = spawn_lock();
        let portrait = act::PROBE_OK.replace(
            "\"width\":640,\"height\":480",
            "\"width\":360,\"height\":640",
        );
        assert_ne!(portrait, act::PROBE_OK, "the portrait probe was not built");
        let tools = fake(&portrait, act::OK);
        let rig = Rig::new();
        let mut scn = rig.scenario(tools.resolver(), cpu_caps());
        let memory_read = Rc::new(Cell::new(false));
        scn.mem = Box::new({
            let memory_read = Rc::clone(&memory_read);
            move || {
                memory_read.set(true);
                Some(64 * GIB)
            }
        });
        let ui = TestUi::quiet("libx264", 2_000_000);

        let err = scn.run(&ui).expect_err("a portrait frame folds");

        let text = err.to_string();
        assert!(
            text.contains("360x640") && text.contains("9:8"),
            "not refused for its geometry: {text}"
        );
        assert!(
            !memory_read.get(),
            "the memory check ran: the portrait frame was not refused before it"
        );
        assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new());
        assert_eq!(ui.progress.get(), 0);
        rig.assert_clean();
    }

    // Go: TestIntegration_CancelDuringEncoding (already cancelled before the start)
    #[cfg(unix)]
    #[test]
    fn a_conversion_cancelled_before_it_starts_is_interrupted_and_leaves_nothing() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        scn.cancel.store(true, Ordering::Release);
        let ui = TestUi::quiet("libx264", 2_000_000);

        let err = scn.run(&ui).expect_err("cancelled");

        assert!(matches!(err, Error::Cancelled), "{err:?}");
        assert!(err.to_string().contains("interrupted"), "{err}");
        assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new());
        // The stop check itself is pinned by
        // `a_stop_raised_during_the_validations_ends_the_run_before_the_session`.
        rig.assert_clean();
    }

    // ---- the bitrate ----

    // Go: PerformEncoding's `if bitrate == 0 { bitrate = source }`
    #[cfg(unix)]
    #[test]
    fn the_source_bitrate_is_used_when_the_interface_gives_none() {
        let _guard = spawn_lock();
        for (asked, expected) in [
            (Some(0), 2_500_000),
            (None, 2_500_000),
            (Some(-5), 2_500_000),
            (Some(1_500_000), 1_500_000),
        ] {
            let tools = fake(act::PROBE_OK, act::OK);
            let rig = Rig::new();
            let scn = rig.scenario(tools.resolver(), cpu_caps());
            let ui = TestUi::new(&Events::default(), "libx264", asked);

            let outcome = scn.run(&ui).expect("the conversion");

            assert_eq!(outcome.bitrate, expected, "asked {asked:?}");
            assert_eq!(outcome.metrics.output_bitrate, expected);
            let calls = tools.ffmpeg_calls();
            assert!(
                calls
                    .iter()
                    .any(|c| c.contains(&format!("-b:v {expected} "))),
                "ffmpeg was not given {expected}: {calls:?}"
            );
        }
    }

    // ---- a conversion, and what it leaves ----

    // Go: TestIntegration_PerformEncoding (with a stand-in ffmpeg: the output is the empty
    // working file, renamed)
    #[cfg(unix)]
    #[test]
    fn a_conversion_ends_with_the_destination_and_nothing_else() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        let events = Events::default();
        let ui = TestUi::new(&events, "libx264", Some(2_000_000));

        let outcome = scn.run(&ui).expect("the conversion");

        assert_eq!(names(&rig.out_dir), ["output.mp4"], "working file left");
        assert_eq!(names(&rig.tmp), Vec::<String>::new(), "session left");
        assert_eq!(outcome.encoder, "libx264");
        assert_eq!(outcome.bitrate, 2_000_000);
        assert_eq!(
            outcome.summary.hardware_acceleration,
            "Hardware: used CPU encode (libx264) + CPU decode"
        );
        let m = &outcome.metrics;
        assert!(m.success);
        assert_eq!(m.output_codec, "libx264");
        assert_eq!(m.output_bitrate, 2_000_000);
        assert_eq!(m.input_file_size, CLIP_TEXT.len() as i64);
        assert_eq!(m.output_file_size, 0, "the stand-in wrote nothing");
        assert!(m.last_progress > 0.0);
        // Go: one ShowProgressDetail per ShowProgress.
        assert!(ui.progress.get() >= 1);
        assert_eq!(ui.detail.get(), ui.progress.get());
        // The encode went to a working file next to the destination, not to the destination.
        let calls = tools.ffmpeg_calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(
            calls[0].contains(&format!("{}/.superview-partial-", rig.out_dir.display())),
            "{}",
            calls[0]
        );
        assert!(!calls[0].contains(&rig.output().display().to_string()));
    }

    // Go: TestIntegration_SqueezeKeepsTheSourceWidth (the flag reaches the maps; with a
    // stand-in ffmpeg the width is read in the session's map by the real test below)
    #[cfg(unix)]
    #[test]
    fn the_squeeze_is_asked_once() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        let events = Events::default();
        let mut ui = TestUi::new(&events, "libx264", Some(2_000_000));
        ui.squeeze = true;

        scn.run(&ui).expect("the conversion");

        let asked = events
            .borrow()
            .iter()
            .filter(|e| *e == "get_squeeze")
            .count();
        assert_eq!(asked, 1);
    }

    // Go: the deferred cleanup after a failure
    #[cfg(unix)]
    #[test]
    fn a_failure_after_frames_leaves_nothing_behind() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::LATE);
        let rig = Rig::new();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        let ui = TestUi::quiet("libx264", 2_000_000);

        let err = scn.run(&ui).expect_err("ffmpeg fails");

        // The type survives: the cascade's verdict is what the caller reads.
        assert!(matches!(err, Error::FailedAfterOutput(_)), "{err:?}");
        assert!(err.should_not_retry());
        assert_eq!(tools.ffmpeg_calls().len(), 1);
        tools.assert_no_process_left();
        rig.assert_clean();
    }

    // Go: TestIntegration_CancelLeavesNoPartialOutput, and the same for a system signal
    // (stand-in ffmpeg that hangs after its first frame).
    #[cfg(unix)]
    #[test]
    fn a_stop_leaves_no_file_no_session_and_no_process() {
        let _guard = spawn_lock();
        for by_signal in [false, true] {
            let tools = fake(act::PROBE_OK, act::HANG);
            let rig = Rig::new();
            let scn = rig.scenario(tools.resolver(), cpu_caps());
            let ui = TestUi::quiet("libx264", 2_000_000).on_progress(|_| {
                if by_signal {
                    scn.signal.store(15, Ordering::Release);
                } else {
                    scn.cancel.store(true, Ordering::Release);
                }
            });
            let started = Instant::now();

            let err = scn.run(&ui).expect_err("stopped");

            assert!(
                started.elapsed() < Duration::from_secs(20),
                "the stop did not end the encode"
            );
            if by_signal {
                assert!(matches!(err, Error::StoppedBySignal(_)), "{err:?}");
            } else {
                assert!(matches!(err, Error::Cancelled), "{err:?}");
            }
            assert!(err.is_stop_request());
            assert_eq!(tools.ffmpeg_calls().len(), 1, "one launch, then nothing");
            tools.assert_no_process_left();
            rig.assert_clean();
        }
    }

    // Go: TestIntegration_CancelLeavesNoPartialOutput, "an existing destination survives"
    #[cfg(unix)]
    #[test]
    fn a_stop_keeps_the_file_that_was_already_there() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::HANG);
        let rig = Rig::new();
        fs::write(rig.output(), "the file the user already had").unwrap();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        let ui = TestUi::quiet("libx264", 2_000_000)
            .on_progress(|_| scn.cancel.store(true, Ordering::Release));

        scn.run(&ui).expect_err("cancelled");

        assert_eq!(names(&rig.out_dir), ["output.mp4"]);
        assert_eq!(
            fs::read_to_string(rig.output()).unwrap(),
            "the file the user already had"
        );
        assert_eq!(names(&rig.tmp), Vec::<String>::new());
    }

    // A stop raised while the validations run (here by the memory reading, the last step
    // before the session) ends the run before any session exists. The reading also removes
    // the temporary directory: a session attempted anyway would fail with a session error
    // instead of the stop.
    #[cfg(unix)]
    #[test]
    fn a_stop_raised_during_the_validations_ends_the_run_before_the_session() {
        let _guard = spawn_lock();
        for by_signal in [false, true] {
            let tools = fake(act::PROBE_OK, act::OK);
            let rig = Rig::new();
            let mut scn = rig.scenario(tools.resolver(), cpu_caps());
            let tmp = rig.tmp.clone();
            if by_signal {
                scn.signal.store(2, Ordering::Release);
            } else {
                scn.cancel.store(true, Ordering::Release);
            }
            scn.mem = Box::new(move || {
                let _ = fs::remove_dir(&tmp);
                Some(64 * GIB)
            });
            let ui = TestUi::quiet("libx264", 2_000_000);

            let err = scn.run(&ui).expect_err("stopped");

            if by_signal {
                assert!(matches!(err, Error::StoppedBySignal(_)), "{err:?}");
            } else {
                assert!(matches!(err, Error::Cancelled), "{err:?}");
            }
            assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new());
            assert_eq!(names(&rig.out_dir), Vec::<String>::new());
        }
    }

    // A session that cannot be created: the error is returned and nothing exists at the
    // destination (Go: `InitEncodingSession` failing).
    #[cfg(unix)]
    #[test]
    fn a_session_that_cannot_be_created_leaves_nothing_at_the_destination() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let mut scn = rig.scenario(tools.resolver(), cpu_caps());
        scn.tmp = rig.tmp.join("does-not-exist");
        let ui = TestUi::quiet("libx264", 2_000_000);

        let err = scn
            .run(&ui)
            .expect_err("no directory to create the session in");

        assert!(matches!(err, Error::Session(_)), "{err:?}");
        assert!(
            err.to_string().contains("failed to create temp directory"),
            "{err}"
        );
        assert_eq!(tools.ffmpeg_calls(), Vec::<String>::new());
        assert_eq!(names(&rig.out_dir), Vec::<String>::new());
    }

    // Go: TestIntegration_RenameFailureKeepsTheFinishedFile
    #[cfg(unix)]
    #[test]
    fn a_failed_rename_keeps_the_finished_video_under_its_working_name() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        let rig = Rig::new();
        let scn = rig.scenario(tools.resolver(), cpu_caps());
        // The destination becomes a directory once the encode is provably running, so the
        // validation has passed and only the rename can fail.
        let output = rig.output();
        let ui = TestUi::quiet("libx264", 2_000_000).on_progress(|_| {
            let _ = fs::create_dir(&output);
        });

        let err = scn
            .run(&ui)
            .expect_err("renaming onto a directory must fail");

        let kept: Vec<String> = names(&rig.out_dir)
            .into_iter()
            .filter(|n| n.starts_with(".superview-partial-") && n.ends_with(".mp4"))
            .collect();
        assert_eq!(kept.len(), 1, "{:?}", names(&rig.out_dir));
        let kept_path = rig.out_dir.join(&kept[0]);
        let text = err.to_string();
        assert!(matches!(err, Error::Plain(_)), "{err:?}");
        assert!(
            text.starts_with(&format!(
                "failed to move the finished file to {}: ",
                rig.output().display()
            )),
            "{text}"
        );
        assert!(
            text.ends_with(&format!(
                ". The finished video is kept at {}; move or rename it yourself",
                kept_path.display()
            )),
            "{text}"
        );
        assert!(rig.output().is_dir());
        // The session is closed, finished video or not.
        assert_eq!(names(&rig.tmp), Vec::<String>::new());
    }

    // ---- the encoder and the memory guard ----

    // Go: TestIntegration_PerformEncodingAsksTheEncoderAboutTheRealFrame
    #[cfg(unix)]
    #[test]
    fn the_encoder_is_asked_about_the_real_frame() {
        let _guard = spawn_lock();
        // Everything is refused: the probe comes back "unusable" for h264_vaapi, as an
        // Intel HD 620 does at 5120x2880. The conversion itself must be launched once, on
        // the CPU encoder; without the verification it would be launched three times.
        let tools = fake(act::PROBE_OK, act::REFUSE);
        let rig = Rig::new();
        let mut scn = rig.scenario(
            tools.resolver(),
            caps("h264_vaapi,libx264,libx265", "vaapi"),
        );
        scn.cfg = Config {
            performance_mode: "safe".into(),
            ..Config::default()
        };
        let ui = TestUi::new(&Events::default(), "", None);

        scn.run(&ui).expect_err("the stand-in refuses everything");

        let calls = tools.ffmpeg_calls();
        assert!(!calls.is_empty(), "the stand-in ffmpeg was never run");
        let conversions: Vec<&String> = calls.iter().filter(|c| c.contains("remap")).collect();
        assert_eq!(conversions.len(), 1, "{conversions:#?}");
        assert!(
            conversions[0].contains("-c:v libx264"),
            "{}",
            conversions[0]
        );
        assert!(!conversions[0].contains("h264_vaapi"), "{}", conversions[0]);
        rig.assert_clean();
    }

    /// Go: TestIntegration_MemoryCheckStopsBeforeFfmpegRuns and
    /// TestIntegration_MemoryCheckPricesTheConfiguredThreads, against whichever tools.
    /// `input` is a 640x480 clip.
    fn memory_wiring(resolver: &ToolResolver, caps: &HashMap<String, String>, input: &Path) {
        // 64 MB free: far under what even this small frame needs. The refusal reaches the
        // caller, names the reason, and nothing was produced or started.
        let rig = Rig::new();
        let mut scn = rig.scenario(resolver, caps.clone());
        scn.input = input.to_path_buf();
        scn.mem = Box::new(|| Some(64 * MIB));
        let ui = TestUi::quiet("libx264", 2_000_000);
        let text = message(scn.run(&ui));
        assert!(text.contains("not enough memory"), "{text}");
        assert_eq!(
            ui.progress.get(),
            0,
            "ffmpeg was started before the memory check"
        );
        rig.assert_clean();

        // A 640x480 clip's maps are 853 wide (the encoded frame 852, an even width); 160 MB covers its four-thread estimate with the
        // quarter of headroom (about 148 MB) and not the sixteen-thread one (about 176 MB),
        // so only the configured count decides.
        for (threads, fits) in [(16, false), (4, true)] {
            let rig = Rig::new();
            let mut scn = rig.scenario(resolver, caps.clone());
            scn.input = input.to_path_buf();
            scn.cfg.encoder_threads = threads;
            scn.cfg.video_preset = "ultrafast".into();
            scn.mem = Box::new(|| Some(160 * MIB));
            let ui = TestUi::quiet("libx264", 2_000_000);
            let result = scn.run(&ui);
            if fits {
                result.unwrap_or_else(|e| panic!("{threads} threads fit in 160 MB: {e}"));
            } else {
                let text = message(result);
                assert!(
                    text.contains("not enough memory"),
                    "{threads} threads: {text}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_memory_guard_is_wired_early_and_prices_the_threads() {
        let _guard = spawn_lock();
        let tools = fake(act::PROBE_OK, act::OK);
        memory_wiring(tools.resolver(), &cpu_caps(), &Rig::new().input);
        // Refused before any ffmpeg ran, then the 4-thread conversion ran once.
        assert_eq!(tools.ffmpeg_calls().len(), 1);
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

    /// A 4:3 clip of `width`x`height`, `seconds` long at 30 fps, made by ffmpeg itself.
    fn lavfi_clip(
        resolver: &ToolResolver,
        dir: &Path,
        width: i64,
        height: i64,
        seconds: i64,
    ) -> PathBuf {
        let file = dir.join("clip.mp4");
        let cmd = resolver.ffmpeg_command([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={width}x{height}:rate=30:duration={seconds}"),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-b:v",
            "2M",
            "-pix_fmt",
            "yuv420p",
            &file.to_string_lossy(),
        ]);
        let (status, text) = crate::process::combined_output(cmd).unwrap();
        assert!(status.success(), "cannot make the test clip: {text}");
        file
    }

    fn dimensions(resolver: &ToolResolver, file: &Path) -> (i64, i64) {
        let cmd = resolver.ffprobe_command([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            &file.to_string_lossy(),
        ]);
        let (status, text) = crate::process::combined_output(cmd).unwrap();
        assert!(status.success(), "ffprobe failed: {text}");
        let (w, h) = text.trim().split_once(',').expect("width,height");
        (w.parse().unwrap(), h.parse().unwrap())
    }

    // Go: TestIntegration_PerformEncoding and TestIntegration_SqueezeKeepsTheSourceWidth
    #[test]
    fn a_real_conversion_widens_or_squeezes_the_clip() {
        let _guard = spawn_lock();
        let Some((resolver, caps)) = real_ffmpeg() else {
            return;
        };
        let work = TempDir::new().unwrap();
        let clip = lavfi_clip(&resolver, work.path(), 640, 480, 3);

        for (squeeze, want_w) in [(false, 852), (true, 640)] {
            let rig = Rig::new();
            let mut scn = rig.scenario(&resolver, caps.clone());
            scn.input = clip.clone();
            scn.cfg.video_preset = "ultrafast".into();
            // The real readings, but the temporary directory is the rig's.
            scn.free = Box::new(free_disk_gb);
            scn.mem = Box::new(available_memory_bytes);
            let events = Events::default();
            let mut ui = TestUi::new(&events, "libx264", Some(2_000_000));
            ui.squeeze = squeeze;

            let outcome = scn
                .run(&ui)
                .unwrap_or_else(|e| panic!("squeeze={squeeze}: {e}"));

            // The conversion widens 4:3 to 16:9 keeping the height; squeezed it keeps the width.
            assert_eq!(
                dimensions(&resolver, &rig.output()),
                (want_w, 480),
                "squeeze={squeeze}"
            );
            assert_eq!(names(&rig.out_dir), ["output.mp4"], "working file left");
            assert_eq!(names(&rig.tmp), Vec::<String>::new(), "session left");
            let m = &outcome.metrics;
            assert!(m.success);
            assert_eq!(m.output_codec, "libx264");
            assert!(m.output_file_size > 0);
            assert!(m.input_file_size > 0);
            assert!(ui.progress.get() > 0, "the progress never fired");
            assert_eq!(ui.detail.get(), ui.progress.get());
            assert!(!outcome.summary.hardware_acceleration.is_empty());
        }
    }

    // Go: TestIntegration_CancelLeavesNoPartialOutput, against a real ffmpeg
    #[test]
    fn a_real_encode_cancelled_from_the_progress_callback_leaves_nothing_behind() {
        let _guard = spawn_lock();
        let Some((real, caps)) = real_ffmpeg() else {
            return;
        };
        let work = TempDir::new().unwrap();
        // 60 seconds at the slowest preset: far longer than the half second before the first
        // progress report, whatever the machine.
        let clip = lavfi_clip(&real, work.path(), 480, 360, 60);

        // On Unix the resolver finds a wrapper that writes its pid and becomes the real
        // ffmpeg (exec keeps the pid), so the test can look for the process afterwards.
        #[cfg(unix)]
        let (wrapper, pids) = {
            let pids = work.path().join("pids.txt");
            let body = format!(
                "echo $$ >> '{}'\nexec '{}' \"$@\"",
                pids.display(),
                real.resolve("ffmpeg").display()
            );
            (crate::probe::tests::fake_tools(&[("ffmpeg", &body)]), pids)
        };
        #[cfg(unix)]
        let resolver = &wrapper.resolver;
        #[cfg(not(unix))]
        let resolver = &real;

        for existing in [false, true] {
            let rig = Rig::new();
            if existing {
                fs::write(rig.output(), "the file the user already had").unwrap();
            }
            let mut scn = rig.scenario(resolver, caps.clone());
            scn.input = clip.clone();
            scn.cfg.performance_mode = "safe".into();
            scn.cfg.video_preset = "veryslow".into();
            scn.free = Box::new(free_disk_gb);
            scn.mem = Box::new(available_memory_bytes);
            let ui = TestUi::quiet("libx264", 1_000_000)
                .on_progress(|_| scn.cancel.store(true, Ordering::Release));
            let started = Instant::now();

            let result = scn.run(&ui);

            assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
            assert!(ui.progress.get() >= 1, "ffmpeg was never really running");
            assert!(
                started.elapsed() < Duration::from_secs(50),
                "the cancellation did not end the encode"
            );
            // No working file, no session; the destination is as it was.
            if existing {
                assert_eq!(names(&rig.out_dir), ["output.mp4"]);
                assert_eq!(
                    fs::read_to_string(rig.output()).unwrap(),
                    "the file the user already had"
                );
            } else {
                assert_eq!(names(&rig.out_dir), Vec::<String>::new());
            }
            assert_eq!(names(&rig.tmp), Vec::<String>::new());
        }

        #[cfg(unix)]
        {
            let text = fs::read_to_string(&pids).unwrap();
            assert_eq!(text.lines().count(), 2, "one launch per run: {text}");
            for pid in text.lines() {
                assert_no_trace_of(pid);
            }
        }
    }

    // Go: TestIntegration_MemoryCheckStopsBeforeFfmpegRuns and ...PricesTheConfiguredThreads
    #[test]
    fn the_memory_guard_in_front_of_a_real_ffmpeg() {
        let _guard = spawn_lock();
        let Some((resolver, caps)) = real_ffmpeg() else {
            return;
        };
        let work = TempDir::new().unwrap();
        let clip = lavfi_clip(&resolver, work.path(), 640, 480, 1);
        memory_wiring(&resolver, &caps, &clip);
    }
}
