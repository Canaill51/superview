//! The Diagnostic report, read line by line so the window can draw an icon where the core wrote
//! an emoji. Pure: no Slint here. The core's text (`get_health_report`, `describe_encoder_probe`)
//! stays as it is; this reads it and never loses a character of it.

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
mod tests;
