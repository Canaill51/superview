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
mod tests;
