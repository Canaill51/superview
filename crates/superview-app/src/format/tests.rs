use std::fmt;
use std::time::Instant;

use superview_core::error::Error;
#[cfg(unix)]
use superview_core::tools::{ToolResolver, check_ffmpeg};
use superview_core::video::VideoStream;

use super::*;

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

#[test]
fn format_results_panel_zero_input_size_gives_no_non_finite_ratio() {
    let mut m = metrics(0, 1024);
    m.end = Some(m.start);
    let text = format_results_panel(Some(&m));
    assert!(!text.contains("NaN") && !text.contains("inf"), "{text}");
    assert!(text.contains("(0.0%)"), "{text}");
}

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

// (R-04)
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

// Checks the rule the window applies.
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

// (E-21) There is no environment step between the file and
// the defaults (DEC-43), so both refusals end on the defaults.
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

#[test]
fn the_input_line_names_the_codec_the_size_and_the_length() {
    let video = VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width: 1440,
            height: 1080,
            duration_float: 135.0,
            ..VideoStream::default()
        }],
    };
    assert_eq!(input_specs(&video), "h264 · 1440×1080 · 2m15s");
    // No stream, nothing to say: never a panic on an index.
    let none = VideoSpecs {
        file: "x.mp4".into(),
        streams: Vec::new(),
    };
    assert_eq!(input_specs(&none), "");
}
