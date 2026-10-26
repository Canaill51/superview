//! The application log: every `log` record goes to the capped file under the user cache
//! directory. There is no console to write to, and
//! discarding the records would make every failure unreportable, so the README asks users to
//! attach this file to bug reports.
//!
//! The lines are written by `superview_core::logfile::FileLogger`, shared with the OpenFX
//! plugin. If the file cannot be opened the records are discarded and the window says so in the
//! Diagnostic report; startup never waits on logging.

use std::panic::Location;
use std::path::PathBuf;
use std::sync::OnceLock;

use log::LevelFilter;
use superview_core::logfile::{FileLogger, open_log_file, user_cache_dir};

/// The diagnostic log's cap; past this size it restarts empty.
pub const MAX_LOG_FILE_BYTES: i64 = 5 << 20;

static LOGGER: OnceLock<FileLogger> = OnceLock::new();
/// The log line for a panic: where it happened and what it said. `format_line` keeps a line
/// break in the message from forging a record.
fn panic_line(thread: Option<&str>, location: Option<&Location<'_>>, message: &str) -> String {
    let place = location.map_or_else(
        || "<unknown>".to_string(),
        |at| format!("{}:{}:{}", at.file(), at.line(), at.column()),
    );
    format!(
        "A thread panicked: thread={} location={place} panic={message}",
        thread.unwrap_or("<unnamed>")
    )
}

/// Makes a panic leave a line in the log file, then runs the hook that was there (the message on
/// standard error). Worker threads catch their panics and report them; this is for the ones
/// nobody catches, a callback of the window among them, which otherwise vanish with the
/// process: a user attaching the log to a bug report would attach nothing.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload_as_str()
            .unwrap_or("<payload that is not text>");
        let thread = std::thread::current();
        log::error!("{}", panic_line(thread.name(), info.location(), message));
        log::logger().flush();
        previous(info);
    }));
}

/// Opens the log file and routes the `log` records to it at `level`. Returns the file's path, or
/// `None` when it could not be opened (the records are then discarded).
pub fn install(level: LevelFilter) -> Option<PathBuf> {
    let env = |key: &str| std::env::var(key).ok();
    let opened = user_cache_dir(&env)
        .ok_or_else(|| std::io::Error::other("no user cache directory"))
        .and_then(|dir| open_log_file("superview", MAX_LOG_FILE_BYTES, &dir));
    match opened {
        Ok((writer, path)) => {
            let logger = LOGGER.get_or_init(|| FileLogger::new(writer));
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
    fn a_panic_line_names_the_thread_the_place_and_the_message() {
        let here = std::panic::Location::caller();
        let line = panic_line(Some("worker"), Some(here), "index out of bounds");
        assert_eq!(
            line,
            format!(
                "A thread panicked: thread=worker location={}:{}:{} panic=index out of bounds",
                here.file(),
                here.line(),
                here.column()
            )
        );
        assert_eq!(
            panic_line(None, None, "x"),
            "A thread panicked: thread=<unnamed> location=<unknown> panic=x"
        );
    }
}
