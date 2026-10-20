//! One line of ffmpeg's `-progress` output. Port of the read loop of `EncodeVideo` in
//! Go's `internal/common/encode.go` (removed in phase 8; last release with it: v0.2.9) (the `out_time_ms=` branch).
//!
//! Deliberate differences from the Go code (exhaustive):
//! - The line is a `&str`; Go reads bytes, and a line that is not UTF-8 never reaches here.
//! - The number is read by `parse_go_float` (`video.rs`), the port of `strconv.ParseFloat`:
//!   `1e3`, `+5`, `inf` and `nan` are values as in Go. Not reproduced: hexadecimal floats,
//!   which ffmpeg never prints (see `parse_go_float`).
//! - The logs are `log` records with `key=value` text, where Go uses `slog` attributes, and
//!   the `error` text of an unreadable value is always `invalid syntax`, where Go's is
//!   `strconv.ParseFloat: parsing "...": invalid syntax` (or `value out of range`, for `1e999`);
//!   the classification is the same, a warning and the line skipped.
//! - The log goes through a `&dyn Log` ([`log_progress_line_to`]): the `log` crate has one
//!   global logger per process and the tests of `health.rs` already own it. Production
//!   passes `log::logger()`.

use log::{Level, Log, Record};

use crate::video::parse_go_float;

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
    /// Whether this line shows a frame written: a time above zero (Go's `timeF > 0`).
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
    match parse_go_float(raw) {
        Some(value) => ProgressLine::Value(value),
        None => ProgressLine::Unparsable(raw.to_string()),
    }
}

/// The progress percentage: `min(t / (duration * 10000), 100)`. `out_time_ms` is in
/// microseconds and the duration in seconds, so this is the elapsed share times 100. A NaN
/// stays NaN, as in Go.
pub fn percent(out_time_ms: f64, duration_float: f64) -> f64 {
    let ratio = out_time_ms / (duration_float * 10000.0);
    // Go's `math.Min(NaN, 100)` is NaN; `f64::min` would answer 100 ("done") for a NaN
    // duration (ffprobe can say "nan") or time.
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
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A logger of its own, so the test needs no global one.
    #[derive(Default)]
    struct Capture(Mutex<Vec<(Level, String)>>);

    impl Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &Record) {
            self.0
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
        fn flush(&self) {}
    }

    fn logged(lines: &[&str]) -> Vec<(Level, String)> {
        let capture = Capture::default();
        for line in lines {
            log_progress_line_to(&capture, &parse_progress_line(line));
        }
        capture.0.into_inner().unwrap()
    }

    #[test]
    fn a_line_without_the_key_is_ignored() {
        for line in ["frame=12", "progress=continue", "out_time=00:00:01.0", ""] {
            assert_eq!(parse_progress_line(line), ProgressLine::Ignored, "{line}");
        }
        assert!(logged(&["frame=12", "out_time_us=5"]).is_empty());
    }

    #[test]
    fn a_value_is_read_after_trimming() {
        assert_eq!(
            parse_progress_line("out_time_ms=600000000"),
            ProgressLine::Value(6e8)
        );
        assert_eq!(
            parse_progress_line("out_time_ms= 42 \r"),
            ProgressLine::Value(42.0)
        );
        // ffmpeg prints a negative time before the first timestamp: a value, not output.
        let negative = parse_progress_line("out_time_ms=-9223372036854775807");
        assert!(matches!(negative, ProgressLine::Value(t) if t < 0.0));
        assert!(!negative.produced_output());
    }

    // Go's loop tests Contains, then removes the first occurrence only: what surrounds the key
    // stays, and a prefix makes the value unreadable.
    #[test]
    fn the_key_only_has_to_be_contained_and_the_first_one_goes() {
        assert_eq!(
            parse_progress_line("xx out_time_ms=7"),
            ProgressLine::Unparsable("xx 7".into())
        );
        assert_eq!(
            parse_progress_line("out_time_ms=out_time_ms=7"),
            ProgressLine::Unparsable("out_time_ms=7".into())
        );
        assert_eq!(
            parse_progress_line("7out_time_ms="),
            ProgressLine::Value(7.0)
        );
    }

    // The text is read as strconv.ParseFloat does, not as str::parse does.
    #[test]
    fn the_number_is_read_as_go_reads_it() {
        assert_eq!(
            parse_progress_line("out_time_ms=1e3"),
            ProgressLine::Value(1000.0)
        );
        assert_eq!(
            parse_progress_line("out_time_ms=+5"),
            ProgressLine::Value(5.0)
        );
        assert_eq!(
            parse_progress_line("out_time_ms=1_0"),
            ProgressLine::Value(10.0)
        );
        assert_eq!(
            parse_progress_line("out_time_ms=1e999"),
            ProgressLine::Unparsable("1e999".into())
        );
        assert_eq!(
            parse_progress_line("out_time_ms=-nan"),
            ProgressLine::Unparsable("-nan".into())
        );
    }

    #[test]
    fn not_available_is_not_a_value_whatever_its_case() {
        for line in ["out_time_ms=N/A", "out_time_ms=n/a", "out_time_ms= N/a "] {
            assert_eq!(
                parse_progress_line(line),
                ProgressLine::NotYetAvailable,
                "{line}"
            );
        }
        assert!(!ProgressLine::NotYetAvailable.produced_output());
    }

    #[test]
    fn only_a_time_above_zero_marks_output() {
        assert!(parse_progress_line("out_time_ms=1").produced_output());
        assert!(!parse_progress_line("out_time_ms=0").produced_output());
        assert!(!parse_progress_line("out_time_ms=soon").produced_output());
        assert!(!parse_progress_line("out_time_ms=nan").produced_output());
        assert!(!parse_progress_line("frame=3").produced_output());
    }

    // Go: TestEncodeVideo_ProgressNotAvailableIsNotAWarning, at the level of the line.
    #[test]
    fn not_available_is_debug_never_a_warning() {
        let records = logged(&[
            "out_time_ms=N/A",
            "out_time_ms=N/A",
            "out_time_ms=600000000",
        ]);
        assert_eq!(records.len(), 2, "{records:?}");
        for (level, text) in &records {
            assert_eq!(*level, Level::Debug);
            assert_eq!(text, "ffmpeg has not produced a frame yet");
            assert!(!text.contains("Failed to parse progress value"));
        }
        // Silencing N/A must not silence progress itself.
        assert_eq!(
            parse_progress_line("out_time_ms=600000000"),
            ProgressLine::Value(6e8)
        );
    }

    // Go: TestEncodeVideo_UnparsableProgressIsStillAWarning
    #[test]
    fn an_unparsable_value_still_warns() {
        let records = logged(&["out_time_ms=soon"]);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].0, Level::Warn);
        assert!(
            records[0].1.starts_with("Failed to parse progress value:"),
            "{records:?}"
        );
        assert!(records[0].1.contains("raw_value=soon"), "{records:?}");
        assert_eq!(
            parse_progress_line("out_time_ms=soon"),
            ProgressLine::Unparsable("soon".into())
        );
    }

    #[test]
    fn the_percentage_is_the_elapsed_share_capped_at_100() {
        // 60 s clip, 30 s done: out_time_ms is in microseconds.
        assert_eq!(percent(30_000_000.0, 60.0), 50.0);
        assert_eq!(percent(0.0, 60.0), 0.0);
        assert_eq!(percent(600_000_000.0, 60.0), 100.0);
        // Past the end (ffmpeg overshoots a little): capped, not 101.
        assert_eq!(percent(61_000_000.0 * 2.0, 60.0), 100.0);
        assert_eq!(percent(f64::INFINITY, 60.0), 100.0);
    }

    // Go: math.Min(NaN, 100) is NaN, not 100 ("done").
    #[test]
    fn a_nan_percentage_stays_nan() {
        assert!(percent(f64::NAN, 60.0).is_nan());
        assert!(percent(1.0, f64::NAN).is_nan());
        assert!(percent(0.0, 0.0).is_nan());
    }
}
