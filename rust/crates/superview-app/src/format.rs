//! Pure functions of the window, with no dependency on Slint: the texts it shows and the
//! rules that turn a choice into numbers. Port of the free functions of
//! Go's `cmd/superview/gui_main.go` (removed in phase 8; last release with it: v0.2.9); the Go test that guarded each one is named beside it.

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

/// The extensions the file pickers offer (Go: `supportedInputExtensions`). Both cases, because
/// extension filters are case-sensitive.
pub const SUPPORTED_INPUT_EXTENSIONS: [&str; 2] = [".mp4", ".MP4"];

/// How much the conversion multiplies the pixel count.
///
/// Widening 4:3 to 16:9 at constant height takes the width from 4h/3 to 16h/9, so the frame
/// carries exactly 4/3 as many pixels. Asking for 4/3 of the input bitrate preserves the bits
/// per pixel of the source (measured, Q-01: 1.19 by XPSNR and 1.30 by SSIM already match the
/// source's own quality, so 4/3 carries a small margin).
const WIDENED_PIXEL_RATIO: f64 = 4.0 / 3.0;

/// Appends ".mp4" unless the path already carries it, whatever the case (Go:
/// `ensureMP4Extension`). The extension is read as Go's `filepath.Ext` does: from the last dot
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
/// an install link (Go: `isMissingFFmpegError`, which looks for "cannot find ffmpeg/ffprobe").
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

/// The results line (Go: `formatResultsPanel`). `None` before any completed run.
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

/// The line shown while a conversion runs (Go: `formatEncodingStatus`).
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

/// Names the running binary, for the diagnostic report and the log (Go: `buildIdentity`).
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

/// Maps a quality profile onto an output bitrate and an ffmpeg preset (Go:
/// `qualityProfileSettings`). Both profiles ask for the same bitrate; they differ by encoder
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

/// The codec dropdown's entries from ffmpeg's comma-separated encoder list (Go:
/// `encoderOptionsFor`). Empty items are ignored, so a missing ffmpeg gives the first entry
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

/// The encoder name behind a dropdown entry (Go: `ParseEncoderSelection`): "" for the first
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

/// Loads the configuration and never fails (Go: `loadStartupConfig`).
///
/// A file `load_config` refuses (an unknown key, `min_bitrate` above `max_bitrate`) is logged
/// in its own words and the built-in defaults apply. Go then tried the environment alone; the
/// port reads no `SUPERVIEW_*` variable, so there is no step in between.
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
mod tests {
    use std::fmt;
    use std::time::Instant;

    use superview_core::error::Error;
    #[cfg(unix)]
    use superview_core::tools::{ToolResolver, check_ffmpeg};

    use super::*;

    // TestEnsureMP4Extension
    #[test]
    fn ensure_mp4_extension_appends_unless_present() {
        let cases = [
            ("/home/u/out", "/home/u/out.mp4"),
            ("/home/u/out.mp4", "/home/u/out.mp4"),
            ("/home/u/out.MP4", "/home/u/out.MP4"),
            ("/home/u/out.Mp4", "/home/u/out.Mp4"),
            ("/home/u/out.mkv", "/home/u/out.mkv.mp4"),
            ("/home/u/my.video.clip", "/home/u/my.video.clip.mp4"),
            ("C:/Users/u/out", "C:/Users/u/out.mp4"),
            ("", ""),
            // A dot in a directory name is not an extension.
            ("/home/u.d/out", "/home/u.d/out.mp4"),
            (".mp4", ".mp4"),
        ];
        for (input, want) in cases {
            assert_eq!(ensure_mp4_extension(input), want, "input {input:?}");
        }
    }

    // TestEnsureMP4ExtensionMatchesTheInputFilter
    #[test]
    fn ensure_mp4_extension_matches_the_input_filter() {
        for ext in SUPPORTED_INPUT_EXTENSIONS {
            let path = format!("/home/u/clip{ext}");
            assert_eq!(
                ensure_mp4_extension(&path),
                path,
                "an accepted input extension must be left untouched"
            );
        }
    }

    // TestSupportedInputExtensionsAreMP4Only
    #[test]
    fn supported_input_extensions_are_mp4_only_in_both_cases() {
        assert!(
            SUPPORTED_INPUT_EXTENSIONS
                .iter()
                .all(|ext| ext.eq_ignore_ascii_case(".mp4"))
        );
        assert!(SUPPORTED_INPUT_EXTENSIONS.contains(&".mp4"));
        assert!(SUPPORTED_INPUT_EXTENSIONS.contains(&".MP4"));
    }

    /// An error that hides its cause from its own text, as a wrapper that only says "startup
    /// failed" does.
    #[derive(Debug)]
    struct Opaque(Box<dyn StdError + 'static>);
    impl fmt::Display for Opaque {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("startup failed")
        }
    }
    impl StdError for Opaque {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(self.0.as_ref())
        }
    }

    // TestIsMissingFFmpegError
    #[test]
    fn is_missing_ffmpeg_error_matches_only_the_real_message() {
        let other = Error::Plain("some other failure".into());
        assert!(!is_missing_ffmpeg_error(&other), "unrelated errors");
        check_wrappers("cannot find ffmpeg/ffprobe on your system");
        assert!(!is_missing_ffmpeg_error(&Opaque(Box::new(other))));
    }

    /// The same, with the error `check_ffmpeg` really produces, so that a change of its
    /// wording breaks this test. Unix only: on Windows the system search finds the
    /// machine's own ffmpeg whatever the injected environment says.
    #[cfg(unix)]
    #[test]
    fn is_missing_ffmpeg_error_matches_what_check_ffmpeg_reports() {
        let resolver = ToolResolver::new(|_| None, None);
        let missing = check_ffmpeg(&Config::default(), &resolver)
            .expect_err("no ffmpeg can be found without a PATH");
        assert!(is_missing_ffmpeg_error(&missing), "{missing}");
        check_wrappers(&missing.to_string());
    }

    /// `message` must be recognised bare, wrapped in text, and behind a wrapper error.
    fn check_wrappers(message: &str) {
        assert!(is_missing_ffmpeg_error(&Error::Plain(message.into())));
        // Wrapped, as the pipeline wraps errors on the way up: in the text...
        let wrapped = Error::Plain(format!("startup: {message}"));
        assert!(is_missing_ffmpeg_error(&wrapped));
        // ... and only in the chain of causes.
        let hidden = Opaque(Box::new(Error::Plain(message.into())));
        assert!(
            is_missing_ffmpeg_error(&hidden),
            "a cause hidden behind a wrapper must still match"
        );
    }

    fn metrics(input: i64, output: i64) -> EncodingMetrics {
        let mut m = EncodingMetrics::new_at("in.mp4", "out.mp4", Instant::now());
        m.input_file_size = input;
        m.output_file_size = output;
        m
    }

    // TestFormatResultsPanel
    #[test]
    fn format_results_panel_reports_sizes_ratio_and_times() {
        assert_eq!(format_results_panel(None), "Results: no completed run yet");

        let mut m = metrics(100 * 1024 * 1024, 150 * 1024 * 1024);
        m.encode_duration = Duration::from_secs(42);
        m.end = Some(m.start + Duration::from_millis(83_400));
        assert_eq!(
            format_results_panel(Some(&m)),
            "Results: in=100.0 MB | out=150.0 MB (150.0%) | total=1m23s | encode=42s"
        );

        // The encode time keeps the millisecond, the total does not.
        m.encode_duration = Duration::from_micros(1_500_400);
        m.end = Some(m.start + Duration::from_millis(500));
        assert_eq!(
            format_results_panel(Some(&m)),
            "Results: in=100.0 MB | out=150.0 MB (150.0%) | total=1s | encode=1.5s"
        );
    }

    // TestFormatResultsPanel_ZeroInputSizeDoesNotDivideByZero
    #[test]
    fn format_results_panel_zero_input_size_gives_no_non_finite_ratio() {
        let mut m = metrics(0, 1024);
        m.end = Some(m.start);
        let text = format_results_panel(Some(&m));
        assert!(!text.contains("NaN") && !text.contains("inf"), "{text}");
        assert!(text.contains("(0.0%)"), "{text}");
    }

    // TestFormatEncodingStatus
    #[test]
    fn format_encoding_status_hides_an_estimate_that_rounds_to_zero() {
        let ms = Duration::from_millis;
        let s = Duration::from_secs;
        let cases = [
            // Nothing to extrapolate from yet: never "about 0s left".
            (0.0, Duration::ZERO, "Status: Transforming... 0%"),
            (42.4, s(25), "Status: Transforming... 42% - about 25s left"),
            // Under half a second rounds to zero, and the line must not say so.
            (99.9, ms(499), "Status: Transforming... 100%"),
            (
                99.9,
                ms(500),
                "Status: Transforming... 100% - about 1s left",
            ),
            (
                71.6,
                ms(80_400),
                "Status: Transforming... 72% - about 1m20s left",
            ),
        ];
        for (percent, remaining, want) in cases {
            assert_eq!(
                format_encoding_status(percent, remaining),
                want,
                "{percent} {remaining:?}"
            );
        }
    }

    // TestBuildIdentity (R-04)
    #[test]
    fn build_identity_covers_the_degraded_cases() {
        let rev = "85b66712872f83ec87bfc7f9cfc38d128cd3a88d";
        let vcs = |modified| Vcs {
            revision: rev,
            modified,
        };
        // (name, version, stamped, vcs, want)
        let cases: [(&str, &str, bool, Option<Vcs>, &str); 9] = [
            // An unpackaged build must not present a placeholder as a release.
            (
                "unstamped build reads as dev",
                "0.0.1",
                false,
                Some(vcs(true)),
                "dev (85b6671, modified)",
            ),
            ("unstamped, no vcs", "0.0.1", false, None, "dev"),
            (
                "released build",
                "0.2.3",
                true,
                Some(vcs(false)),
                "0.2.3 (85b6671)",
            ),
            (
                "dirty tree",
                "0.2.3",
                true,
                Some(vcs(true)),
                "0.2.3 (85b6671, modified)",
            ),
            (
                "vcs without a revision leaves the version alone",
                "0.2.3",
                true,
                Some(Vcs {
                    revision: "",
                    modified: false,
                }),
                "0.2.3",
            ),
            ("no build info", "0.2.3", true, None, "0.2.3"),
            (
                "metadata but no version reads as dev",
                "",
                true,
                Some(vcs(false)),
                "dev (85b6671)",
            ),
            ("a blank version is not real", "   ", true, None, "dev"),
            (
                "a short revision is not truncated",
                "0.2.3",
                true,
                Some(Vcs {
                    revision: "85b6671",
                    modified: false,
                }),
                "0.2.3 (85b6671)",
            ),
        ];
        for (name, version, stamped, vcs, want) in cases {
            assert_eq!(
                build_identity(version, stamped, vcs.as_ref()),
                want,
                "{name}"
            );
        }
    }

    // TestQualityProfileSettings
    #[test]
    fn quality_profile_settings_widens_by_four_thirds_and_picks_the_preset() {
        const INPUT: i64 = 10_000_000;
        // 4/3 of the input: the request that holds the source's bits per pixel.
        const WIDENED: i64 = 13_333_333;
        let cases = [
            ("Fast", "fast"),
            ("Balanced", "medium"),
            // An unexpected value behaves like Balanced, not like a zero bitrate.
            ("", "medium"),
            ("Nonsense", "medium"),
        ];
        for (profile, preset) in cases {
            assert_eq!(
                quality_profile_settings(profile, INPUT),
                (WIDENED, preset),
                "profile {profile:?}"
            );
        }
    }

    // TestConfiguredPresetWinsOverQualityProfile (the Go test checked a copy of the rule; this
    // one checks the rule the window applies)
    #[test]
    fn a_configured_preset_wins_over_the_quality_profile() {
        let configured = |preset: &str| Config {
            video_preset: preset.into(),
            ..Config::default()
        };
        assert_eq!(
            resolve_start_settings(&configured("slow"), "Fast", 10_000_000).1,
            "slow"
        );
        let unset = configured("");
        assert_eq!(resolve_start_settings(&unset, "Fast", 10_000_000).1, "fast");
        assert_eq!(
            resolve_start_settings(&unset, "Balanced", 10_000_000).1,
            "medium"
        );
    }

    #[test]
    fn the_bitrate_is_bounded_by_the_configuration() {
        let bounded = |min_bitrate, max_bitrate| Config {
            min_bitrate,
            max_bitrate,
            ..Config::default()
        };
        let cfg = bounded(1_000_000, 20_000_000);
        // 4/3 of 300 kb/s is under the floor; 4/3 of 30 Mb/s is over the ceiling.
        assert_eq!(
            resolve_start_settings(&cfg, "Balanced", 300_000).0,
            1_000_000
        );
        assert_eq!(
            resolve_start_settings(&cfg, "Balanced", 30_000_000).0,
            20_000_000
        );
        assert_eq!(
            resolve_start_settings(&cfg, "Balanced", 10_000_000).0,
            13_333_333
        );
        // A ceiling of zero is no ceiling.
        assert_eq!(
            resolve_start_settings(&bounded(1_000_000, 0), "Balanced", 30_000_000).0,
            40_000_000
        );
    }

    // TestEncoderOptionsFor
    #[test]
    fn encoder_options_for_ignores_empty_items_and_trims() {
        let u = USE_INPUT_CODEC;
        let cases: [(&str, &str, Vec<&str>); 6] = [
            ("empty means ffmpeg missing", "", vec![u]),
            ("single", "libx264", vec![u, "libx264 encoder"]),
            (
                "several",
                "libx264,libx265",
                vec![u, "libx264 encoder", "libx265 encoder"],
            ),
            (
                "stray separators",
                ",libx264,,libx265,",
                vec![u, "libx264 encoder", "libx265 encoder"],
            ),
            (
                "whitespace",
                " libx264 , libx265 ",
                vec![u, "libx264 encoder", "libx265 encoder"],
            ),
            ("only separators", ",,,", vec![u]),
        ];
        for (name, input, want) in cases {
            assert_eq!(encoder_options_for(input), want, "{name}");
        }
    }

    // TestParseEncoderSelection_UseInputCodecWithPadding
    #[test]
    fn parse_encoder_selection_ignores_padding_around_the_first_entry() {
        assert_eq!(
            parse_encoder_selection(&format!("  {USE_INPUT_CODEC} \t")),
            ""
        );
    }

    #[test]
    fn parse_encoder_selection_takes_the_first_word() {
        assert_eq!(parse_encoder_selection(""), "");
        assert_eq!(parse_encoder_selection("   "), "");
        assert_eq!(parse_encoder_selection(USE_INPUT_CODEC), "");
        assert_eq!(parse_encoder_selection("libx265 encoder"), "libx265");
        assert_eq!(
            parse_encoder_selection("  hevc_nvenc encoder "),
            "hevc_nvenc"
        );
    }

    // TestLoadStartupConfig_NeverNil (E-21). Go had an environment step between the file and
    // the defaults; the port has none, so both refusals end on the defaults.
    #[test]
    fn load_startup_config_never_returns_an_unusable_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let write = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, text).expect("write");
            path
        };

        // An unknown key: the file is refused, the defaults apply.
        let unknown = write("unknown.yaml", "video-preset: slow\n");
        assert_eq!(load_startup_config(Some(&unknown)), Config::default());

        // Inconsistent bounds: refused too, and the defaults are consistent.
        let bounds = write(
            "bounds.yaml",
            "min_bitrate: 500\nmax_bitrate: 100\nlog_level: debug\n",
        );
        let cfg = load_startup_config(Some(&bounds));
        assert!(cfg.min_bitrate <= cfg.max_bitrate, "{cfg:?}");
        assert_eq!(cfg.log_level, "info", "nothing of a refused file survives");

        // A good file is used, a missing one gives the defaults.
        let good = write("good.yaml", "video_preset: slow\n");
        assert_eq!(load_startup_config(Some(&good)).video_preset, "slow");
        assert_eq!(
            load_startup_config(Some(&dir.path().join("absent.yaml"))),
            Config::default()
        );
        assert_eq!(load_startup_config(None), Config::default());
    }
}
