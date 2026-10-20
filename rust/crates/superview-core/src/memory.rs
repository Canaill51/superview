//! The memory guard: what a software encode is measured to need, and the refusal. Port of
//! Go's `internal/common/memory.go` (removed in phase 8; last release with it: v0.2.9), plus the real reading of `health.go`
//! (`availableMemoryBytes`, `cgroupMemoryLeft`, `parseMemAvailable`,
//! `formatAvailableMemory`).
//!
//! `tests/fixtures/parity/memory.json` pins the estimate over ~1000 cases
//! (`tests/parity_memory.rs`); the refusal text is Go's, word for word.
//!
//! Deliberate differences from the Go code:
//! - The reading is a parameter of the checks (`available: impl Fn() -> Option<u64>`),
//!   where Go swaps the package variable `readAvailableMemory`. [`available_memory_bytes`]
//!   is the real one; [`available_memory_bytes_from`] takes the three paths it reads, as
//!   the Go test does by overwriting the path variables.
//! - On Windows [`available_memory_bytes`] is `None` without looking: there is no
//!   `/proc/meminfo`, which is also what Go ends up with.
//! - The sizes are `i64` for Go's `int`; products saturate where Go wraps.
//! - `warn` goes through `log::warn!`, the attributes becoming `key=value` text (same
//!   convention as `hardware.rs`).
//! - `str::parse::<u64>` accepts a leading `+` where Go's `ParseUint` refuses it, in
//!   `meminfo` and in the cgroup files. The kernel never writes one.
//! - Left out: `checkMemoryHealth`, the Diagnostic button's own memory line.

use std::path::Path;

use crate::encode::args::encodes_in_ten_bits;
use crate::error::EncoderError;
use crate::hardware::{is_hardware_encoder, source_pixel_format};
use crate::pgm::remap_output_size;
use crate::video::VideoSpecs;

/// Memory a software encode asks for, per pixel of the output frame.
///
/// Measured, not reasoned. The geometry is the one from the report that produced this
/// check -- a 3840x2880 source widened to 5120x2880 -- encoded with libx265 at preset
/// medium, "-threads 4" and "pools=4", which is what a four-core laptop runs:
///
/// ```text
/// 8-bit source .... 3.97 GiB peak RSS -> 290 bytes per output pixel
/// 10-bit source ... 6.25 GiB peak RSS -> 455 bytes per output pixel
/// ```
///
/// That is the figure at four threads. x265 allocates per frame thread, and the
/// application encodes with one per core (up to 16), so the estimate grows with the thread
/// count: see [`MEMORY_THREAD_PERCENT`]. It belongs to the frame rather than to the codec
/// (libx264 peaked within 1% of libx265), and it does not grow with the clip's length.
pub const SOFTWARE_ENCODE_BYTES_PER_PIXEL: u64 = 290;
/// See [`SOFTWARE_ENCODE_BYTES_PER_PIXEL`].
pub const SOFTWARE_ENCODE_BYTES_PER_PIXEL_10_BIT: u64 = 455;

/// The thread count the figures above were measured at.
pub const MEMORY_BASELINE_THREADS: i64 = 4;
/// What each thread beyond the baseline adds, in percent of the baseline figure.
///
/// Q-02, measured on a 16-core machine with libx265 at preset medium (peak RSS): +16% at
/// 8 threads, +47% at 16, so about 4% per thread, on both depths and both sizes. Below the
/// baseline nothing was measured, so nothing is credited.
pub const MEMORY_THREAD_PERCENT: u64 = 4;

/// The margin below which the estimate is worth a warning: needed + needed/4.
pub const MEMORY_HEADROOM_DIVISOR: u64 = 4;

/// What the helpers report where the reading cannot be taken. A word rather than a zero
/// on purpose: "0 MB available" and "we could not look" lead to opposite decisions.
pub const UNKNOWN_MEMORY: &str = "unknown";

/// Estimates the peak memory a conversion will ask for, or 0 when there is no estimate.
///
/// Hardware encoders return 0 deliberately. Nothing here has measured one, and the buffers
/// that dominate the figure above belong to the CPU encoder -- a GPU encode keeps its
/// reference frames in video memory instead. Refusing a conversion on a number nobody
/// measured would be worse than not checking.
///
/// `threads` is the `-threads` value the encode will really be given.
pub fn memory_needed_for_encode(
    video: Option<&VideoSpecs>,
    squeeze: bool,
    encoder: &str,
    threads: i64,
) -> u64 {
    // `remap_output_size` reads the first stream without a guard of its own, so the empty
    // cases are settled before it is called rather than inside it.
    match video {
        Some(v) if !v.streams.is_empty() => {
            let (out_x, out_y) = remap_output_size(v, squeeze);
            memory_needed_for_frame(out_x, out_y, video, encoder, threads)
        }
        _ => 0,
    }
}

/// The estimate for a frame whose size is already known.
///
/// Split out for the caller that has the geometry but not the squeeze flag: the fallback
/// inside the encode, which reads the size from the remap maps rather than recomputing it.
pub fn memory_needed_for_frame(
    out_x: i64,
    out_y: i64,
    video: Option<&VideoSpecs>,
    encoder: &str,
    threads: i64,
) -> u64 {
    let has_stream = video.is_some_and(|v| !v.streams.is_empty());
    if !has_stream || encoder.is_empty() || is_hardware_encoder(encoder) {
        return 0;
    }
    if out_x <= 0 || out_y <= 0 {
        return 0;
    }

    let per_pixel = if encodes_in_ten_bits(source_pixel_format(video), encoder) {
        SOFTWARE_ENCODE_BYTES_PER_PIXEL_10_BIT
    } else {
        SOFTWARE_ENCODE_BYTES_PER_PIXEL
    };

    let mut needed = (out_x as u64)
        .saturating_mul(out_y as u64)
        .saturating_mul(per_pixel);
    if threads > MEMORY_BASELINE_THREADS {
        let extra = (threads - MEMORY_BASELINE_THREADS) as u64;
        needed = needed.saturating_add(
            needed
                .saturating_mul(extra)
                .saturating_mul(MEMORY_THREAD_PERCENT)
                / 100,
        );
    }
    needed
}

/// Renders a byte count the way the messages here quote memory.
pub fn format_gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// Refuses to start a conversion this machine cannot hold, and warns when it will only
/// just fit.
///
/// The disk equivalent, `check_temp_space_for_maps`, guards the remap maps. Nothing
/// guarded the several gigabytes the encode itself asks for, and that failure is the worse
/// of the two by far: the kernel kills ffmpeg, systemd stops the whole application unit
/// with it, and the window disappears mid-conversion. It happened to a user on 8 GiB.
///
/// A reading that cannot be taken (`available` returns `None`) is not a refusal -- same
/// rule as the disk check. On Windows there is no `/proc/meminfo`, so this never fires.
pub fn check_memory_for_encode(
    video: Option<&VideoSpecs>,
    squeeze: bool,
    encoder: &str,
    threads: i64,
    available: impl Fn() -> Option<u64>,
) -> Result<(), EncoderError> {
    match video {
        Some(v) if !v.streams.is_empty() => {
            let (out_x, out_y) = remap_output_size(v, squeeze);
            check_memory_for_frame(out_x, out_y, video, encoder, threads, available)
        }
        _ => Ok(()),
    }
}

/// [`check_memory_for_encode`] for a frame already measured.
pub fn check_memory_for_frame(
    out_x: i64,
    out_y: i64,
    video: Option<&VideoSpecs>,
    encoder: &str,
    threads: i64,
    available: impl Fn() -> Option<u64>,
) -> Result<(), EncoderError> {
    let needed = memory_needed_for_frame(out_x, out_y, video, encoder, threads);
    if needed == 0 {
        return Ok(());
    }

    let Some(available) = available() else {
        return Ok(());
    };

    if available >= needed.saturating_add(needed / MEMORY_HEADROOM_DIVISOR) {
        return Ok(());
    }

    let depth = if encodes_in_ten_bits(source_pixel_format(video), encoder) {
        "10-bit"
    } else {
        "8-bit"
    };

    log::warn!(
        "Memory will be tight for this conversion: output_width={out_x} output_height={out_y} encoder={encoder} depth={depth} threads={threads} needed={} available={}",
        format_gb(needed),
        format_gb(available)
    );

    if available >= needed {
        return Ok(());
    }

    Err(EncoderError {
        msg: format!(
            "not enough memory for this conversion: encoding a {out_x}x{out_y} {depth} frame with {encoder} needs about {}, \
             and only {} is available. Close other applications, convert a video with fewer pixels, \
             or pick a hardware encoder in the codec list if Diagnostic says this machine accepts one.",
            format_gb(needed),
            format_gb(available)
        ),
    })
}

/// How much memory the kernel says it can hand out without swapping, or `None` where the
/// reading cannot be taken -- no `/proc/meminfo`, which includes every Windows machine.
/// Callers must have an answer for "unknown": reporting an encode as impossible because
/// the question could not be asked would be worse than letting it try.
///
/// `MemAvailable`, not `MemFree`. The free figure leaves out the page cache, which the
/// kernel reclaims on demand, so reading it makes a healthy desktop that has been up for a
/// day look like it is out of memory. `MemAvailable` is the kernel's own estimate of what
/// an allocation can actually get.
#[cfg(not(windows))]
pub fn available_memory_bytes() -> Option<u64> {
    available_memory_bytes_from(
        Path::new("/proc/meminfo"),
        Path::new("/sys/fs/cgroup/memory.max"),
        Path::new("/sys/fs/cgroup/memory.current"),
    )
}

/// See the Unix version: Windows has no such reading.
#[cfg(windows)]
pub fn available_memory_bytes() -> Option<u64> {
    None
}

/// [`available_memory_bytes`] over the given files (a test points them at temporary ones).
pub fn available_memory_bytes_from(
    meminfo: &Path,
    cgroup_max: &Path,
    cgroup_current: &Path,
) -> Option<u64> {
    let available = parse_mem_available(&std::fs::read(meminfo).ok()?)?;

    // In a container the kernel's figure describes the host, not the budget the process
    // may use: a cgroup limit of 1 GiB on a 64 GiB machine reads as 60 GiB free. When the
    // cgroup v2 limit is a number, what is left of it is the other ceiling, and the
    // smaller of the two is the honest answer.
    match cgroup_memory_left(cgroup_max, cgroup_current) {
        Some(left) if left < available => Some(left),
        _ => Some(available),
    }
}

/// The memory left under the cgroup v2 limit, or `None` when there is no limit to speak
/// of: no such file, "max", or an unreadable current usage.
pub fn cgroup_memory_left(max_path: &Path, current_path: &Path) -> Option<u64> {
    let read =
        |path: &Path| -> Option<u64> { std::fs::read_to_string(path).ok()?.trim().parse().ok() };
    // "max", or something unparseable, is `None` here.
    let limit = read(max_path)?;
    let current = read(current_path)?;
    Some(limit.saturating_sub(current))
}

/// Pulls the `MemAvailable` line out of `/proc/meminfo`'s contents.
///
/// Separate from the file read so the rejections can be tested: a file without the line, a
/// truncated line, a value that is not a number. Reading the real file can only ever
/// exercise the success path.
pub fn parse_mem_available(data: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(data);
    let line = text
        .split('\n')
        .find(|line| line.starts_with("MemAvailable:"))?;
    // "MemAvailable:   3894132 kB" -- the value is always in kB, and the unit is part of
    // the format rather than something that varies.
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

/// Renders the reading for a log line or a message, or [`UNKNOWN_MEMORY`] when there is
/// none.
pub fn format_available_memory() -> String {
    available_memory_bytes().map_or_else(|| UNKNOWN_MEMORY.to_string(), format_gb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::VideoStream;

    const GIB: u64 = 1024 * 1024 * 1024;

    // Go: reportedClip -- the geometry the whole memory finding was measured on: a
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

    // Go: TestMemoryNeededForEncode_MatchesTheMeasurement
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

    // Go: TestMemoryNeededForEncode_FollowsTheFilterChain
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

    // Go: TestMemoryNeededForEncode_SaysNothingAboutHardwareEncoders
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

    // Go: TestMemoryNeededForEncode_HandlesNothingToEstimate
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

    // Go: TestMemoryNeededForEncode_CostsTheOutputFrameNotTheInput
    #[test]
    fn the_widened_output_costs_more_than_the_unwidened_one() {
        let video = reported_clip("hevc", "yuv420p");
        let widened = needed(&video, false, "libx265", MEMORY_BASELINE_THREADS);
        let unwidened = needed(&video, true, "libx265", MEMORY_BASELINE_THREADS);
        assert!(widened > unwidened, "{widened} vs {unwidened}");
    }

    // Go: TestMemoryNeededForEncode_GrowsWithThreads
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

    // Go: TestMemoryNeededForEncode_NeverBelowTheBaseline
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

    // Go: TestCheckMemoryForEncode_RefusesWhatCannotFit
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

    // Go: TestCheckMemoryForEncode_AllowsWhatFitsWithRoom
    #[test]
    fn allows_what_fits_with_room() {
        let video = reported_clip("hevc", "yuv420p10le");
        check(&video, "libx265", MEMORY_BASELINE_THREADS, Some(16 * GIB)).unwrap();
    }

    // Go: TestCheckMemoryForEncode_AllowsATightFit
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

    // Go: TestCheckMemoryForEncode_DoesNotRefuseWhatItCannotMeasure
    #[test]
    fn an_unreadable_memory_is_not_a_refusal() {
        let video = reported_clip("hevc", "yuv420p10le");
        check(&video, "libx265", MEMORY_BASELINE_THREADS, None).unwrap();
    }

    // Go: TestCheckMemoryForEncode_IgnoresHardwareEncoders
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

    // Go: TestCheckMemoryForEncode_RefusesWhatOnlyManyThreadsCannotHold
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

    // Go: TestParseMemAvailable_ReadsTheKernelEstimate
    #[test]
    fn parse_reads_the_kernel_estimate_in_bytes() {
        let meminfo = b"MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\nBuffers:           12345 kB\n";
        assert_eq!(parse_mem_available(meminfo), Some(3_894_132 * 1024));
    }

    // Go: TestParseMemAvailable_IsNotMemFree
    #[test]
    fn parse_does_not_read_mem_free() {
        let meminfo = b"MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\n";
        assert_ne!(parse_mem_available(meminfo), Some(204_552 * 1024));
    }

    // Go: TestParseMemAvailable_RejectsWhatItCannotRead
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

    // Go: TestAvailableMemoryBytes_MatchesProcMeminfo
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

    // Go: TestFormatAvailableMemory_AgreesWithTheHelper
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

    // Go: TestCgroupLimitsAvailableMemory -- in a container MemAvailable describes the host.
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
}
