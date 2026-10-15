//! Reproduces the Go parity oracle `testdata/parity/memory.json` case by case: the memory
//! a software encode is estimated to need, over the matrix `TestParity_Memory` records.
//!
//! The matrix is rebuilt here from the same tables, and the set of case names must be the
//! oracle's exactly, so a case cannot be skipped or invented without a failure.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;
use superview_core::memory::{memory_needed_for_encode, memory_needed_for_frame};
use superview_core::video::{VideoSpecs, VideoStream};

fn load() -> serde_json::Map<String, Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../internal/common/testdata/parity/memory.json");
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(map) => map,
        other => panic!("memory.json: not an object: {other}"),
    }
}

/// Go: parityVideo.
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

// Go: TestParity_Memory
#[test]
fn memory_estimates_match_the_go_oracle() {
    let mut got: BTreeMap<String, u64> = BTreeMap::new();
    let mut add = |name: String, value: u64| {
        assert!(
            got.insert(name.clone(), value).is_none(),
            "duplicate {name}"
        );
    };

    let geometries = [
        (640, 480),
        (1920, 1080),
        (2560, 1440),
        (5120, 2880),
        (7680, 4320),
    ];
    let encoders = [
        "libx264",
        "libx265",
        "h264_nvenc",
        "hevc_vaapi",
        "h264_v4l2m2m",
        "",
    ];
    for (width, height) in geometries {
        for enc in encoders {
            for pf in ["yuv420p", "yuv420p10le"] {
                for threads in [0, 1, 4, 5, 8, 16, 32] {
                    let video = parity_video("hevc", pf);
                    add(
                        format!("{width}x{height}/enc={enc}/pix={pf}/threads={threads}"),
                        memory_needed_for_frame(width, height, Some(&video), enc, threads),
                    );
                }
            }
        }
    }
    // Degenerate cases: zero or negative size, missing source or no stream.
    for (x, y) in [(0, 1080), (1920, 0), (-1, 1080)] {
        let video = parity_video("h264", "");
        add(
            format!("degenerate/{x}x{y}"),
            memory_needed_for_frame(x, y, Some(&video), "libx264", 8),
        );
    }
    add(
        "degenerate/nil_video".into(),
        memory_needed_for_frame(1920, 1080, None, "libx264", 8),
    );
    let empty = VideoSpecs {
        file: String::new(),
        streams: vec![],
    };
    add(
        "degenerate/no_streams".into(),
        memory_needed_for_frame(1920, 1080, Some(&empty), "libx264", 8),
    );

    // The same estimate from the source: goes through remap_output_size.
    for squeeze in [false, true] {
        for enc in ["libx264", "libx265"] {
            let video = parity_video("h264", "yuv420p");
            add(
                format!("from_source/1440x1080/squeeze={squeeze}/enc={enc}/threads=8"),
                memory_needed_for_encode(Some(&video), squeeze, enc, 8),
            );
        }
    }
    add(
        "from_source/nil".into(),
        memory_needed_for_encode(None, false, "libx264", 8),
    );
    add(
        "from_source/no_streams".into(),
        memory_needed_for_encode(Some(&empty), false, "libx264", 8),
    );

    let oracle = load();
    let want: Vec<&String> = oracle.keys().collect();
    let have: Vec<&String> = got.keys().collect();
    let mut want_sorted = want.clone();
    want_sorted.sort();
    assert_eq!(have, want_sorted, "the case names differ from the oracle's");
    for (name, value) in &got {
        assert_eq!(
            oracle[name].as_u64(),
            Some(*value),
            "{name}: oracle {} vs port {value}",
            oracle[name]
        );
    }
}
