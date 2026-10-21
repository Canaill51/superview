//! Tests that need a real ffmpeg and are covered by no other test: the remap stability
//! of the binary maps, what a conversion preserves, the HEVC tag, and the device-frame
//! encoders. Public API only.
//!
//! Without ffmpeg, ffprobe or the encoder a test needs, it skips with a message, unless
//! `SUPERVIEW_REQUIRE_FFMPEG=1` (CI), where it panics instead. A clip that cannot be rendered
//! panics in every case: stricter than a skip. The
//! device-frame test skips even then, and has never run on a CI runner (no GPU). The
//! tests start processes, so
//! they run one at a time. Nothing here modifies the process environment.

mod common;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use common::{parse_p5, sha256_hex, video};
use superview_core::config::Config;
use superview_core::encode::run::StopControl;
use superview_core::error::Error;
use superview_core::pgm::generate_pgm;
use superview_core::pipeline::{PipelineServices, PipelineUi, perform_encoding};
use superview_core::probe::Prober;
use superview_core::tools::{ToolResolver, check_ffmpeg};
use tempfile::TempDir;

static SERIAL: Mutex<()> = Mutex::new(());

/// What a test needs once the machine has what it asks for.
struct Rig {
    _lock: MutexGuard<'static, ()>,
    resolver: ToolResolver,
    caps: HashMap<String, String>,
}

/// The machine's ffmpeg, or `None` (test skipped) when it, ffprobe or one of `encoders` is
/// missing -- unless `SUPERVIEW_REQUIRE_FFMPEG=1`, where that panics.
fn real_ffmpeg(encoders: &[&str]) -> Option<Rig> {
    let lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
    let resolver = ToolResolver::from_process();
    let has = |mut cmd: Command| {
        cmd.arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let problem = if !has(resolver.ffmpeg_command(Vec::<&str>::new()))
        || !has(resolver.ffprobe_command(Vec::<&str>::new()))
    {
        Some("no ffmpeg/ffprobe".to_string())
    } else {
        match check_ffmpeg(&Config::default(), &resolver) {
            Err(err) => Some(format!("no usable ffmpeg ({err})")),
            Ok(caps) => match encoders.iter().find(|e| !caps["encoders"].contains(**e)) {
                Some(missing) => Some(format!("this ffmpeg has no {missing}")),
                None => {
                    return Some(Rig {
                        _lock: lock,
                        resolver,
                        caps,
                    });
                }
            },
        }
    };
    let problem = problem.unwrap();
    assert!(!require, "{problem} although SUPERVIEW_REQUIRE_FFMPEG=1");
    eprintln!("skipped: {problem}");
    None
}

/// Runs ffmpeg and returns its standard output; fails the test if it exits with an error.
fn ffmpeg_stdout(resolver: &ToolResolver, args: &[&str]) -> Vec<u8> {
    let out = resolver
        .ffmpeg_command(args)
        .output()
        .expect("ffmpeg starts");
    assert!(
        out.status.success(),
        "ffmpeg {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// `ffprobe -v error <args> -of default=nw=1:nk=1 <file>`, trimmed (Windows prints `\r\n`).
fn probe_entry(resolver: &ToolResolver, file: &Path, args: &[&str]) -> String {
    let out = resolver
        .ffprobe_command(["-v", "error"])
        .args(args)
        .args(["-of", "default=nw=1:nk=1"])
        .arg(file)
        .output()
        .expect("ffprobe starts");
    assert!(
        out.status.success(),
        "ffprobe {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Makes a test clip (here with `-preset ultrafast`): a 4:3 `testsrc` clip, `seconds` long
/// at 30 fps.
fn test_clip(resolver: &ToolResolver, dir: &Path, w: i64, h: i64, seconds: i64) -> PathBuf {
    let file = dir.join("clip.mp4");
    ffmpeg_stdout(
        resolver,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={w}x{h}:rate=30"),
            "-t",
            &seconds.to_string(),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-b:v",
            "2M",
            "-pix_fmt",
            "yuv420p",
            &file.to_string_lossy(),
        ],
    );
    file
}

/// The pipeline's questions, answered with a fixed encoder and bitrate.
struct Ui {
    encoder: &'static str,
    bitrate: i64,
}

impl PipelineUi for Ui {
    fn get_encoder(&self) -> String {
        self.encoder.to_string()
    }
    fn get_bitrate(&self) -> Result<i64, Error> {
        Ok(self.bitrate)
    }
    fn get_squeeze(&self) -> bool {
        false
    }
    fn show_progress(&self, _percent: f64) {}
}

/// One whole conversion with the default configuration, on the real machine.
fn convert(
    rig: &Rig,
    input: &Path,
    output: &Path,
    ui: &Ui,
) -> superview_core::pipeline::EncodeOutcome {
    use std::sync::atomic::{AtomicBool, AtomicI32};
    let prober = Prober::new(&rig.resolver);
    let services = PipelineServices::system(&rig.resolver, &prober);
    let cancel = AtomicBool::new(false);
    let signal = AtomicI32::new(0);
    let stop = StopControl {
        cancel: &cancel,
        signal: &signal,
    };
    perform_encoding(None, input, output, ui, &rig.caps, &services, &stop)
        .unwrap_or_else(|e| panic!("perform_encoding with {}: {e}", ui.encoder))
}

// ---- remap output is stable ----

/// Rewrites a binary map as the ASCII form, every sample kept.
fn p5_to_p2(src: &Path, dst: &Path) {
    let data = std::fs::read(src).unwrap();
    let (_, body, width, height) = parse_p5(&data);
    let mut text = format!("P2 {width} {height} 65535\n");
    for row in body.chunks(width * 2) {
        for pair in row.chunks(2) {
            write!(text, "{} ", u16::from(pair[0]) << 8 | u16::from(pair[1])).unwrap();
        }
        text.push('\n');
    }
    std::fs::write(dst, text).unwrap();
}

/// The SHA-256 of the frames the production filter chain decodes with these maps.
fn remap_fingerprint(resolver: &ToolResolver, clip: &Path, x: &Path, y: &Path) -> String {
    let out = ffmpeg_stdout(
        resolver,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            &clip.to_string_lossy(),
            "-i",
            &x.to_string_lossy(),
            "-i",
            &y.to_string_lossy(),
            "-filter_complex",
            "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-",
        ],
    );
    assert!(
        !out.is_empty(),
        "remap with {} produced no frames",
        x.display()
    );
    sha256_hex(&out)
}

// Self-referential on purpose (L-36): two runs of the ffmpeg that is present, never a
// fingerprint written down, which would depend on the encoder build.
#[test]
fn the_binary_maps_remap_like_the_ascii_ones() {
    let Some(rig) = real_ffmpeg(&["libx264"]) else {
        return;
    };
    for (w, h, squeeze) in [(320, 240, false), (320, 240, true), (642, 482, false)] {
        let work = TempDir::new().unwrap();
        let clip = test_clip(&rig.resolver, work.path(), w, h, 1);
        let (x, y) = generate_pgm(&video(w, h), squeeze, work.path()).unwrap();
        let ascii_x = work.path().join("x_ascii.pgm");
        let ascii_y = work.path().join("y_ascii.pgm");
        p5_to_p2(&x, &ascii_x);
        p5_to_p2(&y, &ascii_y);

        let from_binary = remap_fingerprint(&rig.resolver, &clip, &x, &y);
        let from_ascii = remap_fingerprint(&rig.resolver, &clip, &ascii_x, &ascii_y);
        assert_eq!(
            from_binary, from_ascii,
            "{w}x{h} squeeze={squeeze}: the binary maps do not remap like the ASCII ones"
        );
    }
}

// ---- depth, tracks and metadata are preserved ----

const CREATION_TIME: &str = "2026-01-15T10:30:00Z";

// Each assertion fails on the argument list from before the fix: the chain pinned to
// yuv420p flattened 10 bits, nothing mapped the second track, and with three inputs ffmpeg
// inherits the global metadata from none of them.
#[test]
fn a_conversion_keeps_the_depth_the_tracks_and_the_date() {
    let Some(rig) = real_ffmpeg(&["libx265"]) else {
        return;
    };
    let work = TempDir::new().unwrap();
    let input = work.path().join("rich.mp4");
    ffmpeg_stdout(
        &rig.resolver,
        &[
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-t",
            "1",
            "-c:v",
            "libx265",
            "-x265-params",
            "log-level=error",
            "-pix_fmt",
            "yuv420p10le",
            "-b:v",
            "2M",
            "-c:a",
            "aac",
            "-metadata",
            &format!("creation_time={CREATION_TIME}"),
            &input.to_string_lossy(),
        ],
    );

    // Guard the fixture: if it is not 10-bit with two tracks, the assertions below would
    // pass vacuously.
    let r = &rig.resolver;
    let depth = |file: &Path| {
        probe_entry(
            r,
            file,
            &["-select_streams", "v:0", "-show_entries", "stream=pix_fmt"],
        )
    };
    let audio_tracks = |file: &Path| {
        probe_entry(
            r,
            file,
            &["-select_streams", "a", "-show_entries", "stream=index"],
        )
        .split_whitespace()
        .count()
    };
    let problem = if depth(&input) != "yuv420p10le" {
        Some(format!(
            "fixture is not 10-bit (pix_fmt={:?})",
            depth(&input)
        ))
    } else if audio_tracks(&input) != 2 {
        Some("fixture does not carry two audio tracks".to_string())
    } else {
        None
    };
    if let Some(problem) = problem {
        assert!(
            std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() != Ok("1"),
            "{problem} although SUPERVIEW_REQUIRE_FFMPEG=1"
        );
        eprintln!("skipped: {problem}");
        return;
    }

    let output = work.path().join("rich-out.mp4");
    convert(
        &rig,
        &input,
        &output,
        &Ui {
            encoder: "libx265",
            bitrate: 2_000_000,
        },
    );

    assert_eq!(
        depth(&output),
        "yuv420p10le",
        "the 10-bit source was flattened to 8 bits"
    );
    assert_eq!(audio_tracks(&output), 2, "an audio track was dropped");
    let created = probe_entry(r, &output, &["-show_entries", "format_tags=creation_time"]);
    assert!(
        created.starts_with("2026-01-15T10:30:00"),
        "creation_time = {created:?}, want the source's {CREATION_TIME:?}"
    );
}

// ---- HEVC output is tagged hvc1 ----

// MP4 can tag HEVC "hev1" or "hvc1"; ffmpeg's default is hev1, which Apple players refuse.
#[test]
fn hevc_output_is_tagged_hvc1() {
    let Some(rig) = real_ffmpeg(&["libx264", "libx265"]) else {
        return;
    };
    let work = TempDir::new().unwrap();
    let input = test_clip(&rig.resolver, work.path(), 320, 240, 1);
    let output = work.path().join("out.mp4");
    convert(
        &rig,
        &input,
        &output,
        &Ui {
            encoder: "libx265",
            bitrate: 1_000_000,
        },
    );
    let tag = probe_entry(
        &rig.resolver,
        &output,
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_tag_string",
        ],
    );
    assert_eq!(tag, "hvc1", "Apple players refuse hev1");
}

// ---- a device-frame encoder runs the whole pipeline ----

// VAAPI, Vulkan and D3D12 need -init_hw_device before the input and an hwupload after the
// remap. A mistake in either does not fail loudly: the encode falls back to the CPU and
// succeeds, slower. So the assertion that matters is which path produced the file.
#[test]
fn a_device_frame_encoder_runs_the_whole_pipeline() {
    let Some(rig) = real_ffmpeg(&["libx264"]) else {
        return;
    };
    let prober = Prober::new(&rig.resolver);
    let encoder = ["h264_vulkan", "h264_vaapi", "h264_d3d12va"]
        .into_iter()
        .find(|candidate| {
            if !rig.caps["encoders"].contains(candidate) {
                return false;
            }
            let probe = prober.probe_encoder(candidate, None);
            if !probe.usable {
                eprintln!(
                    "{candidate} is advertised but refused the probe: {}",
                    probe.reason
                );
            }
            probe.usable
        });
    // A plain skip, even under SUPERVIEW_REQUIRE_FFMPEG=1: this is about the
    // machine's GPU, not about ffmpeg being installed, and CI runners have no usable one.
    // Failing here would make the suite red on hardware grounds.
    let Some(encoder) = encoder else {
        eprintln!("skipped: no device-frame encoder is usable on this machine");
        return;
    };
    eprintln!("using {encoder}");

    let work = TempDir::new().unwrap();
    let input = test_clip(&rig.resolver, work.path(), 640, 480, 2);
    let output = work.path().join("output.mp4");
    let outcome = convert(
        &rig,
        &input,
        &output,
        &Ui {
            encoder,
            bitrate: 2_000_000,
        },
    );

    // The upload comes after the remap, so it cannot move a pixel.
    let size = probe_entry(
        &rig.resolver,
        &output,
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
        ],
    );
    assert_eq!(size.split_whitespace().collect::<Vec<_>>(), ["852", "480"]);

    // The summary must say what it did, not merely name the encoder: the fallback line
    // "Hardware: h264_vulkan failed; used CPU encode (libx264)" names it too.
    let summary = &outcome.summary.hardware_acceleration;
    let want = format!("Hardware: used {encoder} encode");
    assert!(
        summary.starts_with(&want),
        "the conversion did not use {encoder} -- it fell back to the CPU: {summary:?}"
    );
}
