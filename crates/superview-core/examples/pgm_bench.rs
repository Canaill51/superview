//! Times the generation of one map pair (not shipped: `cargo run --release --example pgm_bench`).
//! Measures generation at several sizes (files in the temp dir).

use std::time::Instant;

use superview_core::pgm::{generate_pgm, generate_pgm_bytes};
use superview_core::video::{VideoSpecs, VideoStream};

fn main() {
    for (w, h) in [(1440, 1080), (5120, 3840)] {
        for squeeze in [false, true] {
            let video = VideoSpecs {
                file: "bench.mp4".into(),
                streams: vec![VideoStream {
                    codec: "h264".into(),
                    width: w,
                    height: h,
                    duration_float: 60.0,
                    bitrate_int: 5_000_000,
                    ..VideoStream::default()
                }],
            };
            let dir = std::env::temp_dir().join(format!("pgm-bench-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let (mut files, mut memory) = (f64::MAX, f64::MAX);
            for _ in 0..5 {
                let start = Instant::now();
                generate_pgm(&video, squeeze, &dir).unwrap();
                files = files.min(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                std::hint::black_box(generate_pgm_bytes(&video, squeeze).unwrap());
                memory = memory.min(start.elapsed().as_secs_f64() * 1000.0);
            }
            std::fs::remove_dir_all(&dir).unwrap();
            println!(
                "{w}x{h} squeeze={squeeze}: files {files:.1} ms, memory {memory:.1} ms (best of 5, both maps)"
            );
        }
    }
}
