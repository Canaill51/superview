//! Reproduces the frozen recorded fixture `tests/fixtures/recorded/encode_args.json` field by field:
//! the full argv of `build_encode_base_args` over the matrix `TestParity_EncodeArgs`
//! records.
//!
//! (A fixture frozen from the former Go test of the same name, now removed; not regenerated.)
//! The matrix is rebuilt here from the same tables, and the set of case names must be
//! the oracle's exactly, so a case cannot be skipped or invented without a failure.
//! `probe_args.json` belongs to the probe's port, not to this file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;
use superview_core::encode::args::{RateControl, build_encode_base_args};
use superview_core::video::{VideoSpecs, VideoStream};

fn load() -> serde_json::Map<String, Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recorded/encode_args.json");
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(map) => map,
        other => panic!("encode_args.json: not an object: {other}"),
    }
}

/// A source video of the matrix (the codec is always h264 here).
fn recorded_video(pix_fmt: &str) -> VideoSpecs {
    VideoSpecs {
        file: "input.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
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

/// DEC-52: the one place where the argv is not the frozen oracle's. The input is read with
/// `-f mov`, so a file that is not MP4 is refused instead of being followed (a concat script
/// named .mp4 reads other files). Everything else, field by field, stays the oracle's.
fn with_forced_demuxer(mut oracle: Vec<&str>) -> Vec<&str> {
    let input = oracle
        .iter()
        .position(|arg| *arg == "-i")
        .expect("an input");
    oracle.splice(input..input, ["-f", "mov"]);
    oracle
}

/// DEC-67: a D3D12 encoder is asked for no B-frames, `-bf 0` right after `-c:v <encoder>`. With
/// them the third frame fails, and the oracle predates the measurement.
fn without_d3d12_b_frames(mut oracle: Vec<&str>) -> Vec<&str> {
    if let Some(codec) = oracle.iter().position(|arg| arg.ends_with("_d3d12va")) {
        oracle.splice(codec + 1..codec + 1, ["-bf", "0"]);
    }
    oracle
}

// Go: TestParity_EncodeArgs
#[test]
fn encode_args_match_the_recording() {
    let mut got: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |name: String,
                   pix_fmt: &str,
                   encoder: &str,
                   bitrate: i64,
                   audio: &str,
                   threads: i64,
                   filter_threads: i64,
                   preset: &str| {
        let argv = build_encode_base_args(
            &recorded_video(pix_fmt),
            "x.pgm",
            "y.pgm",
            encoder,
            RateControl::Bitrate,
            bitrate,
            audio,
            threads,
            filter_threads,
            preset,
        );
        assert!(got.insert(name.clone(), argv).is_none(), "duplicate {name}");
    };

    let encoders = [
        "libx264",
        "libx264rgb",
        "libx265",
        "h264_nvenc",
        "hevc_nvenc",
        "h264_amf",
        "hevc_amf",
        "h264_qsv",
        "hevc_qsv",
        "h264_vaapi",
        "hevc_vaapi",
        "h264_vulkan",
        "hevc_vulkan",
        "h264_d3d12va",
        "hevc_d3d12va",
        "h264_v4l2m2m",
        "hevc_v4l2m2m",
    ];
    let pix_fmts = ["", "yuv420p", "yuv420p10le"];
    // The x264 presets plus two edge cases: case/whitespace, and an unknown name.
    let presets = [
        "",
        "ultrafast",
        "superfast",
        "veryfast",
        "faster",
        "fast",
        "medium",
        "slow",
        "slower",
        "veryslow",
        "placebo",
        " FAST ",
        "bogus",
    ];
    let preset_encoders = [
        "libx264",
        "libx265",
        "h264_nvenc",
        "h264_amf",
        "h264_qsv",
        "h264_vaapi",
        "h264_v4l2m2m",
    ];

    // Every encoder x pixel format x (no preset, medium).
    for enc in encoders {
        for pf in pix_fmts {
            for preset in ["", "medium"] {
                add(
                    format!("enc={enc}/pix={pf}/preset={preset}"),
                    pf,
                    enc,
                    8_000_000,
                    "copy",
                    8,
                    0,
                    preset,
                );
            }
        }
    }
    // Every preset on the families that translate them. The recorded case names quote the preset (%q).
    for enc in preset_encoders {
        for preset in presets {
            add(
                format!("preset-sweep/enc={enc}/preset={preset:?}"),
                "yuv420p",
                enc,
                8_000_000,
                "copy",
                8,
                0,
                preset,
            );
        }
    }
    // Threads: libx265's ceiling (16), hardware encoders (no -threads).
    for enc in ["libx264", "libx265", "h264_nvenc", "h264_vaapi"] {
        for threads in [1, 4, 16, 17, 64] {
            add(
                format!("threads/enc={enc}/n={threads}"),
                "yuv420p",
                enc,
                8_000_000,
                "copy",
                threads,
                0,
                "medium",
            );
        }
    }
    // Bitrates in bits per second, and -bufsize = 2 x bitrate.
    for bitrate in [1, 2_000_000, 8_000_000, 100_000_000] {
        add(
            format!("bitrate/n={bitrate}"),
            "yuv420p",
            "libx264",
            bitrate,
            "copy",
            8,
            0,
            "medium",
        );
    }
    // Pixel format names that is_high_bit_depth must tell apart, on a software and a
    // device-frame HEVC encoder (the latter changes the upload format).
    for enc in ["libx265", "hevc_vaapi"] {
        for pf in [
            "yuv420p12le",
            "p010le",
            "yuv420p10be",
            "YUV420P10LE",
            "rgb565le",
            "nv12",
            "yuv410p",
            "gray16le",
        ] {
            add(
                format!("pixfmt/enc={enc}/pix={pf}"),
                pf,
                enc,
                8_000_000,
                "copy",
                8,
                0,
                "medium",
            );
        }
    }
    // Audio and filter threads.
    for audio in ["copy", "aac"] {
        for ft in [0, 2] {
            add(
                format!("audio={audio}/filter_threads={ft}"),
                "yuv420p",
                "libx265",
                8_000_000,
                audio,
                8,
                ft,
                "medium",
            );
        }
    }

    let oracle = load();
    let want_names: BTreeSet<&String> = oracle.keys().collect();
    let got_names: BTreeSet<&String> = got.keys().collect();
    assert_eq!(
        oracle.len(),
        237,
        "the oracle changed size: update this test on purpose"
    );
    assert_eq!(
        got_names, want_names,
        "the case names differ from the oracle's"
    );
    for (name, want) in &oracle {
        let want: Vec<&str> = want
            .as_array()
            .unwrap_or_else(|| panic!("{name}: not an array"))
            .iter()
            .map(|arg| {
                arg.as_str()
                    .unwrap_or_else(|| panic!("{name}: not a string"))
            })
            .collect();
        let got: Vec<&str> = got[name].iter().map(String::as_str).collect();
        assert_eq!(
            got,
            with_forced_demuxer(without_d3d12_b_frames(want)),
            "{name}"
        );
    }
}
