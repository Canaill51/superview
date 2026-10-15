//! Reproduces the Go parity oracle (`internal/common/testdata/parity/*.json`)
//! field by field. The oracle is produced by `TestParity_PGM` and
//! `TestParity_Specs` in `parity_test.go`.

mod common;

use std::path::PathBuf;

use serde_json::Value;
use superview_core::error::Error;
use superview_core::pgm::{check_geometry, generate_pgm_bytes, remap_map_bytes, remap_output_size};
use superview_core::video::parse_video_specs;

fn go_testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../internal/common/testdata")
        .join(rel)
}

fn load(rel: &str) -> serde_json::Map<String, Value> {
    let text = std::fs::read_to_string(go_testdata(rel)).unwrap();
    match serde_json::from_str(&text).unwrap() {
        Value::Object(map) => map,
        other => panic!("{rel}: not an object: {other}"),
    }
}

/// The oracle's `type` for a Rust error.
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

/// Type for every error; message too for typed ones. The text of "plain"
/// errors follows Go's standard library and is not reproduced.
fn assert_error(case: &str, want: &Value, got: &Error) {
    assert_eq!(kind(got), want["type"], "{case}: error type");
    if want["type"] != "plain" {
        assert_eq!(
            got.to_string(),
            want["message"].as_str().unwrap(),
            "{case}: message"
        );
    }
}

// Go: TestParity_PGM
#[test]
fn pgm_matches_the_go_oracle() {
    let oracle = load("parity/pgm.json");
    assert_eq!(oracle.len(), 14);
    let mut seen_error = false;
    for (name, want) in &oracle {
        let (dims, squeeze) = name.split_once("_squeeze=").unwrap();
        let (w, h) = dims.split_once('x').unwrap();
        let video = common::video(w.parse().unwrap(), h.parse().unwrap());
        let squeeze: bool = squeeze.parse().unwrap();

        let (out_x, out_y) = remap_output_size(&video, squeeze);
        assert_eq!(
            out_x,
            want["out_width"].as_i64().unwrap(),
            "{name}: out_width"
        );
        assert_eq!(
            out_y,
            want["out_height"].as_i64().unwrap(),
            "{name}: out_height"
        );

        match generate_pgm_bytes(&video, squeeze) {
            Err(err) => {
                seen_error = true;
                assert_error(name, &want["error"], &err);
                assert!(
                    want.get("x_sha256").is_none(),
                    "{name}: Go wrote maps, Rust refused"
                );
                // The pipeline refuses on the geometry check before anything.
                assert!(check_geometry(&video, squeeze).is_err(), "{name}");
            }
            Ok((x, y)) => {
                assert!(
                    want.get("error").is_none(),
                    "{name}: Go refused, Rust wrote maps"
                );
                assert_eq!(
                    x.len() as i64,
                    want["x_bytes"].as_i64().unwrap(),
                    "{name}: x_bytes"
                );
                assert_eq!(
                    y.len() as i64,
                    want["y_bytes"].as_i64().unwrap(),
                    "{name}: y_bytes"
                );
                assert_eq!(
                    common::sha256_hex(&x),
                    want["x_sha256"].as_str().unwrap(),
                    "{name}: x"
                );
                assert_eq!(
                    common::sha256_hex(&y),
                    want["y_sha256"].as_str().unwrap(),
                    "{name}: y"
                );
                assert_eq!(
                    remap_map_bytes(&video, squeeze),
                    (x.len() + y.len()) as i64,
                    "{name}"
                );
            }
        }
    }
    assert!(seen_error, "the oracle holds refused portraits");
}

// Go: TestParity_Specs
#[test]
fn specs_match_the_go_oracle() {
    let oracle = load("parity/specs.json");
    let fixtures_dir = go_testdata("ffprobe");
    let mut fixtures: Vec<String> = std::fs::read_dir(&fixtures_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    fixtures.sort();
    let mut recorded: Vec<&String> = oracle.keys().collect();
    recorded.sort();
    assert_eq!(
        fixtures.iter().collect::<Vec<_>>(),
        recorded,
        "fixtures and oracle diverge"
    );

    for name in &fixtures {
        let want = &oracle[name];
        let bytes = std::fs::read(fixtures_dir.join(name)).unwrap();
        match parse_video_specs("input.mp4", &bytes) {
            Err(err) => assert_error(name, &want["error"], &err),
            Ok(specs) => {
                assert!(
                    want.get("error").is_none(),
                    "{name}: Go refused, Rust accepted"
                );
                assert_eq!(specs.file, want["file"].as_str().unwrap(), "{name}: file");
                let streams = want["streams"].as_array().unwrap();
                assert_eq!(specs.streams.len(), streams.len(), "{name}: stream count");
                for (got, want) in specs.streams.iter().zip(streams) {
                    let text = |k: &str| want[k].as_str().unwrap().to_owned();
                    assert_eq!(got.codec, text("codec"), "{name}: codec");
                    assert_eq!(got.width, want["width"].as_i64().unwrap(), "{name}: width");
                    assert_eq!(
                        got.height,
                        want["height"].as_i64().unwrap(),
                        "{name}: height"
                    );
                    assert_eq!(got.duration, text("duration"), "{name}: duration");
                    assert_eq!(
                        got.duration_float,
                        want["duration_float"].as_f64().unwrap(),
                        "{name}"
                    );
                    assert_eq!(got.bitrate, text("bitrate"), "{name}: bitrate");
                    assert_eq!(
                        got.bitrate_int,
                        want["bitrate_int"].as_i64().unwrap(),
                        "{name}"
                    );
                    assert_eq!(got.pix_fmt, text("pix_fmt"), "{name}: pix_fmt");
                    assert_eq!(got.r_frame_rate, text("r_frame_rate"), "{name}");
                    assert_eq!(
                        got.frame_rate,
                        want["frame_rate"].as_f64().unwrap(),
                        "{name}"
                    );
                }
            }
        }
    }
}
