use super::*;

/// The text the rows stand for, with a newline after each line: the report again, to the
/// character, when it ended with a newline.
fn recompose(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| {
            let mark = if r.ok { OK_MARK } else { FAIL_MARK };
            let line = match r.kind {
                RowKind::Blank => String::new(),
                RowKind::Section | RowKind::Plain => r.text.clone(),
                RowKind::Check => format!("{mark}{}", r.text),
                RowKind::Value => format!("{VALUE_INDENT}{}", r.text),
                RowKind::Status => format!("{STATUS_PREFIX}{mark}{}", r.text),
            };
            format!("{line}\n")
        })
        .collect()
}

fn kinds(rows: &[Row]) -> Vec<RowKind> {
    rows.iter().map(|r| r.kind).collect()
}

const HEALTHY: &str = "=== System Health Check ===\nStatus: ✅ HEALTHY\n\n\
        ✅ FFMPEG: FFmpeg 7.1 available\n   Value: /usr/bin/ffmpeg\n\
        ✅ DISK: 150.0 GB free\n";

#[test]
fn a_healthy_report_is_all_green() {
    let rows = parse_report(HEALTHY);
    assert_eq!(
        kinds(&rows),
        [
            RowKind::Section,
            RowKind::Status,
            RowKind::Blank,
            RowKind::Check,
            RowKind::Value,
            RowKind::Check
        ]
    );
    assert!(
        rows.iter()
            .filter(|r| r.kind != RowKind::Blank)
            .skip(1)
            .all(|r| { matches!(r.kind, RowKind::Value) || r.ok })
    );
    assert_eq!(rows[1].text, "HEALTHY");
    assert_eq!(rows[3].text, "FFMPEG: FFmpeg 7.1 available");
    assert_eq!(rows[4].text, "Value: /usr/bin/ffmpeg");
}

#[test]
fn a_failure_is_red_and_the_others_stay_green() {
    let rows = parse_report(
        "=== System Health Check ===\nStatus: ❌ UNHEALTHY\n\n\
             ✅ FFMPEG: ok\n❌ DISK: only 0.1 GB free\n",
    );
    assert!(!rows[1].ok);
    assert_eq!(rows[1].text, "UNHEALTHY");
    assert!(rows[3].ok);
    assert!(!rows[4].ok);
    assert_eq!(rows[4].kind, RowKind::Check);
}

#[test]
fn encoder_probes_keep_a_long_reason_whole() {
    let reason = "ffmpeg failed: Cannot load libcuda.so.1 ".repeat(20);
    let text = format!(
        "=== Encoders (probed in 1.2s) ===\n✅ libx264: works (0.3s)\n❌ h264_nvenc: {reason}\n"
    );
    let rows = parse_report(&text);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].kind, RowKind::Section);
    assert!(rows[1].ok && rows[1].text == "libx264: works (0.3s)");
    assert!(!rows[2].ok);
    assert_eq!(rows[2].text, format!("h264_nvenc: {reason}"));
}

// DEC-48: the line the core prints for a driver that only does constant quality is a pass,
// read from the core's own text so the two cannot drift apart.
#[test]
fn a_constant_quality_only_encoder_reads_as_a_pass() {
    use std::time::Duration;
    use superview_core::encode::args::RateControl;
    use superview_core::probe::{EncoderProbe, EncoderProbeReport, describe_encoder_probe};

    let report = EncoderProbeReport {
        results: vec![EncoderProbe {
            encoder: "h264_vaapi".into(),
            usable: true,
            rate_control: RateControl::ConstantQuality,
            took: Duration::from_millis(300),
            ..EncoderProbe::default()
        }],
        took: Duration::from_millis(300),
    };
    let rows = parse_report(&describe_encoder_probe(Some(&report)));
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[1].ok, "{rows:?}");
    assert_eq!(
        rows[1].text,
        "h264_vaapi: works, constant quality only (300ms)"
    );
}

#[test]
fn what_is_not_recognised_stays_a_plain_line() {
    let rows = parse_report(
        "Superview build: dev\n\nLog file: /tmp/x.log\n  Value: two spaces\n⚠ odd\n=== open\n",
    );
    assert_eq!(
        kinds(&rows),
        [
            RowKind::Plain,
            RowKind::Blank,
            RowKind::Plain,
            RowKind::Plain,
            RowKind::Plain,
            RowKind::Plain
        ]
    );
    assert_eq!(rows[0].text, "Superview build: dev");
    assert_eq!(rows[2].text, "Log file: /tmp/x.log");
    // A status with no mark is only text.
    assert_eq!(parse_report("Status: HEALTHY")[0].kind, RowKind::Plain);
}

#[test]
fn an_empty_report_has_no_rows_and_a_bare_message_is_one() {
    assert!(parse_report("").is_empty());
    let rows = parse_report("No health data available");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, RowKind::Plain);
    assert_eq!(rows[0].text, "No health data available");
}

#[test]
fn no_text_is_ever_lost() {
    let samples = [
        HEALTHY.to_string(),
        "Superview build: dev\n\n".to_string() + HEALTHY + "\nLog file: /tmp/a.log\n",
        "=== E ===\n❌ x: line one\nline two of the reason\n✅\n✅ \n❌❌ a\n".to_string(),
        "\n\n\n".to_string(),
        "Status: ✅ \nStatus: ❌ ✅ inner\n   Value: \n   Value: ✅ v\n".to_string(),
        "No health data available\n".to_string(),
        "tab\tand ünïcode ✅ mid ❌ line\n".to_string(),
        "Superview build: dev\r\n\r\n✅ FFMPEG: ok\r\n   Value: v\r\nLog file: x\r\n".to_string(),
    ];
    for sample in &samples {
        let rows = parse_report(sample);
        // The `\r` of a CRLF line end is dropped (it is not text), so the recomposed report
        // has plain `\n` line ends; everything else comes back to the character.
        let again = recompose(&rows);
        assert_eq!(
            again,
            sample.replace("\r\n", "\n"),
            "text lost in {sample:?}"
        );
    }
    // And a CRLF line is classified as its LF twin: no `\r` stays in a row.
    let crlf = parse_report("✅ A: b\r\n   Value: v\r\n=== T ===\r\n");
    assert_eq!(
        kinds(&crlf),
        [RowKind::Check, RowKind::Value, RowKind::Section]
    );
    assert!(crlf.iter().all(|r| !r.text.contains('\r')));
    // Without a final newline the last line still comes back.
    assert_eq!(recompose(&parse_report("a\nb")), "a\nb\n");
}
