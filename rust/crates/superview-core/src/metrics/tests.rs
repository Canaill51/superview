use super::*;
use crate::video::VideoStream;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn video() -> VideoSpecs {
    VideoSpecs {
        file: "in.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width: 1920,
            height: 1080,
            duration_float: 10.0,
            bitrate_int: 1_000_000,
            ..VideoStream::default()
        }],
    }
}

// Go: TestNewEncodingMetrics
#[test]
fn a_new_tracker_names_its_files() {
    let t0 = Instant::now();
    let m = EncodingMetrics::new_at("in.mp4", "out.mp4", t0);
    assert_eq!(
        (m.input_file.as_str(), m.output_file.as_str()),
        ("in.mp4", "out.mp4")
    );
    assert_eq!(m.start, t0);
    assert!(m.end.is_none() && !m.success);
}

// Go: TestEncodingMetrics_RecordAndCompute (the recorded values; the figures computed
// from them are not ported)
#[test]
fn recording_keeps_what_the_interface_reads() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("in.mp4", "out.mp4", t0);
    m.record_input_metadata(&video(), 2_000_000);
    m.record_output_metadata(500_000, "libx264");
    m.record_progress_at(50.0, t0 + secs(1));
    m.record_stage_durations(secs(1), secs(2), secs(3), secs(4));
    m.record_completion_at(1_000_000, t0 + secs(10));
    assert!(m.success);
    assert_eq!(
        (m.input_file_size, m.output_file_size),
        (2_000_000, 1_000_000)
    );
    assert_eq!(
        (m.output_bitrate, m.output_codec.as_str()),
        (500_000, "libx264")
    );
    assert_eq!(m.last_progress, 100.0);
    assert_eq!(m.encode_duration, secs(3));
    assert_eq!(
        (
            m.video_check_duration,
            m.pgm_generation_time,
            m.cleanup_duration
        ),
        (secs(1), secs(2), secs(4))
    );
}

// No Go test: RecordInputMetadata ignores a video with no stream.
#[test]
fn a_video_without_streams_records_no_input_size() {
    let mut m = EncodingMetrics::new("in.mp4", "out.mp4");
    m.record_input_metadata(
        &VideoSpecs {
            file: "x".into(),
            streams: vec![],
        },
        99,
    );
    assert_eq!(m.input_file_size, 0);
}

// Go: TestEncodingMetrics_RecordProgressClamp
#[test]
fn progress_is_clamped_to_0_and_100() {
    let mut m = EncodingMetrics::new("in.mp4", "out.mp4");
    m.record_progress(150.0);
    assert_eq!(m.last_progress, 100.0);
    m.record_progress(-10.0);
    assert_eq!(m.last_progress, 0.0);
    assert_eq!(m.progress_updates, 2);
}

// Go: TestEncodingMetrics_RecordErrorAndSummary (the error part)
#[test]
fn an_error_is_recorded_with_its_exit_code() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("in.mp4", "out.mp4", t0);
    m.record_error_at(234, "ffmpeg failed", t0 + secs(2));
    assert!(!m.success);
    assert_eq!(
        (m.ffmpeg_exit_code, m.error_message.as_str()),
        (234, "ffmpeg failed")
    );
    assert_eq!(
        m.elapsed_at(t0 + secs(60)),
        secs(2),
        "a finished run stops counting"
    );
}

#[test]
fn elapsed_runs_until_the_end() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("a", "b", t0);
    assert_eq!(m.elapsed_at(t0 + secs(5)), secs(5));
    m.record_completion_at(1, t0 + secs(7));
    assert_eq!(m.elapsed_at(t0 + secs(100)), secs(7));
}

// Go: TestEncodingMetrics_EstimateStartsAtFirstProgress -- from the first report, not the
// start.
#[test]
fn the_estimate_waits_for_two_percent_and_runs_from_the_first_report() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("a", "b", t0);
    // A minute of validation, then the first frame at 10%.
    m.record_progress_at(10.0, t0 + secs(60));
    assert_eq!(m.remaining(), Duration::ZERO);
    // 1.9 points since the origin: still too little.
    m.record_progress_at(11.9, t0 + secs(61));
    assert_eq!(m.remaining(), Duration::ZERO);
    // 2 points in 10 s: 5 s per point, 88 points left.
    m.record_progress_at(12.0, t0 + secs(70));
    assert_eq!(m.remaining(), secs(440));
    // 10 points in 20 s, 2 s per point, 80 points left -- not read from the start.
    m.record_progress_at(20.0, t0 + secs(80));
    assert_eq!(m.remaining(), secs(160));
}

// Go: TestEncodingMetrics_EstimateRestartsWithTheEncode
#[test]
fn a_progress_that_goes_back_drops_the_estimate_and_its_origin() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("a", "b", t0);
    m.record_progress_at(1.0, t0);
    m.record_progress_at(50.0, t0 + secs(49));
    assert_eq!(m.remaining(), secs(50));
    // The fallback encoder starts again at 0%.
    m.record_progress_at(0.0, t0 + secs(60));
    assert_eq!(m.remaining(), Duration::ZERO);
    // A new origin at 5%: 1 point in 30 s is not enough...
    m.record_progress_at(5.0, t0 + secs(100));
    m.record_progress_at(6.0, t0 + secs(130));
    assert_eq!(m.remaining(), Duration::ZERO);
    // ...and 7% is 2 points in 40 s, 20 s a point, 93 left.
    m.record_progress_at(7.0, t0 + secs(140));
    assert_eq!(m.remaining(), secs(1860));
}

#[test]
fn a_report_of_100_percent_computes_no_new_estimate() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("a", "b", t0);
    m.record_progress_at(1.0, t0);
    m.record_progress_at(50.0, t0 + secs(49));
    assert_eq!(m.remaining(), secs(50));
    m.record_progress_at(100.0, t0 + secs(99));
    // A fresh estimate would be zero; the previous one stays, as in Go (the interface
    // reads 100% and shows the result panel, not a countdown).
    assert_eq!(m.last_progress, 100.0);
    assert_eq!(m.remaining(), secs(50));
}

// A first report of 0% is not an origin: the clock starts at the first real progress.
#[test]
fn a_zero_percent_report_is_not_an_origin() {
    let t0 = Instant::now();
    let mut m = EncodingMetrics::new_at("a", "b", t0);
    m.record_progress_at(0.0, t0 + secs(30));
    m.record_progress_at(2.0, t0 + secs(40));
    assert_eq!(
        m.remaining(),
        Duration::ZERO,
        "2 points from a 2% origin is none"
    );
    m.record_progress_at(4.0, t0 + secs(50));
    assert_eq!(m.remaining(), secs(480));
}
