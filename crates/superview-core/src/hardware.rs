//! Encoder and hardware-path selection: the pure half, with the encoder lookup.
//!
//! Every verdict and every string (shown to the user and written to the log) is the
//! recorded one; `tests/fixtures/parity/decisions.json` pins `find_encoder`,
//! `software_encoder_for_fallback` and `select_hardware_decode_accel` over ~900 cases
//! (`tests/parity_decisions.rs`).
//!
//! Design notes:
//! - `analyze_machine_profile` takes the core count instead of reading it from the
//!   machine; nothing in the selection reads `cpu_cores`, so `find_encoder` and
//!   `build_hardware_plan` pass 0.
//! - An absent map is an empty `HashMap`; an absent `MachineProfile` or `VideoSpecs` is
//!   `None`.
//! - The helpers that find nothing (`hw_device_args`) return an empty `Vec`.
//! - `find_encoder`, `is_hevc_encoder`, `is_high_bit_depth` and `source_pixel_format` are
//!   here because the selection cannot run without them, and the argument builder reuses
//!   them from this module.
//! - `find_encoder` logs through `log::info!`, the attributes as `key=value` text (same
//!   convention as `config.rs`).
//! - What depends on the probe (the verified plan, the output frame size) is in
//!   `probe.rs`.

use std::collections::HashMap;

use crate::error::{EncoderError, Error, InvalidVideoError};
use crate::video::VideoSpecs;

/// Runtime hardware/software capabilities detected from ffmpeg and the host CPU.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineProfile {
    pub cpu_cores: usize,
    pub hardware_accels: Vec<String>,
    pub available_encoders: Vec<String>,
}

/// The encoder and decode path Superview expects to use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwarePlan {
    pub encoder: String,
    pub decode_accel: String,
    pub hardware_encode: bool,
    /// The cost of encoding into a family the source is not in, empty when the plan
    /// stays in the source's own family. It is part of the plan rather than derived by
    /// the caller because the plan is what the window shows before anything runs, and
    /// this is the half of it a user can act on.
    pub codec_switch: String,
}

/// Analyzes ffmpeg's capabilities (the map `check_ffmpeg` returns) for encoder
/// selection. `cpu_cores` is the host's, supplied by the caller.
pub fn analyze_machine_profile(
    ffmpeg: &HashMap<String, String>,
    cpu_cores: usize,
) -> MachineProfile {
    MachineProfile {
        cpu_cores,
        hardware_accels: split_csv(ffmpeg.get("accels").map_or("", String::as_str)),
        available_encoders: split_csv(ffmpeg.get("encoders").map_or("", String::as_str)),
    }
}

/// The profile selection uses: it never reads the core count.
fn selection_profile(ffmpeg: &HashMap<String, String>) -> MachineProfile {
    analyze_machine_profile(ffmpeg, 0)
}

/// Splits a comma-separated list, trimming each item and dropping empty ones.
pub fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

fn contains(items: &[String], wanted: &str) -> bool {
    items.iter().any(|item| item == wanted)
}

pub fn is_hardware_encoder(encoder: &str) -> bool {
    [
        "_nvenc", "_amf", "_qsv", "_vaapi", "_vulkan", "_d3d12va", "_v4l2m2m",
    ]
    .iter()
    .any(|suffix| encoder.contains(suffix))
}

/// Names the hardware device the filter graph uploads to. Any name would do; it only
/// has to be the same in `-init_hw_device` and `-filter_hw_device`.
pub(crate) const HW_DEVICE_ALIAS: &str = "sv";

/// The ffmpeg hardware device an encoder needs its frames to live on, or "" for the
/// encoders that take frames from system memory.
///
/// NVENC, AMF and QSV upload for themselves; VAAPI, Vulkan and D3D12 do not, and
/// handing them software frames fails in the filter graph -- before the driver is ever
/// consulted -- with a message about pixel formats that names neither the encoder nor
/// the device.
pub fn device_type_for_encoder(encoder: &str) -> &'static str {
    if encoder.contains("_vaapi") {
        "vaapi"
    } else if encoder.contains("_vulkan") {
        "vulkan"
    } else if encoder.contains("_d3d12va") {
        "d3d12va"
    } else {
        ""
    }
}

/// Whether an encoder has to be fed uploaded frames.
pub fn needs_device_frames(encoder: &str) -> bool {
    !device_type_for_encoder(encoder).is_empty()
}

/// The global options that create the device and point the filter graph at it. They
/// belong before `-i`, with the other global options.
pub fn hw_device_args(encoder: &str) -> Vec<String> {
    let device_type = device_type_for_encoder(encoder);
    if device_type.is_empty() {
        return Vec::new();
    }
    vec![
        "-init_hw_device".to_string(),
        format!("{device_type}={HW_DEVICE_ALIAS}"),
        "-filter_hw_device".to_string(),
        HW_DEVICE_ALIAS.to_string(),
    ]
}

/// The filter steps that move frames onto the device, for an output pixel format the
/// filter chain has already settled on.
///
/// The upload format is not the chain's output format: hardware frame pools are
/// semi-planar, so 8-bit goes up as nv12 and 10-bit as p010.
pub fn hw_upload_filters(encoder: &str, output_format: &str) -> String {
    if !needs_device_frames(encoder) {
        return String::new();
    }
    let upload_format = if output_format.ends_with("10le") {
        "p010"
    } else {
        "nv12"
    };
    format!(",format={upload_format},hwupload")
}

pub fn accel_for_encoder(encoder: &str) -> &'static str {
    if encoder.contains("_nvenc") {
        "cuda"
    } else if encoder.contains("_qsv") {
        "qsv"
    } else if encoder.contains("_vaapi") {
        "vaapi"
    } else if encoder.contains("_vulkan") {
        "vulkan"
    } else if encoder.contains("_d3d12va") {
        "d3d12va"
    } else {
        // v4l2m2m included: "drm" is a device type, not a -hwaccel token, and
        // "-hwaccel drm" fails at device creation on every machine (E-07). The encoder
        // takes frames from system memory; decoding stays on the CPU.
        ""
    }
}

pub fn decode_accel_candidates_for_encoder(encoder: &str) -> Vec<&'static str> {
    if encoder.contains("_nvenc") {
        vec!["cuda", "d3d11va", "dxva2"]
    } else if encoder.contains("_amf") {
        vec!["d3d11va", "dxva2"]
    } else if encoder.contains("_qsv") {
        // Not "qsv". ffmpeg.html: "Unlike most other values, this option does not
        // enable accelerated decoding ... but accelerated transcoding, without copying
        // the frames into the system memory." The decoded frames stay on the device,
        // and remap is a CPU filter: the graph then fails on every QSV machine, one
        // wasted ffmpeg per conversion (E-07). d3d11va and dxva2 are real hwaccels that
        // hand frames back to RAM.
        vec!["d3d11va", "dxva2"]
    } else if encoder.contains("_d3d12va") {
        vec!["d3d12va", "d3d11va", "dxva2"]
    } else {
        match accel_for_encoder(encoder) {
            "" => Vec::new(),
            required => vec![required],
        }
    }
}

fn can_use_dedicated_hardware_encoder_without_matching_accel(encoder: &str) -> bool {
    encoder.contains("_nvenc") || encoder.contains("_amf") || encoder.contains("_qsv")
}

/// The first decode accelerator the encoder accepts that the machine has, or "".
pub fn select_hardware_decode_accel(encoder: &str, profile: Option<&MachineProfile>) -> String {
    let Some(profile) = profile else {
        return String::new();
    };
    decode_accel_candidates_for_encoder(encoder)
        .into_iter()
        .find(|accel| contains(&profile.hardware_accels, accel))
        .unwrap_or_default()
        .to_string()
}

pub fn describe_planned_hardware_path(plan: &HardwarePlan) -> String {
    if plan.encoder.is_empty() {
        return "Hardware: no compatible encoder selected".to_string();
    }

    let mut line = if !plan.hardware_encode {
        format!(
            "Hardware: planned CPU encode ({}) + CPU decode",
            plan.encoder
        )
    } else if !plan.decode_accel.is_empty() {
        format!(
            "Hardware: planned {} encode + {} decode",
            plan.encoder,
            plan.decode_accel.to_uppercase()
        )
    } else {
        format!(
            "Hardware: planned {} encode + CPU decode fallback",
            plan.encoder
        )
    };

    if !plan.codec_switch.is_empty() {
        line.push_str(" -- ");
        line.push_str(&plan.codec_switch);
    }
    line
}

/// Determines the most likely encoder and decode path for the current input.
pub fn build_hardware_plan(
    ffmpeg: &HashMap<String, String>,
    video: Option<&VideoSpecs>,
    requested_encoder: &str,
) -> Result<HardwarePlan, Error> {
    let video = match video {
        Some(video) if !video.streams.is_empty() => video,
        _ => {
            return Err(InvalidVideoError {
                reason: "no video streams".to_string(),
            }
            .into());
        }
    };

    let encoder = find_encoder(requested_encoder, ffmpeg, video)?;

    let profile = selection_profile(ffmpeg);
    Ok(HardwarePlan {
        decode_accel: select_hardware_decode_accel(&encoder, Some(&profile)),
        hardware_encode: is_hardware_encoder(&encoder),
        codec_switch: describe_codec_family_switch(Some(video), &encoder),
        encoder,
    })
}

/// A user-facing summary of the planned hardware path.
pub fn describe_hardware_acceleration_plan(
    ffmpeg: &HashMap<String, String>,
    video: Option<&VideoSpecs>,
    requested_encoder: &str,
) -> String {
    if video.is_none() {
        return "Hardware: waiting for input video".to_string();
    }

    match build_hardware_plan(ffmpeg, video, requested_encoder) {
        Ok(plan) => describe_planned_hardware_path(&plan),
        Err(err) => format!("Hardware: {err}"),
    }
}

pub fn candidate_encoders_for_codec(codec: &str) -> Vec<&'static str> {
    match codec.to_lowercase().as_str() {
        // The vendor encoders first: they expose the rate control and presets this
        // pipeline actually sets. Then the vendor-neutral pair, which is the point of
        // having them -- D3D12 and Vulkan video encode are driven by the display driver
        // rather than by NVENC's own API, so they are still there when an FFmpeg build
        // demands a newer NVIDIA driver than the machine can install (ANALYSE.md
        // U-03). v4l2m2m last, then the CPU.
        "h264" | "avc" => vec![
            "h264_nvenc",
            "h264_amf",
            "h264_qsv",
            "h264_vaapi",
            "h264_d3d12va",
            "h264_vulkan",
            "h264_v4l2m2m",
            "libx264",
            "libx264rgb",
        ],
        "h265" | "hevc" => vec![
            "hevc_nvenc",
            "hevc_amf",
            "hevc_qsv",
            "hevc_vaapi",
            "hevc_d3d12va",
            "hevc_vulkan",
            "hevc_v4l2m2m",
            "libx265",
        ],
        _ => vec!["libx264", "libx265"],
    }
}

pub fn can_use_encoder_with_profile(encoder: &str, profile: Option<&MachineProfile>) -> bool {
    let Some(profile) = profile else {
        return false;
    };

    if !contains(&profile.available_encoders, encoder) {
        return false;
    }
    if !is_hardware_encoder(encoder) {
        return true;
    }
    if can_use_dedicated_hardware_encoder_without_matching_accel(encoder) {
        return true;
    }

    match accel_for_encoder(encoder) {
        "" => true,
        required => contains(&profile.hardware_accels, required),
    }
}

/// The pair of families this pipeline knows how to encode into. Everything here is
/// written against exactly these two, and a source outside them takes no part in the
/// cross-family logic below.
const FAMILY_H264: &str = "h264";
const FAMILY_HEVC: &str = "hevc";

/// Maps a codec name as ffprobe reports it onto one of the two families, or "" for
/// anything else.
pub fn normalize_codec_family(codec: &str) -> &'static str {
    match codec.trim().to_lowercase().as_str() {
        "h264" | "avc" => FAMILY_H264,
        "h265" | "hevc" => FAMILY_HEVC,
        _ => "",
    }
}

/// Whether the encoder belongs to the H.265/HEVC family.
pub fn is_hevc_encoder(encoder: &str) -> bool {
    let name = encoder.to_lowercase();
    name.contains("hevc") || name.contains("265")
}

/// Maps an encoder name onto the family it produces.
///
/// `is_hevc_encoder` is asked first because it matches on "265" as well as "hevc",
/// which is what puts libx265 in the right family; only then does the H.264 test run,
/// so libx264rgb and h264_vaapi land where they belong.
pub fn encoder_codec_family(encoder: &str) -> &'static str {
    let name = encoder.to_lowercase();
    if name.is_empty() {
        ""
    } else if is_hevc_encoder(&name) {
        FAMILY_HEVC
    } else if name.contains("h264") || name.contains("x264") {
        FAMILY_H264
    } else {
        ""
    }
}

/// The family of the video being converted, or "".
pub fn source_codec_family(video: Option<&VideoSpecs>) -> &'static str {
    match video {
        Some(video) if !video.streams.is_empty() => normalize_codec_family(&video.streams[0].codec),
        _ => "",
    }
}

/// The family a source is not in.
pub fn other_codec_family(codec: &str) -> &'static str {
    match normalize_codec_family(codec) {
        FAMILY_H264 => FAMILY_HEVC,
        FAMILY_HEVC => FAMILY_H264,
        _ => "",
    }
}

/// The CPU encoder that produces a given family.
pub fn software_encoder_for_family(family: &str) -> &'static str {
    match family {
        FAMILY_H264 => "libx264",
        FAMILY_HEVC => "libx265",
        _ => "",
    }
}

/// How a family is named to a user, who has never heard of "hevc" but knows what H.265
/// is.
pub fn display_codec_family(family: &str) -> &str {
    match family {
        FAMILY_H264 => "H.264",
        FAMILY_HEVC => "H.265",
        other => other,
    }
}

/// A usable hardware encoder from the codec family the source is *not* in, or "" when
/// there is none.
///
/// This exists because of a measured case. On an Intel HD 620 laptop, every hevc_*
/// encoder was refused by the driver -- "No usable encoding entrypoint found for profile
/// VAProfileHEVCMain" -- while h264_vaapi passed its probe. The source was HEVC, so the
/// search never looked at the H.264 list, and a machine with a working hardware encoder
/// spent a quarter of an hour on libx265 and 6 GB of memory it did not have.
///
/// A source outside the two known families yields nothing, because
/// `candidate_encoders_for_codec` answers such a source with CPU encoders only.
pub fn cross_family_hardware_encoder(codec: &str, profile: Option<&MachineProfile>) -> String {
    candidate_encoders_for_codec(other_codec_family(codec))
        .into_iter()
        .find(|candidate| {
            is_hardware_encoder(candidate) && can_use_encoder_with_profile(candidate, profile)
        })
        .unwrap_or_default()
        .to_string()
}

/// The first video stream's pixel format, or "" when it is unknown. "" is read as 8-bit
/// everywhere, which is the previous behaviour and the safe answer: no encoder can
/// reject a format we did not ask for.
pub fn source_pixel_format(video: Option<&VideoSpecs>) -> &str {
    match video {
        Some(video) if !video.streams.is_empty() => &video.streams[0].pix_fmt,
        _ => "",
    }
}

/// Whether an ffprobe pixel format name carries more than 8 bits per sample.
///
/// Two conditions, and both are needed. A sample wider than a byte has a byte order, so
/// ffmpeg always suffixes those names with "le" or "be" -- no 8-bit format has one. And
/// the digits just before that suffix are the depth, which must land in 9..16.
///
/// Each condition alone gets it wrong. Looking only for a depth substring reads "nv12"
/// and "yuv410p" -- both plainly 8-bit -- as high depth. Looking only for the
/// endianness suffix accepts packed formats like "rgb565le", where the digits are a
/// channel layout rather than a depth. Anything unrecognised is reported as 8-bit,
/// which is the direction that preserves the existing behaviour.
pub fn is_high_bit_depth(pix_fmt: &str) -> bool {
    let name = pix_fmt.trim().to_lowercase();
    let Some(trimmed) = name.strip_suffix("le").or_else(|| name.strip_suffix("be")) else {
        return false; // no byte order, so a single-byte sample
    };
    let digits = trimmed
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .count();
    if digits == 0 {
        return false;
    }
    match trimmed[trimmed.len() - digits..].parse::<i64>() {
        Ok(depth) => depth > 8 && depth <= 16,
        Err(_) => false,
    }
}

/// States what a cross-family choice costs, in the one clause the GUI has room for. It
/// returns "" for the ordinary case, where the encoder is in the source's own family.
///
/// The clause is not decoration. Moving a conversion to H.264 to reach the hardware also
/// drops a 10-bit source to 8 bits, because the filter chain keeps ten bits only for
/// HEVC encoders -- and a quality decision the user is not told about is one they will
/// discover in the output file.
pub fn describe_codec_family_switch(video: Option<&VideoSpecs>, encoder: &str) -> String {
    let source = source_codec_family(video);
    let target = encoder_codec_family(encoder);
    if source.is_empty() || target.is_empty() || source == target {
        return String::new();
    }

    let mut note = format!(
        "{} has no usable hardware encoder here, so the output is {}",
        display_codec_family(source),
        display_codec_family(target)
    );
    if target == FAMILY_H264 && is_high_bit_depth(source_pixel_format(video)) {
        note.push_str("; the 10-bit source is stored as 8-bit");
    }
    note
}

/// Picks the CPU encoder to retry with after a hardware encoder has failed outright.
///
/// The source's own family is tried first, and that is a change of rule: it used to
/// follow the family of the *failed encoder*. Since Superview now moves a conversion to
/// the other family when that is where the usable hardware is, the old rule turned "your
/// GPU cannot do H.265, so H.264 on the GPU" into "... so H.264 on the CPU" -- a codec
/// nobody had a reason to want once the GPU was out of the picture, and one that costs a
/// 10-bit source its extra bits. Falling back inside the source's family lands where the
/// machine would have gone had the cross-family move never been attempted.
///
/// The failed encoder's own family is still the second choice: it covers a source whose
/// codec is neither H.264 nor H.265, and the case where the source's CPU encoder is
/// missing from this FFmpeg build.
pub fn software_encoder_for_fallback(
    video: Option<&VideoSpecs>,
    failed: &str,
    profile: Option<&MachineProfile>,
) -> String {
    for family in [source_codec_family(video), encoder_codec_family(failed)] {
        let candidate = software_encoder_for_family(family);
        if candidate.is_empty() || candidate == failed {
            continue;
        }
        if can_use_encoder_with_profile(candidate, profile) {
            return candidate.to_string();
        }
    }
    String::new()
}

/// Appends the cost of a cross-family choice to a hardware summary line, and returns the
/// line unchanged when there was no such choice.
///
/// The summary is the one place in the window where a user learns which encoder ran, so
/// it is where the trade has to be stated. The log carries the same sentence at
/// selection time, for the reader who has only the log file.
pub fn with_codec_switch_note(video: Option<&VideoSpecs>, encoder: &str, summary: &str) -> String {
    let note = describe_codec_family_switch(video, encoder);
    if note.is_empty() {
        return summary.to_string();
    }
    format!("{summary} -- {note}")
}

/// Selects the best available video encoder for the job.
///
/// If `codec` is empty, picks the best encoder from the machine profile (GPU first, CPU
/// fallback). Otherwise the requested encoder must be in ffmpeg's list. Returns
/// `Error::Encoder` if it is not, `Error::InvalidVideo` for a video without streams.
pub fn find_encoder(
    codec: &str,
    ffmpeg: &HashMap<String, String>,
    video: &VideoSpecs,
) -> Result<String, Error> {
    if video.streams.is_empty() {
        return Err(InvalidVideoError {
            reason: "no video streams".to_string(),
        }
        .into());
    }

    let profile = selection_profile(ffmpeg);
    let mut encoder = String::new();

    if !codec.is_empty() {
        if !can_use_encoder_with_profile(codec, Some(&profile)) {
            return Err(EncoderError {
                msg: format!(
                    "encoder {codec} not available. Available encoders: {}",
                    ffmpeg.get("encoders").map_or("", String::as_str)
                ),
            }
            .into());
        }
        encoder = codec.to_string();
    } else {
        let source_codec = &video.streams[0].codec;
        if let Some(candidate) = candidate_encoders_for_codec(source_codec)
            .into_iter()
            .find(|candidate| can_use_encoder_with_profile(candidate, Some(&profile)))
        {
            encoder = candidate.to_string();
        }

        // The list above is ordered hardware first, so landing on a CPU encoder means
        // this machine has no usable hardware encoder *for the source's codec*. That is
        // not the same as having none at all, and the difference was measured: an Intel
        // HD 620 refused every hevc_* encoder and accepted h264_vaapi, and because the
        // clip was HEVC the H.264 list was never consulted. The conversion ran on
        // libx265 -- a quarter of an hour, and 6 GB of memory on a machine with 8, which
        // the kernel ended by killing ffmpeg.
        //
        // Keeping the source's codec is a preference. Using the machine's encoder is
        // worth more, and what it costs is said out loud rather than discovered in the
        // output: see describe_codec_family_switch.
        if !encoder.is_empty() && !is_hardware_encoder(&encoder) {
            let cross_family = cross_family_hardware_encoder(source_codec, Some(&profile));
            if !cross_family.is_empty() {
                log::info!(
                    "No hardware encoder for the source codec; using the other codec family: source_codec={source_codec} instead_of={encoder} encoder={cross_family} cost={}",
                    describe_codec_family_switch(Some(video), &cross_family)
                );
                encoder = cross_family;
            }
        }

        if encoder.is_empty()
            && let Some(first) = profile.available_encoders.first()
        {
            encoder = first.clone();
        }
    }

    if encoder.is_empty() {
        return Err(EncoderError {
            msg: "no encoder found".to_string(),
        }
        .into());
    }

    Ok(encoder)
}

#[cfg(test)]
mod tests;
