//! Pure functions of the window, with no dependency on Slint: the texts it shows and the
//! rules that turn a choice into numbers.

use std::error::Error as StdError;
use std::path::Path;
use std::time::Duration;

use superview_core::config::{Config, load_config};
use superview_core::metrics::EncodingMetrics;
use superview_core::probe::format_duration;

/// The first entry of the codec dropdown.
pub const USE_INPUT_CODEC: &str = "Use same video codec as input file";

/// What the window says between "Start transformation" and the first progress figure.
///
/// ffmpeg reports `out_time_ms=N/A` until a frame has come out of the filter graph, which on
/// a large frame takes seconds to a minute. Saying so keeps the window from looking stuck.
pub const STARTING_ENCODER_STATUS: &str =
    "Status: Starting the encoder... (no progress until the first frame)";

/// The extensions the file pickers offer. Both cases, because
/// extension filters are case-sensitive.
pub const SUPPORTED_INPUT_EXTENSIONS: [&str; 2] = [".mp4", ".MP4"];

/// How much the conversion multiplies the pixel count.
///
/// Widening 4:3 to 16:9 at constant height takes the width from 4h/3 to 16h/9, so the frame
/// carries exactly 4/3 as many pixels. Asking for 4/3 of the input bitrate preserves the bits
/// per pixel of the source (measured, Q-01: 1.19 by XPSNR and 1.30 by SSIM already match the
/// source's own quality, so 4/3 carries a small margin).
const WIDENED_PIXEL_RATIO: f64 = 4.0 / 3.0;

/// Appends ".mp4" unless the path already carries it, whatever the case.
/// The extension is read as follows: from the last dot
/// after the last separator, so the hidden name `.mp4` counts as carrying it.
pub fn ensure_mp4_extension(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let is_separator = |c: char| c == '/' || (cfg!(windows) && c == '\\');
    let mut extension = "";
    for (index, c) in path.char_indices().rev() {
        if is_separator(c) {
            break;
        }
        if c == '.' {
            extension = &path[index..];
            break;
        }
    }
    if extension.eq_ignore_ascii_case(".mp4") {
        path.to_string()
    } else {
        format!("{path}.mp4")
    }
}

/// Whether an error is the "ffmpeg is not installed" case, which gets a dedicated dialog with
/// an install link (it looks for "cannot find ffmpeg/ffprobe").
/// The whole chain of causes is read, as callers wrap errors on the way up.
pub fn is_missing_ffmpeg_error(err: &(dyn StdError + 'static)) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(e) = current {
        if e.to_string().contains("cannot find ffmpeg/ffprobe") {
            return true;
        }
        current = e.source();
    }
    false
}

/// `Duration.Round(time.Second)`, for a duration that cannot be negative.
fn round_to_second(duration: Duration) -> Duration {
    let seconds = (duration.as_nanos() + 500_000_000) / 1_000_000_000;
    Duration::from_secs(u64::try_from(seconds).unwrap_or(u64::MAX))
}

/// The results line. `None` before any completed run.
pub fn format_results_panel(metrics: Option<&EncodingMetrics>) -> String {
    let Some(metrics) = metrics else {
        return "Results: no completed run yet".to_string();
    };
    let input_mb = metrics.input_file_size as f64 / 1024.0 / 1024.0;
    let output_mb = metrics.output_file_size as f64 / 1024.0 / 1024.0;
    let ratio = if metrics.input_file_size > 0 {
        (metrics.output_file_size as f64 / metrics.input_file_size as f64) * 100.0
    } else {
        0.0
    };
    format!(
        "Results: in={input_mb:.1} MB | out={output_mb:.1} MB ({ratio:.1}%) | total={} | encode={}",
        format_duration(round_to_second(metrics.elapsed())),
        format_duration(metrics.encode_duration),
    )
}

/// The line shown while a conversion runs.
///
/// The estimate is left out until there is one: "about 0s left" would read as "nearly done" at
/// the very moment the encode starts. The test is on the *rounded* value, so 499 ms is none.
pub fn format_encoding_status(percent: f64, remaining: Duration) -> String {
    let rounded = round_to_second(remaining);
    if rounded.is_zero() {
        return format!("Status: Transforming... {percent:.0}%");
    }
    format!(
        "Status: Transforming... {percent:.0}% - about {} left",
        format_duration(rounded)
    )
}

/// Where the running binary comes from, as the build recorded it.
pub struct Vcs<'a> {
    pub revision: &'a str,
    pub modified: bool,
}

/// Names the running binary, for the diagnostic report and the log.
///
/// `stamped` says whether anything injected a version at all; an unstamped binary and a binary
/// stamped with a blank version are both "dev", because to the reader of a bug report they mean
/// the same thing: this did not come from a release. The revision is cut to seven characters.
pub fn build_identity(version: &str, stamped: bool, vcs: Option<&Vcs>) -> String {
    let version = version.trim();
    let version = if version.is_empty() || !stamped {
        "dev"
    } else {
        version
    };
    let Some(vcs) = vcs.filter(|vcs| !vcs.revision.is_empty()) else {
        return version.to_string();
    };
    let revision: String = vcs.revision.chars().take(7).collect();
    if vcs.modified {
        format!("{version} ({revision}, modified)")
    } else {
        format!("{version} ({revision})")
    }
}

/// Maps a quality profile onto an output bitrate and an ffmpeg preset.
/// Both profiles ask for the same bitrate; they differ by encoder
/// preset alone, which is what "Fast" claims to be: quicker, not coarser. An unexpected value
/// behaves like "Balanced".
pub fn quality_profile_settings(profile: &str, input_bitrate: i64) -> (i64, &'static str) {
    let bitrate = (input_bitrate as f64 * WIDENED_PIXEL_RATIO) as i64;
    match profile {
        "Fast" => (bitrate, "fast"),
        _ => (bitrate, "medium"),
    }
}

/// What Start hands the encoder: the profile's bitrate bounded by the configuration, and the
/// preset. An explicit `video_preset` wins over the profile's (it used to be overwritten with
/// nothing reporting the substitution). The bitrate is raised to `min_bitrate`, and lowered to
/// `max_bitrate` when that is above zero.
pub fn resolve_start_settings(cfg: &Config, profile: &str, input_bitrate: i64) -> (i64, String) {
    let (mut bitrate, profile_preset) = quality_profile_settings(profile, input_bitrate);
    let preset = if cfg.video_preset.is_empty() {
        profile_preset.to_string()
    } else {
        cfg.video_preset.clone()
    };
    if bitrate < cfg.min_bitrate {
        bitrate = cfg.min_bitrate;
    }
    if cfg.max_bitrate > 0 && bitrate > cfg.max_bitrate {
        bitrate = cfg.max_bitrate;
    }
    (bitrate, preset)
}

/// The codec dropdown's entries from ffmpeg's comma-separated encoder list.
/// Empty items are ignored, so a missing ffmpeg gives the first entry
/// alone and not a bogus " encoder".
pub fn encoder_options_for(encoder_list: &str) -> Vec<String> {
    let mut options = vec![USE_INPUT_CODEC.to_string()];
    options.extend(
        encoder_list
            .split(',')
            .map(str::trim)
            .filter(|encoder| !encoder.is_empty())
            .map(|encoder| format!("{encoder} encoder")),
    );
    options
}

/// The encoder name behind a dropdown entry: "" for the first
/// entry or nothing, else the first word.
pub fn parse_encoder_selection(selected: &str) -> String {
    let selected = selected.trim();
    if selected == USE_INPUT_CODEC {
        return String::new();
    }
    selected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Loads the configuration and never fails.
///
/// A file `load_config` refuses (an unknown key, `min_bitrate` above `max_bitrate`) is logged
/// in its own words and the built-in defaults apply. No `SUPERVIEW_*` variable is read (DEC-43), so
/// there is no step in between.
pub fn load_startup_config(config_path: Option<&Path>) -> Config {
    match load_config(config_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            log::error!(
                "Failed to load configuration, using the built-in defaults: path={} error={err}",
                config_path.map_or_else(String::new, |p| p.display().to_string())
            );
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests;
