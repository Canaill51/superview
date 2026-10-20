//! Reproduces the frozen parity fixture `tests/fixtures/parity/probe_args.json` field by field:
//! the full argv of `probe_args` over the matrix `TestParity_ProbeArgs` records.
//!
//! (A fixture frozen from the former Go test of the same name, now removed; not regenerated.)
//! The matrix is rebuilt here from the same tables, and the set of case names must be
//! the oracle's exactly, so a case cannot be skipped or invented without a failure.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;
use superview_core::encode::args::RateControl;
use superview_core::probe::{PROBE_BITRATE, ProbeSpec, probe_args, sweep_spec};

fn load() -> serde_json::Map<String, Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity/probe_args.json");
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(map) => map,
        other => panic!("probe_args.json: not an object: {other}"),
    }
}

// Go: TestParity_ProbeArgs
#[test]
fn probe_args_match_the_go_oracle() {
    let mut got: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |name: String, spec: ProbeSpec| {
        assert!(
            got.insert(name.clone(), probe_args(&spec)).is_none(),
            "duplicate {name}"
        );
    };

    for enc in [
        "libx264",
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
        "h264_v4l2m2m",
    ] {
        add(format!("sweep/{enc}"), sweep_spec(enc));
        for pf in ["yuv420p", "yuv420p10le"] {
            add(
                format!("direct/enc={enc}/pix={pf}/size=5120x2880/preset=slow"),
                ProbeSpec {
                    encoder: enc.into(),
                    size: "5120x2880".into(),
                    pix_fmt: pf.into(),
                    bitrate: 8_000_000,
                    preset: "slow".into(),
                    rate_control: RateControl::Bitrate,
                },
            );
        }
    }
    add(
        "direct/empty_pixfmt_no_preset".into(),
        ProbeSpec {
            encoder: "libx265".into(),
            size: "256x256".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        },
    );
    add(
        "direct/preset_unknown".into(),
        ProbeSpec {
            encoder: "h264_nvenc".into(),
            size: "1920x1080".into(),
            pix_fmt: "yuv420p".into(),
            bitrate: 1,
            preset: "bogus".into(),
            rate_control: RateControl::Bitrate,
        },
    );
    add(
        "direct/empty_encoder".into(),
        ProbeSpec {
            size: "256x256".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        },
    );

    let oracle = load();
    let want_names: BTreeSet<&String> = oracle.keys().collect();
    let got_names: BTreeSet<&String> = got.keys().collect();
    assert_eq!(
        oracle.len(),
        14 * 3 + 3,
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
        assert_eq!(got, want, "{name}");
    }
}
