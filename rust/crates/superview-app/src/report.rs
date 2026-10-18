//! The Diagnostic report, read line by line so the window can draw an icon where the core wrote
//! an emoji. Pure: no Slint here. The core's text (`get_health_report`, `describe_encoder_probe`)
//! is the Go one and stays as it is; this reads it and never loses a character of it.

/// What one line of the report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Nothing on the line.
    Blank,
    /// `=== Title ===`.
    Section,
    /// `✅ NAME: message` or `❌ NAME: message`.
    Check,
    /// `   Value: detail`, under a check.
    Value,
    /// `Status: ✅ HEALTHY` / `Status: ❌ UNHEALTHY`.
    Status,
    /// Anything else, shown as it is.
    Plain,
}

/// One line. For a `Check` and a `Status`, `ok` is the verdict and the mark is not in `text`;
/// for a `Value`, the indentation is not in `text`; any other text is the line untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub ok: bool,
    pub text: String,
}

const OK_MARK: &str = "✅ ";
const FAIL_MARK: &str = "❌ ";
const STATUS_PREFIX: &str = "Status: ";
const VALUE_INDENT: &str = "   ";

fn row(kind: RowKind, ok: bool, text: &str) -> Row {
    Row {
        kind,
        ok,
        text: text.to_string(),
    }
}

/// Splits `mark` (an emoji and its space) off the front of `text`: `Some(verdict, rest)`.
fn take_mark(text: &str) -> Option<(bool, &str)> {
    if let Some(rest) = text.strip_prefix(OK_MARK) {
        Some((true, rest))
    } else {
        text.strip_prefix(FAIL_MARK).map(|rest| (false, rest))
    }
}

fn parse_line(line: &str) -> Row {
    // A report with Windows line ends: the `\r` is not part of the line.
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return row(RowKind::Blank, false, "");
    }
    if line.starts_with("=== ") && line.ends_with(" ===") {
        return row(RowKind::Section, false, line);
    }
    if let Some((ok, rest)) = line.strip_prefix(STATUS_PREFIX).and_then(take_mark) {
        return row(RowKind::Status, ok, rest);
    }
    if let Some((ok, rest)) = take_mark(line) {
        return row(RowKind::Check, ok, rest);
    }
    if let Some(rest) = line
        .strip_prefix(VALUE_INDENT)
        .filter(|rest| rest.starts_with("Value: "))
    {
        return row(RowKind::Value, false, rest);
    }
    row(RowKind::Plain, false, line)
}

/// The report, one row per line. A line that is not recognised is a `Plain` row with its text
/// whole: whatever the core writes, it is shown.
pub fn parse_report(report: &str) -> Vec<Row> {
    report.split_terminator('\n').map(parse_line).collect()
}

#[cfg(test)]
mod tests {
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
            "Superview build: dev\r\n\r\n✅ FFMPEG: ok\r\n   Value: v\r\nLog file: x\r\n"
                .to_string(),
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
}
