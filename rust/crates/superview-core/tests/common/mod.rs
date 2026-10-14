//! Helpers shared by the integration tests.
#![allow(dead_code)]

use sha2::{Digest, Sha256};
use superview_core::video::{VideoSpecs, VideoStream};

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The minimal source the Go tests build for the PGM tests.
pub fn video(width: i64, height: i64) -> VideoSpecs {
    VideoSpecs {
        file: "parity.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width,
            height,
            duration: "60".into(),
            duration_float: 60.0,
            bitrate: "5000000".into(),
            bitrate_int: 5_000_000,
            ..VideoStream::default()
        }],
    }
}

/// Splits a P5 map into (header line, body, width, height).
pub fn parse_p5(data: &[u8]) -> (String, &[u8], usize, usize) {
    let nl = data
        .iter()
        .position(|&b| b == b'\n')
        .expect("no header line");
    let header = String::from_utf8(data[..nl].to_vec()).unwrap();
    let body = &data[nl + 1..];
    let fields: Vec<&str> = header.split(' ').collect();
    assert_eq!(fields.len(), 4, "header {header:?}");
    assert_eq!(fields[3], "65535");
    let (w, h): (usize, usize) = (fields[1].parse().unwrap(), fields[2].parse().unwrap());
    assert_eq!(body.len(), w * h * 2, "pixel data size");
    (header, body, w, h)
}
