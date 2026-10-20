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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Declared first, dropped last: on Windows an open handle keeps the directory from
    // being deleted, so each test drops its writer before the end.
    fn cache() -> TempDir {
        TempDir::new().unwrap()
    }

    // Go: TestParseLogLevel
    #[test]
    fn log_levels_parse_with_info_as_the_fallback() {
        use log::LevelFilter::*;
        let cases = [
            ("debug", Debug),
            ("DEBUG", Debug),
            (" Debug ", Debug),
            ("info", Info),
            ("warn", Warn),
            ("warning", Warn),
            ("error", Error),
            ("ERROR", Error),
            ("", Info),
            ("verbose", Info),
            ("trace", Info),
        ];
        for (input, want) in cases {
            assert_eq!(parse_log_level(input), want, "{input:?}");
        }
    }

    // No Go test: os.UserCacheDir is the standard library's.
    #[test]
    #[cfg(unix)]
    fn user_cache_dir_follows_xdg_then_home() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        let got = user_cache_dir(&env(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")]));
        assert_eq!(got, Some(PathBuf::from("/x")));
        let got = user_cache_dir(&env(&[("HOME", "/h")]));
        assert_eq!(got, Some(PathBuf::from("/h/.cache")));
        let got = user_cache_dir(&env(&[("XDG_CACHE_HOME", ""), ("HOME", "/h")]));
        assert_eq!(got, Some(PathBuf::from("/h/.cache")));
        let relative = env(&[("XDG_CACHE_HOME", "rel"), ("HOME", "/h")]);
        assert_eq!(user_cache_dir(&relative), None);
        assert_eq!(user_cache_dir(&env(&[])), None);
    }

    #[test]
    #[cfg(windows)]
    fn user_cache_dir_is_local_app_data() {
        let env =
            |key: &str| (key == "LocalAppData").then(|| r"C:\Users\u\AppData\Local".to_string());
        assert_eq!(
            user_cache_dir(&env),
            Some(PathBuf::from(r"C:\Users\u\AppData\Local"))
        );
        assert_eq!(user_cache_dir(&|_| None), None);
    }

    // Go: TestOpenLogFile_CreatesAppends
    #[test]
    fn the_file_is_created_and_reopening_appends() {
        let cache = cache();
        let (mut first, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
        first.write_all(b"first\n").unwrap();
        drop(first);
        assert_eq!(path, cache.path().join("superview-test/superview-test.log"));

        let (mut second, path2) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
        second.write_all(b"second\n").unwrap();
        drop(second);
        assert_eq!(path2, path, "the path changed between runs");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");
    }

    // Go: TestOpenLogFile_TruncatesOversized
    #[test]
    fn a_file_over_the_cap_is_emptied_at_open_and_zero_means_no_cap() {
        let cache = cache();
        let (mut writer, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
        writer.write_all(&[b'x'; 4096]).unwrap();
        drop(writer);

        // A cap the file already exceeds.
        drop(
            open_log_file("superview-test", 1024, cache.path())
                .unwrap()
                .0,
        );
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            0,
            "kept bytes past the cap"
        );

        // A cap of 0 (or less) means no cap and must not truncate.
        for no_cap in [0, -1] {
            let (mut writer, _) = open_log_file("superview-test", no_cap, cache.path()).unwrap();
            writer.write_all(&[b'k'; 4096]).unwrap();
            drop(writer);
            drop(
                open_log_file("superview-test", no_cap, cache.path())
                    .unwrap()
                    .0,
            );
        }
        assert_eq!(fs::metadata(&path).unwrap().len(), 8192);
    }

    // Go: TestCappedLogWriter_StaysUnderCap
    #[test]
    fn the_cap_holds_while_writing_and_the_newest_record_survives() {
        let cache = cache();
        const LIMIT: u64 = 1024;
        let (mut writer, path) =
            open_log_file("superview-test", LIMIT as i64, cache.path()).unwrap();
        let record = format!("{}\n", "r".repeat(99));
        let mut truncated = false;
        let mut previous = 0;
        for i in 0..200 {
            writer.write_all(record.as_bytes()).unwrap();
            let size = fs::metadata(&path).unwrap().len();
            assert!(
                size <= LIMIT + record.len() as u64,
                "after {} writes: {size}",
                i + 1
            );
            truncated |= size < previous;
            previous = size;
        }
        assert!(truncated, "the file was never emptied");
        assert!(previous > 0, "the log is empty after the last write");
        assert_eq!(
            fs::read_to_string(&path).unwrap().len() as u64 % record.len() as u64,
            0
        );
    }

    // No Go test: "a single record larger than the cap is still written whole".
    #[test]
    fn a_record_larger_than_the_cap_is_written_whole() {
        let cache = cache();
        let (mut writer, path) = open_log_file("superview-test", 10, cache.path()).unwrap();
        writer.write_all(b"small\n").unwrap();
        writer.write_all(&[b'b'; 100]).unwrap();
        drop(writer);
        assert_eq!(fs::read(&path).unwrap(), vec![b'b'; 100]);
    }

    // No Go test: the line is exact (a write that reaches the cap is kept, one that passes
    // it starts over) and the count starts again after a truncation.
    #[test]
    fn the_cap_is_exact_and_the_count_restarts_after_a_truncation() {
        let cache = cache();
        let (mut writer, path) = open_log_file("superview-test", 10, cache.path()).unwrap();
        let read = || fs::read_to_string(&path).unwrap();
        writer.write_all(b"12345").unwrap();
        writer.write_all(b"67890").unwrap();
        assert_eq!(
            read(),
            "1234567890",
            "a write that reaches the cap was refused"
        );
        writer.write_all(b"a").unwrap();
        assert_eq!(read(), "a", "a write past the cap did not start over");
        writer.write_all(b"bcd").unwrap();
        writer.write_all(b"efg").unwrap();
        assert_eq!(
            read(),
            "abcdefg",
            "the count did not restart after the truncation"
        );
        drop(writer);
    }

    // The shared form: two threads through `&CappedLogWriter` never interleave a record.
    #[test]
    fn a_shared_writer_keeps_records_whole() {
        let cache = cache();
        let (writer, path) = open_log_file("superview-test", 0, cache.path()).unwrap();
        std::thread::scope(|scope| {
            for byte in *b"ab" {
                let writer = &writer;
                scope.spawn(move || {
                    let mut line = vec![byte; 63];
                    line.push(b'\n');
                    for _ in 0..200 {
                        (&*writer).write_all(&line).unwrap();
                    }
                });
            }
        });
        drop(writer);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 400);
        assert!(
            text.lines()
                .all(|l| l.len() == 63 && l.bytes().all(|b| b == l.as_bytes()[0]))
        );
    }

    // Go: TestOpenLogFile_Permissions
    #[cfg(unix)]
    #[test]
    fn the_log_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let cache = cache();
        let (writer, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600, "log file mode");
        assert_eq!(mode(path.parent().unwrap()), 0o700, "log directory mode");
        drop(writer);
    }

    // No Go test: the error names what failed.
    #[test]
    fn an_impossible_directory_is_reported() {
        let cache = cache();
        let blocker = cache.path().join("file");
        fs::write(&blocker, "x").unwrap();
        let err = open_log_file("superview-test", 0, &blocker).err().unwrap();
        assert!(
            err.to_string().starts_with("cannot create log directory"),
            "{err}"
        );
    }
}
