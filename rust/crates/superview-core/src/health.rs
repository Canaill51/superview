//! The Diagnostic button's checks and report. Port of `internal/common/health.go`:
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
use crate::process::combined_output;
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
    match combined_output(resolver.ffprobe_command(["-version"])) {
        Ok((status, output)) if status.success() => {
            // The version is the first line.
            result.healthy = true;
            result.value = output.split('\n').next().unwrap_or("").to_string();
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
mod tests {
    use super::*;
    use crate::process::tests::spawn_lock;
    use log::{Level, Log, Metadata, Record};
    use std::path::PathBuf;
    use std::sync::Mutex;

    // ---- a logger that remembers, for log_health ----

    struct Capture(Mutex<Vec<(Level, String)>>);

    impl Log for Capture {
        fn enabled(&self, _: &Metadata) -> bool {
            true
        }
        fn log(&self, record: &Record) {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((record.level(), record.args().to_string()));
        }
        fn flush(&self) {}
    }

    static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

    /// The records logged by `f` that contain `marker`. The logger is process-wide and
    /// other tests log too, so a test finds its own records by a text only it writes.
    fn logged_with(marker: &str, f: impl FnOnce()) -> Vec<(Level, String)> {
        let _ = log::set_logger(&CAPTURE);
        log::set_max_level(log::LevelFilter::Trace);
        f();
        CAPTURE
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, text)| text.contains(marker))
            .cloned()
            .collect()
    }

    // ---- fixtures ----

    fn check(name: &str, healthy: bool, message: &str, value: &str) -> HealthCheckResult {
        HealthCheckResult {
            name: name.into(),
            healthy,
            message: message.into(),
            value: value.into(),
            timestamp: 1,
        }
    }

    fn temp() -> PathBuf {
        PathBuf::from(if cfg!(windows) { r"C:\tmp" } else { "/tmp" })
    }

    fn plenty(_: &Path) -> io::Result<f64> {
        Ok(150.0)
    }

    /// An ffmpeg that answers the three discovery queries like a real one, refuses the
    /// probe of h264_nvenc and accepts every other; an ffprobe that prints a version.
    #[cfg(unix)]
    fn healthy_tools() -> crate::probe::tests::FakeTools {
        crate::probe::tests::fake_tools(&[
            (
                "ffmpeg",
                "case \"$1\" in\n\
                 -version) echo 'ffmpeg version 9.9 Copyright'; exit 0;;\n\
                 -hwaccels) echo 'Hardware acceleration methods:'; echo cuda; exit 0;;\n\
                 -encoders) echo 'Encoders:'; echo ' ------'; echo ' V....D libx264   H.264'; echo ' V....D h264_nvenc   NVENC'; exit 0;;\n\
                 esac\n\
                 case \"$*\" in *h264_nvenc*) echo 'Driver does not support nvenc' 1>&2; exit 1;; esac\n\
                 exit 0",
            ),
            (
                "ffprobe",
                "echo 'ffprobe version 9.9 Copyright'; echo 'built with gcc'; exit 0",
            ),
        ])
    }

    // ---- the report ----

    // Go: TestGetHealthReportNil
    #[test]
    fn a_missing_health_has_a_report_of_its_own() {
        assert_eq!(get_health_report(None), "No health data available");
    }

    // Go: TestGetHealthReportFormatting, with the exact text
    #[test]
    fn the_report_lists_every_check() {
        let health = SystemHealth {
            overall: false,
            all_checks: vec![
                check("ffmpeg", true, "ok", "6.1.1"),
                check("disk_space", false, "low disk", "1.0 GB free"),
            ],
            ..SystemHealth::default()
        };
        assert_eq!(
            get_health_report(Some(&health)),
            "=== System Health Check ===\n\
             Status: ❌ UNHEALTHY\n\n\
             ✅ FFMPEG: ok\n   Value: 6.1.1\n\
             ❌ DISK_SPACE: low disk\n   Value: 1.0 GB free\n"
        );
    }

    // No Go test: a healthy report, a check without a value, and the encoder section
    // appended after a blank line.
    #[test]
    fn a_healthy_report_ends_with_the_encoder_section() {
        let health = SystemHealth {
            overall: true,
            all_checks: vec![check("ffprobe", true, "FFprobe available", "")],
            encoders: Some(EncoderProbeReport::default()),
            ..SystemHealth::default()
        };
        assert_eq!(
            get_health_report(Some(&health)),
            "=== System Health Check ===\n\
             Status: ✅ HEALTHY\n\n\
             ✅ FFPROBE: FFprobe available\n\
             \nEncoders: none to probe\n"
        );
    }

    // ---- the individual checks ----

    // Go: TestCheckDiskSpaceHealth (the real reading)
    #[test]
    fn the_real_disk_reading_fills_the_check() {
        let result = check_disk_space_health(&std::env::temp_dir(), crate::disk::free_disk_gb, 42);
        assert_eq!(result.name, "disk_space");
        assert_eq!(result.timestamp, 42);
        assert!(!result.message.is_empty());
        assert!(result.value.ends_with(" GB free"), "{}", result.value);
    }

    // No Go test of the three outcomes (it reads the real disk).
    #[test]
    fn disk_space_reports_plenty_little_and_unreadable() {
        let ok = check_disk_space_health(&temp(), plenty, 7);
        assert!(ok.healthy);
        assert_eq!(ok.value, "150.0 GB free");
        assert_eq!(
            ok.message,
            format!("Sufficient disk space available in {}", temp().display())
        );

        let low = check_disk_space_health(&temp(), |_| Ok(0.54), 7);
        assert!(!low.healthy);
        assert_eq!(low.value, "0.5 GB free");
        assert_eq!(
            low.message,
            format!(
                "Very little space in {}: 0.5 GB, and the remap maps are written there",
                temp().display()
            )
        );

        // Exactly the floor is enough: the comparison is `<`.
        assert!(check_disk_space_health(&temp(), |_| Ok(MINIMUM_TEMP_FREE_GB), 7).healthy);

        let broken = check_disk_space_health(&temp(), |_| Err(io::Error::other("boom")), 7);
        assert!(!broken.healthy);
        assert_eq!(broken.value, "");
        assert_eq!(broken.message, "Could not check temp disk: boom");
    }

    // ---- check_health ----

    // Go: TestCheckHealth_WithoutFFmpeg / withoutFFmpegOnPath. Not on Windows, where
    // winget directories PATH does not govern could still answer.
    #[cfg(not(windows))]
    #[test]
    fn without_ffmpeg_the_report_says_so() {
        let _guard = spawn_lock();
        let empty = tempfile::TempDir::new().unwrap();
        let path = empty.path().as_os_str().to_owned();
        let resolver = ToolResolver::new(
            move |key| (key == "PATH").then(|| path.clone()),
            Some(empty.path().to_path_buf()),
        );
        let health = check_health(&Config::default(), &resolver, &temp(), plenty, None);

        assert!(!health.ffmpeg.healthy, "healthy with an empty PATH");
        assert!(
            !health.ffmpeg.message.is_empty(),
            "failed without saying why"
        );
        assert!(!health.ffprobe.healthy);
        assert_eq!(health.ffprobe.message, "FFprobe not found or failed");
        assert!(health.disk.healthy);
        assert!(!health.overall, "healthy while ffmpeg is missing");
        assert!(health.encoders.is_none(), "nothing to probe without ffmpeg");

        let report = get_health_report(Some(&health));
        assert!(report.contains("UNHEALTHY"), "{report}");
        assert!(report.contains(&health.ffmpeg.message), "{report}");
        assert!(!report.contains("Encoders"), "{report}");
    }

    // Go: TestCheckHealth_ReportsEveryCheck (names, order, timestamps), Memory and CPU
    // left out on purpose, see the header.
    #[cfg(unix)]
    #[test]
    fn every_check_is_reported_in_order_with_a_timestamp() {
        let _guard = spawn_lock();
        let fake = healthy_tools();
        let health = check_health(&Config::default(), &fake.resolver, &temp(), plenty, None);

        let names: Vec<&str> = health.all_checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["ffmpeg", "ffprobe", "disk_space"]);
        assert!(
            health
                .all_checks
                .iter()
                .all(|c| c.timestamp > 1_600_000_000)
        );
        assert_eq!(health.all_checks[0], health.ffmpeg);
        assert_eq!(health.all_checks[1], health.ffprobe);
        assert_eq!(health.all_checks[2], health.disk);
        assert!(health.overall);
    }

    // Go: TestCheckHealth_WithFFmpegInstalled
    #[cfg(unix)]
    #[test]
    fn a_healthy_machine_reports_versions_and_values() {
        let _guard = spawn_lock();
        let fake = healthy_tools();
        let health = check_health(&Config::default(), &fake.resolver, &temp(), plenty, None);

        assert!(health.ffmpeg.healthy, "{}", health.ffmpeg.message);
        assert_eq!(health.ffmpeg.value, "9.9");
        assert_eq!(health.ffmpeg.message, "FFmpeg 9.9 available");
        assert!(health.ffprobe.healthy, "{}", health.ffprobe.message);
        assert_eq!(health.ffprobe.value, "ffprobe version 9.9 Copyright");
        assert_eq!(health.ffprobe.message, "FFprobe available");

        let report = get_health_report(Some(&health));
        for want in [
            "FFMPEG",
            "FFPROBE",
            "DISK_SPACE",
            "9.9",
            "Status: ✅ HEALTHY",
        ] {
            assert!(report.contains(want), "report omits {want}:\n{report}");
        }
        for gone in ["MEMORY", "CPU"] {
            assert!(
                !report.contains(gone),
                "the {gone} line is gone on purpose:\n{report}"
            );
        }
    }

    // Go: TestCheckHealth_ReportsEveryCheck, the Overall half: the three critical checks,
    // disk included.
    #[cfg(unix)]
    #[test]
    fn overall_needs_the_disk_too() {
        let _guard = spawn_lock();
        let fake = healthy_tools();
        let health = check_health(
            &Config::default(),
            &fake.resolver,
            &temp(),
            |_| Ok(0.2),
            None,
        );
        assert!(health.ffmpeg.healthy && health.ffprobe.healthy);
        assert!(!health.disk.healthy);
        assert!(!health.overall, "a full disk is unhealthy");
    }

    // Go: Overall = FFmpeg && FFprobe && Disk, the ffprobe term.
    #[cfg(unix)]
    #[test]
    fn overall_needs_ffprobe_too() {
        let _guard = spawn_lock();
        let fake = crate::probe::tests::fake_tools(&[
            (
                "ffmpeg",
                "case \"$1\" in -version) echo 'ffmpeg version 9.9 Copyright';; -hwaccels) echo H; echo cuda;; -encoders) echo E; echo ' ------'; echo ' V....D libx264 H';; esac\nexit 0",
            ),
            ("ffprobe", "exit 1"),
        ]);
        let health = check_health(&Config::default(), &fake.resolver, &temp(), plenty, None);
        assert!(health.ffmpeg.healthy && health.disk.healthy);
        assert!(!health.ffprobe.healthy);
        assert!(!health.overall);
    }

    // Go: TestCheckHealth_ReportsWhatTheEncodersAnswered
    #[cfg(unix)]
    #[test]
    fn the_report_carries_what_the_encoders_answered() {
        let _guard = spawn_lock();
        let fake = healthy_tools();
        let health = check_health(&Config::default(), &fake.resolver, &temp(), plenty, None);

        let encoders = health
            .encoders
            .as_ref()
            .expect("ffmpeg is present but no verdicts");
        assert_eq!(encoders.results.len(), 2, "{encoders:?}");
        let report = get_health_report(Some(&health));
        assert!(report.contains("Encoders"), "no encoder section:\n{report}");
        for probe in &encoders.results {
            assert!(
                report.contains(&probe.encoder),
                "{} missing:\n{report}",
                probe.encoder
            );
            // The reason is the actionable half.
            assert!(
                probe.usable || report.contains(&probe.reason),
                "{} was refused but the report omits why",
                probe.encoder
            );
        }
        assert!(
            report.contains("❌ h264_nvenc: Driver does not support nvenc"),
            "{report}"
        );
        assert!(report.contains("✅ libx264: works ("), "{report}");
        // A refused encoder does not make the machine unhealthy.
        assert!(health.overall);
    }

    // No Go test: a cancelled diagnostic starts no probe.
    #[cfg(unix)]
    #[test]
    fn a_cancelled_diagnostic_probes_no_encoder() {
        let _guard = spawn_lock();
        let fake = healthy_tools();
        let cancel = AtomicBool::new(true);
        let health = check_health(
            &Config::default(),
            &fake.resolver,
            &temp(),
            plenty,
            Some(&cancel),
        );
        assert!(health.ffmpeg.healthy, "the checks themselves still ran");
        assert!(health.encoders.is_none(), "{:?}", health.encoders);
        // As in Go, a report without verdicts has no encoder section at all (it does not
        // claim "none to probe").
        assert!(!get_health_report(Some(&health)).contains("Encoders"));
    }

    // L-50, Go: TestCheckHealth_WithFFmpegInstalled / ReportsWhatTheEncodersAnswered on the
    // machine's own tools -- the one health test that runs on Windows. Skipped without
    // ffmpeg unless SUPERVIEW_REQUIRE_FFMPEG=1, which CI sets.
    #[test]
    fn the_real_machine_is_diagnosed() {
        let _guard = spawn_lock();
        let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
        let resolver = ToolResolver::from_process();
        let health = check_health(
            &Config::default(),
            &resolver,
            &std::env::temp_dir(),
            crate::disk::free_disk_gb,
            None,
        );
        if !health.ffmpeg.healthy {
            assert!(
                !require,
                "no ffmpeg although SUPERVIEW_REQUIRE_FFMPEG=1: {}",
                health.ffmpeg.message
            );
            return;
        }
        assert!(!health.ffmpeg.value.is_empty());
        assert!(health.ffprobe.healthy, "{}", health.ffprobe.message);
        assert!(
            health.ffprobe.value.starts_with("ffprobe version"),
            "{}",
            health.ffprobe.value
        );
        let names: Vec<&str> = health.all_checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["ffmpeg", "ffprobe", "disk_space"]);
        assert_eq!(
            health.overall,
            health.ffmpeg.healthy && health.ffprobe.healthy && health.disk.healthy
        );
        let encoders = health
            .encoders
            .as_ref()
            .expect("ffmpeg is present but no verdicts");
        assert!(
            !encoders.results.is_empty(),
            "the probe ran and asked nothing"
        );
        let report = get_health_report(Some(&health));
        for probe in &encoders.results {
            assert!(
                report.contains(&probe.encoder),
                "{} missing:\n{report}",
                probe.encoder
            );
            assert!(
                probe.usable || report.contains(&probe.reason),
                "{}",
                probe.encoder
            );
        }
    }

    // ---- log_health ----

    // Go: TestLogHealth_WritesFailuresAtWarn
    #[test]
    fn failures_are_logged_at_warn() {
        let health = SystemHealth {
            overall: false,
            ffmpeg: check("ffmpeg", false, "cannot find ffmpeg zq1", ""),
            ffprobe: check("ffprobe", true, "fine zq1", ""),
            disk: check("disk_space", false, "2GB free zq1", "2"),
            all_checks: vec![
                check("ffmpeg", false, "cannot find ffmpeg zq1", ""),
                check("ffprobe", true, "fine zq1", ""),
                check("disk_space", false, "2GB free zq1", "2"),
            ],
            encoders: Some(EncoderProbeReport {
                results: vec![crate::probe::EncoderProbe {
                    encoder: "h264_nvenc".into(),
                    reason: "driver too old zq1".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        };
        let records = logged_with("zq1", || log_health(Some(&health)));
        let text: Vec<&str> = records.iter().map(|(_, t)| t.as_str()).collect();
        let all = text.join("\n");

        assert!(all.contains("cannot find ffmpeg zq1"), "{all}");
        assert!(all.contains("2GB free zq1"), "{all}");
        assert!(all.contains("driver too old zq1"), "{all}");
        assert!(
            records.iter().all(|(level, _)| *level == Level::Warn),
            "failures are warnings: {records:?}"
        );
        assert_eq!(
            records.len(),
            3,
            "one line per failed check and refused encoder: {all}"
        );
        assert!(!all.contains("ffprobe check failed"), "{all}");
    }

    // Go: TestLogHealth_WritesFailuresAtWarn, the overall line
    #[test]
    fn the_overall_status_is_logged_at_info() {
        let health = SystemHealth {
            overall: false,
            ffmpeg: check("ffmpeg", true, "", ""),
            ..SystemHealth::default()
        };
        let records = logged_with("overall=false ffmpeg=true ffprobe=false disk=false", || {
            log_health(Some(&health));
        });
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].0, Level::Info);
        assert!(
            records[0].1.starts_with("❌ UNHEALTHY: overall=false"),
            "{}",
            records[0].1
        );

        let healthy = SystemHealth {
            overall: true,
            ..SystemHealth::default()
        };
        let records = logged_with("overall=true ffmpeg=false", || log_health(Some(&healthy)));
        assert!(records[0].1.starts_with("✅ HEALTHY: "), "{}", records[0].1);
    }

    // Go: TestLogHealthWithNil
    #[test]
    fn logging_a_missing_health_does_nothing() {
        log_health(None);
    }
}
