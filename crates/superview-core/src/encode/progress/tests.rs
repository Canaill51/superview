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

// The loop tests Contains, then removes the first occurrence only: what surrounds the key
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
fn the_number_is_read_by_the_recorded_rules() {
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

// At the level of the line: progress not available is not a warning.
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

// An unparsable progress value is still a warning.
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

// A NaN ratio stays NaN; `f64::min` would give 100 ("done").
#[test]
fn a_nan_percentage_stays_nan() {
    assert!(percent(f64::NAN, 60.0).is_nan());
    assert!(percent(1.0, f64::NAN).is_nan());
    assert!(percent(0.0, 0.0).is_nan());
}
