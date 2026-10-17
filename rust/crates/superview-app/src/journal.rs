//! The application log: every `log` record goes to the capped file under the user cache
//! directory (Go: `common.OpenLogFile` + `slog`). There is no console to write to, and
//! discarding the records would make every failure unreportable, so the README asks users to
//! attach this file to bug reports.
//!
//! Differences from Go: the lines read `time=… level=… msg=…` like slog's text handler, but the
//! message is the whole `key=value` text the core writes. If the file cannot be opened the
//! records are discarded and the window says so in the Diagnostic report; startup never waits on
//! logging.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};
use superview_core::logfile::{CappedLogWriter, open_log_file, user_cache_dir};

/// The diagnostic log's cap; past this size it restarts empty (Go: `maxLogFileBytes`).
pub const MAX_LOG_FILE_BYTES: i64 = 5 << 20;

struct FileLogger {
    writer: CappedLogWriter,
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a number of seconds since the Unix epoch (UTC, proleptic
/// Gregorian; Howard Hinnant's `civil_from_days`).
fn format_utc(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let z = i64::try_from(days).unwrap_or(i64::MAX / 2) + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// One log line, without its trailing newline.
fn format_line(seconds: u64, level: log::Level, message: &str) -> String {
    // A message with a line break would pass for several records: escaped, it stays one.
    let message = message.replace('\n', "\\n").replace('\r', "\\r");
    format!("time={} level={level} msg={message}", format_utc(seconds))
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let line = format_line(seconds, record.level(), &record.args().to_string());
        // A failing log write has nowhere to be reported.
        let _ = writeln!(&self.writer, "{line}");
    }

    fn flush(&self) {
        let _ = (&self.writer).flush();
    }
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

/// Opens the log file and routes the `log` records to it at `level`. Returns the file's path, or
/// `None` when it could not be opened (the records are then discarded).
pub fn install(level: LevelFilter) -> Option<PathBuf> {
    let env = |key: &str| std::env::var(key).ok();
    let opened = user_cache_dir(&env)
        .ok_or_else(|| std::io::Error::other("no user cache directory"))
        .and_then(|dir| open_log_file("superview", MAX_LOG_FILE_BYTES, &dir));
    match opened {
        Ok((writer, path)) => {
            let logger = LOGGER.get_or_init(|| FileLogger { writer });
            if log::set_logger(logger).is_ok() {
                log::set_max_level(level);
                Some(path)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_utc_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(format_utc(1_760_745_599), "2025-10-17T23:59:59Z");
    }

    #[test]
    fn a_line_break_in_a_message_cannot_forge_a_record() {
        let line = format_line(0, log::Level::Info, "a\nlevel=ERROR msg=fake\r\nb");
        assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
        assert!(
            line.ends_with("msg=a\\nlevel=ERROR msg=fake\\r\\nb"),
            "{line}"
        );
    }

    #[test]
    fn a_line_carries_time_level_and_message() {
        assert_eq!(
            format_line(0, log::Level::Warn, "x=1 y=2"),
            "time=1970-01-01T00:00:00Z level=WARN msg=x=1 y=2"
        );
    }
}
