//! DEC-48 end to end: a VAAPI driver that only knows CQP is converted in constant quality
//! instead of being refused, and the CPU encoder it steps back to is asked for a bit rate.
//!
//! The stand-in ffmpeg records every call. It plays an Intel driver: with `-b:v` it answers
//! "supported modes: CQP", with `-rc_mode CQP` it succeeds. Unix only (shell scripts).
#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32};

use superview_core::encode::run::StopControl;
use superview_core::error::Error;
use superview_core::pipeline::{PipelineServices, PipelineUi, perform_encoding};
use superview_core::probe::Prober;
use superview_core::tools::ToolResolver;

struct Ui;

impl PipelineUi for Ui {
    fn get_encoder(&self) -> String {
        "h264_vaapi".into()
    }
    fn get_bitrate(&self) -> Result<i64, Error> {
        Ok(2_000_000)
    }
    fn get_squeeze(&self) -> bool {
        false
    }
    fn show_progress(&self, _: f64) {}
}

fn script(dir: &Path, name: &str, body: &str) {
    let file = dir.join(name);
    fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
}

const PROBE: &str = "cat <<'EOF'\n{\"streams\":[{\"codec_name\":\"h264\",\"width\":640,\"height\":480,\"duration\":\"3.0\",\"bit_rate\":\"2500000\",\"pix_fmt\":\"yuv420p\",\"r_frame_rate\":\"30/1\"}]}\nEOF";

/// Runs one conversion against an ffmpeg that behaves as `driver`; returns the calls.
fn convert(driver: &str) -> Vec<String> {
    let root = tempfile::tempdir().unwrap();
    let (tools, empty) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let ledger = root.path().join("calls");
    script(tools.path(), "ffprobe", PROBE);
    script(
        tools.path(),
        "ffmpeg",
        &format!(
            "echo \"call $*\" >> '{}'\n{driver}\necho out_time_ms=1000000; echo progress=end; exit 0",
            ledger.display()
        ),
    );
    let (out_dir, tmp) = (root.path().join("out"), root.path().join("tmp"));
    fs::create_dir(&out_dir).unwrap();
    fs::create_dir(&tmp).unwrap();
    let input = root.path().join("input.mp4");
    fs::write(&input, "pretend this is a video").unwrap();

    let path = std::env::join_paths(
        std::iter::once(tools.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone()),
        Some(empty.path().to_path_buf()),
    );
    let prober = Prober::new(&resolver);
    let caps: HashMap<String, String> =
        [("encoders", "h264_vaapi,libx264,aac"), ("accels", "vaapi")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
    let services = PipelineServices {
        resolver: &resolver,
        prober: &prober,
        cpu_count: 4,
        temp_dir: tmp,
        free_gb: &|_| Ok(1000.0),
        available_memory: &|| Some(1 << 40),
    };
    let (cancel, signal) = (AtomicBool::new(false), AtomicI32::new(0));
    perform_encoding(
        None,
        &input,
        &out_dir.join("output.mp4"),
        &Ui,
        &caps,
        &services,
        &StopControl {
            cancel: &cancel,
            signal: &signal,
        },
    )
    .expect("the conversion");
    fs::read_to_string(&ledger)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

/// Whether `call` passes `flag` as an argument of its own. A substring test is not enough: the
/// session directory has a random name, and `superview-qpPNs1` once held `-qp`.
fn has_flag(call: &str, flag: &str) -> bool {
    call.split(' ').any(|arg| arg == flag)
}

#[test]
fn a_flag_inside_a_path_is_not_a_flag() {
    // The CPU encode of a CI run that failed on master's code (2026-10-26).
    let call = "call -y -f mov -i /tmp/.tmpNSijha/input.mp4 \
                -i /tmp/.tmpNSijha/tmp/superview-qpPNs1/x.pgm -c:v libx264 -b:v 2000000";
    assert!(!has_flag(call, "-qp"));
    assert!(has_flag(call, "-b:v"));
    assert!(has_flag(&format!("{call} -rc_mode CQP -qp 23"), "-qp"));
}

/// The calls that wrote the video, not the probes (those end in `-f null -`).
fn encodes(calls: &[String]) -> Vec<&String> {
    calls.iter().filter(|c| !c.ends_with("-f null -")).collect()
}

#[test]
fn a_cqp_only_driver_converts_in_constant_quality() {
    let calls = convert(
        "case \"$*\" in *-rc_mode\\ CQP*) ;; *) echo '[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes: CQP).' 1>&2; exit 1;; esac",
    );
    let encodes = encodes(&calls);
    assert_eq!(encodes.len(), 1, "{calls:#?}");
    assert!(
        encodes[0].contains("-c:v h264_vaapi -rc_mode CQP -qp 23 "),
        "{}",
        encodes[0]
    );
    for flag in ["-b:v", "-maxrate", "-bufsize"] {
        assert!(!has_flag(encodes[0], flag), "{flag}: {}", encodes[0]);
    }
}

#[test]
fn a_driver_that_refuses_both_modes_leaves_the_cpu_encoder_a_bitrate() {
    // Bit rate: the CQP-only message, so the second question is asked. Constant quality:
    // another refusal. Only then does the CPU encoder take over.
    let calls = convert(
        "case \"$*\" in *-rc_mode\\ CQP*) echo 'Hardware does not support encoding at size 640x480' 1>&2; exit 1;; *h264_vaapi*) echo '[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes: CQP).' 1>&2; exit 1;; esac",
    );
    let probes: Vec<_> = calls.iter().filter(|c| c.ends_with("-f null -")).collect();
    assert_eq!(probes.len(), 2, "{calls:#?}");
    assert!(
        probes[0].contains("-b:v") && probes[1].contains("-rc_mode CQP"),
        "{probes:#?}"
    );
    let encodes = encodes(&calls);
    assert_eq!(encodes.len(), 1, "{calls:#?}");
    assert!(
        encodes[0].contains("-c:v libx264 -b:v 2000000 "),
        "{}",
        encodes[0]
    );
    assert!(
        !has_flag(encodes[0], "-rc_mode") && !has_flag(encodes[0], "-qp"),
        "{}",
        encodes[0]
    );
}
