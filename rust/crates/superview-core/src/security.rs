//! Path and encoder validation, mirroring Go's `internal/common/security.go` (removed in phase 8; last release with it: v0.2.9).
//!
//! Paths from the file pickers are untrusted until they pass here. Verdicts
//! and message texts follow Go's; the I/O causes inside a message come from
//! Rust's `std::io::Error`, so only the leading text is comparable.
//!
//! `dunce::canonicalize` stands in for `filepath.EvalSymlinks`: on Windows it
//! drops the `\\?\` prefix `std::fs::canonicalize` would add, which would
//! otherwise never match the protected directories.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::Error;

fn plain(msg: String) -> Error {
    Error::Plain(msg)
}

/// Directories that must never be an output destination, whether or not the
/// process could write to them (root in CI or a container). On Windows they
/// come from the environment (the system drive is not always C:) with the
/// usual literals as a fallback; `env` is the environment lookup.
pub fn sensitive_system_directories(env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    if cfg!(windows) {
        let mut dirs: Vec<String> = Vec::new();
        for key in [
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
        ] {
            if let Some(value) = env(key).filter(|v| !v.trim().is_empty()) {
                dirs.push(
                    clean(Path::new(value.trim()))
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        for fallback in [
            r"C:\Windows",
            r"C:\Program Files",
            r"C:\Program Files (x86)",
            r"C:\ProgramData",
        ] {
            if !dirs.iter().any(|d| path_equal(d, fallback)) {
                dirs.push(fallback.to_string());
            }
        }
        return dirs;
    }
    [
        "/etc", "/bin", "/sbin", "/usr", "/boot", "/root", "/sys", "/proc", "/lib", "/lib64",
        "/var/lib",
    ]
    .iter()
    .map(|d| (*d).to_string())
    .collect()
}

/// [`sensitive_system_directories`] over the real environment.
fn system_directories() -> Vec<String> {
    sensitive_system_directories(&|key| std::env::var(key).ok())
}

/// Compares two paths, case-insensitively on Windows.
pub(crate) fn path_equal(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// `filepath.Clean` for the paths that reach it: `.` components, repeated and
/// trailing separators go. `..` never gets here (see [`has_traversal_component`]).
pub(crate) fn clean(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Whether any path component is exactly `..`, with `/` and `\` both taken as
/// separators on every OS.
///
/// Searching the raw string for ".." rejected ordinary names such as
/// "vacances..2024.mp4" or a directory called "GoPro..raw".
pub fn has_traversal_component(path: &str) -> bool {
    path.replace('\\', "/").split('/').any(|c| c == "..")
}

/// Whether the cleaned path is one of `dirs` or lies under one.
fn is_sensitive_in(clean_path: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|dir| {
        if path_equal(clean_path, dir) {
            return true;
        }
        let prefix = format!("{dir}{MAIN_SEPARATOR}");
        if cfg!(windows) {
            clean_path
                .to_lowercase()
                .starts_with(&prefix.to_lowercase())
        } else {
            clean_path.starts_with(&prefix)
        }
    })
}

/// Whether the cleaned path is, or is under, a protected system directory.
pub fn is_sensitive_system_path(clean_path: &str) -> bool {
    is_sensitive_in(clean_path, &system_directories())
}

/// Validates a file path for reading.
///
/// Refused: an empty path, a `..` component, a relative path, a path that
/// cannot be resolved, a directory, anything but a regular file (a device or
/// FIFO would make ffmpeg block). Symlinks are resolved and it is the *target*
/// that must pass: the user picks the file through a native dialog, so
/// rejecting links (a `~/Videos` pointing at another drive) only produced
/// false positives.
pub fn is_valid_input_path(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let shown = path.display();
    if path.as_os_str().is_empty() {
        return Err(plain("file path cannot be empty".into()));
    }
    if has_traversal_component(&path.to_string_lossy()) {
        return Err(plain(format!("path traversal detected: {shown}")));
    }
    let cleaned = clean(path);
    if !cleaned.is_absolute() {
        return Err(plain(format!("path must be absolute: {shown}")));
    }
    let resolved =
        dunce::canonicalize(&cleaned).map_err(|e| plain(format!("cannot access file: {e}")))?;
    if !resolved.is_absolute() {
        return Err(plain(format!(
            "resolved path is not absolute: {}",
            resolved.display()
        )));
    }
    let meta = fs::metadata(&resolved).map_err(|e| plain(format!("cannot access file: {e}")))?;
    if meta.is_dir() {
        return Err(plain(format!(
            "path is a directory, expected file: {shown}"
        )));
    }
    if !meta.is_file() {
        return Err(plain(format!("not a regular file: {shown}")));
    }
    Ok(())
}

/// Validates a file path for writing: no `..`, absolute, outside the protected
/// system directories (also once the parent's symlinks are resolved), not an
/// existing directory, parent existing and writable. An existing file is fine,
/// it will be overwritten.
pub fn is_valid_output_path(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let shown = path.display();
    if path.as_os_str().is_empty() {
        return Err(plain("output path cannot be empty".into()));
    }
    if has_traversal_component(&path.to_string_lossy()) {
        return Err(plain(format!(
            "path traversal detected in output path: {shown}"
        )));
    }
    let cleaned = clean(path);
    if !cleaned.is_absolute() {
        return Err(plain(format!("output path must be absolute: {shown}")));
    }
    let dirs = system_directories();
    if is_sensitive_in(&cleaned.to_string_lossy(), &dirs) {
        return Err(plain(format!(
            "output path targets a protected system directory: {shown}"
        )));
    }
    // A directory as destination would fail at the final rename, after the
    // whole encode.
    if fs::metadata(&cleaned).is_ok_and(|m| m.is_dir()) {
        return Err(plain(format!("output path is a directory: {shown}")));
    }

    let dir = cleaned.parent().unwrap_or(&cleaned);
    let dir_meta = fs::metadata(dir).map_err(|e| {
        plain(format!(
            "cannot access output directory {}: {e}",
            dir.display()
        ))
    })?;
    if !dir_meta.is_dir() {
        return Err(plain(format!(
            "output parent is not a directory: {}",
            dir.display()
        )));
    }

    // The check above read the path as written: <somewhere>/out -> /usr passed
    // it. Resolve the parent (the file does not exist yet) and check where the
    // file will really land.
    if let (Ok(resolved), Some(name)) = (dunce::canonicalize(dir), cleaned.file_name())
        && is_sensitive_in(&resolved.join(name).to_string_lossy(), &dirs)
    {
        return Err(plain(format!(
            "output path targets a protected system directory: {shown} (resolves to {})",
            resolved.display()
        )));
    }

    probe_writable(dir)
}

/// Creates then removes a uniquely named file in `dir`: two concurrent runs
/// cannot clobber each other and a crash cannot leave a predictable leftover.
fn probe_writable(dir: &Path) -> Result<(), Error> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let mut last = None;
    for _ in 0..16 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let name = format!(
            ".superview-write-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let probe = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(file) => {
                drop(file);
                let _ = fs::remove_file(&probe);
                return Ok(());
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(plain(format!("output directory not writable: {e}"))),
        }
    }
    let cause = last.map_or_else(String::new, |e| e.to_string());
    Err(plain(format!("output directory not writable: {cause}")))
}

/// Checks an encoder name against a comma-separated whitelist, so an arbitrary
/// ffmpeg parameter cannot be smuggled in. The empty name is valid (use the
/// input codec).
pub fn sanitize_encoder_input(encoder: &str, available_encoders: &str) -> Result<String, Error> {
    if encoder.is_empty() {
        return Ok(String::new());
    }
    if available_encoders.split(',').any(|a| a.trim() == encoder) {
        return Ok(encoder.to_string());
    }
    Err(plain(format!("encoder {encoder:?} not in approved list")))
}

#[cfg(test)]
mod tests;
