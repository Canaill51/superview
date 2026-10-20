//! The application log file: where it lives, how it is capped, and the level it logs at.
//! Port of `OpenLogFile`, `cappedLogWriter` and `ParseLogLevel` (Go's `observability.go` (removed in phase 8; last release with it: v0.2.9)).
//!
//! Deliberate differences from the Go code:
//! - The user cache directory is a parameter of [`open_log_file`] (Go calls
//!   `os.UserCacheDir`), so no test touches the process environment (L-114);
//!   [`user_cache_dir`] is the lookup, over an injected environment like
//!   `config::user_config_dir`. It follows Go's Windows and Unix rules; Go's macOS
//!   branch (`~/Library/Caches`) is not ported, the application ships for Windows and
//!   Linux only.
//! - Rust refuses `append` together with `truncate`, so the writer empties its file
//!   through a short-lived write-and-truncate handle and then reopens it in append
//!   mode. Go's single `O_TRUNC|O_APPEND` open does both at once. What the Go comment
//!   teaches is kept: the truncation goes through a *fresh* handle, never `set_len` on
//!   the append handle, which Windows opens without `FILE_WRITE_DATA` and so refuses
//!   to truncate (the cap silently never applied there).
//! - Go empties and reopens in one `O_TRUNC|O_APPEND` open: if it fails, nothing is emptied.
//!   Here the truncation and the reopening are two steps; if only the second fails (no
//!   descriptor left), the file is empty, the size count restarts at 0 and the old append
//!   handle goes on writing at the end. Not tested: making the reopening fail without
//!   exhausting the process's descriptors, which would reach the parallel tests.
//! - [`CappedLogWriter`] is closed by `Drop`; Go's `Close` has no counterpart.
//! - `parse_log_level` returns a `log::LevelFilter`, the type the `log` facade filters
//!   on. `"warning"` is an alias of `"warn"`, as in Go; `"trace"` is not a level here,
//!   and falls back to info like any unknown word.
//! - A write that fails part-way is not counted in the size (`write_all` does not say
//!   how much went through); the next record at worst triggers the truncation a little
//!   late.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// The per-user cache directory, as Go's `os.UserCacheDir` finds it on Windows
/// (`%LocalAppData%`) and Unix (`$XDG_CACHE_HOME`, else `$HOME/.cache`). `env` is the
/// environment lookup. `None` when it cannot be worked out.
pub fn user_cache_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let get = |key: &str| env(key).filter(|v| !v.is_empty());
    if cfg!(windows) {
        return get("LocalAppData").map(PathBuf::from);
    }
    if let Some(xdg) = get("XDG_CACHE_HOME") {
        // A relative XDG_CACHE_HOME is invalid per the spec; Go refuses it.
        return Path::new(&xdg).is_absolute().then(|| PathBuf::from(xdg));
    }
    get("HOME").map(|home| Path::new(&home).join(".cache"))
}

/// Creates a directory (and its parents) readable by its owner only on Unix.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// The log file, created if needed (mode 0600 on Unix: it carries home-directory paths),
/// opened for appending.
fn open_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Opens (creating it if needed) `<cache_dir>/<app_name>/<app_name>.log` and returns the
/// writer together with the file's path.
///
/// Diagnostics used to go nowhere, which left nothing to look at when a conversion failed
/// on a user's machine. The file is capped: past `max_bytes` it is truncated, at open and
/// then while it is written (see [`CappedLogWriter`]), so it cannot grow without bound in a
/// session that stays open for days. `max_bytes` of 0 or less means no cap. The directory
/// is 0700 and the file 0600 on Unix.
///
/// On any failure the caller should fall back to a discarding logger rather than refuse to
/// start.
pub fn open_log_file(
    app_name: &str,
    max_bytes: i64,
    cache_dir: &Path,
) -> io::Result<(CappedLogWriter, PathBuf)> {
    let dir = cache_dir.join(app_name);
    create_private_dir(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot create log directory {}: {e}", dir.display()),
        )
    })?;
    let path = dir.join(format!("{app_name}.log"));

    // Simple size cap: start over rather than carry an unbounded file around.
    if let Ok(meta) = fs::metadata(&path)
        && max_bytes > 0
        && i64::try_from(meta.len()).unwrap_or(i64::MAX) > max_bytes
        && let Err(e) = truncate(&path)
    {
        log::warn!(
            "Could not truncate oversized log file: path={} error={e}",
            path.display()
        );
    }

    let file = open_append(&path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot open log file {}: {e}", path.display()),
        )
    })?;
    let size = file
        .metadata()
        .map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX));
    let writer = CappedLogWriter {
        state: Mutex::new(State { file, size }),
        path: path.clone(),
        max: max_bytes,
    };
    Ok((writer, path))
}

/// Empties an existing file through a handle of its own.
fn truncate(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map(drop)
}

struct State {
    file: File,
    /// Bytes in the file as far as this writer knows.
    size: i64,
}

/// Keeps a log file under a size cap while it is being written.
///
/// The cap used to be applied once, at open, so a session that stayed open grew the file
/// without limit. Here, a write that would take the file past the cap empties it first and
/// starts over: it is a diagnostic log, not an archive, and the file never holds more than
/// the cap (a single record larger than it is still written whole). Shareable: the lock is
/// inside, and `&CappedLogWriter` is a [`Write`] too.
pub struct CappedLogWriter {
    state: Mutex<State>,
    path: PathBuf,
    /// 0 or less: no cap.
    max: i64,
}

impl CappedLogWriter {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic elsewhere while logging must not silence the log for good.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write_record(&self, record: &[u8]) -> io::Result<()> {
        let mut state = self.lock();
        let len = i64::try_from(record.len()).unwrap_or(i64::MAX);
        if self.max > 0 && state.size.saturating_add(len) > self.max {
            // Truncate through a fresh handle, not the open append handle: see the
            // header. If the reopening fails, the old append handle keeps writing, at
            // the end of the now empty file.
            if truncate(&self.path).is_ok() {
                state.size = 0;
                if let Ok(fresh) = open_append(&self.path) {
                    state.file = fresh;
                }
            }
        }
        state.file.write_all(record)?;
        state.size = state.size.saturating_add(len);
        Ok(())
    }
}

impl Write for &CappedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_record(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.lock().file.flush()
    }
}

impl Write for CappedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self).flush()
    }
}

/// Maps a configuration string to a log level; unknown values, the empty string included,
/// fall back to info: a typo in `superview.yaml` must not silence the log.
pub fn parse_log_level(level: &str) -> log::LevelFilter {
    match level.trim().to_lowercase().as_str() {
        "debug" => log::LevelFilter::Debug,
        "warn" | "warning" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    }
}

#[cfg(test)]
mod tests;
