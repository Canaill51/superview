//! One line of ffmpeg's `-progress` output (the `out_time_ms=` branch).
//!
//! Design notes (exhaustive):
//! - The line is a `&str`; a line that is not UTF-8 never reaches here.
//! - The number is read by `parse_ffprobe_float` (`video.rs`), which follows the rules of Go's
//!   `strconv.ParseFloat` that the fixtures recorded: `1e3`, `+5`, `inf` and `nan` are
//!   values. Hexadecimal floats are not read, ffmpeg never prints them (see
//!   `parse_ffprobe_float`).
//! - The logs are `log` records with `key=value` text, and the `error` text of an unreadable
//!   value is always `invalid syntax` (v0.2.9 logged Go's own `strconv.ParseFloat` message,
//!   or `value out of range` for `1e999`); the classification is the same, a warning and the
//!   line skipped.
//! - The log goes through a `&dyn Log` ([`log_progress_line_to`]): the `log` crate has one
//!   global logger per process and the tests of `health.rs` already own it. Production
//!   passes `log::logger()`.

use log::{Level, Log, Record};

use crate::video::parse_ffprobe_float;

const KEY: &str = "out_time_ms=";

/// What one line of progress says.
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressLine {
    /// No `out_time_ms=` in the line: another key of the progress block.
    Ignored,
    /// `out_time_ms=N/A`: ffmpeg's normal answer until the first frame leaves the filter
    /// graph (45 of them in 81 seconds on a large frame, at WARN, in a log that worked).
    NotYetAvailable,
    /// A time in microseconds, as ffmpeg's `out_time_ms` is despite its name.
    Value(f64),
    /// Neither `N/A` nor a number: ffmpeg's progress format changed under us. Carries the
    /// raw text.
    Unparsable(String),
}

impl ProgressLine {
    /// Whether this line shows a frame written: a time above zero.
    pub fn produced_output(&self) -> bool {
        matches!(self, Self::Value(t) if *t > 0.0)
    }
}

/// Reads one line the way the loop of `EncodeVideo` does: the line has to *contain*
/// `out_time_ms=`, its first occurrence is removed and the rest trimmed.
pub fn parse_progress_line(line: &str) -> ProgressLine {
    if !line.contains(KEY) {
        return ProgressLine::Ignored;
    }
    let raw = line.replacen(KEY, "", 1);
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("N/A") {
        return ProgressLine::NotYetAvailable;
    }
    match parse_ffprobe_float(raw) {
        Some(value) => ProgressLine::Value(value),
        None => ProgressLine::Unparsable(raw.to_string()),
    }
}

/// The progress percentage: `min(t / (duration * 10000), 100)`. `out_time_ms` is in
/// microseconds and the duration in seconds, so this is the elapsed share times 100. A NaN
/// stays NaN.
pub fn percent(out_time_ms: f64, duration_float: f64) -> f64 {
    let ratio = out_time_ms / (duration_float * 10000.0);
    // `f64::min` would answer 100 ("done") for a NaN duration (ffprobe can say "nan") or
    // time.
    if ratio.is_nan() {
        ratio
    } else {
        ratio.min(100.0)
    }
}

/// Logs what the loop of `EncodeVideo` logs for `parsed`, on the process's logger.
pub fn log_progress_line(parsed: &ProgressLine) {
    log_progress_line_to(log::logger(), parsed);
}

/// [`log_progress_line`] on a given logger. Debug for `N/A` (it is the normal state, and
/// the log is the one users attach to a bug report), warn for a value that is not a number.
pub fn log_progress_line_to(logger: &dyn Log, parsed: &ProgressLine) {
    match parsed {
        ProgressLine::NotYetAvailable => logger.log(
            &Record::builder()
                .level(Level::Debug)
                .target(module_path!())
                .args(format_args!("ffmpeg has not produced a frame yet"))
                .build(),
        ),
        ProgressLine::Unparsable(raw) => logger.log(
            &Record::builder()
                .level(Level::Warn)
                .target(module_path!())
                .args(format_args!(
                    "Failed to parse progress value: raw_value={raw} error=invalid syntax"
                ))
                .build(),
        ),
        ProgressLine::Ignored | ProgressLine::Value(_) => {}
    }
}

#[cfg(test)]
mod tests;
