//! The fallback cascade of an encode (one pass of ffmpeg is [`run_ffmpeg`]).
//!
//! The paths are tried in this order, and each level is given up on only for an error that
//! another path could fix: hardware decode with the hardware encoder, then CPU decode with
//! the same encoder, then the equivalent CPU encoder (`encode_video` again, once). Each
//! pass has its own audio retry (copy, then AAC, in safe-performance mode). Every gate
//! passes through [`Error::should_not_retry`]: a stop request, from the user or the system,
//! and a failure that came after frames were written end the cascade with the error as it
//! is (E-02, E-13).
//!
//! Design notes (exhaustive):
//! - The summary is the return value, [`EncodeSummary`], not a global; there is none to read
//!   after an error.
//! - The session is a parameter, the core count is `cpu_count`, the stop requests are a
//!   [`StopControl`], and the tool lookup is the `resolver`.
//! - The memory reading is injectable ([`encode_video_with_memory`]); [`encode_video`]
//!   reads the real one.
//! - A video with no stream is refused at once with an [`InvalidVideoError`].
//! - The paths of the maps are handed to ffmpeg as text. A session directory whose path is
//!   not valid UTF-8 is refused with a [`SessionError`] before anything is launched (a lossy
//!   conversion would send up to six ffmpeg after a file that does not exist, each failing
//!   with a misleading message). The output path is passed as it is.
//!   `VideoSpecs::file`, the input path, comes from a file picker and is not trusted: it is
//!   handed to `-i` as it is. Validating it is the caller's job (`security.rs`).
//! - A stop that is raised between a failed pass and the next decision is seen by the next
//!   [`run_ffmpeg`], which starts nothing; the one `falling back` warning that was already
//!   logged is not retracted (a window for the log only). Before the memory
//!   guard of the encoder fallback the stop is looked at explicitly and returned as it is
//!   (signal first, then cancellation, as in `run_ffmpeg`): otherwise a user who had just
//!   pressed Cancel would be answered "not enough memory". If the stop is a signal, the "system
//!   signalled" error line is logged a second time by that look.
//! - The logs are `log` records with `key=value` text.
//!
//! [`InvalidVideoError`]: crate::error::InvalidVideoError
//! [`SessionError`]: crate::error::SessionError

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

use crate::config::Config;
use crate::encode::args::{
    RateControl, build_encode_base_args, effective_encoder_threads, requested_encoder_threads,
};
use crate::encode::run::{StopControl, run_ffmpeg, stop_requested};
use crate::error::{Error, InvalidVideoError, SessionError};
use crate::hardware::{
    analyze_machine_profile, is_hardware_encoder, select_hardware_decode_accel,
    software_encoder_for_fallback, with_codec_switch_note,
};
use crate::memory::{available_memory_bytes, check_memory_for_frame};
use crate::session::EncodingSession;
use crate::tools::ToolResolver;
use crate::video::VideoSpecs;

/// What a successful encode says about the path that produced it: the line the window shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncodeSummary {
    pub hardware_acceleration: String,
}

/// Encodes `video` into `output` with the remap maps of `session`, falling back to safer
/// paths as described in the module header. `on_progress` receives the percentage (0 to
/// 100). `rate_control` is how the chosen encoder must be asked to spend bits; the CPU
/// fallback always goes back to a bit rate. Reads the real available memory for the guard of the CPU-encoder fallback.
#[allow(
    clippy::too_many_arguments,
    reason = "a 1:1 port of the Go signature; each parameter replaces a global or a channel of the Go code"
)]
pub fn encode_video(
    cfg: Option<&Config>,
    video: &VideoSpecs,
    encoder: &str,
    rate_control: RateControl,
    bitrate: i64,
    output: &Path,
    ffmpeg: &HashMap<String, String>,
    session: &EncodingSession,
    resolver: &ToolResolver,
    cpu_count: i64,
    stop: &StopControl,
    on_progress: &mut dyn FnMut(f64),
) -> Result<EncodeSummary, Error> {
    encode_video_with_memory(
        cfg,
        video,
        encoder,
        rate_control,
        bitrate,
        output,
        ffmpeg,
        session,
        resolver,
        cpu_count,
        stop,
        &available_memory_bytes,
        on_progress,
    )
}

/// [`encode_video`] with the memory reading chosen (`None` = cannot be read).
#[allow(
    clippy::too_many_arguments,
    reason = "a 1:1 port of the Go signature, plus the seam Go has as a package variable"
)]
pub fn encode_video_with_memory(
    cfg: Option<&Config>,
    video: &VideoSpecs,
    encoder: &str,
    rate_control: RateControl,
    bitrate: i64,
    output: &Path,
    ffmpeg: &HashMap<String, String>,
    session: &EncodingSession,
    resolver: &ToolResolver,
    cpu_count: i64,
    stop: &StopControl,
    available: &dyn Fn() -> Option<u64>,
    on_progress: &mut dyn FnMut(f64),
) -> Result<EncodeSummary, Error> {
    let Some(stream) = video.streams.first() else {
        return Err(InvalidVideoError {
            reason: "the video has no stream to encode".into(),
        }
        .into());
    };
    let duration_float = stream.duration_float;

    let default_cfg;
    let cfg = match cfg {
        Some(cfg) => cfg,
        None => {
            default_cfg = Config::default();
            &default_cfg
        }
    };
    let safe_performance_mode = cfg.is_safe_performance_mode();
    let encoder_threads = requested_encoder_threads(Some(cfg), cpu_count);
    let filter_threads = cfg.filter_threads;
    let video_preset = cfg.video_preset.as_str();
    let (x_path, y_path) = map_paths(session.x_path(), session.y_path())?;

    // One pass of ffmpeg.
    let mut run = |hwaccel: &str, audio_codec: &str| -> Result<(), Error> {
        let base_args = build_encode_base_args(
            video,
            &x_path,
            &y_path,
            encoder,
            rate_control,
            bitrate,
            audio_codec,
            encoder_threads,
            filter_threads,
            video_preset,
        );
        let mut args: Vec<OsString> = Vec::new();
        if !hwaccel.is_empty() {
            args.push("-hwaccel".into());
            args.push(hwaccel.into());
        }
        args.extend(base_args.into_iter().map(OsString::from));
        args.push(output.as_os_str().to_owned());
        run_ffmpeg(
            resolver.ffmpeg_command(args),
            duration_float,
            stop,
            on_progress,
        )
    };

    // A pass with its audio retry (copy, then AAC).
    let mut run_with_audio_fallback = |hwaccel: &str| -> Result<(), Error> {
        let preferred_audio_codec = if safe_performance_mode { "copy" } else { "aac" };
        let err = match run(hwaccel, preferred_audio_codec) {
            Ok(()) => return Ok(()),
            Err(err) => err,
        };
        if err.should_not_retry() {
            return Err(err);
        }
        if safe_performance_mode && preferred_audio_codec == "copy" {
            log::warn!("Audio stream copy failed, retrying with AAC: error={err}");
            return run(hwaccel, "aac");
        }
        Err(err)
    };

    let profile = analyze_machine_profile(ffmpeg, usize::try_from(cpu_count).unwrap_or(0));
    let hwaccel = select_hardware_decode_accel(encoder, Some(&profile));
    if !hwaccel.is_empty() {
        log::info!("Trying hardware decode+encode path: encoder={encoder} hwaccel={hwaccel}");
        match run_with_audio_fallback(&hwaccel) {
            Ok(()) => {
                return Ok(summary(
                    video,
                    encoder,
                    &format!(
                        "Hardware: used {encoder} encode + {} decode",
                        hwaccel.to_uppercase()
                    ),
                ));
            }
            // A stop request is not a reason to try another path: the user, or the system,
            // asked for the work to stop, not for it to be attempted differently. Neither
            // is a failure that came after frames were written: the next path would fail
            // the same way, one full encode later.
            Err(err) if err.should_not_retry() => return Err(err),
            Err(err) => log::warn!(
                "Hardware decode path failed, falling back to CPU decode: encoder={encoder} hwaccel={hwaccel} error={err}"
            ),
        }
    }

    log::info!(
        "Using CPU decode path: encoder={encoder} threads={encoder_threads} filter_threads={filter_threads} video_preset={video_preset}"
    );
    let err = match run_with_audio_fallback("") {
        Ok(()) => {
            let line = if is_hardware_encoder(encoder) {
                format!("Hardware: used {encoder} encode + CPU decode fallback")
            } else {
                format!("Hardware: used CPU encode ({encoder}) + CPU decode")
            };
            return Ok(summary(video, encoder, &line));
        }
        Err(err) if err.should_not_retry() => return Err(err),
        Err(err) => err,
    };

    if is_hardware_encoder(encoder) {
        let fallback_encoder = software_encoder_for_fallback(Some(video), encoder, Some(&profile));
        if !fallback_encoder.is_empty() {
            guard_fallback(
                stop,
                session,
                video,
                cfg,
                encoder,
                &fallback_encoder,
                cpu_count,
                available,
            )?;

            log::warn!(
                "Hardware encoder failed, retrying with CPU encoder: failed_encoder={encoder} fallback_encoder={fallback_encoder} error={err}"
            );
            encode_video_with_memory(
                Some(cfg),
                video,
                &fallback_encoder,
                // x264 and x265 honour a bit rate whatever the GPU driver could not (DEC-48).
                RateControl::Bitrate,
                bitrate,
                output,
                ffmpeg,
                session,
                resolver,
                cpu_count,
                stop,
                available,
                on_progress,
            )?;
            return Ok(summary(
                video,
                &fallback_encoder,
                &format!(
                    "Hardware: {encoder} failed; used CPU encode ({fallback_encoder}) + CPU decode"
                ),
            ));
        }
    }

    Err(err)
}

/// The remap maps' paths as the text ffmpeg is given, or a [`SessionError`] when one is not
/// valid UTF-8.
fn map_paths(x: &Path, y: &Path) -> Result<(String, String), Error> {
    let text = |path: &Path| {
        path.to_str().map(str::to_string).ok_or_else(|| {
            Error::Session(SessionError {
                msg: "the session directory path is not valid UTF-8".into(),
                source: None,
            })
        })
    };
    Ok((text(x)?, text(y)?))
}

/// What stands between a failed CPU pass and the encoder fallback: a stop request wins (a
/// user who pressed Cancel is not told the memory is short), then the memory guard prices
/// the encoder that is about to take over.
#[allow(
    clippy::too_many_arguments,
    reason = "the parts of the Go block, each from a different owner"
)]
fn guard_fallback(
    stop: &StopControl,
    session: &EncodingSession,
    video: &VideoSpecs,
    cfg: &Config,
    encoder: &str,
    fallback_encoder: &str,
    cpu_count: i64,
    available: &dyn Fn() -> Option<u64>,
) -> Result<(), Error> {
    if let Some(reason) = stop_requested(stop) {
        return Err(reason);
    }
    // Price the encoder that is about to take over, not the one that was priced before the
    // conversion started.
    //
    // PerformEncoding's guard was shown a hardware encoder and declined to estimate it,
    // which is correct -- none has been measured. But the encoder arriving here is a
    // software one, and on the machine that produced this finding it is the expensive one:
    // 6.2 GB of libx265 on a laptop with 8, which is how the kernel came to kill ffmpeg in
    // the first place. Falling into that silently would have made the guard a half-truth.
    if let Some((out_x, out_y)) = session.output_size()
        && let Err(mem_err) = check_memory_for_frame(
            out_x,
            out_y,
            Some(video),
            fallback_encoder,
            effective_encoder_threads(Some(cfg), fallback_encoder, cpu_count),
            available,
        )
    {
        log::error!(
            "The CPU encoder that would take over does not fit in memory: failed_encoder={encoder} fallback_encoder={fallback_encoder} error={mem_err}"
        );
        return Err(mem_err.into());
    }
    Ok(())
}

/// The summary line, with the note about a change of codec family when there was one.
fn summary(video: &VideoSpecs, encoder: &str, line: &str) -> EncodeSummary {
    EncodeSummary {
        hardware_acceleration: with_codec_switch_note(Some(video), encoder, line),
    }
}

#[cfg(test)]
mod tests;
