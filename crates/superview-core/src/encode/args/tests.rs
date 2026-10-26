use super::*;
use crate::video::VideoStream;

fn no_streams() -> VideoSpecs {
    VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![],
    }
}

fn build(
    video: &VideoSpecs,
    encoder: &str,
    bitrate: i64,
    audio: &str,
    threads: i64,
    filter_threads: i64,
    preset: &str,
) -> Vec<String> {
    build_encode_base_args(
        video,
        "x.pgm",
        "y.pgm",
        encoder,
        RateControl::Bitrate,
        bitrate,
        audio,
        threads,
        filter_threads,
        preset,
    )
}

fn joined(args: &[String]) -> String {
    args.join(" ")
}

#[test]
fn validate_bitrate_cases() {
    // (name, bitrate, min, max, wants an error)
    let cases: [(&str, i64, i64, i64, bool); 8] = [
        (
            "valid bitrate in range",
            5_000_000,
            100_000,
            50_000_000,
            false,
        ),
        (
            "bitrate at minimum boundary",
            100_000,
            100_000,
            50_000_000,
            false,
        ),
        (
            "bitrate at maximum boundary",
            50_000_000,
            100_000,
            50_000_000,
            false,
        ),
        ("bitrate below minimum", 50_000, 100_000, 50_000_000, true),
        (
            "bitrate above maximum",
            100_000_000,
            100_000,
            50_000_000,
            true,
        ),
        ("zero bitrate", 0, 100_000, 50_000_000, true),
        ("negative bitrate", -1_000_000, 100_000, 50_000_000, true),
        ("no min/max constraints", 12345, 0, 0, false),
    ];
    for (name, bitrate, min, max, wants_err) in cases {
        assert_eq!(
            validate_bitrate(bitrate, min, max).is_err(),
            wants_err,
            "{name}"
        );
    }
}

#[test]
fn validate_bitrate_with_config() {
    let cfg = Config {
        min_bitrate: 256_000,
        max_bitrate: 10_000_000,
        ..Config::default()
    };
    for (name, bitrate, wants_err) in [
        ("valid bitrate", 5_000_000, false),
        ("below custom min", 100_000, true),
        ("above custom max", 50_000_000, true),
        ("at custom min", 256_000, false),
        ("at custom max", 10_000_000, false),
    ] {
        assert_eq!(
            validate_bitrate(bitrate, cfg.min_bitrate, cfg.max_bitrate).is_err(),
            wants_err,
            "{name}"
        );
    }
}

// The messages are pinned here.
#[test]
fn validate_bitrate_messages_are_v0_2_9s() {
    let text = |r: Result<(), Error>| r.unwrap_err().to_string();
    assert_eq!(
        text(validate_bitrate(0, 0, 0)),
        "bitrate must be positive, got 0"
    );
    assert_eq!(
        text(validate_bitrate(5, 10, 0)),
        "bitrate 5 is below minimum 10 bits/second"
    );
    assert_eq!(
        text(validate_bitrate(50, 0, 10)),
        "bitrate 50 exceeds maximum 10 bits/second"
    );
}

#[test]
fn build_encode_base_args_defaults() {
    let args = build(&no_streams(), "libx264", 2_000_000, "aac", 6, 0, "");
    let j = joined(&args);
    // -re throttles the input to realtime; it must never appear for file output.
    assert!(!args.iter().any(|a| a == "-re"), "{args:?}");
    assert!(!j.contains("-preset"), "{args:?}");
    assert!(!j.contains("-filter_threads"), "{args:?}");
    assert!(j.contains("-threads 6"), "{args:?}");
    assert!(j.contains("-b:v 2000000"), "{args:?}");
}

#[test]
fn build_encode_base_args_preset_and_filter_threads() {
    let args = build(&no_streams(), "libx264", 2_000_000, "copy", 8, 4, "fast");
    let j = joined(&args);
    assert!(!args.iter().any(|a| a == "-re"), "{args:?}");
    assert!(j.contains("-preset fast"), "{args:?}");
    assert!(j.contains("-filter_threads 4"), "{args:?}");
    assert!(j.contains("-threads 8"), "{args:?}");
}

fn preset_of(encoder: &str, preset: &str) -> String {
    joined(&build(
        &no_streams(),
        encoder,
        2_000_000,
        "copy",
        8,
        4,
        preset,
    ))
}

#[test]
fn amf_maps_fast_preset() {
    let j = preset_of("h264_amf", "fast");
    assert!(j.contains("-preset speed"), "{j}");
    assert!(!j.contains("-preset fast"), "{j}");
}

#[test]
fn amf_maps_medium_preset() {
    assert!(preset_of("hevc_amf", "medium").contains("-preset balanced"));
}

#[test]
fn amf_suppresses_unsupported_preset() {
    assert!(!preset_of("h264_amf", "p4").contains("-preset"));
}

#[test]
fn qsv_keeps_supported_preset() {
    assert!(preset_of("h264_qsv", "fast").contains("-preset fast"));
}

#[test]
fn qsv_maps_ultrafast_preset() {
    assert!(preset_of("h264_qsv", "ultrafast").contains("-preset veryfast"));
}

#[test]
fn qsv_maps_placebo_preset() {
    assert!(preset_of("hevc_qsv", "placebo").contains("-preset veryslow"));
}

#[test]
fn qsv_suppresses_unsupported_preset() {
    assert!(!preset_of("h264_qsv", "p4").contains("-preset"));
}

#[test]
fn libx265_adds_quiet_params() {
    let args = build(&no_streams(), "libx265", 2_000_000, "aac", 4, 0, "slow");
    assert!(
        joined(&args).contains("-x265-params log-level=error"),
        "{args:?}"
    );
    let other = build(&no_streams(), "libx264", 2_000_000, "aac", 4, 0, "slow");
    assert!(!joined(&other).contains("-x265-params"), "{other:?}");
}

#[test]
fn remap_filter_chain_cases() {
    let head = "[0:v:0][1:v:0][2:v:0]remap,";
    // (name, pix_fmt, encoder, chain after remap)
    let cases = [
        (
            "8-bit source stays 8-bit",
            "yuv420p",
            "libx265",
            "format=yuv444p,format=yuv420p[v]",
        ),
        (
            "10-bit source with an HEVC encoder keeps its depth",
            "yuv420p10le",
            "libx265",
            "format=yuv444p10le,format=yuv420p10le[v]",
        ),
        (
            "10-bit source with hardware HEVC keeps its depth",
            "yuv420p10le",
            "hevc_nvenc",
            "format=yuv444p10le,format=yuv420p10le[v]",
        ),
        // h264_nvenc cannot encode 10-bit at all.
        (
            "10-bit source with an H.264 encoder falls back to 8-bit",
            "yuv420p10le",
            "h264_nvenc",
            "format=yuv444p,format=yuv420p[v]",
        ),
        // Old ffprobe builds do not report pix_fmt; 8-bit is the safe read.
        (
            "unknown pixel format is treated as 8-bit",
            "",
            "libx265",
            "format=yuv444p,format=yuv420p[v]",
        ),
        (
            "a Vulkan encoder gets the frames uploaded",
            "yuv420p",
            "h264_vulkan",
            "format=yuv444p,format=yuv420p,format=nv12,hwupload[v]",
        ),
        (
            "a VAAPI encoder gets the frames uploaded",
            "yuv420p",
            "h264_vaapi",
            "format=yuv444p,format=yuv420p,format=nv12,hwupload[v]",
        ),
        (
            "a 10-bit upload goes through p010, not the planar format",
            "yuv420p10le",
            "hevc_vulkan",
            "format=yuv444p10le,format=yuv420p10le,format=p010,hwupload[v]",
        ),
        (
            "NVENC is left to upload for itself",
            "yuv420p",
            "h264_nvenc",
            "format=yuv444p,format=yuv420p[v]",
        ),
    ];
    for (name, pix_fmt, encoder, tail) in cases {
        assert_eq!(
            remap_filter_chain(pix_fmt, encoder),
            format!("{head}{tail}"),
            "{name}"
        );
    }
}

#[test]
fn device_options_come_before_the_input() {
    let args = build(&no_streams(), "h264_vulkan", 2_000_000, "aac", 6, 0, "");
    let device_at = args
        .iter()
        .position(|a| a == "-init_hw_device")
        .unwrap_or_else(|| panic!("a Vulkan encode carries no -init_hw_device: {args:?}"));
    let input_at = args
        .iter()
        .position(|a| a == "-i")
        .unwrap_or_else(|| panic!("no -i in the arguments at all: {args:?}"));
    assert!(
        device_at < input_at,
        "-init_hw_device is at {device_at}, after the input at {input_at}"
    );
    // And nothing of the sort for an encoder that uploads for itself.
    let cpu = joined(&build(&no_streams(), "libx264", 2_000_000, "aac", 6, 0, ""));
    assert!(!cpu.contains("-init_hw_device"), "{cpu}");
}

#[test]
fn maps_streams_and_metadata() {
    let video = VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![VideoStream {
            codec: "hevc".into(),
            width: 1440,
            height: 1080,
            pix_fmt: "yuv420p10le".into(),
            duration: "1".into(),
            duration_float: 1.0,
            bitrate: "2000000".into(),
            bitrate_int: 2_000_000,
            ..VideoStream::default()
        }],
    };
    let args = build(&video, "libx265", 2_000_000, "aac", 6, 0, "");
    let j = joined(&args);
    // Without these three, ffmpeg keeps one audio track and no global metadata.
    assert!(j.contains("-map [v] -map 0:a?"), "{args:?}");
    assert!(j.contains("-map_metadata 0"), "{args:?}");
    // The "?" is what makes a clip with no audio work rather than fail.
    assert!(!j.contains("-map 0:a "), "{args:?}");
    assert!(j.contains("format=yuv420p10le[v]"), "{args:?}");
}

#[test]
fn no_streams_falls_back_to_eight_bit() {
    let args = build(&no_streams(), "libx265", 2_000_000, "aac", 6, 0, "");
    assert!(
        joined(&args).contains("format=yuv444p,format=yuv420p[v]"),
        "{args:?}"
    );
}

#[test]
fn clamp_encoder_threads_cases() {
    assert_eq!(clamp_encoder_threads("libx265", 24), X265_MAX_FRAME_THREADS);
    // Below the cap, leave it alone.
    assert_eq!(clamp_encoder_threads("libx265", 8), 8);
    // Everything else clamps internally instead of failing; do not second-guess it.
    assert_eq!(clamp_encoder_threads("libx264", 24), 24);
    assert_eq!(clamp_encoder_threads("hevc_nvenc", 24), 24);
}

#[test]
fn libx265_threads_are_capped() {
    let args = build(&no_streams(), "libx265", 2_000_000, "aac", 32, 0, "");
    assert!(joined(&args).contains("-threads 16"), "{args:?}");
}

#[test]
fn constrains_the_bitrate_ceiling() {
    let args = build(&no_streams(), "libx264", 8_000_000, "aac", 6, 0, "");
    let j = joined(&args);
    assert!(j.contains("-b:v 8000000"), "{args:?}");
    assert!(j.contains("-maxrate 8000000"), "{args:?}");
    assert!(j.contains("-bufsize 16000000"), "{args:?}");
}

#[test]
fn encoder_thread_args_cases() {
    assert_eq!(joined(&encoder_thread_args("libx264", 24)), "-threads 24");
    assert_eq!(joined(&encoder_thread_args("libx265", 24)), "-threads 16");
    for enc in [
        "h264_nvenc",
        "hevc_nvenc",
        "h264_amf",
        "hevc_qsv",
        "h264_vaapi",
    ] {
        assert!(encoder_thread_args(enc, 24).is_empty(), "{enc}");
    }
}

#[test]
fn no_threads_for_hardware_encoders() {
    let args = build(&no_streams(), "h264_nvenc", 2_000_000, "aac", 24, 0, "");
    assert!(!joined(&args).contains("-threads"), "{args:?}");
}

// The core count is a parameter here.
#[test]
fn effective_encoder_threads_cases() {
    let cfg = Config {
        encoder_threads: 24,
        ..Config::default()
    };
    assert_eq!(
        effective_encoder_threads(Some(&cfg), "libx265", 8),
        X265_MAX_FRAME_THREADS
    );
    assert_eq!(effective_encoder_threads(Some(&cfg), "libx264", 8), 24);
    // Unconfigured: one per core, clamped like the command line.
    let unset = Config {
        encoder_threads: 0,
        ..Config::default()
    };
    assert_eq!(effective_encoder_threads(Some(&unset), "libx265", 40), 16);
    assert_eq!(effective_encoder_threads(Some(&unset), "libx265", 6), 6);
    assert_eq!(effective_encoder_threads(None, "libx265", 40), 16);
    assert_eq!(effective_encoder_threads(None, "libx264", 40), 40);
    // A negative configured value is not a request either.
    let negative = Config {
        encoder_threads: -3,
        ..Config::default()
    };
    assert_eq!(requested_encoder_threads(Some(&negative), 12), 12);
}

#[test]
fn nvenc_takes_its_own_scale() {
    let cases = [
        ("ultrafast", "p1"),
        ("superfast", "p1"),
        ("veryfast", "p1"),
        ("faster", "p2"),
        ("fast", "p3"),
        ("medium", "p4"),
        ("slow", "p5"),
        ("slower", "p6"),
        ("veryslow", "p7"),
        ("placebo", "p7"),
        ("", ""),
        ("bogus", ""),
    ];
    for (input, want) in cases {
        for encoder in ["h264_nvenc", "hevc_nvenc"] {
            assert_eq!(
                map_video_preset_for_encoder(encoder, input),
                want,
                "{encoder} {input:?}"
            );
        }
    }
}

#[test]
fn device_frame_encoders_take_no_preset() {
    for encoder in ["h264_vaapi", "hevc_vulkan", "h264_d3d12va", "h264_v4l2m2m"] {
        assert_eq!(
            map_video_preset_for_encoder(encoder, "medium"),
            "",
            "{encoder}"
        );
    }
    // The CPU encoders still take the name verbatim.
    assert_eq!(
        map_video_preset_for_encoder("libx264", "veryslow"),
        "veryslow"
    );
    // Case and whitespace are normalised.
    assert_eq!(map_video_preset_for_encoder("libx264", " FAST "), "fast");
    assert_eq!(map_video_preset_for_encoder("h264_qsv", " SLOW "), "slow");
}

// DEC-67: with B-frames, D3D12 encodes two frames and fails on the third.
#[test]
fn d3d12_encoders_are_asked_for_no_b_frames() {
    for rate_control in [RateControl::Bitrate, RateControl::ConstantQuality] {
        for encoder in ["h264_d3d12va", "hevc_d3d12va"] {
            let args = encoder_facing_args(encoder, rate_control, 1_000_000, "slow");
            assert_eq!(args[..4], ["-c:v", encoder, "-bf", "0"], "{encoder}");
        }
        // Nobody else: B-frames were measured broken on D3D12 only.
        for encoder in [
            "libx264",
            "h264_nvenc",
            "hevc_amf",
            "h264_qsv",
            "h264_vaapi",
            "hevc_vulkan",
        ] {
            let args = encoder_facing_args(encoder, rate_control, 1_000_000, "slow");
            assert!(!args.iter().any(|arg| arg == "-bf"), "{encoder}: {args:?}");
        }
    }
}

#[test]
fn hevc_is_tagged_for_apple_players() {
    for encoder in ["libx265", "hevc_nvenc", "hevc_vaapi"] {
        let j = joined(&encoder_facing_args(
            encoder,
            RateControl::Bitrate,
            1_000_000,
            "",
        ));
        assert!(j.contains("-tag:v hvc1"), "{encoder}: {j}");
    }
    for encoder in ["libx264", "h264_nvenc"] {
        let j = joined(&encoder_facing_args(
            encoder,
            RateControl::Bitrate,
            1_000_000,
            "",
        ));
        assert!(!j.contains("-tag:v"), "{encoder}: {j}");
    }
}

// DEC-48: constant quality asks for a quantiser and for no bit rate at all.
#[test]
fn constant_quality_asks_for_a_quantiser_and_no_bitrate() {
    for (encoder, qp) in [("h264_vaapi", "23"), ("hevc_vaapi", "25")] {
        let args = encoder_facing_args(encoder, RateControl::ConstantQuality, 8_000_000, "");
        let j = joined(&args);
        assert!(
            j.contains(&format!("-rc_mode CQP -qp {qp}")),
            "{encoder}: {j}"
        );
        for flag in ["-b:v", "-maxrate", "-bufsize"] {
            assert!(!j.contains(flag), "{encoder} must not carry {flag}: {j}");
        }
    }
}

#[test]
fn constant_quality_is_two_coarser_in_the_fast_profile() {
    for (encoder, medium, fast) in [("h264_vaapi", 23, 25), ("hevc_vaapi", 25, 27)] {
        assert_eq!(constant_quality_qp(encoder, "medium"), medium);
        assert_eq!(constant_quality_qp(encoder, ""), medium);
        assert_eq!(constant_quality_qp(encoder, "fast"), fast);
        assert_eq!(constant_quality_qp(encoder, " Fast "), fast);
        let j = joined(&encoder_facing_args(
            encoder,
            RateControl::ConstantQuality,
            1,
            "fast",
        ));
        assert!(j.contains(&format!("-qp {fast}")), "{encoder}: {j}");
    }
}

#[test]
fn bitrate_mode_is_untouched_by_the_constant_quality_option() {
    let j = joined(&encoder_facing_args(
        "h264_vaapi",
        RateControl::Bitrate,
        4_000_000,
        "fast",
    ));
    assert!(
        j.contains("-b:v 4000000 -maxrate 4000000 -bufsize 8000000"),
        "{j}"
    );
    assert!(!j.contains("-rc_mode") && !j.contains("-qp"), "{j}");
    assert_eq!(RateControl::default(), RateControl::Bitrate);
}

// The ten-bit rule, which `encodes_in_ten_bits` names.
#[test]
fn ten_bits_need_a_high_depth_source_and_an_hevc_encoder() {
    assert!(encodes_in_ten_bits("yuv420p10le", "libx265"));
    assert!(encodes_in_ten_bits("p010le", "hevc_vaapi"));
    assert!(!encodes_in_ten_bits("yuv420p10le", "libx264"));
    assert!(!encodes_in_ten_bits("yuv420p", "libx265"));
    assert!(!encodes_in_ten_bits("", "libx265"));
}
