//! The Diagnostic button's checks and report. Port of Go's `internal/common/health.go` (removed in phase 8; last release with it: v0.2.9):
//! `CheckHealth`, `checkFFmpegHealth`, `checkFFprobeHealth`, `checkDiskSpaceHealth`,
//! `LogHealth` and `GetHealthReport` (the disk reading is `disk::free_disk_gb`).
//!
//! The README asks users to attach this report to a bug report, so its text is Go's, word
//! for word, for everything that stays.
//!
//! Deliberate differences from the Go code (exhaustive):
//! - **VISIBLE CHANGE OF THE DIAGNOSTIC: no memory line and no CPU line.** The port plan
//!   keeps only the ffmpeg, ffprobe and disk checks plus the encoder report;
//!   `checkMemoryHealth`, `checkCPUHealth` and the `Memory` / `CPU` fields are gone. The
//!   report therefore has three check lines where Go's has five, `AllChecks` has three
//!   entries, and the `memory=` and `cpu=` attributes leave the log line. Neither ever
//!   counted in `Overall`. (Decided in the plan; do not reopen here.)
//! - The tool lookup, the temporary directory and the free-space reading are parameters
//!   ([`check_health`]), where Go reads `PATH`, `os.TempDir()` and the disk. The
//!   encoder probe's cancellation is an optional flag, where Go passes
//!   `context.Background()`.
//! - ffmpeg is queried once: Go calls `CheckFfmpeg` in `checkFFmpegHealth` and again to
//!   probe the encoders; here the first answer is reused.
//! - Go never cancels this probe sweep (`context.Background()`); here a cancel flag raised
//!   before it leaves `encoders` at `None` (no encoder section in the report, as in Go for a nil
//!   `Encoders`; never an empty "none to probe" one), and
//!   one raised during it stops the sweep after the encoder being probed.
//! - `std::env::temp_dir()` keeps a trailing separator on Windows where Go's `os.TempDir()`
//!   is cleaned, so the path in the disk messages differs by that character unless the
//!   caller (the GUI) passes a cleaned `temp_dir`. It should.
//! - A Go `nil` `*SystemHealth` is `None` in [`log_health`] and [`get_health_report`], and
//!   a nil `Encoders` report is `Option` too.
//! - Messages carrying a system error (`Could not check temp disk: ...`) carry Rust's text
//!   for it, not Go's.
//! - The log is `log` records with `key=value` text, where Go uses `slog` attributes.

use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::probe::{EncoderProbeReport, Prober, describe_encoder_probe, is_cancelled};
use crate::process::combined_output_within;
use crate::tools::{ToolResolver, check_ffmpeg};

/// The result of a single health check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HealthCheckResult {
    /// Check name (e.g. "ffmpeg", "disk_space").
    pub name: String,
    /// True if the check passed.
    pub healthy: bool,
    /// Description or error details.
    pub message: String,
    /// Current value (e.g. "6.1.1", "150.0 GB free").
    pub value: String,
    /// Unix timestamp when the check was performed.
    pub timestamp: i64,
}

/// Overall system health for encoding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemHealth {
    pub overall: bool,
    pub ffmpeg: HealthCheckResult,
    pub ffprobe: HealthCheckResult,
    pub disk: HealthCheckResult,
    /// Every check performed, in order.
    pub all_checks: Vec<HealthCheckResult>,
    /// What each advertised encoder answered when asked to encode a frame. It is not one
    /// of `all_checks` and does not affect `overall`: a machine with no working hardware
    /// encoder is not unhealthy, it encodes on the CPU. But it is the section that
    /// explains why, and the report exists to be attached to bug reports.
    pub encoders: Option<EncoderProbeReport>,
}

/// The floor below which the temp directory is worth reporting, in GB. The temp
/// directory holds the two remap maps and nothing else: even the largest frame this
/// pipeline handles (7082x3984) makes about 113 MB of them, so 1 GB is generous. It used
/// to be 10 GB, which a tmpfs cannot satisfy: current distributions size `/tmp` at half
/// the RAM, so an 8 GiB machine reported UNHEALTHY for its whole life. The check that
/// decides is `check_temp_space_for_maps`, which knows the geometry.
pub const MINIMUM_TEMP_FREE_GB: f64 = 1.0;

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Performs the system health checks: ffmpeg, ffprobe and the temp directory's free
/// space, then (when ffmpeg answers) the encoder probe.
///
/// `free_gb` reads the free space of a directory (`disk::free_disk_gb` for the real one);
/// `temp_dir` is where the remap maps are written (`std::env::temp_dir()`).
pub fn check_health(
    cfg: &Config,
    resolver: &ToolResolver,
    temp_dir: &Path,
    free_gb: impl Fn(&Path) -> io::Result<f64>,
    cancel: Option<&AtomicBool>,
) -> SystemHealth {
    let now = unix_now();
    let (ffmpeg, capabilities) = ffmpeg_health(cfg, resolver, now);
    let ffprobe = check_ffprobe_health(resolver, now);
    let disk = check_disk_space_health(temp_dir, free_gb, now);

    let mut health = SystemHealth {
        // All critical checks must pass.
        overall: ffmpeg.healthy && ffprobe.healthy && disk.healthy,
        all_checks: vec![ffmpeg.clone(), ffprobe.clone(), disk.clone()],
        ffmpeg,
        ffprobe,
        disk,
        encoders: None,
    };
    // Probing needs an ffmpeg to run; without one there is nothing to ask. Nor is there
    // when the caller has already cancelled: the report then says "not probed", where an
    // empty report would claim there was "none to probe".
    if let Some(capabilities) = capabilities.filter(|_| !is_cancelled(cancel)) {
        health.encoders = Some(Prober::new(resolver).probe_hardware_support(&capabilities, cancel));
    }
    health
}

/// Verifies ffmpeg's availability and version, and hands back the capability map the
/// check read when it is healthy.
fn ffmpeg_health(
    cfg: &Config,
    resolver: &ToolResolver,
    timestamp: i64,
) -> (
    HealthCheckResult,
    Option<std::collections::HashMap<String, String>>,
) {
    let mut result = HealthCheckResult {
        name: "ffmpeg".to_string(),
        timestamp,
        ..HealthCheckResult::default()
    };
    let capabilities = match check_ffmpeg(cfg, resolver) {
        Ok(capabilities) => capabilities,
        Err(err) => {
            result.message = err.to_string();
            return (result, None);
        }
    };
    match capabilities
        .get("version")
        .filter(|version| !version.is_empty())
    {
        Some(version) => {
            result.healthy = true;
            result.value = version.clone();
            result.message = format!("FFmpeg {version} available");
            (result, Some(capabilities))
        }
        None => {
            result.message = "Could not determine FFmpeg version".to_string();
            (result, None)
        }
    }
}

/// Verifies ffmpeg's availability and version (the check alone, without the probe).
pub fn check_ffmpeg_health(
    cfg: &Config,
    resolver: &ToolResolver,
    timestamp: i64,
) -> HealthCheckResult {
    ffmpeg_health(cfg, resolver, timestamp).0
}

/// Verifies ffprobe's availability by running `ffprobe -version`.
pub fn check_ffprobe_health(resolver: &ToolResolver, timestamp: i64) -> HealthCheckResult {
    let mut result = HealthCheckResult {
        name: "ffprobe".to_string(),
        timestamp,
        ..HealthCheckResult::default()
    };
    match combined_output_within(
        resolver.ffprobe_command(["-version"]),
        resolver.query_timeout(),
    ) {
        Ok((status, output)) if status.success() => {
            // The version is the first line.
            result.healthy = true;
            result.value = output
                .split('\n')
                .next()
                .unwrap_or("")
                .trim_end()
                .to_string();
            result.message = "FFprobe available".to_string();
        }
        _ => result.message = "FFprobe not found or failed".to_string(),
    }
    result
}

/// Verifies there is room for the encoding's temporary files. It checks the temp
/// directory only, where the remap maps are written: no output has been chosen when the
/// diagnostic runs. The destination volume is checked per conversion, against the size of
/// the file to come.
pub fn check_disk_space_health(
    temp_dir: &Path,
    free_gb: impl Fn(&Path) -> io::Result<f64>,
    timestamp: i64,
) -> HealthCheckResult {
    let mut result = HealthCheckResult {
        name: "disk_space".to_string(),
        timestamp,
        ..HealthCheckResult::default()
    };
    let free = match free_gb(temp_dir) {
        Ok(free) => free,
        Err(err) => {
            result.message = format!("Could not check temp disk: {err}");
            return result;
        }
    };
    result.value = format!("{free:.1} GB free");
    if free < MINIMUM_TEMP_FREE_GB {
        result.message = format!(
            "Very little space in {}: {free:.1} GB, and the remap maps are written there",
            temp_dir.display()
        );
        return result;
    }
    result.healthy = true;
    result.message = format!("Sufficient disk space available in {}", temp_dir.display());
    result
}

/// Logs the system health status.
pub fn log_health(health: Option<&SystemHealth>) {
    let Some(health) = health else {
        return;
    };
    let status = if health.overall {
        "✅ HEALTHY"
    } else {
        "❌ UNHEALTHY"
    };
    log::info!(
        "{status}: overall={} ffmpeg={} ffprobe={} disk={}",
        health.overall,
        health.ffmpeg.healthy,
        health.ffprobe.healthy,
        health.disk.healthy
    );

    for probe in health.encoders.iter().flat_map(|report| report.unusable()) {
        log::warn!(
            "encoder advertised but unusable: encoder={} reason={}",
            probe.encoder,
            probe.reason
        );
    }
    // Details for the failed checks.
    for check in health.all_checks.iter().filter(|check| !check.healthy) {
        log::warn!(
            "{} check failed: message={} value={}",
            check.name,
            check.message,
            check.value
        );
    }
}

/// A formatted health report, as the Diagnostic button shows it.
pub fn get_health_report(health: Option<&SystemHealth>) -> String {
    let Some(health) = health else {
        return "No health data available".to_string();
    };
    let mut report = "=== System Health Check ===\n".to_string();
    report.push_str(if health.overall {
        "Status: ✅ HEALTHY\n\n"
    } else {
        "Status: ❌ UNHEALTHY\n\n"
    });
    for check in &health.all_checks {
        let status = if check.healthy { "✅" } else { "❌" };
        report.push_str(&format!(
            "{status} {}: {}\n",
            check.name.to_uppercase(),
            check.message
        ));
        if !check.value.is_empty() {
            report.push_str(&format!("   Value: {}\n", check.value));
        }
    }
    if let Some(encoders) = &health.encoders {
        report.push('\n');
        report.push_str(&describe_encoder_probe(Some(encoders)));
    }
    report
}

#[cfg(test)]
mod tests;
