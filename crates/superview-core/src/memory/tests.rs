use super::*;
use crate::video::VideoStream;

const GIB: u64 = 1024 * 1024 * 1024;

// reported_clip -- the geometry the whole memory finding was measured on: a
// 3840x2880 source, which the conversion widens to 5120x2880.
fn reported_clip(codec: &str, pix_fmt: &str) -> VideoSpecs {
    VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![VideoStream {
            codec: codec.into(),
            width: 3840,
            height: 2880,
            pix_fmt: pix_fmt.into(),
            duration_float: 43.0,
            bitrate_int: 120_000_000,
            ..VideoStream::default()
        }],
    }
}

fn needed(video: &VideoSpecs, squeeze: bool, encoder: &str, threads: i64) -> u64 {
    memory_needed_for_encode(Some(video), squeeze, encoder, threads)
}

fn check(
    video: &VideoSpecs,
    encoder: &str,
    threads: i64,
    available: Option<u64>,
) -> Result<(), EncoderError> {
    check_memory_for_encode(Some(video), false, encoder, threads, || available)
}

#[test]
fn the_estimate_matches_the_measurement() {
    for (pix_fmt, measured) in [("yuv420p", 3.97), ("yuv420p10le", 6.25)] {
        let video = reported_clip("hevc", pix_fmt);
        let got = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS) as f64 / GIB as f64;
        assert!(
            (measured - 0.15..=measured + 0.15).contains(&got),
            "{pix_fmt}: estimate {got:.2} GiB is not the measured {measured:.2} GiB"
        );
    }
}

#[test]
fn a_ten_bit_source_on_h264_costs_the_eight_bit_rate() {
    let ten = reported_clip("hevc", "yuv420p10le");
    let on_hevc = needed(&ten, false, "libx265", MEMORY_BASELINE_THREADS);
    let on_h264 = needed(&ten, false, "libx264", MEMORY_BASELINE_THREADS);
    assert!(on_h264 < on_hevc, "{on_h264} vs {on_hevc}");
    let eight = reported_clip("hevc", "yuv420p");
    assert_eq!(
        on_h264,
        needed(&eight, false, "libx264", MEMORY_BASELINE_THREADS)
    );
}

#[test]
fn hardware_encoders_have_no_estimate() {
    let video = reported_clip("hevc", "yuv420p10le");
    for encoder in ["h264_vaapi", "hevc_nvenc", "h264_qsv"] {
        assert_eq!(
            needed(&video, false, encoder, MEMORY_BASELINE_THREADS),
            0,
            "{encoder}"
        );
    }
}

#[test]
fn nothing_to_estimate_gives_zero() {
    let no_streams = VideoSpecs {
        file: "clip.mp4".into(),
        streams: vec![],
    };
    let clip = reported_clip("hevc", "yuv420p");
    let baseline = MEMORY_BASELINE_THREADS;
    assert_eq!(
        memory_needed_for_encode(None, false, "libx265", baseline),
        0
    );
    assert_eq!(
        memory_needed_for_encode(Some(&no_streams), false, "libx265", baseline),
        0
    );
    assert_eq!(needed(&clip, false, "", baseline), 0);
}

#[test]
fn the_widened_output_costs_more_than_the_unwidened_one() {
    let video = reported_clip("hevc", "yuv420p");
    let widened = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS);
    let unwidened = needed(&video, true, "libx265", MEMORY_BASELINE_THREADS);
    assert!(widened > unwidened, "{widened} vs {unwidened}");
}

#[test]
fn the_estimate_grows_with_the_threads_as_measured() {
    for pix_fmt in ["yuv420p", "yuv420p10le"] {
        let video = reported_clip("hevc", pix_fmt);
        let base = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS) as f64;
        for (threads, ratio) in [(8, 1.16), (16, 1.47)] {
            let got = needed(&video, false, "libx265", threads) as f64 / base;
            assert!(
                (ratio - 0.03..=ratio + 0.03).contains(&got),
                "{pix_fmt}: {threads} threads cost x{got:.2} the 4-thread estimate, measured x{ratio:.2}"
            );
        }
    }
}

#[test]
fn fewer_threads_than_the_baseline_are_not_credited() {
    let video = reported_clip("hevc", "yuv420p");
    let base = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS);
    for threads in [-1, 0, 1, 2, MEMORY_BASELINE_THREADS] {
        assert_eq!(
            needed(&video, false, "libx265", threads),
            base,
            "{threads} threads"
        );
    }
}

#[test]
fn refuses_what_cannot_fit_and_says_why() {
    let video = reported_clip("hevc", "yuv420p10le");
    let err = check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(4 * GIB))
        .expect_err("expected a refusal when the encode cannot fit in memory");
    for want in ["5120x2880", "10-bit", "libx265", "6.2 GB", "4.0 GB"] {
        assert!(
            err.msg.contains(want),
            "the refusal must say {want:?}, got {:?}",
            err.msg
        );
    }
    assert_eq!(
        err.msg,
        "not enough memory for this conversion: encoding a 5120x2880 10-bit frame with libx265 needs about 6.2 GB, and only 4.0 GB is available. Close other applications, convert a video with fewer pixels, or pick a hardware encoder in the codec list if Diagnostic says this machine accepts one."
    );
}

#[test]
fn allows_what_fits_with_room() {
    let video = reported_clip("hevc", "yuv420p10le");
    check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(16 * GIB)).unwrap();
}

#[test]
fn a_tight_fit_is_warned_about_not_refused() {
    let video = reported_clip("hevc", "yuv420p10le");
    let n = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS);
    check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(n + n / 8)).unwrap();
    // The line is `needed` itself: one byte short is a refusal.
    check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(n)).unwrap();
    check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(n - 1))
        .expect_err("one byte under the estimate must be refused");
}

#[test]
fn an_unreadable_memory_is_not_a_refusal() {
    let video = reported_clip("hevc", "yuv420p10le");
    check(&video, "libx265", MEMORY_BASELINE_THREADS, None).unwrap();
}

#[test]
fn hardware_encoders_are_not_refused() {
    let video = reported_clip("hevc", "yuv420p10le");
    check(&video, "h264_vaapi", MEMORY_BASELINE_THREADS, Some(GIB)).unwrap();
}

#[test]
fn nothing_to_check_is_never_refused() {
    check_memory_for_encode(None, false, "libx265", 4, || Some(0)).unwrap();
    check_memory_for_frame(
        0,
        1080,
        Some(&reported_clip("hevc", "yuv420p")),
        "libx265",
        4,
        || Some(0),
    )
    .unwrap();
}

#[test]
fn refuses_what_only_many_threads_cannot_hold() {
    let video = reported_clip("hevc", "yuv420p10le");
    let available = Some(7 * GIB + GIB / 2);
    check(&video, "libx265", MEMORY_BASELINE_THREADS, available)
        .expect("four threads fit in 7.5 GiB");
    check(&video, "libx265", 16, available)
        .expect_err("sixteen threads need about 9 GiB: expected a refusal with 7.5 GiB free");
}

#[test]
fn format_gb_has_one_decimal() {
    assert_eq!(format_gb(0), "0.0 GB");
    assert_eq!(format_gb(GIB * 3 / 2), "1.5 GB");
    assert_eq!(format_gb(4 * GIB), "4.0 GB");
}

#[test]
fn parse_reads_the_kernel_estimate_in_bytes() {
    let meminfo = b"MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\nBuffers:           12345 kB\n";
    assert_eq!(parse_mem_available(meminfo), Some(3_894_132 * 1024));
}

#[test]
fn parse_does_not_read_mem_free() {
    let meminfo =
        b"MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\n";
    assert_ne!(parse_mem_available(meminfo), Some(204_552 * 1024));
}

#[test]
fn parse_rejects_what_it_cannot_read() {
    for (what, meminfo) in [
        (
            "field absent",
            "MemTotal:        8039284 kB\nMemFree:          204552 kB\n",
        ),
        ("value missing", "MemAvailable:\n"),
        ("value not numeric", "MemAvailable:    plenty kB\n"),
        ("empty file", ""),
    ] {
        assert_eq!(parse_mem_available(meminfo.as_bytes()), None, "{what}");
    }
}

#[cfg(not(windows))]
#[test]
fn the_real_reading_matches_proc_meminfo() {
    let Ok(data) = std::fs::read("/proc/meminfo") else {
        // No procfs: the contract is that the reading is refused, not invented.
        assert_eq!(available_memory_bytes(), None);
        return;
    };
    let Some(expected) = parse_mem_available(&data) else {
        return; // no MemAvailable line on this kernel
    };
    // Inside a container the cgroup may be the smaller ceiling; that is the intended
    // behaviour, so it is part of the expectation.
    let expected = cgroup_memory_left(
        Path::new("/sys/fs/cgroup/memory.max"),
        Path::new("/sys/fs/cgroup/memory.current"),
    )
    .map_or(expected, |left| left.min(expected));
    let available = available_memory_bytes().expect("a reading on a machine with procfs");
    // Memory moves between the two reads; agreement within 25% proves the same field
    // is read, which is what this test is about.
    let delta = (available as f64 - expected as f64).abs();
    assert!(
        delta <= 0.25 * expected as f64,
        "reading {available} is not {expected}"
    );
}

#[cfg(windows)]
#[test]
fn windows_has_no_reading() {
    assert_eq!(available_memory_bytes(), None);
    assert_eq!(format_available_memory(), UNKNOWN_MEMORY);
}

#[test]
fn the_formatted_reading_agrees_with_the_helper() {
    let formatted = format_available_memory();
    if available_memory_bytes().is_none() {
        assert_eq!(formatted, UNKNOWN_MEMORY);
    } else {
        assert_ne!(formatted, UNKNOWN_MEMORY);
        assert!(formatted.ends_with(" GB"), "{formatted}");
    }
}

// In a container MemAvailable describes the host.
#[test]
fn a_cgroup_limit_caps_the_available_memory() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, content: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    };
    let meminfo = write("meminfo", "MemAvailable:   8388608 kB\n"); // 8 GiB
    let current = write("current", "629145600\n"); // 600 MiB

    let max = write("max", "1073741824\n"); // 1 GiB
    assert_eq!(
        available_memory_bytes_from(&meminfo, &max, &current),
        Some(1_073_741_824 - 629_145_600),
        "with a 1 GiB cgroup limit"
    );

    let max = write("max", "max\n");
    assert_eq!(
        available_memory_bytes_from(&meminfo, &max, &current),
        Some(8 * GIB),
        "with no cgroup limit"
    );

    // Usage past the limit leaves nothing; a missing cgroup or meminfo leaves the rest.
    let max = write("max", "1000\n");
    assert_eq!(
        available_memory_bytes_from(&meminfo, &max, &current),
        Some(0)
    );
    let absent = dir.path().join("absent");
    assert_eq!(
        available_memory_bytes_from(&meminfo, &absent, &current),
        Some(8 * GIB)
    );
    assert_eq!(available_memory_bytes_from(&absent, &max, &current), None);
}

// F-19: a MemoryMax on the unit or the slice sits below the cgroup root.
#[test]
fn a_limit_on_a_parent_cgroup_caps_the_available_memory() {
    let root = tempfile::tempdir().unwrap();
    let unit = root.path().join("user.slice/app.service");
    std::fs::create_dir_all(&unit).unwrap();
    let slice = root.path().join("user.slice");
    std::fs::write(slice.join("memory.max"), "2147483648\n").unwrap(); // 2 GiB
    std::fs::write(slice.join("memory.current"), "1073741824\n").unwrap(); // 1 GiB
    std::fs::write(unit.join("memory.max"), "max\n").unwrap();
    std::fs::write(unit.join("memory.current"), "1\n").unwrap();
    let meminfo = root.path().join("meminfo");
    std::fs::write(&meminfo, "MemAvailable:   8388608 kB\n").unwrap(); // 8 GiB

    let files = cgroup_limit_files("0::/user.slice/app.service\n", root.path());
    assert_eq!(files.len(), 3, "root, slice, unit");
    assert_eq!(
        available_memory_bytes_under(&meminfo, &files),
        Some(GIB),
        "the slice's 2 GiB limit, 1 GiB used"
    );
    // No v2 line (a v1 machine): the root alone.
    assert_eq!(cgroup_limit_files("4:memory:/x\n", root.path()).len(), 1);
    // A hostile or odd path cannot climb out of the root.
    let odd = cgroup_limit_files("0::/../../etc\n", root.path());
    assert!(odd.iter().all(|(max, _)| max.starts_with(root.path())));
}
