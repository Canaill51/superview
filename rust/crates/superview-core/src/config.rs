//! Configuration, mirroring Go's `internal/common/config.go` (removed in phase 8; last release with it: v0.2.9).
//!
//! # YAML crate: `serde-saphyr`
//!
//! `serde_yaml` is archived. `serde_yaml_ng` (its fork) was rejected because it
//! still sits on `unsafe-libyaml`, a c2rust translation of libyaml that RustSec
//! flags as unmaintained (RUSTSEC-2024-0449, which names `serde_yaml_ng`). The
//! RustSec database has no entry for `serde-saphyr`, `granit-parser` or `dunce`
//! (checked 2026-10-15). `serde-saphyr` 2.0.0 is safe Rust, MIT OR Apache-2.0,
//! and refuses unknown fields (through serde's `deny_unknown_fields`, with an
//! error naming the key) and duplicate keys, as Go's strict decoder did.
//!
//! # What the port leaves out, on purpose
//!
//! * The `SUPERVIEW_*` variables (`SUPERVIEW_FFMPEG_DIR`, the only one kept, is
//!   read by the tool lookup, not here). Hence no `SUPERVIEW_CONFIG` either.
//! * The migration of retired keys (`quality_preset`, `min_video_width`,
//!   `min_video_height`): a file still carrying them is now refused, naming the
//!   key, like any other unknown key.
//! * `configOrDefault`: Rust callers take a `Config` by value or reference.
//!
//! # Differences from Go's strict decoder, on purpose
//!
//! * A quoted number (`min_bitrate: "5"`) is accepted; yaml.v3 refused it.
//! * Several documents in one file (`---`) are refused; Go read the first only.
//! * A decimal in an integer field (`1e6`, `1.5`) is refused; yaml.v3 took
//!   `1e6` and silently truncated `1.5` to 1, which was a defect.
//!
//! The file is decoded by hand (UTF-8, UTF-8 with BOM, UTF-16 LE/BE with BOM,
//! as yaml.v3 does) rather than through `serde_saphyr::from_reader`.
//!
//! The environment is never read directly: lookups take it as a parameter, so
//! no test needs `std::env::set_var` (unsafe in edition 2024, and tests run in
//! parallel).

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

/// Name of the YAML configuration file looked up by [`resolve_config_path`].
pub const CONFIG_FILE_NAME: &str = "superview.yaml";

const DEFAULT_ENCODER_CODECS: [&str; 3] = ["264", "265", "hevc"];

/// All configuration parameters for the video encoding pipeline.
///
/// Bitrates are in bits per second, as everywhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Minimum acceptable output bitrate (prevents lossy compression).
    pub min_bitrate: i64,
    /// Maximum acceptable output bitrate (controls file size).
    pub max_bitrate: i64,
    /// Template for temporary directory creation.
    pub temp_dir_prefix: String,
    /// H.264/H.265 encoder codec identifiers to recognise.
    pub encoder_codecs: Vec<String>,
    /// Verbosity of logging (debug, info, warn, error).
    pub log_level: String,
    /// `safe_performance` (try to copy the audio, fall back to AAC) or `safe`
    /// (always re-encode the audio to AAC).
    pub performance_mode: String,
    /// ffmpeg encoder preset; empty keeps ffmpeg's default.
    pub video_preset: String,
    /// ffmpeg filter graph thread count; 0 is ffmpeg's own choice.
    pub filter_threads: i64,
    /// ffmpeg encoder thread count; 0 is auto.
    pub encoder_threads: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_bitrate: 102_400,     // ~0.1 Mbps
            max_bitrate: 209_715_200, // ~200 Mbps
            temp_dir_prefix: "superview-*".into(),
            encoder_codecs: default_encoder_codecs(),
            log_level: "info".into(),
            performance_mode: "safe_performance".into(),
            video_preset: String::new(),
            filter_threads: 0,
            encoder_threads: 0,
        }
    }
}

fn default_encoder_codecs() -> Vec<String> {
    DEFAULT_ENCODER_CODECS.iter().map(|c| (*c).into()).collect()
}

/// The file as written: every key optional, so an absent key (or a `null`)
/// keeps the default. Unknown keys are an error that names them.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    min_bitrate: Option<i64>,
    max_bitrate: Option<i64>,
    temp_dir_prefix: Option<String>,
    encoder_codecs: Option<Vec<String>>,
    log_level: Option<String>,
    performance_mode: Option<String>,
    video_preset: Option<String>,
    filter_threads: Option<i64>,
    encoder_threads: Option<i64>,
}

/// Loads the configuration from a YAML file. `None` yields the defaults.
///
/// A missing file is not an error (defaults, with a log line). An unreadable
/// file, a key `Config` does not know, a wrongly typed value, or `min_bitrate`
/// above `max_bitrate` is.
pub fn load_config(path: Option<&Path>) -> Result<Config, Error> {
    let mut config = Config::default();

    if let Some(path) = path {
        match std::fs::read(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!(
                    "Config file not found, using defaults: path={}",
                    path.display()
                );
            }
            Err(e) => return Err(Error::Plain(format!("failed to read config file: {e}"))),
            Ok(bytes) => {
                let text = decode_text(&bytes)
                    .map_err(|e| Error::Plain(format!("failed to parse config file: {e}")))?;
                // An empty document (nothing, or only comments) is valid and
                // changes nothing.
                let raw: RawConfig = serde_saphyr::from_str(&text)
                    .map_err(|e| Error::Plain(format!("failed to parse config file: {e}")))?;
                apply(&mut config, raw);
                config.encoder_codecs =
                    normalize_encoder_codecs(&config.encoder_codecs, "encoder_codecs");
                log::info!("Configuration loaded from file: path={}", path.display());
            }
        }
    }

    // The GUI clamps its bitrate into [min, max]: with min above max every
    // value it can produce is one the bitrate validation then refuses.
    if config.min_bitrate > 0 && config.max_bitrate > 0 && config.min_bitrate > config.max_bitrate {
        return Err(Error::Plain(format!(
            "invalid configuration: min_bitrate ({}) is above max_bitrate ({})",
            config.min_bitrate, config.max_bitrate
        )));
    }

    config.performance_mode = normalize_performance_mode(&config.performance_mode);
    config.video_preset = normalize_video_preset(&config.video_preset);
    config.filter_threads = normalize_thread_count(config.filter_threads, "filter_threads");
    config.encoder_threads = normalize_thread_count(config.encoder_threads, "encoder_threads");
    Ok(config)
}

/// The file's text, as yaml.v3 reads it: UTF-16 (little or big endian) when it
/// opens with that byte-order mark, otherwise UTF-8, whose own BOM is dropped.
/// Invalid UTF-8 is an error.
fn decode_text(bytes: &[u8]) -> Result<String, String> {
    let utf16 = |rest: &[u8], from: fn([u8; 2]) -> u16| -> Result<String, String> {
        let (pairs, tail) = rest.as_chunks::<2>();
        if !tail.is_empty() {
            return Err("invalid UTF-16: odd number of bytes".into());
        }
        let units: Vec<u16> = pairs.iter().map(|p| from(*p)).collect();
        String::from_utf16(&units).map_err(|e| format!("invalid UTF-16: {e}"))
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] | rest => {
            String::from_utf8(rest.to_vec()).map_err(|e| format!("invalid UTF-8: {e}"))
        }
    }
}

fn apply(config: &mut Config, raw: RawConfig) {
    macro_rules! overlay {
        ($($field:ident),*) => {
            $(if let Some(value) = raw.$field { config.$field = value; })*
        };
    }
    overlay!(
        min_bitrate,
        max_bitrate,
        temp_dir_prefix,
        encoder_codecs,
        log_level,
        performance_mode,
        video_preset,
        filter_threads,
        encoder_threads
    );
}

/// Trims every codec identifier and drops the empty ones; an empty result falls
/// back to the defaults with a warning instead of emptying the encoder list.
fn normalize_encoder_codecs(codecs: &[String], source: &str) -> Vec<String> {
    let cleaned: Vec<String> = codecs
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(String::from)
        .collect();
    if cleaned.is_empty() {
        log::warn!(
            "No encoder codec configured, using the defaults: source={source} defaults={}",
            DEFAULT_ENCODER_CODECS.join(",")
        );
        return default_encoder_codecs();
    }
    cleaned
}

fn normalize_performance_mode(mode: &str) -> String {
    match mode.trim().to_lowercase().as_str() {
        "safe" => "safe".into(),
        "" | "safe_performance" | "performance" => "safe_performance".into(),
        _ => {
            log::warn!("Invalid performance_mode, falling back to safe: value={mode}");
            "safe".into()
        }
    }
}

fn normalize_video_preset(preset: &str) -> String {
    let lowered = preset.trim().to_lowercase();
    match lowered.as_str() {
        "" | "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium" | "slow"
        | "slower" | "veryslow" | "placebo" => lowered,
        _ => {
            log::warn!("Invalid video_preset, falling back to ffmpeg default: value={preset}");
            String::new()
        }
    }
}

fn normalize_thread_count(value: i64, key: &str) -> i64 {
    if value < 0 {
        log::warn!("Invalid thread count, falling back to auto: key={key} value={value}");
        return 0;
    }
    value
}

impl Config {
    /// Whether optional performance optimisations (audio copy) are enabled.
    pub fn is_safe_performance_mode(&self) -> bool {
        normalize_performance_mode(&self.performance_mode) == "safe_performance"
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Configuration:")?;
        writeln!(
            f,
            "  Min Bitrate: {} bits/sec ({:.2} Mbps)",
            self.min_bitrate,
            self.min_bitrate as f64 / 1_000_000.0
        )?;
        writeln!(
            f,
            "  Max Bitrate: {} bits/sec ({:.2} Mbps)",
            self.max_bitrate,
            self.max_bitrate as f64 / 1_000_000.0
        )?;
        writeln!(f, "  Temp Dir Prefix: {}", self.temp_dir_prefix)?;
        writeln!(f, "  Encoder Codecs: {}", self.encoder_codecs.join(","))?;
        writeln!(f, "  Log Level: {}", self.log_level)?;
        writeln!(
            f,
            "  Performance Mode: {}",
            normalize_performance_mode(&self.performance_mode)
        )?;
        writeln!(
            f,
            "  Video Preset: {}",
            normalize_video_preset(&self.video_preset)
        )?;
        writeln!(
            f,
            "  Filter Threads: {}",
            normalize_thread_count(self.filter_threads, "filter_threads")
        )?;
        writeln!(
            f,
            "  Encoder Threads: {}",
            normalize_thread_count(self.encoder_threads, "encoder_threads")
        )
    }
}

/// The per-user configuration directory, as Go's `os.UserConfigDir` finds it
/// on Windows (`%AppData%`) and Unix (`$XDG_CONFIG_HOME`, else `$HOME/.config`).
/// `env` is the environment lookup. `None` when it cannot be worked out.
pub fn user_config_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let get = |key: &str| env(key).filter(|v| !v.is_empty());
    if cfg!(windows) {
        return get("AppData").map(PathBuf::from);
    }
    if let Some(xdg) = get("XDG_CONFIG_HOME") {
        // A relative XDG_CONFIG_HOME is invalid per the spec; Go refuses it.
        return Path::new(&xdg).is_absolute().then(|| PathBuf::from(xdg));
    }
    get("HOME").map(|home| Path::new(&home).join(".config"))
}

/// The configuration file locations to probe, in priority order: next to the
/// executable (where a packaged install keeps it), the per-user directory, then
/// the working directory (a developer running from the repository root).
///
/// `exe` is the executable's path, symlinks already resolved when possible.
pub fn config_candidate_paths(exe: Option<&Path>, user_config_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) {
        candidates.push(dir.join(CONFIG_FILE_NAME));
    }
    if let Some(dir) = user_config_dir {
        candidates.push(dir.join("superview").join(CONFIG_FILE_NAME));
    }
    candidates.push(PathBuf::from(CONFIG_FILE_NAME));
    candidates
}

/// The first candidate that is an existing file (a directory named
/// `superview.yaml` is not one), or `None`; `load_config(None)` yields the
/// defaults.
pub fn resolve_config_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|c| std::fs::metadata(c).is_ok_and(|m| !m.is_dir()))
        .cloned()
}

/// [`resolve_config_path`] over the real process: the actual executable, the
/// actual environment.
pub fn find_config_path() -> Option<PathBuf> {
    let exe = std::env::current_exe()
        .ok()
        .map(|exe| dunce::canonicalize(&exe).unwrap_or(exe));
    let user = user_config_dir(&|key| std::env::var(key).ok());
    resolve_config_path(&config_candidate_paths(exe.as_deref(), user.as_deref()))
}

#[cfg(test)]
mod tests;
