//! What an encode measures about itself, for the interface and the log.
//! Port of the part of Go's `internal/common/metrics.go` (removed in phase 8; last release with it: v0.2.9) the interface reads.
//!
//! Deliberate differences from the Go code:
//! - Removed, as decided for phase 4: `Summary()`, `LogMetrics()` and `computeMetrics`
//!   (encoding speed, compression ratio, bitrate reduction). Nothing in the interface
//!   reads them, and the result panel only needs the two sizes and two durations kept
//!   here. The input's codec, size in pixels, duration, bitrate and frame rate, which fed
//!   only those figures, are not stored either.
//! - No lock and no global: the metrics are a plain value (`&mut self` to record) that
//!   `perform_encoding` returns; a caller sharing one across threads wraps it in a
//!   `Mutex`. Go's `GetLast*`/`SetLast*` have no counterpart.
//! - Times are `Instant`s (monotonic) where Go keeps UTC wall-clock times, and every
//!   recording method has an `_at(now)` form taking the instant, so the remaining-time
//!   estimate is tested without sleeping. The plain form passes `Instant::now()`.
//! - The estimate is stored as it is computed, as in Go; it is *not* cleared by
//!   [`EncodingMetrics::record_completion`], which, like Go's, only forces the
//!   progress to 100.

use std::time::{Duration, Instant};

use crate::video::VideoSpecs;

/// How many percent must elapse after the first progress report before a remaining time is
/// estimated: a rate read over a fraction of a percent is noise, and the first figures were
/// the absurd ones.
pub const MINIMUM_PROGRESS_FOR_ESTIMATE: f64 = 2.0;

/// Performance figures of one encode. Sizes are in bytes, bitrates in bits per second.
#[derive(Debug, Clone)]
pub struct EncodingMetrics {
    pub start: Instant,
    pub end: Option<Instant>,
    pub progress_updates: u32,

    pub input_file: String,
    pub input_file_size: i64,

    pub output_file: String,
    /// 0 until the encode is complete.
    pub output_file_size: i64,
    /// The configured output bitrate, in bits per second.
    pub output_bitrate: i64,
    pub output_codec: String,

    /// Last reported progress, 0 to 100.
    pub last_progress: f64,
    pub estimated_remaining: Duration,
    pub video_check_duration: Duration,
    pub pgm_generation_time: Duration,
    pub encode_duration: Duration,
    pub cleanup_duration: Duration,

    /// Origin of the remaining-time estimate: the first non-zero progress and when it
    /// arrived. `start` is taken before validation, map generation and the wait for the
    /// first frame, so estimating from it inflated the figure to hours at the first percent.
    first_progress: Option<(Instant, f64)>,

    pub ffmpeg_exit_code: i32,
    pub error_message: String,
    pub success: bool,
}

impl EncodingMetrics {
    /// A tracker started now (Go: `NewEncodingMetrics`).
    pub fn new(input_file: &str, output_file: &str) -> Self {
        Self::new_at(input_file, output_file, Instant::now())
    }

    pub fn new_at(input_file: &str, output_file: &str, now: Instant) -> Self {
        Self {
            start: now,
            end: None,
            progress_updates: 0,
            input_file: input_file.into(),
            input_file_size: 0,
            output_file: output_file.into(),
            output_file_size: 0,
            output_bitrate: 0,
            output_codec: String::new(),
            last_progress: 0.0,
            estimated_remaining: Duration::ZERO,
            video_check_duration: Duration::ZERO,
            pgm_generation_time: Duration::ZERO,
            encode_duration: Duration::ZERO,
            cleanup_duration: Duration::ZERO,
            first_progress: None,
            ffmpeg_exit_code: 0,
            error_message: String::new(),
            success: false,
        }
    }

    /// Keeps the input's size once the video is validated; a video without streams
    /// records nothing.
    pub fn record_input_metadata(&mut self, video: &VideoSpecs, file_size: i64) {
        if video.streams.is_empty() {
            return;
        }
        self.input_file_size = file_size;
    }

    /// Keeps the configured output parameters; call before the encode starts.
    pub fn record_output_metadata(&mut self, bitrate: i64, encoder: &str) {
        self.output_bitrate = bitrate;
        self.output_codec = encoder.into();
    }

    /// Stores the per-stage durations for the interface and the log.
    pub fn record_stage_durations(
        &mut self,
        video_check: Duration,
        pgm_generation: Duration,
        encode: Duration,
        cleanup: Duration,
    ) {
        self.video_check_duration = video_check;
        self.pgm_generation_time = pgm_generation;
        self.encode_duration = encode;
        self.cleanup_duration = cleanup;
    }

    pub fn record_progress(&mut self, percent: f64) {
        self.record_progress_at(percent, Instant::now());
    }

    /// Records a progress report (clamped to 0..=100) and updates the remaining-time
    /// estimate.
    ///
    /// The estimate runs from the first progress report, not from `start`: what precedes
    /// it (validation, the maps, an encoder that takes a minute to produce its first
    /// frame) says nothing about how fast frames come once they do. A report lower than the
    /// previous one means the encode restarted -- a fallback to another encoder begins again
    /// at 0% -- so the origin is dropped and taken again.
    pub fn record_progress_at(&mut self, percent: f64, now: Instant) {
        let previous = self.last_progress;
        let current = percent.clamp(0.0, 100.0);
        self.last_progress = current;
        self.progress_updates = self.progress_updates.saturating_add(1);

        if current < previous {
            self.first_progress = None;
            self.estimated_remaining = Duration::ZERO;
        }
        if current > 0.0 && self.first_progress.is_none() {
            self.first_progress = Some((now, current));
        }

        // Nothing is shown until two percent have gone by since the origin.
        if let Some((first_time, first_percent)) = self.first_progress {
            let advanced = current - first_percent;
            if advanced >= MINIMUM_PROGRESS_FOR_ESTIMATE && current < 100.0 {
                let elapsed = now.saturating_duration_since(first_time).as_secs_f64();
                let remaining = elapsed / advanced * (100.0 - current);
                self.estimated_remaining =
                    Duration::try_from_secs_f64(remaining).unwrap_or(Duration::ZERO);
            }
        }
    }

    pub fn record_completion(&mut self, output_file_size: i64) {
        self.record_completion_at(output_file_size, Instant::now());
    }

    /// Marks the encode as successfully finished.
    pub fn record_completion_at(&mut self, output_file_size: i64, now: Instant) {
        self.end = Some(now);
        self.output_file_size = output_file_size;
        self.success = true;
        self.ffmpeg_exit_code = 0;
        self.last_progress = 100.0;
    }

    pub fn record_error(&mut self, exit_code: i32, message: &str) {
        self.record_error_at(exit_code, message, Instant::now());
    }

    /// Marks the encode as failed.
    pub fn record_error_at(&mut self, exit_code: i32, message: &str, now: Instant) {
        self.end = Some(now);
        self.success = false;
        self.ffmpeg_exit_code = exit_code;
        self.error_message = message.into();
    }

    /// Time since the start, or the whole run once it has ended (Go: `ElapsedTime`).
    pub fn elapsed(&self) -> Duration {
        self.elapsed_at(Instant::now())
    }

    pub fn elapsed_at(&self, now: Instant) -> Duration {
        self.end
            .unwrap_or(now)
            .saturating_duration_since(self.start)
    }

    /// The estimate of the time left, or zero when there is not enough data to form one:
    /// callers render that as "no estimate", not as "no time left".
    pub fn remaining(&self) -> Duration {
        self.estimated_remaining
    }
}

#[cfg(test)]
mod tests;
