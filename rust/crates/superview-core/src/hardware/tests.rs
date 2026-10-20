use super::*;
use crate::video::VideoStream;

fn ffmpeg(accels: &str, encoders: &str) -> HashMap<String, String> {
    HashMap::from([
        ("accels".to_string(), accels.to_string()),
        ("encoders".to_string(), encoders.to_string()),
    ])
}

fn profile(accels: &[&str], encoders: &[&str]) -> MachineProfile {
    MachineProfile {
        cpu_cores: 0,
        hardware_accels: accels.iter().map(|s| s.to_string()).collect(),
        available_encoders: encoders.iter().map(|s| s.to_string()).collect(),
    }
}

fn no_streams() -> VideoSpecs {
    VideoSpecs {
        file: "test.mp4".into(),
        streams: vec![],
    }
}

// Go: testVideoSpecs
fn test_video_specs(codec: &str) -> VideoSpecs {
    VideoSpecs {
        file: "test.mp4".into(),
        streams: vec![VideoStream {
            codec: codec.into(),
            width: 1920,
            height: 1080,
            bitrate_int: 5_000_000,
            duration_float: 60.0,
            ..VideoStream::default()
        }],
    }
}

// Go: hevcSource
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

fn found(codec: &str, info: &HashMap<String, String>, video: &VideoSpecs) -> String {
    find_encoder(codec, info, video).unwrap()
}

// Go: TestAnalyzeMachineProfile
#[test]
fn analyze_machine_profile_splits_the_capabilities() {
    let p = analyze_machine_profile(&ffmpeg("cuda,vaapi", "h264_nvenc,libx264"), 8);
    assert_eq!(p.cpu_cores, 8);
    assert_eq!(p.hardware_accels, ["cuda", "vaapi"]);
    assert_eq!(p.available_encoders, ["h264_nvenc", "libx264"]);
    let empty = analyze_machine_profile(&HashMap::new(), 4);
    assert!(empty.hardware_accels.is_empty() && empty.available_encoders.is_empty());
}

// Go: parityCapabilities "csv_with_spaces"
#[test]
fn split_csv_trims_and_drops_empty_items() {
    assert_eq!(split_csv(" libx265 , ,libx264 "), ["libx265", "libx264"]);
    assert!(split_csv("  ").is_empty());
    assert!(split_csv("").is_empty());
}

// Go: TestCanUseEncoderWithProfile
#[test]
fn can_use_encoder_with_profile_follows_the_list() {
    let p = profile(&["cuda"], &["h264_nvenc", "libx264"]);
    assert!(can_use_encoder_with_profile("h264_nvenc", Some(&p)));
    assert!(can_use_encoder_with_profile("libx264", Some(&p)));
    assert!(!can_use_encoder_with_profile("hevc_qsv", Some(&p)));
    assert!(!can_use_encoder_with_profile("libx264", None));
}

// Go: TestCanUseEncoderWithProfile_AllowsNVENCWithoutCUDAAccel, _AllowsAMFWithoutDedicatedAccel,
// _AllowsQSVWithoutDedicatedAccel
#[test]
fn can_use_encoder_with_profile_allows_vendor_encoders_without_their_accel() {
    for enc in ["h264_nvenc", "h264_amf", "h264_qsv"] {
        let p = profile(&["d3d11va", "dxva2"], &[enc, "libx264"]);
        assert!(can_use_encoder_with_profile(enc, Some(&p)), "{enc}");
    }
    // The other families do need the matching accelerator.
    let without = profile(&[], &["h264_vaapi"]);
    assert!(!can_use_encoder_with_profile("h264_vaapi", Some(&without)));
    let with = profile(&["vaapi"], &["h264_vaapi"]);
    assert!(can_use_encoder_with_profile("h264_vaapi", Some(&with)));
    // v4l2m2m has no required accelerator.
    let v4l2 = profile(&[], &["h264_v4l2m2m"]);
    assert!(can_use_encoder_with_profile("h264_v4l2m2m", Some(&v4l2)));
}

// Go: TestSelectHardwareDecodeAccel_NVENCPrefersWindowsAccelFallbacks, _AMFPrefersWindowsAccelFallbacks,
// _QSVFallsBackToWindowsAccels
#[test]
fn select_hardware_decode_accel_prefers_the_windows_fallbacks() {
    let p = profile(&["d3d11va", "dxva2"], &[]);
    for enc in ["h264_nvenc", "h264_amf", "h264_qsv"] {
        assert_eq!(
            select_hardware_decode_accel(enc, Some(&p)),
            "d3d11va",
            "{enc}"
        );
    }
    assert_eq!(select_hardware_decode_accel("h264_nvenc", None), "");
    // qsv is never a decode accelerator for a QSV encode (E-07).
    let qsv = profile(&["qsv"], &[]);
    assert_eq!(select_hardware_decode_accel("h264_qsv", Some(&qsv)), "");
}

// Go: TestDescribeHardwareAccelerationPlan_HardwareFallback
#[test]
fn describe_plan_hardware_fallback() {
    let info = ffmpeg("d3d11va,dxva2", "h264_nvenc,libx264");
    assert_eq!(
        describe_hardware_acceleration_plan(&info, Some(&test_video_specs("h264")), ""),
        "Hardware: planned h264_nvenc encode + D3D11VA decode"
    );
}

// Go: TestDescribeHardwareAccelerationPlan_CPUFallback
#[test]
fn describe_plan_cpu_fallback() {
    let info = ffmpeg("", "libx264,libx265");
    assert_eq!(
        describe_hardware_acceleration_plan(&info, Some(&test_video_specs("h264")), ""),
        "Hardware: planned CPU encode (libx264) + CPU decode"
    );
}

// Go: describePlannedHardwarePath, DescribeHardwareAccelerationPlan (the three other lines)
#[test]
fn describe_plan_other_lines() {
    let info = ffmpeg("", "libx264");
    assert_eq!(
        describe_hardware_acceleration_plan(&info, None, ""),
        "Hardware: waiting for input video"
    );
    let no_streams = no_streams();
    assert_eq!(
        describe_hardware_acceleration_plan(&info, Some(&no_streams), ""),
        "Hardware: invalid video: no video streams"
    );
    assert_eq!(
        describe_planned_hardware_path(&HardwarePlan::default()),
        "Hardware: no compatible encoder selected"
    );
    assert_eq!(
        describe_planned_hardware_path(&HardwarePlan {
            encoder: "h264_vaapi".into(),
            hardware_encode: true,
            ..HardwarePlan::default()
        }),
        "Hardware: planned h264_vaapi encode + CPU decode fallback"
    );
    assert_eq!(
        describe_hardware_acceleration_plan(&info, Some(&test_video_specs("h264")), "nope"),
        "Hardware: encoder error: encoder nope not available. Available encoders: libx264"
    );
}

// Go: BuildHardwarePlan
#[test]
fn build_hardware_plan_fills_every_field() {
    let info = ffmpeg("vaapi", "h264_vaapi,libx264,libx265");
    let plan = build_hardware_plan(&info, Some(&hevc_source("yuv420p10le")), "").unwrap();
    assert_eq!(plan.encoder, "h264_vaapi");
    assert_eq!(plan.decode_accel, "vaapi");
    assert!(plan.hardware_encode);
    assert!(plan.codec_switch.contains("8-bit"));
    let err = build_hardware_plan(&info, None, "").unwrap_err();
    assert!(matches!(err, Error::InvalidVideo(_)));
}

// Go: TestIsHardwareEncoder_CoversTheVendorNeutralPair
#[test]
fn is_hardware_encoder_covers_the_vendor_neutral_pair() {
    for encoder in [
        "h264_nvenc",
        "hevc_amf",
        "h264_qsv",
        "hevc_vaapi",
        "h264_vulkan",
        "hevc_vulkan",
        "h264_d3d12va",
        "hevc_d3d12va",
        "h264_v4l2m2m",
    ] {
        assert!(is_hardware_encoder(encoder), "{encoder}");
    }
    for encoder in ["libx264", "libx264rgb", "libx265", ""] {
        assert!(!is_hardware_encoder(encoder), "{encoder:?}");
    }
}

// Go: TestDeviceTypeForEncoder
#[test]
fn device_type_for_encoder_separates_the_upload_paths() {
    let cases = [
        ("h264_vaapi", "vaapi"),
        ("hevc_vaapi", "vaapi"),
        ("h264_vulkan", "vulkan"),
        ("hevc_d3d12va", "d3d12va"),
        // These three create their own context from software frames.
        ("h264_nvenc", ""),
        ("hevc_amf", ""),
        ("h264_qsv", ""),
        ("libx264", ""),
    ];
    for (encoder, want) in cases {
        assert_eq!(device_type_for_encoder(encoder), want, "{encoder}");
        assert_eq!(needs_device_frames(encoder), !want.is_empty(), "{encoder}");
    }
}

// Go: hwDeviceArgs, hwUploadFilters (pinned in Go by the encode-args oracle)
#[test]
fn hw_device_args_and_upload_filters() {
    assert_eq!(
        hw_device_args("h264_vaapi"),
        ["-init_hw_device", "vaapi=sv", "-filter_hw_device", "sv"]
    );
    assert!(hw_device_args("h264_nvenc").is_empty());
    assert_eq!(
        hw_upload_filters("h264_vaapi", "yuv420p10le"),
        ",format=p010,hwupload"
    );
    assert_eq!(
        hw_upload_filters("h264_vulkan", "yuv420p"),
        ",format=nv12,hwupload"
    );
    assert_eq!(hw_upload_filters("libx264", "yuv420p10le"), "");
}

// Go: accelForEncoder (E-07: v4l2m2m has none)
#[test]
fn accel_for_encoder_names_the_hwaccel_token() {
    for (enc, want) in [
        ("h264_nvenc", "cuda"),
        ("h264_qsv", "qsv"),
        ("h264_vaapi", "vaapi"),
        ("h264_vulkan", "vulkan"),
        ("h264_d3d12va", "d3d12va"),
        ("h264_v4l2m2m", ""),
        ("h264_amf", ""),
        ("libx264", ""),
    ] {
        assert_eq!(accel_for_encoder(enc), want, "{enc}");
    }
}

// Go: TestCandidateEncoders_VendorFirstThenTheNeutralPair
#[test]
fn candidate_encoders_vendor_first_then_the_neutral_pair() {
    let cases: [(&str, &[&str]); 2] = [
        (
            "h264",
            &[
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
        ),
        (
            "hevc",
            &[
                "hevc_nvenc",
                "hevc_amf",
                "hevc_qsv",
                "hevc_vaapi",
                "hevc_d3d12va",
                "hevc_vulkan",
                "hevc_v4l2m2m",
                "libx265",
            ],
        ),
    ];
    for (codec, want) in cases {
        let got = candidate_encoders_for_codec(codec);
        assert_eq!(got, want, "{codec}");
        // Whatever the order, no hardware encoder may follow a CPU one.
        let mut seen_cpu = false;
        for encoder in got {
            if !is_hardware_encoder(encoder) {
                seen_cpu = true;
            } else {
                assert!(!seen_cpu, "{codec}: {encoder} comes after a CPU encoder");
            }
        }
    }
    // Aliases and case.
    assert_eq!(candidate_encoders_for_codec("AVC")[0], "h264_nvenc");
    assert_eq!(candidate_encoders_for_codec("h265")[0], "hevc_nvenc");
    assert_eq!(candidate_encoders_for_codec("av1"), ["libx264", "libx265"]);
    // No trimming here, unlike normalize_codec_family.
    assert_eq!(
        candidate_encoders_for_codec(" hevc "),
        ["libx264", "libx265"]
    );
}

// Go: TestDecodeAccelCandidates_D3D12FallsBackThroughTheOlderWindowsPaths
#[test]
fn decode_accel_candidates_d3d12_falls_back_through_the_older_windows_paths() {
    assert_eq!(
        decode_accel_candidates_for_encoder("h264_d3d12va"),
        ["d3d12va", "d3d11va", "dxva2"]
    );
    // Vulkan has no such family: it decodes through Vulkan or not at all.
    assert_eq!(
        decode_accel_candidates_for_encoder("h264_vulkan"),
        ["vulkan"]
    );
    assert_eq!(
        decode_accel_candidates_for_encoder("h264_nvenc"),
        ["cuda", "d3d11va", "dxva2"]
    );
    assert_eq!(
        decode_accel_candidates_for_encoder("h264_qsv"),
        ["d3d11va", "dxva2"]
    );
    assert!(decode_accel_candidates_for_encoder("h264_v4l2m2m").is_empty());
    assert!(decode_accel_candidates_for_encoder("libx264").is_empty());
}

// Go: TestFindEncoder_UseInputCodec
#[test]
fn find_encoder_use_input_codec() {
    let info = ffmpeg("", "libx264,libx265,hevc");
    assert_eq!(found("", &info, &test_video_specs("h264")), "libx264");
}

// Go: TestFindEncoder_PreferHardwareWhenAvailable
#[test]
fn find_encoder_prefer_hardware_when_available() {
    let info = ffmpeg("cuda", "h264_nvenc,libx264,libx265");
    assert_eq!(found("", &info, &test_video_specs("h264")), "h264_nvenc");
}

// Go: TestFindEncoder_AllowsNVENCWithoutCUDAHwAccel, _AllowsAMFWithoutDedicatedHwAccel,
// _AllowsQSVWithoutDedicatedHwAccel, _AllowsHEVCNVENCWithoutCUDAHwAccel,
// _AllowsHEVCAMFWithoutDedicatedHwAccel, _AllowsHEVCQSVWithoutDedicatedHwAccel
#[test]
fn find_encoder_allows_vendor_encoders_without_their_accel() {
    for (codec, enc) in [
        ("h264", "h264_nvenc"),
        ("h264", "h264_amf"),
        ("h264", "h264_qsv"),
        ("hevc", "hevc_nvenc"),
        ("hevc", "hevc_amf"),
        ("hevc", "hevc_qsv"),
    ] {
        let info = ffmpeg("d3d11va,dxva2", &format!("{enc},libx264,libx265"));
        assert_eq!(found("", &info, &test_video_specs(codec)), enc);
    }
}

// Go: TestFindEncoder_SelectValidEncoder
#[test]
fn find_encoder_select_valid_encoder() {
    let info = ffmpeg("", "libx264,libx265,hevc");
    assert_eq!(
        found("libx265", &info, &test_video_specs("h264")),
        "libx265"
    );
}

// Go: TestFindEncoder_InvalidEncoder
#[test]
fn find_encoder_invalid_encoder() {
    let info = HashMap::from([("encoders".to_string(), "libx264,libx265".to_string())]);
    let err = find_encoder("nonexistent", &info, &test_video_specs("h264")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "encoder error: encoder nonexistent not available. Available encoders: libx264,libx265"
    );
    assert!(matches!(err, Error::Encoder(_)));
}

// Go: TestFindEncoder_NoStreams
#[test]
fn find_encoder_no_streams() {
    let info = ffmpeg("", "libx264,libx265");
    let video = VideoSpecs {
        file: "test.mp4".into(),
        streams: vec![],
    };
    let err = find_encoder("libx264", &info, &video).unwrap_err();
    assert!(matches!(err, Error::InvalidVideo(_)));
}

// Go: FindEncoder, last resort (the first listed encoder) and "no encoder found"
#[test]
fn find_encoder_last_resort_and_nothing() {
    let video = test_video_specs("h264");
    assert_eq!(
        found("", &ffmpeg("", "foo_enc,libaom-av1"), &video),
        "foo_enc"
    );
    let err = find_encoder("", &HashMap::new(), &video).unwrap_err();
    assert_eq!(err.to_string(), "encoder error: no encoder found");
}

// Go: TestFindEncoder_UsesHardwareFromTheOtherFamilyWhenTheSourceFamilyHasNone
#[test]
fn find_encoder_uses_hardware_from_the_other_family_when_the_source_family_has_none() {
    let info = ffmpeg("vaapi", "h264_vaapi,libx264,libx265");
    assert_eq!(found("", &info, &hevc_source("yuv420p10le")), "h264_vaapi");
}

// Go: TestFindEncoder_KeepsTheSourceFamilyWhenItsHardwareWorks
#[test]
fn find_encoder_keeps_the_source_family_when_its_hardware_works() {
    let info = ffmpeg("vaapi", "h264_vaapi,hevc_vaapi,libx264,libx265");
    assert_eq!(found("", &info, &hevc_source("yuv420p10le")), "hevc_vaapi");
}

// Go: TestFindEncoder_DoesNotSwitchFamilyForAnotherCPUEncoder
#[test]
fn find_encoder_does_not_switch_family_for_another_cpu_encoder() {
    let info = ffmpeg("", "libx264,libx265");
    assert_eq!(found("", &info, &hevc_source("yuv420p")), "libx265");
}

// Go: TestFindEncoder_ExplicitChoiceIsNotOverridden
#[test]
fn find_encoder_explicit_choice_is_not_overridden() {
    let info = ffmpeg("vaapi", "h264_vaapi,libx264,libx265");
    assert_eq!(
        found("libx265", &info, &hevc_source("yuv420p10le")),
        "libx265"
    );
}

// Go: TestDescribeCodecFamilySwitch_NamesWhatItCosts
#[test]
fn describe_codec_family_switch_names_what_it_costs() {
    let note = describe_codec_family_switch(Some(&hevc_source("yuv420p10le")), "h264_vaapi");
    assert_eq!(
        note,
        "H.265 has no usable hardware encoder here, so the output is H.264; the 10-bit source is stored as 8-bit"
    );
}

// Go: TestDescribeCodecFamilySwitch_MentionsBitDepthOnlyWhenItIsLost
#[test]
fn describe_codec_family_switch_mentions_bit_depth_only_when_it_is_lost() {
    let note = describe_codec_family_switch(Some(&hevc_source("yuv420p")), "h264_vaapi");
    assert_eq!(
        note,
        "H.265 has no usable hardware encoder here, so the output is H.264"
    );
    // Going to HEVC loses nothing either: only the H.264 target drops bits.
    let h264 = test_video_specs("h264");
    let up = describe_codec_family_switch(Some(&h264), "hevc_vaapi");
    assert_eq!(
        up,
        "H.264 has no usable hardware encoder here, so the output is H.265"
    );
}

// Go: TestDescribeCodecFamilySwitch_SaysNothingInTheOrdinaryCase
#[test]
fn describe_codec_family_switch_says_nothing_in_the_ordinary_case() {
    let ten = hevc_source("yuv420p10le");
    let eight = hevc_source("yuv420p");
    let cases: [(&str, Option<&VideoSpecs>, &str); 4] = [
        ("same family, CPU", Some(&ten), "libx265"),
        ("same family, hardware", Some(&ten), "hevc_vaapi"),
        ("no video", None, "h264_vaapi"),
        ("no encoder", Some(&eight), ""),
    ];
    for (name, video, encoder) in cases {
        assert_eq!(describe_codec_family_switch(video, encoder), "", "{name}");
    }
}

// Go: TestSoftwareEncoderForFallback_ReturnsToTheSourceFamily
#[test]
fn software_encoder_for_fallback_returns_to_the_source_family() {
    let p = analyze_machine_profile(&ffmpeg("vaapi", "h264_vaapi,libx264,libx265"), 0);
    assert_eq!(
        software_encoder_for_fallback(Some(&hevc_source("yuv420p10le")), "h264_vaapi", Some(&p)),
        "libx265"
    );
}

// Go: TestSoftwareEncoderForFallback_UsesTheEncoderFamilyWhenTheSourceHasNoneKnown
#[test]
fn software_encoder_for_fallback_uses_the_encoder_family_when_the_source_has_none_known() {
    let p = analyze_machine_profile(&ffmpeg("vaapi", "h264_vaapi,libx264,libx265"), 0);
    let mut video = test_video_specs("vp9");
    video.streams[0].height = 1440;
    assert_eq!(
        software_encoder_for_fallback(Some(&video), "h264_vaapi", Some(&p)),
        "libx264"
    );
    assert_eq!(
        software_encoder_for_fallback(None, "h264_vaapi", Some(&p)),
        "libx264"
    );
    assert_eq!(software_encoder_for_fallback(None, "foo_bar", Some(&p)), "");
}

// Go: TestSoftwareEncoderForFallback_SkipsWhatThisFFmpegDoesNotHave
#[test]
fn software_encoder_for_fallback_skips_what_this_ffmpeg_does_not_have() {
    // An ffmpeg built without libx265: the source family cannot be honoured, so the
    // failed encoder's family is the answer rather than nothing at all.
    let p = analyze_machine_profile(&ffmpeg("vaapi", "h264_vaapi,libx264"), 0);
    assert_eq!(
        software_encoder_for_fallback(Some(&hevc_source("yuv420p")), "h264_vaapi", Some(&p)),
        "libx264"
    );
    // The failed encoder is never its own fallback, and its own family adds nothing.
    assert_eq!(
        software_encoder_for_fallback(Some(&hevc_source("yuv420p")), "libx265", Some(&p)),
        ""
    );
}

// Go: TestDescribePlannedHardwarePath_ShowsWhatTheSwitchCosts
#[test]
fn describe_planned_hardware_path_shows_what_the_switch_costs() {
    let info = ffmpeg("vaapi", "h264_vaapi,libx264,libx265");
    let line = describe_hardware_acceleration_plan(&info, Some(&hevc_source("yuv420p10le")), "");
    assert_eq!(
        line,
        "Hardware: planned h264_vaapi encode + VAAPI decode -- H.265 has no usable hardware encoder here, so the output is H.264; the 10-bit source is stored as 8-bit"
    );
}

// Go: TestWithCodecSwitchNote_LeavesAnOrdinarySummaryAlone
#[test]
fn with_codec_switch_note_leaves_an_ordinary_summary_alone() {
    let summary = "Hardware: used hevc_vaapi encode + VAAPI decode";
    assert_eq!(
        with_codec_switch_note(Some(&hevc_source("yuv420p10le")), "hevc_vaapi", summary),
        summary
    );
    assert_eq!(
        with_codec_switch_note(Some(&hevc_source("yuv420p")), "h264_vaapi", "S"),
        "S -- H.265 has no usable hardware encoder here, so the output is H.264"
    );
}

// Go: normalizeCodecFamily, encoderCodecFamily, otherCodecFamily, softwareEncoderForFamily,
// displayCodecFamily, crossFamilyHardwareEncoder
#[test]
fn codec_family_helpers() {
    assert_eq!(normalize_codec_family(" HEVC "), "hevc");
    assert_eq!(normalize_codec_family("avc"), "h264");
    assert_eq!(normalize_codec_family("av1"), "");
    // libx265 is HEVC through "265", libx264rgb and h264_vaapi are H.264.
    assert_eq!(encoder_codec_family("libx265"), "hevc");
    assert_eq!(encoder_codec_family("HEVC_AMF"), "hevc");
    assert_eq!(encoder_codec_family("libx264rgb"), "h264");
    assert_eq!(encoder_codec_family("h264_vaapi"), "h264");
    assert_eq!(encoder_codec_family("foo_bar"), "");
    assert_eq!(encoder_codec_family(""), "");
    assert_eq!(other_codec_family("h265"), "h264");
    assert_eq!(other_codec_family("avc"), "hevc");
    assert_eq!(other_codec_family("vp9"), "");
    assert_eq!(software_encoder_for_family("h264"), "libx264");
    assert_eq!(software_encoder_for_family("hevc"), "libx265");
    assert_eq!(software_encoder_for_family("x"), "");
    assert_eq!(display_codec_family("h264"), "H.264");
    assert_eq!(display_codec_family("hevc"), "H.265");
    assert_eq!(display_codec_family("other"), "other");

    let p = profile(&["vaapi"], &["h264_vaapi", "libx264"]);
    assert_eq!(
        cross_family_hardware_encoder("hevc", Some(&p)),
        "h264_vaapi"
    );
    assert_eq!(cross_family_hardware_encoder("h264", Some(&p)), "");
    assert_eq!(cross_family_hardware_encoder("av1", Some(&p)), "");
    assert_eq!(cross_family_hardware_encoder("hevc", None), "");
    assert_eq!(source_codec_family(None), "");
    assert_eq!(source_codec_family(Some(&no_streams())), "");
}

// Go: TestIsHighBitDepth
#[test]
fn is_high_bit_depth_tells_the_formats_apart() {
    let cases = [
        ("yuv420p", false),
        ("yuvj420p", false),
        ("", false),
        // No endianness suffix means a single-byte sample, whatever digits the name
        // happens to contain.
        ("nv12", false),
        ("nv16", false),
        ("nv21", false),
        ("rgb24", false),
        ("gray", false),
        // The trap: yuv410p is a genuine 8-bit 4:1:0 format whose name contains "10".
        ("yuv410p", false),
        ("yuv411p", false),
        ("yuv420p10le", true),
        ("YUV420P10LE", true),
        ("  yuv422p10be  ", true),
        ("p010le", true),
        ("yuv420p12le", true),
        ("yuv420p9le", true),
        ("yuv420p8le", false),
        ("gray16be", true),
        ("p016le", true),
        // Packed formats carry a byte order but their digits are a channel layout,
        // not a depth.
        ("rgb565le", false),
        ("rgb444le", false),
        // Not a depth at all: too wide for an integer, and no digits.
        ("yuv99999999999999999999le", false),
        ("yuvle", false),
    ];
    for (pix_fmt, want) in cases {
        assert_eq!(is_high_bit_depth(pix_fmt), want, "{pix_fmt:?}");
    }
}

// Go: TestIsHEVCEncoder
#[test]
fn is_hevc_encoder_matches_hevc_and_265() {
    for enc in [
        "libx265",
        "hevc_nvenc",
        "hevc_qsv",
        "hevc_vaapi",
        "HEVC_AMF",
    ] {
        assert!(is_hevc_encoder(enc), "{enc}");
    }
    for enc in ["libx264", "h264_nvenc", "h264_amf", ""] {
        assert!(!is_hevc_encoder(enc), "{enc:?}");
    }
}

// Go: TestBuildEncodeBaseArgs_NoStreamsFallsBackToEightBit (sourcePixelFormat must not panic)
#[test]
fn source_pixel_format_is_empty_without_streams() {
    assert_eq!(source_pixel_format(None), "");
    assert_eq!(source_pixel_format(Some(&no_streams())), "");
    assert_eq!(
        source_pixel_format(Some(&hevc_source("yuv420p10le"))),
        "yuv420p10le"
    );
}
