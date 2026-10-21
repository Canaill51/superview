use super::*;
use crate::config::Config;
use crate::hardware::HW_DEVICE_ALIAS;
use crate::process::tests::spawn_lock;
use crate::tools::check_ffmpeg;
use crate::video::VideoStream;

// ---- fixtures ----

/// An HEVC source.
fn hevc_source(pix_fmt: &str) -> VideoSpecs {
    VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![VideoStream {
            codec: "hevc".into(),
            width: 3840,
            height: 2880,
            pix_fmt: pix_fmt.into(),
            duration_float: 43.0,
            bitrate_int: 120_000_000,
            ..VideoStream::default()
        }],
    }
}

fn caps(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The laptop from the report.
fn vaapi_machine() -> HashMap<String, String> {
    caps(&[
        ("encoders", "h264_vaapi,libx264,libx265"),
        ("accels", "vaapi"),
    ])
}

/// The Intel HD 620 refusal.
const HD620_REFUSAL: &str = "[h264_vaapi @ 0x55] Hardware does not support encoding at size 5120x2880 (constraints: width 32-4096 height 32-4096)";

/// A resolver that never finds anything: for tests that must not start a process.
fn no_tools() -> ToolResolver {
    ToolResolver::new(|_| None, None)
}

/// Seeds a verdict. The key carries the source depth too (E-01), so the verdict is
/// seeded for every depth the fixtures use.
fn seed(prober: &Prober, encoder: &str, size: &str, usable: bool, reason: &str) {
    for pix_fmt in ["", "yuv420p", "yuv420p10le"] {
        let spec = ProbeSpec {
            encoder: encoder.into(),
            size: size.into(),
            pix_fmt: pix_fmt.into(),
            bitrate: PROBE_BITRATE,
            preset: String::new(),
            rate_control: RateControl::Bitrate,
        };
        prober.cache().insert(
            spec.cache_key(),
            EncoderProbe {
                encoder: encoder.into(),
                usable,
                reason: reason.into(),
                took: Duration::ZERO,
                rate_control: RateControl::Bitrate,
            },
        );
    }
}

pub(crate) struct FakeTools {
    _dir: tempfile::TempDir,
    _empty: tempfile::TempDir,
    pub resolver: ToolResolver,
}

/// Writes the stand-ins (`name`, file content, executable bit) into a directory that
/// goes first on a PATH that still holds the real one (the scripts call `sleep`,
/// `ping`, `echo`...). The bundled lookup points at an empty directory; the override
/// is unset.
fn install_tools(files: &[(String, String)]) -> FakeTools {
    let (dir, empty) = (
        tempfile::TempDir::new().unwrap(),
        tempfile::TempDir::new().unwrap(),
    );
    for (name, content) in files {
        let file = dir.path().join(name);
        std::fs::write(&file, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let real = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&real)),
    )
    .unwrap();
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone()),
        Some(empty.path().to_path_buf()),
    )
    .with_batch_stand_ins();
    FakeTools {
        _dir: dir,
        _empty: empty,
        resolver,
    }
}

/// Shell stand-ins: each `(name, script body)` becomes an executable script.
#[cfg(unix)]
pub(crate) fn fake_tools(scripts: &[(&str, &str)]) -> FakeTools {
    let files: Vec<(String, String)> = scripts
        .iter()
        .map(|(name, body)| (name.to_string(), format!("#!/bin/sh\n{body}\n")))
        .collect();
    install_tools(&files)
}

/// Batch stand-ins, found on `PATH` through `with_batch_stand_ins` (`ffmpeg.bat`).
#[cfg(windows)]
pub(crate) fn fake_tools(scripts: &[(&str, &str)]) -> FakeTools {
    let files: Vec<(String, String)> = scripts
        .iter()
        .map(|(name, body)| (format!("{name}.bat"), format!("@echo off\r\n{body}\r\n")))
        .collect();
    install_tools(&files)
}

/// A hanging ffmpeg. On Unix `exec`, so the process the deadline kills is the
/// one that sleeps and holds no pipe behind it. On Windows the `.bat` runs under
/// cmd.exe, which the kill reaches alone: `ping` keeps the pipe open after it, which
/// is what the bounded wait for the output is for.
fn hanging_ffmpeg() -> FakeTools {
    #[cfg(unix)]
    let body = "exec sleep 30";
    #[cfg(windows)]
    let body = "ping -n 30 127.0.0.1 >nul";
    fake_tools(&[("ffmpeg", body)])
}

/// An ffmpeg that records the argv of every call in `ledger` and refuses.
fn refusing_ffmpeg(ledger: &std::path::Path) -> FakeTools {
    #[cfg(unix)]
    let body = format!(
        "echo \"call $*\" >> '{}'\necho 'Hardware does not support encoding at size' 1>&2\nexit 1",
        ledger.display()
    );
    #[cfg(windows)]
    let body = format!(
        "echo call %* >> \"{}\"\r\necho Hardware does not support encoding at size 1>&2\r\nexit /b 1",
        ledger.display()
    );
    fake_tools(&[("ffmpeg", &body)])
}

fn joined(args: &[String]) -> String {
    args.join(" ")
}

// ---- the argv ----

#[test]
fn device_frame_encoders_get_a_device_and_an_upload() {
    for (encoder, device_type) in [
        ("h264_vaapi", "vaapi"),
        ("hevc_vulkan", "vulkan"),
        ("h264_d3d12va", "d3d12va"),
    ] {
        let args = joined(&probe_args(&sweep_spec(encoder)));
        for needed in [
            format!("-init_hw_device {device_type}={HW_DEVICE_ALIAS}"),
            format!("-filter_hw_device {HW_DEVICE_ALIAS}"),
            "format=nv12,hwupload".to_string(),
        ] {
            assert!(
                args.contains(&needed),
                "the {encoder} probe is missing {needed:?}: {args}"
            );
        }
    }
    // NVENC, AMF and QSV upload for themselves; a device they do not need would fail
    // on machines that cannot create one.
    for encoder in ["h264_nvenc", "hevc_amf", "h264_qsv", "libx264"] {
        let args = joined(&probe_args(&sweep_spec(encoder)));
        for unwanted in ["-init_hw_device", "hwupload"] {
            assert!(
                !args.contains(unwanted),
                "the {encoder} probe should not carry {unwanted:?}: {args}"
            );
        }
    }
}

/// The value of an option in an argv.
fn value_of<'a>(args: &'a [String], option: &str) -> &'a str {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map_or("", |pair| pair[1].as_str())
}

// The probe and the conversion ask the same question. The whole argv of both builders is
// compared, minus the differences listed here.
#[test]
fn probe_and_conversion_ask_the_same_question() {
    use crate::encode::args::{build_encode_base_args, remap_filter_chain};
    use std::collections::BTreeMap;

    // Options the probe is allowed to lack or to differ on, with their value count.
    let conversion_only: BTreeMap<&str, usize> = [
        ("-progress", 1),
        ("-y", 0),
        ("-i", 1),
        ("-filter_complex", 1),
        ("-map", 1),
        ("-map_metadata", 1),
        ("-c:a", 1),
        ("-threads", 1),
        ("-filter_threads", 1),
        ("-x265-params", 1),
    ]
    .into();
    let probe_only: BTreeMap<&str, usize> = [("-f", 1), ("-i", 1), ("-vf", 1)].into();

    // The argv without the listed options and without the trailing output (a path
    // for the conversion, "-" for the probe).
    let strip = |args: &[String], drop: &BTreeMap<&str, usize>| -> Vec<String> {
        let mut kept = Vec::new();
        let mut i = 0;
        while i + 1 < args.len() {
            if let Some(n) = drop.get(args[i].as_str()) {
                i += 1 + n;
                continue;
            }
            kept.push(args[i].clone());
            i += 1;
        }
        kept
    };

    let video = hevc_source("yuv420p10le");
    let video = VideoSpecs {
        file: "in.mp4".into(),
        streams: vec![VideoStream {
            width: 1440,
            height: 1080,
            duration_float: 10.0,
            bitrate_int: 8_000_000,
            ..video.streams[0].clone()
        }],
    };
    for encoder in [
        "h264_nvenc",
        "hevc_nvenc",
        "hevc_amf",
        "h264_qsv",
        "h264_vaapi",
        "hevc_vaapi",
        "hevc_vulkan",
        "h264_d3d12va",
        "h264_v4l2m2m",
        "libx264",
        "libx265",
    ] {
        // Both modes are one question asked twice (DEC-05, DEC-48): constant quality
        // goes through `encoder_facing_args` like the bit rate does.
        for (rate_control, preset) in [RateControl::Bitrate, RateControl::ConstantQuality]
            .into_iter()
            .flat_map(|mode| ["", "fast", "veryslow"].map(move |preset| (mode, preset)))
        {
            let conversion = build_encode_base_args(
                &video,
                "x.pgm",
                "y.pgm",
                encoder,
                rate_control,
                8_000_000,
                "aac",
                4,
                0,
                preset,
            );
            let probe = probe_args(&ProbeSpec {
                encoder: encoder.into(),
                size: "1920x1080".into(),
                pix_fmt: source_pixel_format(Some(&video)).into(),
                bitrate: 8_000_000,
                preset: preset.into(),
                rate_control,
            });
            // DEC-52: the one `-f` the conversion has, how the source file is read; the probe's
            // input is lavfi. Only that pair goes, so another `-f` would still show.
            let mut conversion = conversion;
            let mov = conversion
                .windows(2)
                .position(|pair| pair == ["-f", "mov"])
                .expect("the conversion reads its source with -f mov");
            conversion.drain(mov..mov + 2);
            let (got, want) = (
                joined(&strip(&probe, &probe_only)),
                joined(&strip(&conversion, &conversion_only)),
            );
            assert_eq!(
                got, want,
                "{encoder} {rate_control:?} preset={preset:?}: the probe and the conversion ask the encoder different questions"
            );

            // And the pixel-format steps: everything after remap in the conversion
            // must be the probe's whole -vf chain.
            let chain = remap_filter_chain(source_pixel_format(Some(&video)), encoder);
            let after_remap = chain
                .split_once("remap,")
                .unwrap()
                .1
                .strip_suffix("[v]")
                .unwrap();
            assert_eq!(value_of(&probe, "-vf"), after_remap, "{encoder}");
        }
    }
}

#[test]
fn names_the_encoder_and_discards_output() {
    let args = joined(&probe_args(&sweep_spec("hevc_nvenc")));
    assert!(args.contains("-c:v hevc_nvenc"), "{args}");
    assert!(args.ends_with("-f null -"), "{args}");
    assert!(args.contains("-loglevel error"), "{args}");
}

#[test]
fn uses_the_size_it_is_given() {
    let args = joined(&probe_args(&ProbeSpec {
        encoder: "h264_vaapi".into(),
        size: "5120x2880".into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    }));
    assert!(args.contains("nullsrc=s=5120x2880"), "{args}");
    assert!(!args.contains(PROBE_FRAME_SIZE), "{args}");
}

#[test]
fn asks_for_rate_control() {
    let args = joined(&probe_args(&sweep_spec("h264_vaapi")));
    for needed in ["-b:v ", "-maxrate ", "-bufsize "] {
        assert!(args.contains(needed), "no {needed:?}: {args}");
    }
}

#[test]
fn asks_at_the_source_depth() {
    let spec = |pix_fmt: &str| ProbeSpec {
        encoder: "hevc_vaapi".into(),
        size: "1920x1080".into(),
        pix_fmt: pix_fmt.into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    };
    let ten = joined(&probe_args(&spec("yuv420p10le")));
    assert!(
        ten.contains("format=yuv444p10le,format=yuv420p10le,format=p010,hwupload"),
        "a 10-bit source must be probed in 10 bits: {ten}"
    );
    let eight = joined(&probe_args(&spec("yuv420p")));
    assert!(!eight.contains("10le"), "{eight}");
}

#[test]
fn cache_key_tells_every_field_apart() {
    let base = sweep_spec("libx264");
    assert_eq!(base.cache_key(), "libx264@256x256//2000000//bitrate");
    let mut keys = vec![base.cache_key()];
    for changed in [
        ProbeSpec {
            encoder: "libx265".into(),
            ..base.clone()
        },
        ProbeSpec {
            size: "640x480".into(),
            ..base.clone()
        },
        ProbeSpec {
            pix_fmt: "yuv420p10le".into(),
            ..base.clone()
        },
        ProbeSpec {
            bitrate: 1,
            ..base.clone()
        },
        ProbeSpec {
            preset: "slow".into(),
            ..base.clone()
        },
        ProbeSpec {
            rate_control: RateControl::ConstantQuality,
            ..base.clone()
        },
    ] {
        keys.push(changed.cache_key());
    }
    let distinct: HashSet<&String> = keys.iter().collect();
    assert_eq!(distinct.len(), keys.len(), "{keys:?}");
}

// ---- the reason ----

// Rust only (DEC-48). libva prints three informational lines before any VAAPI or QSV
// session, and they filled the whole three-line reason: a QSV refusal read
// "libva info: VA-API version 1.23.0; libva info: Trying to open ...; libva info: Found init
// function" and the actual error was cut off (RC-0.3.0, Tiger Lake). They are not a cause.
#[test]
fn libva_information_lines_do_not_crowd_out_the_error() {
    let ms = Duration::from_millis;
    let qsv = "libva info: VA-API version 1.23.0\n\
                   libva info: Trying to open /usr/lib/x86_64-linux-gnu/dri/iHD_drv_video.so\n\
                   libva info: Found init function __vaDriverInit_1_22\n\
                   [h264_qsv @ 0x59] Error creating a MFX session: -9.\n\
                   [vost#0:0/h264_qsv @ 0x59] Error while opening encoder\n";
    let got = summarize_probe_failure(qsv, "exit status: 218", false, ms(0));
    assert!(!got.contains("libva info"), "{got}");
    assert!(got.contains("Error creating a MFX session: -9."), "{got}");

    // The CQP-only marker must survive the same noise in front of it.
    let vaapi = "libva info: VA-API version 1.23.0\nlibva info: Trying to open x\nlibva info: Found init function y\n\
             [h264_vaapi @ 0x59] Driver does not support any RC mode compatible with selected options (supported modes: CQP).\n";
    let got = summarize_probe_failure(vaapi, "exit status: 234", false, ms(0));
    assert!(is_cqp_only_refusal("h264_vaapi", &got), "{got}");

    // Only the information lines go: a libva *error* is a cause and stays; and a probe
    // whose whole output is noise still says something.
    let err = "libva error: vaGetDriverNames() failed with unknown libva error\n";
    assert!(summarize_probe_failure(err, "x", false, ms(0)).contains("libva error"));
    assert_eq!(
        summarize_probe_failure("libva info: only noise\n", "exit status: 1", false, ms(0)),
        "exit status: 1"
    );
}

#[test]
fn summarizes_a_failure_in_one_line() {
    let ms = Duration::from_millis;
    let nvenc = "[h264_nvenc @ 0x55] Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0\n\
                     [h264_nvenc @ 0x55] The minimum required Nvidia driver for nvenc is 610.00 or newer\n";
    // NVENC prints the mismatch and then the driver it wants; both lines have to
    // survive, because together they tell the user what to do.
    let got = summarize_probe_failure(nvenc, "exit status: 1", false, ms(0));
    assert!(got.contains("Required: 13.1 Found: 13.0"), "{got}");
    assert!(got.contains("610.00 or newer"), "{got}");

    assert_eq!(
        summarize_probe_failure("   \n\n", "exit status: 218", false, ms(0)),
        "exit status: 218",
        "a silent failure still says something"
    );
    assert_eq!(
        summarize_probe_failure("", "killed", true, ms(302)),
        "gave no answer in 302ms",
        "a deadline reports what was actually waited"
    );
    // The cause first, the consequences after: keeping the tail would keep the
    // restatements and throw away the only line that identifies the problem.
    assert_eq!(
        summarize_probe_failure(
            "Unknown encoder 'nope'\nError selecting an encoder\nError opening output file -.\nError opening output files: Encoder not found\n",
            "exit status: 1",
            false,
            ms(0)
        ),
        "Unknown encoder 'nope'; Error selecting an encoder; Error opening output file -."
    );
    let long = "x".repeat(PROBE_REASON_BUDGET + 50);
    let got = summarize_probe_failure(&long, "exit status: 1", false, ms(0));
    assert!(got.ends_with("..."), "a talkative failure is capped: {got}");
    assert_eq!(got.len(), PROBE_REASON_BUDGET + 3);
}

// The constants are the measured ones (v0.2.9), spelled out rather than read
// back from the code they govern.
#[test]
fn the_constants_are_gos() {
    assert_eq!(PROBE_TIMEOUT, Duration::from_secs(15));
    assert_eq!(PROBE_FRAME_SIZE, "256x256");
    assert_eq!(PROBE_REASON_LINES, 3);
    assert_eq!(PROBE_REASON_BUDGET, 300);
    assert_eq!(PROBE_BITRATE, 2_000_000);
    let long = "x".repeat(1000);
    let got = summarize_probe_failure(&long, "x", false, Duration::ZERO);
    assert_eq!(got, format!("{}...", "x".repeat(300)));
}

// A reason is cut at 300 bytes, never mid-character.
#[test]
fn a_reason_is_cut_on_a_character_boundary() {
    // One ASCII byte, then two-byte characters: byte 300 falls inside a character
    // (299 and 300 are the two halves of one), so the cut must back off to 299.
    let long = format!("x{}", "é".repeat(PROBE_REASON_BUDGET));
    let got = summarize_probe_failure(&long, "x", false, Duration::ZERO);
    assert_eq!(got.len(), PROBE_REASON_BUDGET - 1 + 3);
    assert!(got.ends_with("é..."), "{got}");
}

// Windows ffmpeg ends its lines with CRLF.
#[test]
fn crlf_lines_leave_no_carriage_return() {
    let got = summarize_probe_failure("one\r\ntwo\r\n", "x", false, Duration::ZERO);
    assert_eq!(got, "one; two");
}

// Durations print like "1.5s" after rounding to a millisecond.
#[test]
fn durations_read_like_go_after_rounding_to_a_millisecond() {
    let ms = Duration::from_millis;
    assert_eq!(format_duration(Duration::from_micros(499)), "0s");
    assert_eq!(format_duration(Duration::from_micros(500)), "1ms");
    assert_eq!(format_duration(ms(302)), "302ms");
    assert_eq!(format_duration(ms(999)), "999ms");
    assert_eq!(format_duration(ms(1000)), "1s");
    assert_eq!(format_duration(ms(1500)), "1.5s");
    assert_eq!(format_duration(ms(15_005)), "15.005s");
    assert_eq!(format_duration(ms(62_500)), "1m2.5s");
    assert_eq!(format_duration(ms(60_000)), "1m0s");
    assert_eq!(format_duration(ms(3_600_000)), "1h0m0s");
    assert_eq!(format_duration(ms(3_723_000)), "1h2m3s");
}

// ---- which encoders, what the verdict does ----

#[test]
fn encoders_to_probe_keeps_hardware_and_the_cpu_pair() {
    let ffmpeg = caps(&[(
        "encoders",
        "libx264,libx264rgb,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc",
    )]);
    // h264_vulkan is in there: a hardware encoder like the rest, and it survives an
    // NVENC the driver refuses. Probing it is how that survival gets established.
    assert_eq!(
        encoders_to_probe(&ffmpeg).join(","),
        "libx264,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc"
    );
}

#[test]
fn nothing_advertised_means_nothing_to_probe() {
    assert!(encoders_to_probe(&HashMap::new()).is_empty());
}

fn refused(encoder: &str) -> EncoderProbe {
    EncoderProbe {
        encoder: encoder.into(),
        reason: "driver too old".into(),
        ..EncoderProbe::default()
    }
}

fn worked(encoder: &str) -> EncoderProbe {
    EncoderProbe {
        encoder: encoder.into(),
        usable: true,
        ..EncoderProbe::default()
    }
}

#[test]
fn apply_removes_only_what_was_refused() {
    let ffmpeg = caps(&[
        ("version", "8.1.2"),
        ("encoders", "libx264,h264_nvenc,h264_vulkan,hevc_nvenc"),
    ]);
    let report = EncoderProbeReport {
        results: vec![
            worked("libx264"),
            refused("h264_nvenc"),
            refused("hevc_nvenc"),
        ],
        took: Duration::ZERO,
    };
    let updated = apply_encoder_probe(&ffmpeg, Some(&report));
    assert_eq!(
        updated["encoders"], "libx264,h264_vulkan",
        "an unprobed encoder must survive"
    );
    assert_eq!(updated["version"], "8.1.2");
}

#[test]
fn apply_does_not_edit_the_map_it_was_given() {
    let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc")]);
    let report = EncoderProbeReport {
        results: vec![refused("h264_nvenc")],
        took: Duration::ZERO,
    };
    let updated = apply_encoder_probe(&ffmpeg, Some(&report));
    assert_eq!(updated["encoders"], "libx264");
    assert_eq!(ffmpeg["encoders"], "libx264,h264_nvenc");
}

#[test]
fn apply_without_a_report_changes_nothing() {
    let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc")]);
    assert_eq!(
        apply_encoder_probe(&ffmpeg, None)["encoders"],
        "libx264,h264_nvenc"
    );
}

// A missing report is an Option here, see the
// header).
#[test]
fn find_and_unusable() {
    let report = EncoderProbeReport {
        results: vec![worked("libx264"), refused("h264_nvenc")],
        took: Duration::ZERO,
    };
    assert_eq!(report.find("h264_nvenc").unwrap().reason, "driver too old");
    assert!(report.find("hevc_qsv").is_none(), "never probed");
    let unusable = report.unusable();
    assert_eq!(unusable.len(), 1);
    assert_eq!(unusable[0].encoder, "h264_nvenc");
}

#[test]
fn describe_carries_the_reason() {
    let report = EncoderProbeReport {
        took: Duration::from_millis(812),
        results: vec![
            EncoderProbe {
                took: Duration::from_millis(40),
                ..worked("libx264")
            },
            EncoderProbe {
                encoder: "h264_nvenc".into(),
                reason: "The minimum required Nvidia driver for nvenc is 610.00 or newer".into(),
                ..EncoderProbe::default()
            },
        ],
    };
    assert_eq!(
        describe_encoder_probe(Some(&report)),
        "=== Encoders (probed in 812ms) ===\n\
             ✅ libx264: works (40ms)\n\
             ❌ h264_nvenc: The minimum required Nvidia driver for nvenc is 610.00 or newer\n"
    );
}

#[test]
fn describe_degraded_cases() {
    assert_eq!(describe_encoder_probe(None), "Encoders: not probed\n");
    assert_eq!(
        describe_encoder_probe(Some(&EncoderProbeReport::default())),
        "Encoders: none to probe\n"
    );
}

// ---- the sweep, with a cancellation ----

#[test]
fn a_cancelled_sweep_probes_nothing() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    let cancel = AtomicBool::new(true);
    let encoders = ["libx264", "libx265", "h264_nvenc"].map(String::from);
    let report = prober.probe_encoders(&encoders, Some(&cancel));
    assert!(report.results.is_empty(), "{:?}", report.results);
}

// A cancelled probe is not remembered (E-23). Nothing is started
// here (the flag is up at the start), so it runs on every platform.
#[test]
fn a_probe_cancelled_before_it_starts_is_not_cached() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    let cancel = AtomicBool::new(true);
    let spec = ProbeSpec {
        encoder: "h264_vaapi".into(),
        size: "640x480".into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    };
    let probe = prober.probe_encoder_for_size_cached(&spec, Some(&cancel));
    assert!(
        !probe.usable,
        "a cancelled probe cannot report the encoder usable"
    );
    assert_eq!(probe.reason, "cancelled");
    assert!(
        prober.cache().get(&spec.cache_key()).is_none(),
        "a cancelled probe was cached as the driver's answer"
    );
}

// A cancelled probe is not remembered, with a probe actually cut
// short while it runs.
#[test]
fn a_probe_cut_short_while_running_is_not_cached() {
    let _guard = spawn_lock();
    let fake = hanging_ffmpeg();
    let prober = Prober::new(&fake.resolver);
    let cancel = AtomicBool::new(false);
    let spec = ProbeSpec {
        encoder: "h264_vaapi".into(),
        size: "640x480".into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    };
    let started = Instant::now();
    let probe = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::Release);
        });
        prober.probe_encoder_for_size_cached(&spec, Some(&cancel))
    });
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(!probe.usable);
    assert!(
        !probe.reason.starts_with("gave no answer"),
        "a cancellation is not the probe's own timeout: {}",
        probe.reason
    );
    assert!(prober.cache().is_empty(), "{:?}", prober.cache());
}

// ---- the frame, and verifying an encoder against it ----

#[test]
fn output_frame_size_follows_the_geometry() {
    let video = hevc_source("yuv420p10le"); // 3840x2880
    assert_eq!(output_frame_size(Some(&video), false), "5120x2880");
    // Already stretched: the frame keeps its width.
    assert_eq!(output_frame_size(Some(&video), true), "3840x2880");
    assert_eq!(output_frame_size(None, false), "");
    let no_streams = VideoSpecs {
        file: "x.mp4".into(),
        streams: vec![],
    };
    assert_eq!(output_frame_size(Some(&no_streams), false), "");
}

#[test]
fn software_encoders_are_left_alone() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    let (encoder, reason) = prober.verify_encoder_for_output(
        "libx265",
        Some(&hevc_source("yuv420p10le")),
        false,
        &vaapi_machine(),
        "",
        None,
    );
    assert_eq!((encoder.as_str(), reason.as_str()), ("libx265", ""));
    assert!(
        prober.cache().is_empty(),
        "a software encoder must not be probed"
    );
}

fn verify(
    prober: &Prober,
    video: &VideoSpecs,
    squeeze: bool,
    ffmpeg: &HashMap<String, String>,
) -> (String, String) {
    prober.verify_encoder_for_output("h264_vaapi", Some(video), squeeze, ffmpeg, "", None)
}

#[test]
fn keeps_a_hardware_encoder_that_accepts_the_frame() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", true, "");
    let (encoder, reason) = verify(
        &prober,
        &hevc_source("yuv420p10le"),
        false,
        &vaapi_machine(),
    );
    assert_eq!((encoder.as_str(), reason.as_str()), ("h264_vaapi", ""));
}

#[test]
fn steps_back_to_the_source_family_when_refused() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
    let (encoder, reason) = verify(
        &prober,
        &hevc_source("yuv420p10le"),
        false,
        &vaapi_machine(),
    );
    assert_eq!(encoder, "libx265");
    assert_eq!(
        reason,
        format!("h264_vaapi cannot encode 5120x2880 on this machine: {HD620_REFUSAL}")
    );
}

#[test]
fn keeps_the_encoder_with_nothing_to_step_back_to() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
    let bare = caps(&[("encoders", "h264_vaapi"), ("accels", "vaapi")]);
    let (encoder, reason) = verify(&prober, &hevc_source("yuv420p10le"), false, &bare);
    assert_eq!(
        encoder, "h264_vaapi",
        "with no CPU encoder the choice stands"
    );
    assert!(!reason.is_empty(), "the refusal must still be reported");
}

#[test]
fn follows_the_squeeze_geometry() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "3840x2880", true, "");
    seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
    let video = hevc_source("yuv420p10le");
    assert_eq!(
        verify(&prober, &video, true, &vaapi_machine()).0,
        "h264_vaapi"
    );
    assert_eq!(
        verify(&prober, &video, false, &vaapi_machine()).0,
        "libx265"
    );
}

// A missing video or an empty encoder name is answered without a probe.
#[test]
fn nothing_to_verify_without_an_encoder_or_a_frame() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    let ffmpeg = vaapi_machine();
    assert_eq!(
        prober.verify_encoder_for_output("", None, false, &ffmpeg, "", None),
        (String::new(), String::new())
    );
    assert_eq!(
        prober.verify_encoder_for_output("h264_vaapi", None, false, &ffmpeg, "", None),
        ("h264_vaapi".to_string(), String::new())
    );
    assert!(prober.cache().is_empty());
}

#[test]
fn the_plan_names_what_will_really_run() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
    let line = prober.describe_verified_hardware_plan(
        &vaapi_machine(),
        Some(&hevc_source("yuv420p10le")),
        false,
        "",
        "",
        None,
    );
    assert!(line.contains("libx265"), "{line}");
    assert!(
        !line.contains("10-bit source is stored as 8-bit"),
        "the conversion stays in H.265, so nothing is downgraded: {line}"
    );
    assert!(line.contains("5120x2880"), "{line}");
    assert!(
        line.contains(" -- h264_vaapi cannot encode 5120x2880"),
        "{line}"
    );
}

#[test]
fn the_plan_is_unchanged_when_the_hardware_accepts() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", true, "");
    let line = prober.describe_verified_hardware_plan(
        &vaapi_machine(),
        Some(&hevc_source("yuv420p10le")),
        false,
        "",
        "",
        None,
    );
    assert!(line.contains("h264_vaapi"), "{line}");
    assert!(
        line.contains("8-bit"),
        "the codec-switch note must survive: {line}"
    );
    assert!(!line.contains("cannot encode"), "{line}");
}

// The two early returns of describe_verified_hardware_plan.
#[test]
fn the_plan_without_a_video_or_an_encoder_says_so() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    assert_eq!(
        prober.describe_verified_hardware_plan(&vaapi_machine(), None, false, "", "", None),
        "Hardware: waiting for input video"
    );
    let no_streams = VideoSpecs {
        file: "x.mp4".into(),
        streams: vec![],
    };
    assert_eq!(
        prober.describe_verified_hardware_plan(
            &vaapi_machine(),
            Some(&no_streams),
            false,
            "",
            "",
            None
        ),
        "Hardware: invalid video: no video streams"
    );
}

// ---- with a stand-in ffmpeg ----

// The driver is asked once. The ledger also shows the argv really
// reached the process.
#[test]
fn the_driver_is_asked_once_for_the_same_question() {
    let _guard = spawn_lock();
    let ledger_dir = tempfile::TempDir::new().unwrap();
    let ledger = ledger_dir.path().join("calls.txt");
    let fake = refusing_ffmpeg(&ledger);
    let prober = Prober::new(&fake.resolver);

    let video = hevc_source("yuv420p10le");
    for call in 0..3 {
        let (encoder, _) = verify(&prober, &video, false, &vaapi_machine());
        assert_eq!(encoder, "libx265", "call {call}: expected the step-back");
    }
    // std quotes the arguments it hands a `.bat` that hold `=` or `,`
    // (`"vaapi=sv"`); the quotes are the batch file's view, not a defect.
    let data = std::fs::read_to_string(&ledger)
        .expect("the stand-in was never run")
        .replace('"', "");
    assert_eq!(
        data.matches("call").count(),
        1,
        "the driver was asked more than once: {data}"
    );
    assert!(
        data.contains("-init_hw_device vaapi=") && data.contains("nullsrc=s=5120x2880"),
        "the argv that reached ffmpeg: {data}"
    );
}

// A different question (squeeze) is a different cache entry.
#[test]
fn a_different_frame_size_is_asked_again() {
    let _guard = spawn_lock();
    let ledger_dir = tempfile::TempDir::new().unwrap();
    let ledger = ledger_dir.path().join("calls.txt");
    let fake = refusing_ffmpeg(&ledger);
    let prober = Prober::new(&fake.resolver);
    let video = hevc_source("yuv420p10le");
    verify(&prober, &video, false, &vaapi_machine());
    verify(&prober, &video, true, &vaapi_machine());
    verify(&prober, &video, true, &vaapi_machine());
    let data = std::fs::read_to_string(&ledger).unwrap();
    assert_eq!(data.matches("call").count(), 2, "{data}");
    prober.cache().reset();
    assert!(prober.cache().is_empty());
    verify(&prober, &video, true, &vaapi_machine());
    let data = std::fs::read_to_string(&ledger).unwrap();
    assert_eq!(data.matches("call").count(), 3, "reset must forget: {data}");
}

// The probe bounds itself without a caller deadline. The probe's own timeout is
// a field here, so the test shortens it without a global.
#[test]
fn the_probe_bounds_itself_without_a_caller_deadline() {
    let _guard = spawn_lock();
    let fake = hanging_ffmpeg();
    let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    let probe = prober.probe_encoder("libx264", None);
    assert!(
        !probe.usable,
        "an ffmpeg that never exits cannot have succeeded"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the probe has no timeout of its own"
    );
    assert!(
        probe.reason.starts_with("gave no answer in "),
        "a timeout must be reported as one: {}",
        probe.reason
    );
    assert!(probe.took >= Duration::from_millis(300));
}

// "Only the probe's own deadline is a verdict" (E-23,
// the other half): a probe that timed out by itself IS cached, so a wedged driver is
// not asked again at every click.
#[test]
fn a_probe_that_timed_out_by_itself_is_cached() {
    let _guard = spawn_lock();
    let fake = hanging_ffmpeg();
    let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(300));
    let spec = ProbeSpec {
        encoder: "h264_vaapi".into(),
        size: "640x480".into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    };
    let probe = prober.probe_encoder_for_size_cached(&spec, None);
    assert!(!probe.usable);
    assert!(
        probe.reason.starts_with("gave no answer in "),
        "{}",
        probe.reason
    );
    assert_eq!(prober.cache().get(&spec.cache_key()), Some(probe.clone()));

    let again = Instant::now();
    let second = prober.probe_encoder_for_size_cached(&spec, None);
    assert_eq!(second, probe);
    assert!(
        again.elapsed() < Duration::from_millis(100),
        "asked the driver again"
    );
}

// The caller's deadline, as a cancellation.
#[test]
fn the_probe_honours_a_cancellation() {
    let _guard = spawn_lock();
    let fake = hanging_ffmpeg();
    let prober = Prober::new(&fake.resolver);
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let probe = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::Release);
        });
        prober.probe_encoder("libx264", Some(&cancel))
    });
    assert!(!probe.usable);
    assert!(started.elapsed() < Duration::from_secs(5));
}

// An unknown encoder is refused with ffmpeg's words, against a stand-in
// that prints ffmpeg's four lines.
#[cfg(unix)]
#[test]
fn a_refusal_carries_ffmpegs_first_words() {
    let _guard = spawn_lock();
    let fake = fake_tools(&[(
        "ffmpeg",
        "echo \"Unknown encoder 'superview_no_such_encoder'\" 1>&2\n\
             echo 'Error selecting an encoder' 1>&2\n\
             echo 'Error opening output file -.' 1>&2\n\
             echo 'Error opening output files: Encoder not found' 1>&2\n\
             exit 1",
    )]);
    let probe = Prober::new(&fake.resolver).probe_encoder("superview_no_such_encoder", None);
    assert!(!probe.usable);
    assert_eq!(
        probe.reason,
        "Unknown encoder 'superview_no_such_encoder'; Error selecting an encoder; Error opening output file -."
    );
}

// "A silent failure still says something", end to end.
#[cfg(unix)]
#[test]
fn a_silent_refusal_reports_the_exit_status() {
    let _guard = spawn_lock();
    let fake = fake_tools(&[("ffmpeg", "exit 218")]);
    let probe = Prober::new(&fake.resolver).probe_encoder("libx264", None);
    assert!(!probe.usable);
    assert!(probe.reason.contains("218"), "{}", probe.reason);
}

// The stand-in's success, so the refusals above cannot come from a
// broken stand-in.
#[cfg(unix)]
#[test]
fn an_ffmpeg_that_encodes_makes_the_probe_usable() {
    let _guard = spawn_lock();
    let fake = fake_tools(&[("ffmpeg", "exit 0")]);
    let probe = Prober::new(&fake.resolver).probe_encoder("libx264", None);
    assert!(probe.usable, "{}", probe.reason);
    assert!(probe.reason.is_empty());
    assert!(probe.took > Duration::ZERO);
}

// No ffmpeg to start is a refusal with the cause, not a panic.
#[cfg(not(windows))]
#[test]
fn a_missing_ffmpeg_is_a_refusal_with_a_reason() {
    let _guard = spawn_lock();
    let empty = tempfile::TempDir::new().unwrap();
    let path = empty.path().as_os_str().to_owned();
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone()),
        Some(empty.path().to_path_buf()),
    );
    let probe = Prober::new(&resolver).probe_encoder("libx264", None);
    assert!(!probe.usable);
    assert!(!probe.reason.is_empty());
}

// The probe agrees with what was advertised, with a stand-in that
// refuses one encoder by name.
#[cfg(unix)]
#[test]
fn the_sweep_probes_what_was_advertised_and_apply_drops_what_failed() {
    let _guard = spawn_lock();
    let fake = fake_tools(&[(
        "ffmpeg",
        "case \"$*\" in *h264_nvenc*) echo 'Driver too old' 1>&2; exit 1;; esac\nexit 0",
    )]);
    let prober = Prober::new(&fake.resolver);
    let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc,libx264rgb,h264_vulkan")]);
    let report = prober.probe_hardware_support(&ffmpeg, None);

    let probed: Vec<&str> = report.results.iter().map(|p| p.encoder.as_str()).collect();
    assert_eq!(probed, ["libx264", "h264_nvenc", "h264_vulkan"]);
    assert_eq!(report.unusable().len(), 1);
    assert_eq!(report.find("h264_nvenc").unwrap().reason, "Driver too old");
    assert!(report.took > Duration::ZERO);
    assert_eq!(
        apply_encoder_probe(&ffmpeg, Some(&report))["encoders"],
        "libx264,libx264rgb,h264_vulkan"
    );
}

// ---- the real ffmpeg ----

/// The machine's ffmpeg, or `None` where there is none -- unless
/// `SUPERVIEW_REQUIRE_FFMPEG=1`, which CI sets, where its absence fails the test.
fn real_ffmpeg_with_libx264() -> Option<ToolResolver> {
    let resolver = ToolResolver::from_process();
    let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
    match check_ffmpeg(&Config::default(), &resolver) {
        Ok(caps) if caps["encoders"].contains("libx264") => Some(resolver),
        Ok(_) => {
            assert!(
                !require,
                "this ffmpeg has no libx264 although SUPERVIEW_REQUIRE_FFMPEG=1"
            );
            None
        }
        Err(err) => {
            assert!(
                !require,
                "no usable ffmpeg although SUPERVIEW_REQUIRE_FFMPEG=1: {err}"
            );
            None
        }
    }
}

// A software encoder works. libx264 is the right subject: if this
// reddens on a machine that has ffmpeg, the probe itself is malformed.
#[test]
fn libx264_accepts_a_256x256_frame() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffmpeg_with_libx264() else {
        return;
    };
    let probe = Prober::new(&resolver).probe_encoder("libx264", None);
    assert!(
        probe.usable,
        "libx264 refused a 256x256 frame, so the probe asks a malformed question: {}",
        probe.reason
    );
    assert!(probe.took > Duration::ZERO);
}

#[test]
fn an_unknown_encoder_is_refused_with_ffmpegs_words() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffmpeg_with_libx264() else {
        return;
    };
    let probe = Prober::new(&resolver).probe_encoder("superview_no_such_encoder", None);
    assert!(
        !probe.usable,
        "ffmpeg accepted an encoder that does not exist"
    );
    assert!(
        probe.reason.contains("superview_no_such_encoder"),
        "the reason does not name the encoder ffmpeg rejected: {:?}",
        probe.reason
    );
}

#[test]
fn the_real_sweep_agrees_with_what_was_advertised() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffmpeg_with_libx264() else {
        return;
    };
    let ffmpeg = check_ffmpeg(&Config::default(), &resolver).unwrap();
    let report = Prober::new(&resolver).probe_hardware_support(&ffmpeg, None);
    let wanted = encoders_to_probe(&ffmpeg);
    assert_eq!(report.results.len(), wanted.len(), "{wanted:?}");
    // A refusal is a finding on this machine, not a failure (a CI runner without a
    // GPU refuses most): what must hold is that the verdict carries a reason.
    for probe in &report.results {
        assert!(
            probe.usable || !probe.reason.is_empty(),
            "{} was refused with no reason recorded",
            probe.encoder
        );
    }
    let filtered = split_csv(&apply_encoder_probe(&ffmpeg, Some(&report))["encoders"]);
    for probe in report.unusable() {
        assert!(
            !filtered.contains(&probe.encoder),
            "{} refused the work but is still advertised",
            probe.encoder
        );
    }
}

// ---- constant quality (DEC-48) ----

#[cfg(unix)]
const CQP_ONLY: &str = "[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes: CQP).";

/// An ffmpeg that records each call, takes `-rc_mode` and refuses everything else with
/// `refusal`, the way an Intel VAAPI driver that only knows CQP does.
#[cfg(unix)]
fn cqp_only_ffmpeg(ledger: &std::path::Path, refusal: &str) -> FakeTools {
    fake_tools(&[(
        "ffmpeg",
        &format!(
            "echo \"call $*\" >> '{}'\ncase \"$*\" in *-rc_mode\\ CQP*) exit 0;; esac\necho '{refusal}' 1>&2\nexit 1",
            ledger.display()
        ),
    )])
}

#[cfg(unix)]
fn calls(ledger: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(ledger)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

fn seed_constant_quality(prober: &Prober, encoder: &str, size: &str) {
    let spec = ProbeSpec {
        encoder: encoder.into(),
        size: size.into(),
        pix_fmt: "yuv420p10le".into(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    };
    prober.cache().insert(
        spec.cache_key(),
        EncoderProbe {
            encoder: encoder.into(),
            usable: true,
            rate_control: RateControl::ConstantQuality,
            ..EncoderProbe::default()
        },
    );
}

// DEC-05 and DEC-48: the second question is the first with only the rate control
// changed, built by the same function.
#[cfg(unix)]
#[test]
fn a_cqp_only_driver_is_asked_again_in_constant_quality_and_used() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("calls");
    let fake = cqp_only_ffmpeg(&ledger, CQP_ONLY);
    let prober = Prober::new(&fake.resolver);
    for (encoder, qp) in [("h264_vaapi", 23), ("hevc_vaapi", 25)] {
        let _ = std::fs::remove_file(&ledger);
        let probe = prober.probe_encoder(encoder, None);
        assert!(probe.usable, "{encoder}: {}", probe.reason);
        assert_eq!(probe.rate_control, RateControl::ConstantQuality);
        let calls = calls(&ledger);
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert!(calls[0].contains("-b:v"), "{calls:?}");
        assert!(
            calls[1].contains(&format!("-rc_mode CQP -qp {qp}")) && !calls[1].contains("-b:v"),
            "{calls:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn any_other_refusal_is_not_asked_again() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("calls");
    let fake = cqp_only_ffmpeg(&ledger, HD620_REFUSAL);
    let probe = Prober::new(&fake.resolver).probe_encoder("h264_vaapi", None);
    assert!(!probe.usable);
    assert_eq!(probe.rate_control, RateControl::Bitrate);
    assert_eq!(calls(&ledger).len(), 1, "{:?}", calls(&ledger));
}

#[cfg(unix)]
#[test]
fn only_the_vaapi_family_is_asked_again() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("calls");
    let fake = cqp_only_ffmpeg(&ledger, CQP_ONLY);
    let prober = Prober::new(&fake.resolver);
    for encoder in ["h264_qsv", "hevc_qsv", "h264_nvenc", "libx264"] {
        let _ = std::fs::remove_file(&ledger);
        let probe = prober.probe_encoder(encoder, None);
        assert!(!probe.usable, "{encoder} must stay refused");
        assert_eq!(calls(&ledger).len(), 1, "{encoder}: {:?}", calls(&ledger));
    }
}

// The verdict of the conversion's own probe, at the output size, carries the mode.
#[cfg(unix)]
#[test]
fn the_conversion_probe_returns_the_mode_the_encoder_works_in() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let fake = cqp_only_ffmpeg(&dir.path().join("calls"), CQP_ONLY);
    let prober = Prober::new(&fake.resolver);
    let (encoder, refusal, mode) = prober.verify_encoder_with_rate_control(
        "h264_vaapi",
        Some(&hevc_source("yuv420p10le")),
        false,
        &vaapi_machine(),
        "",
        None,
    );
    assert_eq!(
        (encoder.as_str(), refusal.as_str(), mode),
        ("h264_vaapi", "", RateControl::ConstantQuality)
    );
}

// Stepping back to the CPU leaves constant quality behind: x264 and x265 keep a rate.
#[test]
fn a_cpu_encoder_stepped_back_to_is_asked_for_a_bitrate() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
    let (encoder, _, mode) = prober.verify_encoder_with_rate_control(
        "h264_vaapi",
        Some(&hevc_source("yuv420p10le")),
        false,
        &vaapi_machine(),
        "",
        None,
    );
    assert_eq!(encoder, "libx265");
    assert_eq!(mode, RateControl::Bitrate);
}

#[test]
fn the_plan_line_says_when_the_hardware_works_in_constant_quality_only() {
    let resolver = no_tools();
    let prober = Prober::new(&resolver);
    seed_constant_quality(&prober, "h264_vaapi", "5120x2880");
    let line = prober.describe_verified_hardware_plan(
        &vaapi_machine(),
        Some(&hevc_source("yuv420p10le")),
        false,
        "",
        "",
        None,
    );
    assert!(line.contains("h264_vaapi"), "{line}");
    assert!(line.ends_with(" (constant quality)"), "{line}");
    // And the ordinary case says nothing of it.
    let ordinary = Prober::new(&resolver);
    seed(&ordinary, "h264_vaapi", "5120x2880", true, "");
    let line = ordinary.describe_verified_hardware_plan(
        &vaapi_machine(),
        Some(&hevc_source("yuv420p10le")),
        false,
        "",
        "",
        None,
    );
    assert!(!line.contains("constant quality"), "{line}");
}

#[test]
fn the_diagnostic_says_constant_quality_only() {
    let report = EncoderProbeReport {
        results: vec![
            EncoderProbe {
                encoder: "h264_vaapi".into(),
                usable: true,
                rate_control: RateControl::ConstantQuality,
                took: Duration::from_millis(300),
                ..EncoderProbe::default()
            },
            EncoderProbe {
                encoder: "libx264".into(),
                usable: true,
                took: Duration::from_millis(300),
                ..EncoderProbe::default()
            },
        ],
        took: Duration::from_millis(600),
    };
    let text = describe_encoder_probe(Some(&report));
    assert!(
        text.contains("✅ h264_vaapi: works, constant quality only (300ms)\n"),
        "{text}"
    );
    assert!(text.contains("✅ libx264: works (300ms)\n"), "{text}");
}

// The exact text measured on 2026-10-19; vaapi_encode.c prints each mode after a space,
// so a driver with several modes closes the list later.
#[test]
fn only_a_list_of_cqp_alone_is_the_cqp_only_refusal() {
    let text = |modes: &str| {
        format!(
            "[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes:{modes})."
        )
    };
    assert!(is_cqp_only_refusal("h264_vaapi", &text(" CQP")));
    assert!(is_cqp_only_refusal("hevc_vaapi", &text(" CQP")));
    assert!(!is_cqp_only_refusal("h264_vaapi", &text(" CQP CBR VBR")));
    assert!(!is_cqp_only_refusal("h264_vaapi", &text(" CBR CQP")));
    assert!(!is_cqp_only_refusal("h264_qsv", &text(" CQP")));
}

/// Bit rate: the CQP-only message. Constant quality: `cqp_answer` (a shell snippet).
#[cfg(unix)]
fn cqp_then(ledger: &std::path::Path, cqp_answer: &str) -> FakeTools {
    fake_tools(&[(
        "ffmpeg",
        &format!(
            "echo \"call $*\" >> '{}'\ncase \"$*\" in *-rc_mode\\ CQP*) {cqp_answer};; esac\necho '{CQP_ONLY}' 1>&2\nexit 1",
            ledger.display()
        ),
    )])
}

#[cfg(unix)]
const REFUSE_BIG_FRAME: &str = "case \"$*\" in *s=256x256*) exit 0;; esac; echo 'Hardware does not support encoding at size 5120x2880' 1>&2; exit 1";

#[cfg(unix)]
#[test]
fn a_driver_that_refuses_both_modes_is_asked_twice_and_says_both() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("calls");
    let fake = cqp_then(
        &ledger,
        "echo 'Hardware does not support encoding at size' 1>&2; exit 1",
    );
    let probe = Prober::new(&fake.resolver).probe_encoder("h264_vaapi", None);
    assert!(!probe.usable);
    assert_eq!(calls(&ledger).len(), 2, "{:?}", calls(&ledger));
    assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
    assert!(
        probe
            .reason
            .contains("in constant quality: Hardware does not support encoding"),
        "{}",
        probe.reason
    );
}

// CQP is accepted by the sweep's small frame and refused at the real size: the window
// must not read "supported modes: CQP" alone, which would send the user to the firmware.
#[cfg(unix)]
#[test]
fn constant_quality_accepted_small_and_refused_at_the_real_size_says_so() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let fake = cqp_then(&dir.path().join("calls"), REFUSE_BIG_FRAME);
    let prober = Prober::new(&fake.resolver);
    assert!(prober.probe_encoder("h264_vaapi", None).usable);
    let (encoder, refusal, mode) = prober.verify_encoder_with_rate_control(
        "h264_vaapi",
        Some(&hevc_source("yuv420p10le")),
        false,
        &vaapi_machine(),
        "",
        None,
    );
    assert_eq!((encoder.as_str(), mode), ("libx265", RateControl::Bitrate));
    assert!(
        refusal.contains("supported modes: CQP).")
            && refusal.contains(
                "in constant quality: Hardware does not support encoding at size 5120x2880"
            ),
        "{refusal}"
    );
}

#[cfg(unix)]
#[test]
fn a_second_probe_that_times_out_is_a_verdict_with_both_reasons() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let fake = cqp_then(&dir.path().join("calls"), "exec sleep 30");
    let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(400));
    let spec = sweep_spec("h264_vaapi");
    let probe = prober.probe_encoder_for_size_cached(&spec, None);
    assert!(!probe.usable);
    assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
    assert!(
        probe
            .reason
            .contains("in constant quality: gave no answer in "),
        "{}",
        probe.reason
    );
    assert_eq!(prober.cache().get(&spec.cache_key()), Some(probe));
}

#[cfg(unix)]
#[test]
fn a_second_probe_that_is_cancelled_is_not_cached() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let fake = cqp_then(&dir.path().join("calls"), "exec sleep 30");
    let prober = Prober::new(&fake.resolver);
    let cancel = AtomicBool::new(false);
    let spec = sweep_spec("h264_vaapi");
    let probe = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            cancel.store(true, Ordering::Release);
        });
        prober.probe_encoder_for_size_cached(&spec, Some(&cancel))
    });
    assert!(!probe.usable);
    assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
    assert!(
        !probe.reason.contains("in constant quality"),
        "a cancelled question answered nothing: {}",
        probe.reason
    );
    assert!(prober.cache().is_empty());
}

// F-07: the Hardware line used to ask with no preset while the conversion asks with the
// profile's, so the two questions never shared a cache entry and the driver was asked twice.
#[cfg(unix)]
#[test]
fn the_preview_and_the_conversion_share_one_question_to_the_driver() {
    let _guard = spawn_lock();
    let dir = tempfile::tempdir().unwrap();
    let calls = dir.path().join("calls");
    let fake = fake_tools(&[(
        "ffmpeg",
        &format!("echo \"call $*\" >> '{}'\nexit 0", calls.display()),
    )]);
    let prober = Prober::new(&fake.resolver);
    let video = hevc_source("yuv420p10le");
    prober.describe_verified_hardware_plan(&vaapi_machine(), Some(&video), false, "", "fast", None);
    let (_, refusal, _) = prober.verify_encoder_with_rate_control(
        "h264_vaapi",
        Some(&video),
        false,
        &vaapi_machine(),
        "fast",
        None,
    );
    assert_eq!(refusal, "");
    let launches = std::fs::read_to_string(&calls).unwrap().lines().count();
    assert_eq!(launches, 1, "the conversion asked the driver again");
}

// F-10: a failure to start ffmpeg says nothing about the driver; a later try must be able to
// succeed, so the failure is not remembered.
#[cfg(unix)]
#[test]
fn an_ffmpeg_that_cannot_be_started_is_not_cached() {
    let _guard = spawn_lock();
    // Executable, but its interpreter does not exist: the system refuses to start it.
    let fake = install_tools(&[(
        "ffmpeg".to_string(),
        "#!/nonexistent/interpreter\n".to_string(),
    )]);
    let prober = Prober::new(&fake.resolver);
    let spec = sweep_spec("h264_vaapi");
    let probe = prober.probe_encoder_for_size_cached(&spec, None);
    assert!(!probe.usable);
    assert!(
        prober.cache().is_empty(),
        "a launch failure was cached as the driver's answer"
    );
}
