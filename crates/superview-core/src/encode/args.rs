//! The ffmpeg argument list of one encode, from the bitrate validation to the preset mapping.
//!
//! Every argument and its order are the recorded ones: `tests/fixtures/recorded/encode_args.json`
//! pins the full argv of `build_encode_base_args` over 237 cases
//! (`tests/recorded_encode_args.rs`), and a different argv is a defect.
//!
//! Design notes:
//! - `requested_encoder_threads` and `effective_encoder_threads` take the logical core
//!   count as a parameter: the logic reads nothing from the machine, and a test can pick
//!   the count.
//! - An absent `Config` is `None`; no thread arguments is an empty `Vec`.
//! - `clamp_encoder_threads` logs through `log::debug!`, the attributes as `key=value` text
//!   (same convention as `hardware.rs`).
//! - The bitrate validation returns [`Error::Plain`].
//! - `build_encode_base_args` takes the video as `&VideoSpecs` (never absent), and the
//!   sizes as `i64`.
//! - `is_hevc_encoder`, `is_high_bit_depth` and `source_pixel_format` are in `hardware.rs`
//!   and are reused from there.
//! - `encoder_rate_args` saturates `-bufsize` for a bit rate above i64::MAX / 2 (v0.2.9
//!   wrapped around to a negative number, and a debug build here would panic). Only an
//!   absurd ffprobe figure with no `max_bitrate` ceiling gets there.
//! - `map_video_preset_for_encoder` lowercases with Rust's `to_lowercase`, which applies
//!   the final-sigma rule (`"ΟΣ"`) that Go's `strings.ToLower`, used by v0.2.9, did not;
//!   ffmpeg refuses such a preset either way.
//! - [`RateControl`] is new (DEC-48): the default, [`RateControl::Bitrate`], builds the
//!   argv the oracle pins, byte for byte; [`RateControl::ConstantQuality`] exists only for
//!   a VAAPI driver that answers "supported modes: CQP", which v0.2.9 had no answer for.
//! - The run and the failure classification are in `run.rs` and `failure.rs`, the encoder
//!   choice in `hardware.rs`.

use crate::config::Config;
use crate::error::Error;
use crate::hardware::{
    hw_device_args, hw_upload_filters, is_hardware_encoder, is_hevc_encoder, is_high_bit_depth,
    needs_device_frames, source_pixel_format,
};
use crate::video::VideoSpecs;

/// The largest `-threads` value libx265 accepts.
///
/// ffmpeg maps `-threads` onto x265's `--frame-threads`, which rejects anything higher
/// with "frameNumThreads (--frame-threads) must be [0 .. X265_MAX_FRAME_THREADS)" and
/// then fails to open the encoder at all. Since the default thread count is one per core,
/// every H.265 encode failed outright on a machine with more than 16 logical cores -- and
/// libx265 is a CPU encoder, so the hardware-to-CPU fallback never fires for it. Verified
/// against ffmpeg 8.0.1: 16 works, 17 does not.
pub const X265_MAX_FRAME_THREADS: i64 = 16;

/// How the encoder is asked to spend bits (DEC-48).
///
/// [`Bitrate`](Self::Bitrate) is what every encode and probe has always asked for. Some
/// Intel VAAPI drivers accept no rate-control mode but CQP ("Driver does not support any
/// RC mode compatible with selected options (supported modes: CQP)"): there a bit rate is
/// refused outright and a fixed quantiser is the only thing that works.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RateControl {
    /// `-b:v` with a `-maxrate` ceiling; the default.
    #[default]
    Bitrate,
    /// `-rc_mode CQP -qp N`: a constant quality, the file size follows the content.
    ConstantQuality,
}

impl RateControl {
    /// A stable word for cache keys and logs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Bitrate => "bitrate",
            Self::ConstantQuality => "cqp",
        }
    }
}

/// The CQP quantiser for H.264 at the Balanced profile (preset `medium` or none).
/// Measured on 2026-10-19 with the bundled FFmpeg on a Tiger Lake: `h264_vaapi -rc_mode CQP
/// -qp 23` writes a valid MP4. 23 is x264's own default CRF, a visually transparent
/// middle for a 16:9 stretch of a camera clip.
pub const CQP_H264_BALANCED: i64 = 23;

/// The CQP quantiser for HEVC at the Balanced profile. Measured on the same machine
/// (`hevc_vaapi -qp 25`). Two above H.264's: HEVC reaches the same look at a higher
/// quantiser, which is why x265's default CRF is 28 against x264's 23.
pub const CQP_HEVC_BALANCED: i64 = 25;

/// Added to the quantiser when the configured preset is `fast`: a coarser quality, the
/// same direction the Fast profile takes on every other path (a smaller file sooner).
pub const CQP_FAST_OFFSET: i64 = 2;

/// The `-qp` for an encoder and the configured preset (x264-style name; "" is Balanced).
pub fn constant_quality_qp(encoder: &str, video_preset: &str) -> i64 {
    let base = if is_hevc_encoder(encoder) {
        CQP_HEVC_BALANCED
    } else {
        CQP_H264_BALANCED
    };
    if video_preset.trim().eq_ignore_ascii_case("fast") {
        base + CQP_FAST_OFFSET
    } else {
        base
    }
}

/// Checks that the bitrate is within the acceptable constraints. `min_bitrate` and
/// `max_bitrate` are in bits per second; a constraint of 0 is not applied.
pub fn validate_bitrate(bitrate: i64, min_bitrate: i64, max_bitrate: i64) -> Result<(), Error> {
    if bitrate <= 0 {
        return Err(Error::Plain(format!(
            "bitrate must be positive, got {bitrate}"
        )));
    }
    if min_bitrate > 0 && bitrate < min_bitrate {
        return Err(Error::Plain(format!(
            "bitrate {bitrate} is below minimum {min_bitrate} bits/second"
        )));
    }
    if max_bitrate > 0 && bitrate > max_bitrate {
        return Err(Error::Plain(format!(
            "bitrate {bitrate} exceeds maximum {max_bitrate} bits/second"
        )));
    }
    Ok(())
}

/// Whether a conversion keeps its source's ten bits.
///
/// One function rather than the same condition written twice: the filter chain decides
/// the working depth here, and the memory estimate has to reach the same answer -- ten
/// bits costs about half as much memory again, so a copy of this test that drifted would
/// silently make the estimate wrong by that much.
pub fn encodes_in_ten_bits(pix_fmt: &str, encoder: &str) -> bool {
    is_high_bit_depth(pix_fmt) && is_hevc_encoder(encoder)
}

/// Builds the filter graph, choosing the working and output pixel formats from the
/// source depth and the target encoder.
///
/// The remap filter needs a planar 4:4:4 intermediate, so a conversion is unavoidable;
/// the question is only what it converts *to*. The chain used to be pinned to 8-bit,
/// which silently flattened every 10-bit source -- and HERO 10 and later GoPros, the
/// cameras this tool exists for, record 10-bit.
///
/// Ten bits are kept only for HEVC encoders. HEVC Main10 is supported by libx265 and by
/// every consumer hardware HEVC encoder, whereas h264_nvenc cannot encode 10-bit at all
/// and libx264's High 10 profile plays back poorly. GoPro's 10-bit modes record HEVC, so
/// the case that matters is covered without risking a failed encode on the H.264 path.
/// Sources deeper than 10 bits are brought to 10, not to their native depth: nothing in
/// this pipeline targets Main12.
pub fn remap_filter_chain(pix_fmt: &str, encoder: &str) -> String {
    format!(
        "[0:v:0][1:v:0][2:v:0]remap,{}[v]",
        pixel_format_chain(pix_fmt, encoder)
    )
}

/// Everything after remap: the working and output pixel formats, then the upload for the
/// encoders that need their frames on the device. The probe runs exactly this chain on a
/// synthetic source, so a frame the conversion would hand the encoder is a frame the
/// probe has already handed it -- depth included (E-01).
pub fn pixel_format_chain(pix_fmt: &str, encoder: &str) -> String {
    let (intermediate, output) = if encodes_in_ten_bits(pix_fmt, encoder) {
        ("yuv444p10le", "yuv420p10le")
    } else {
        ("yuv444p", "yuv420p")
    };
    // remap is a CPU filter, so the frames are in system memory at this point whatever
    // the encoder. The ones that cannot take them from there get the upload appended.
    format!(
        "format={intermediate},format={output}{}",
        hw_upload_filters(encoder, output)
    )
}

/// Lowers the requested thread count to what the encoder can actually accept. Only
/// libx265 has a ceiling low enough to matter; x264 clamps internally instead of failing.
pub fn clamp_encoder_threads(encoder: &str, threads: i64) -> i64 {
    if encoder == "libx265" && threads > X265_MAX_FRAME_THREADS {
        log::debug!(
            "Capping encoder threads for libx265 requested={threads} capped={X265_MAX_FRAME_THREADS}"
        );
        return X265_MAX_FRAME_THREADS;
    }
    threads
}

/// The thread count asked for, before any encoder clamps it: the configured value, else
/// one per core (`cpu_count`).
pub fn requested_encoder_threads(cfg: Option<&Config>, cpu_count: i64) -> i64 {
    match cfg {
        Some(cfg) if cfg.encoder_threads > 0 => cfg.encoder_threads,
        _ => cpu_count,
    }
}

/// The `-threads` value an encode with this encoder really passes to ffmpeg. The memory
/// guard prices this count, not the host's core count, so the two cannot disagree about
/// libx265's 16-thread ceiling.
pub fn effective_encoder_threads(cfg: Option<&Config>, encoder: &str, cpu_count: i64) -> i64 {
    clamp_encoder_threads(encoder, requested_encoder_threads(cfg, cpu_count))
}

/// The `-threads` option for the chosen encoder, or nothing at all.
///
/// A hardware encoder does its work on the GPU's dedicated block; the CPU thread count
/// says nothing about it, and passing the host core count was only ever noise in the
/// command line. Software encoders get the value, clamped to what they accept.
pub fn encoder_thread_args(encoder: &str, threads: i64) -> Vec<String> {
    if is_hardware_encoder(encoder) {
        return Vec::new();
    }
    vec![
        "-threads".to_string(),
        clamp_encoder_threads(encoder, threads).to_string(),
    ]
}

/// The argument list of one encode, up to and including the audio and filter options.
#[allow(
    clippy::too_many_arguments,
    reason = "the recorded matrix drives every parameter"
)]
pub fn build_encode_base_args(
    video: &VideoSpecs,
    x_path: &str,
    y_path: &str,
    encoder: &str,
    rate_control: RateControl,
    bitrate: i64,
    audio_codec: &str,
    encoder_threads: i64,
    filter_threads: i64,
    video_preset: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-progress",
        "pipe:1",
        "-loglevel",
        "error",
        "-y",
    ]
    .map(String::from)
    .to_vec();
    // No "-re" here on purpose. It throttles reading the input to its native frame rate,
    // which only makes sense when streaming to a realtime sink. For a file-to-file
    // transcode it just caps the run at the clip's duration, so a 10-minute video could
    // never convert in less than 10 minutes regardless of the hardware.

    // Before -i, with the other global options: this creates the device the filter graph
    // uploads to, and is nothing at all for the encoders that take frames from system
    // memory.
    args.extend(hw_device_args(encoder));

    args.extend(
        [
            // DEC-52: the source is read as MP4, never as what its content looks like (a
            // concat script named .mp4 would read other files). Only this input: the maps
            // are ours.
            "-f",
            "mov",
            "-i",
            video.file.as_str(),
            "-i",
            x_path,
            "-i",
            y_path,
            "-filter_complex",
            &remap_filter_chain(source_pixel_format(Some(video)), encoder),
            // Explicit maps. Left to itself with three inputs, ffmpeg picks one stream per
            // type and keeps a single audio track -- a camera or an edit carrying two of
            // them silently lost one. "0:a?" is optional on purpose: the "?" is what keeps
            // a clip with no audio from failing.
            "-map",
            "[v]",
            "-map",
            "0:a?",
            // With three inputs ffmpeg cannot tell which one holds the global metadata, so
            // it kept none and the recording date was dropped. Point it at the source.
            "-map_metadata",
            "0",
        ]
        .map(String::from),
    );

    // Everything the encoder itself is asked for comes from one function the probe
    // shares, so the two cannot drift apart again (E-01).
    args.extend(encoder_facing_args(
        encoder,
        rate_control,
        bitrate,
        video_preset,
    ));

    // After -c:v so it applies to the encoder. Placed before -i it would have configured
    // the input decoder instead, which is not the intent.
    args.extend(encoder_thread_args(encoder, encoder_threads));

    args.push("-c:a".to_string());
    args.push(audio_codec.to_string());

    if filter_threads > 0 {
        args.push("-filter_threads".to_string());
        args.push(filter_threads.to_string());
    }

    if encoder == "libx265" {
        args.push("-x265-params".to_string());
        args.push("log-level=error".to_string());
    }

    args
}

/// What the encoder itself is asked for: which codec, what rate control, which preset,
/// which tag. One function, used by the conversion and by the probe, so the probe cannot
/// pass an encoder that the conversion then fails on. It used to: the probe passed none
/// of this, and an Intel driver that only knows the CQP rate-control mode accepted the
/// probe and refused every conversion, silently, onto the CPU (E-01).
///
/// `rate_control` is part of the question, so it travels through here for the probe and
/// the conversion alike (DEC-05, DEC-48).
pub fn encoder_facing_args(
    encoder: &str,
    rate_control: RateControl,
    bitrate: i64,
    video_preset: &str,
) -> Vec<String> {
    let mut args = vec!["-c:v".to_string(), encoder.to_string()];
    args.extend(encoder_rate_args(
        encoder,
        rate_control,
        bitrate,
        video_preset,
    ));
    let mapped_preset = map_video_preset_for_encoder(encoder, video_preset);
    if !mapped_preset.is_empty() {
        args.push("-preset".to_string());
        args.push(mapped_preset);
    }
    if is_hevc_encoder(encoder) {
        // MP4 can tag HEVC as "hev1" or "hvc1"; ffmpeg's default is hev1, which QuickTime,
        // iOS Photos and Final Cut refuse to open. hvc1 is what every camera writes.
        // Measured: without this the output reads codec_tag_string=hev1 (E-09).
        args.push("-tag:v".to_string());
        args.push("hvc1".to_string());
    }
    args
}

/// The rate control every encode and every probe asks for.
///
/// -b:v on its own is an *average* target with no ceiling, so a demanding scene
/// overshoots freely: measured at +83% on incompressible content (8 Mbps requested,
/// 14.7 Mbps produced). Constraining the VBV brings that to +24% for the same request.
/// maxrate equal to the target with a buffer of twice it measured better than the usual
/// 1.5x headroom (+24% against +29%) and keeps the resulting file size predictable, which
/// is the point: the quality profile already builds its own headroom into the request.
///
/// In [`RateControl::ConstantQuality`] there is no bit rate at all: `-rc_mode CQP -qp N`,
/// and none of `-b:v`, `-maxrate`, `-bufsize`, which the CQP-only drivers reject.
pub fn encoder_rate_args(
    encoder: &str,
    rate_control: RateControl,
    bitrate: i64,
    video_preset: &str,
) -> Vec<String> {
    if rate_control == RateControl::ConstantQuality {
        return vec![
            "-rc_mode".to_string(),
            "CQP".to_string(),
            "-qp".to_string(),
            constant_quality_qp(encoder, video_preset).to_string(),
        ];
    }
    vec![
        "-b:v".to_string(),
        bitrate.to_string(),
        "-maxrate".to_string(),
        bitrate.to_string(),
        "-bufsize".to_string(),
        bitrate.saturating_mul(2).to_string(),
    ]
}

/// Translates an x264-style preset name into what the target encoder accepts, or "" when
/// the encoder takes none.
///
/// The names in the configuration are x264's, and only libx264 and libx265 take them
/// verbatim. NVENC's option parser rejects most of them outright -- measured on ffmpeg
/// 8.0.1: "ultrafast", "superfast", "veryfast", "faster", "slower", "veryslow" and
/// "placebo" all fail with "Undefined constant or missing '(' in 'veryslow'", exit 234,
/// before the driver is consulted -- and the cascade then landed on libx264 with nothing
/// saying why (E-08). NVENC's own scale is p1 (fastest) to p7 (slowest); its legacy "fast"
/// is an alias of p1, the coarsest of all, which is not what "Fast" promises. The encoders
/// that take frames on the device have no preset to speak of: an unknown -preset is only a
/// warning there, but a warning on every run is noise.
pub fn map_video_preset_for_encoder(encoder: &str, video_preset: &str) -> String {
    let preset = video_preset.trim().to_lowercase();
    if preset.is_empty() {
        return String::new();
    }

    if encoder.contains("_nvenc") {
        return match preset.as_str() {
            "ultrafast" | "superfast" | "veryfast" => "p1",
            "faster" => "p2",
            "fast" => "p3",
            "medium" => "p4",
            "slow" => "p5",
            "slower" => "p6",
            "veryslow" | "placebo" => "p7",
            _ => "",
        }
        .to_string();
    }

    if needs_device_frames(encoder) || encoder.contains("_v4l2m2m") {
        return String::new();
    }

    if encoder.contains("_amf") {
        return match preset.as_str() {
            "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
            "medium" => "balanced",
            "slow" | "slower" | "veryslow" | "placebo" => "quality",
            _ => "",
        }
        .to_string();
    }

    if encoder.contains("_qsv") {
        return match preset.as_str() {
            "ultrafast" | "superfast" => "veryfast".to_string(),
            "placebo" => "veryslow".to_string(),
            "veryfast" | "faster" | "fast" | "medium" | "slow" | "slower" | "veryslow" => preset,
            _ => String::new(),
        };
    }

    preset
}

#[cfg(test)]
mod tests;
