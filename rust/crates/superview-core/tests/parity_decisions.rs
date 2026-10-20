//! Reproduces the frozen parity fixture `tests/fixtures/parity/decisions.json` field by field:
//! `find_encoder`, `software_encoder_for_fallback` and `select_hardware_decode_accel`
//! over the matrix `TestParity_Decisions` records.
//!
//! (A fixture frozen from the former Go test of the same name, now removed; not regenerated.)
//! The matrix is rebuilt here from the same tables, and the set of case names must be
//! the oracle's exactly, so a case cannot be skipped or invented without a failure.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use serde_json::{Value, json};
use superview_core::error::Error;
use superview_core::hardware::{
    MachineProfile, find_encoder, select_hardware_decode_accel, software_encoder_for_fallback,
};
use superview_core::video::{VideoSpecs, VideoStream};

fn load() -> serde_json::Map<String, Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity/decisions.json");
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(map) => map,
        other => panic!("decisions.json: not an object: {other}"),
    }
}

/// Go: parityVideo
fn parity_video(codec: &str, pix_fmt: &str) -> VideoSpecs {
    VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![VideoStream {
            codec: codec.into(),
            width: 1440,
            height: 1080,
            duration: "60".into(),
            duration_float: 60.0,
            bitrate: "5000000".into(),
            bitrate_int: 5_000_000,
            pix_fmt: pix_fmt.into(),
            ..VideoStream::default()
        }],
    }
}

/// Go: parityCapabilities. A nil Go map is an empty one.
fn capabilities() -> Vec<(&'static str, HashMap<String, String>)> {
    let caps = |accels: &str, encoders: &str| {
        let mut map = HashMap::new();
        if !accels.is_empty() {
            map.insert("accels".to_string(), accels.to_string());
        }
        if !encoders.is_empty() {
            map.insert("encoders".to_string(), encoders.to_string());
        }
        map
    };
    vec![
        ("nil_map", HashMap::new()),
        ("empty", HashMap::new()),
        ("cpu_only", caps("", "libx264,libx265")),
        ("cpu_x264_only", caps("", "libx264")),
        ("cpu_x265_only", caps("", "libx265")),
        (
            "nvidia",
            caps("cuda", "h264_nvenc,hevc_nvenc,libx264,libx265"),
        ),
        (
            "nvidia_h264_only",
            caps("cuda", "h264_nvenc,libx264,libx265"),
        ),
        (
            "vaapi_h264_only",
            caps("vaapi", "h264_vaapi,libx264,libx265"),
        ),
        (
            "vaapi_encoder_without_accel",
            caps("", "h264_vaapi,hevc_vaapi,libx264,libx265"),
        ),
        (
            "qsv_without_accel",
            caps("", "h264_qsv,hevc_qsv,libx264,libx265"),
        ),
        (
            "amf_windows",
            caps("d3d11va,dxva2", "h264_amf,hevc_amf,libx264,libx265"),
        ),
        (
            "vulkan_hevc_only",
            caps("vulkan", "hevc_vulkan,libx264,libx265"),
        ),
        (
            "d3d12va_with_accel",
            caps("d3d12va", "h264_d3d12va,hevc_d3d12va,libx264,libx265"),
        ),
        (
            "d3d12va_without_accel",
            caps("", "h264_d3d12va,hevc_d3d12va,libx264,libx265"),
        ),
        (
            "vulkan_hevc_without_accel",
            caps("", "hevc_vulkan,libx264,libx265"),
        ),
        ("only_libx264rgb", caps("", "libx264rgb")),
        ("v4l2m2m_only", caps("", "h264_v4l2m2m,libx264")),
        ("hardware_only_no_cpu", caps("cuda", "h264_nvenc")),
        ("unknown_encoder_only", caps("", "foo_enc,libaom-av1")),
        ("csv_with_spaces", caps("", " libx265 , ,libx264 ")),
    ]
}

/// Go: the `sources` table.
fn sources() -> Vec<(&'static str, VideoSpecs)> {
    vec![
        ("src=h264", parity_video("h264", "yuv420p")),
        ("src=avc", parity_video("avc", "yuv420p")),
        ("src=hevc_padded", parity_video(" hevc ", "yuv420p")),
        ("src=hevc_10bit", parity_video("hevc", "yuv420p10le")),
        ("src=h265", parity_video("h265", "yuv420p")),
        ("src=HEVC_upper", parity_video("HEVC", "yuv420p")),
        ("src=av1", parity_video("av1", "yuv420p")),
        ("src=no_codec_name", parity_video("", "")),
        (
            "src=no_streams",
            VideoSpecs {
                file: "input.mp4".into(),
                streams: vec![],
            },
        ),
    ]
}

fn profile(accels: &[&str], encoders: &[&str]) -> MachineProfile {
    MachineProfile {
        cpu_cores: 0,
        hardware_accels: accels.iter().map(|s| s.to_string()).collect(),
        available_encoders: encoders.iter().map(|s| s.to_string()).collect(),
    }
}

/// The oracle's `error.type` for a Rust error.
fn kind(err: &Error) -> &'static str {
    match err {
        Error::InvalidVideo(_) => "InvalidVideoError",
        Error::Encoder(_) => "EncoderError",
        Error::Session(_) => "SessionError",
        Error::Plain(_) => "plain",
        // No oracle case produces a stop (PR 6a): named so the match stays exhaustive.
        Error::Cancelled => "cancelled",
        Error::StoppedBySignal(_) => "stopped_by_signal",
        Error::FailedAfterOutput(_) => "failed_after_output",
    }
}

fn encoder_result(result: Result<String, Error>) -> Value {
    match result {
        Ok(encoder) => json!({ "encoder": encoder }),
        Err(err) => json!({ "error": { "type": kind(&err), "message": err.to_string() } }),
    }
}

// Go: TestParity_Decisions
#[test]
fn decisions_match_the_go_oracle() {
    let mut got: BTreeMap<String, Value> = BTreeMap::new();
    let mut add = |name: String, value: Value| {
        assert!(
            got.insert(name.clone(), value).is_none(),
            "duplicate {name}"
        );
    };

    for (caps_name, ffmpeg) in capabilities() {
        for (src_name, video) in sources() {
            for requested in ["", "libx264", "hevc_nvenc", "h264_vaapi", "nonexistent"] {
                // A requested encoder ignores the source's codec: two sources are enough.
                if !requested.is_empty() && src_name != "src=h264" && src_name != "src=no_streams" {
                    continue;
                }
                add(
                    format!("FindEncoder/{caps_name}/{src_name}/requested={requested}"),
                    encoder_result(find_encoder(requested, &ffmpeg, &video)),
                );
            }
        }
    }

    let profiles: Vec<(&str, Option<MachineProfile>)> = vec![
        ("nil", None),
        ("empty", Some(MachineProfile::default())),
        ("cpu_both", Some(profile(&[], &["libx264", "libx265"]))),
        ("cpu_x264", Some(profile(&[], &["libx264"]))),
        ("cpu_x265", Some(profile(&[], &["libx265"]))),
        (
            "hardware_accels_only",
            Some(profile(&["cuda", "vaapi"], &[])),
        ),
        (
            "nvidia_full",
            Some(profile(&["cuda"], &["h264_nvenc", "libx264", "libx265"])),
        ),
    ];
    let failed_encoders = [
        "h264_nvenc",
        "hevc_vaapi",
        "libx264",
        "libx265",
        "hevc_amf",
        "foo_bar",
        "",
    ];
    for (p_name, p) in &profiles {
        for (src_name, video) in sources() {
            for failed in failed_encoders {
                add(
                    format!("softwareEncoderForFallback/{p_name}/{src_name}/failed={failed}"),
                    json!({ "encoder": software_encoder_for_fallback(Some(&video), failed, p.as_ref()) }),
                );
            }
        }
    }
    // A nil source, which the code accepts explicitly.
    for (p_name, p) in &profiles {
        add(
            format!("softwareEncoderForFallback/{p_name}/src=nil_video/failed=h264_nvenc"),
            json!({ "encoder": software_encoder_for_fallback(None, "h264_nvenc", p.as_ref()) }),
        );
    }

    let accel_profiles: Vec<(&str, Option<MachineProfile>)> = vec![
        ("nil", None),
        ("empty", Some(MachineProfile::default())),
        ("cuda", Some(profile(&["cuda"], &[]))),
        ("vaapi", Some(profile(&["vaapi"], &[]))),
        ("qsv", Some(profile(&["qsv"], &[]))),
        ("vulkan", Some(profile(&["vulkan"], &[]))),
        ("d3d11va_dxva2", Some(profile(&["dxva2", "d3d11va"], &[]))),
        ("dxva2_only", Some(profile(&["dxva2"], &[]))),
        (
            "d3d12va_d3d11va",
            Some(profile(&["d3d11va", "d3d12va"], &[])),
        ),
        ("cuda_and_d3d11va", Some(profile(&["d3d11va", "cuda"], &[]))),
        (
            "everything",
            Some(profile(
                &[
                    "cuda", "vaapi", "qsv", "vulkan", "d3d11va", "dxva2", "d3d12va",
                ],
                &[],
            )),
        ),
    ];
    for (p_name, p) in &accel_profiles {
        for enc in [
            "h264_nvenc",
            "hevc_nvenc",
            "h264_amf",
            "h264_qsv",
            "h264_vaapi",
            "hevc_vulkan",
            "h264_d3d12va",
            "h264_v4l2m2m",
            "libx264",
            "libx265",
            "",
        ] {
            add(
                format!("selectHardwareDecodeAccel/{p_name}/enc={enc}"),
                json!({ "accel": select_hardware_decode_accel(enc, p.as_ref()) }),
            );
        }
    }

    let oracle = load();
    let want_names: BTreeSet<&String> = oracle.keys().collect();
    let got_names: BTreeSet<&String> = got.keys().collect();
    assert_eq!(
        oracle.len(),
        909,
        "the oracle changed size: update this test on purpose"
    );
    assert_eq!(
        got_names, want_names,
        "the case names differ from the oracle's"
    );
    for (name, want) in &oracle {
        assert_eq!(&got[name], want, "{name}");
    }
}
