//! One whole conversion: `PerformEncoding` in Go's `internal/common/pipeline.go` (removed in phase 8; last release with it: v0.2.9) (and the
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
//! - A stop requested while the validations ran (Cancel ends the encoder probe at once; a
//!   system signal is only seen once it returns) is returned as it is just before the
//!   session is created: nothing is created, no map is generated, no ffmpeg is started. Go
//!   goes on to create the session and the maps, launches ffmpeg and kills it.
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
        Some(c.stop.cancel),
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

    // A stop asked while the validations ran ends the run here, before the session and the
    // maps (large frames make those expensive) rather than after them. Cancel also ends the
    // verification at the output size under way; a system signal does not, so it may wait for
    // the probe's timeout (twice, in constant quality).
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
mod tests;
