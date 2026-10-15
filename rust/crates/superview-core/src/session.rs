//! The per-conversion temporary directory and the remap maps it holds. Port of
//! `internal/common/session.go`.
//!
//! Deliberate differences from the Go code:
//! - The session is a value the caller owns, not a package global behind a mutex. The Go
//!   error "encoding session already active" and "no encoding session active" disappear by
//!   construction: two sessions are two directories, and a session that does not exist
//!   cannot be asked for its paths.
//! - `InitEncodingSession` is [`EncodingSession::new`], `CleanUp` is
//!   [`EncodingSession::close`]. Dropping a session removes its directory too (Go needed
//!   `defer common.CleanUp()`), so an early return or a panic leaves nothing behind.
//!   `close` is only there to report a removal failure.
//! - Go's `TempDirPrefix` is an `os.MkdirTemp` pattern ("superview-*": the last `*` is
//!   replaced by random digits, the rest kept around it). [`tempfile::Builder`] takes a
//!   prefix and a suffix, so the pattern is split at its last `*`; without one, the random
//!   part comes last, as in Go.
//! - `close` reports a failed removal as a [`SessionError`]; Go returns the bare
//!   `os.RemoveAll` error.
//! - A pattern containing a path separator (`/`, and on Windows `\` and `:`) is refused,
//!   as `os.MkdirTemp` does ("pattern contains path separator"): otherwise the directory
//!   would be created outside the temporary directory.
//! - `parse_p5_size` is more permissive than Go's `Sscanf("P5 %d %d")`: it accepts
//!   leading whitespace, a newline between fields, and a `+` sign (`P5 +5 ...`). The
//!   header `generate_pgm` writes is read the same.
//! - `getSessionPaths` is [`EncodingSession::x_path`] and [`EncodingSession::y_path`];
//!   `sessionOutputSize` is [`EncodingSession::output_size`].

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::config::Config;
use crate::error::SessionError;

/// Temporary files of a single encoding job, in a private directory created under the
/// system's temporary directory. The directory and what is in it go with the session.
#[derive(Debug)]
pub struct EncodingSession {
    dir: TempDir,
    pgm_x_path: PathBuf,
    pgm_y_path: PathBuf,
}

impl EncodingSession {
    /// Creates the directory, named from `cfg.temp_dir_prefix`, in `std::env::temp_dir()`.
    pub fn new(cfg: &Config) -> Result<Self, SessionError> {
        Self::new_in(&std::env::temp_dir(), cfg)
    }

    /// [`EncodingSession::new`] with the parent directory chosen (a test reaches the
    /// failure path through it, and the pipeline's injected temporary directory is used here).
    pub fn new_in(parent: &Path, cfg: &Config) -> Result<Self, SessionError> {
        let pattern = cfg.temp_dir_prefix.as_str();
        if pattern
            .chars()
            .any(|c| std::path::is_separator(c) || (cfg!(windows) && c == ':'))
        {
            return Err(SessionError {
                msg: "failed to create temp directory".into(),
                source: Some(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pattern contains path separator",
                ))),
            });
        }
        let (prefix, suffix) = match pattern.rfind('*') {
            Some(star) => (&pattern[..star], &pattern[star + 1..]),
            None => (pattern, ""),
        };
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .suffix(suffix)
            .tempdir_in(parent)
            .map_err(|e| SessionError {
                msg: "failed to create temp directory".into(),
                source: Some(Box::new(e)),
            })?;
        Ok(Self {
            pgm_x_path: dir.path().join("x.pgm"),
            pgm_y_path: dir.path().join("y.pgm"),
            dir,
        })
    }

    /// The session's directory.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// Where the X-coordinate remap map (PGM) goes.
    pub fn x_path(&self) -> &Path {
        &self.pgm_x_path
    }

    /// Where the Y-coordinate remap map (PGM) goes.
    pub fn y_path(&self) -> &Path {
        &self.pgm_y_path
    }

    /// Reads the frame geometry out of the remap map this session generated.
    ///
    /// The maps are the conversion's own statement of what it is producing, so a caller
    /// that has them needs no squeeze flag and cannot disagree with what ffmpeg is about
    /// to be fed. `generate_pgm` writes the header as "P5 <width> <height> 65535\n", which
    /// is the whole of what is read here (at most 64 bytes).
    ///
    /// Not being able to read it is not an error worth propagating: the caller uses the
    /// size to price memory, and no price is better than a wrong one.
    pub fn output_size(&self) -> Option<(i64, i64)> {
        let file = std::fs::File::open(&self.pgm_x_path).ok()?;
        let mut header = Vec::new();
        file.take(64).read_to_end(&mut header).ok()?;
        parse_p5_size(&header)
    }

    /// Removes the directory and reports a failure (Go: `CleanUp`). Dropping the session
    /// removes it too, silently.
    pub fn close(self) -> Result<(), SessionError> {
        self.dir.close().map_err(|e| SessionError {
            msg: "failed to remove temp directory".into(),
            source: Some(Box::new(e)),
        })
    }
}

/// "P5 <width> <height>" at the start of `header`, both positive.
fn parse_p5_size(header: &[u8]) -> Option<(i64, i64)> {
    let text = String::from_utf8_lossy(header);
    let mut fields = text.split_ascii_whitespace();
    if fields.next()? != "P5" {
        return None;
    }
    let width: i64 = fields.next()?.parse().ok()?;
    let height: i64 = fields.next()?.parse().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::pgm::{generate_pgm, remap_output_size};
    use crate::video::{VideoSpecs, VideoStream};

    fn video(width: i64, height: i64) -> VideoSpecs {
        VideoSpecs {
            file: "clip.mp4".into(),
            streams: vec![VideoStream {
                codec: "h264".into(),
                width,
                height,
                duration: "1".into(),
                duration_float: 1.0,
                bitrate: "100000".into(),
                bitrate_int: 100_000,
                ..VideoStream::default()
            }],
        }
    }

    // Go: TestInitEncodingSession_NilConfigUsesDefaults
    #[test]
    fn default_prefix_names_the_directory_under_the_temp_dir() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        let name = session.dir().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("superview-"), "{name}");
        assert!(!name.contains('*'), "{name}");
        // Under the temporary directory, not the working directory.
        assert!(session.dir().starts_with(std::env::temp_dir()));
        assert_ne!(
            session.dir().parent(),
            Some(std::env::current_dir().unwrap().as_path())
        );
        assert!(session.dir().is_dir());
    }

    // Go: TestPipelineHonoursExplicitConfig
    #[test]
    fn supplied_prefix_is_used_and_the_star_is_the_random_part() {
        let cfg = Config {
            temp_dir_prefix: "superview-explicit-*-end".into(),
            ..Config::default()
        };
        let session = EncodingSession::new(&cfg).unwrap();
        let name = session.dir().file_name().unwrap().to_string_lossy();
        assert!(
            name.starts_with("superview-explicit-") && name.ends_with("-end"),
            "{name}"
        );
        assert!(name.len() > "superview-explicit--end".len(), "{name}");

        // No star: the random part comes last.
        let cfg = Config {
            temp_dir_prefix: "plain-".into(),
            ..Config::default()
        };
        let session = EncodingSession::new(&cfg).unwrap();
        let name = session.dir().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("plain-") && name.len() > "plain-".len());
    }

    #[test]
    fn map_paths_are_in_the_session_directory() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        assert_eq!(session.x_path(), session.dir().join("x.pgm"));
        assert_eq!(session.y_path(), session.dir().join("y.pgm"));
    }

    // Go: os.MkdirTemp refuses "pattern contains path separator".
    #[test]
    fn a_prefix_with_a_path_separator_is_refused_and_creates_nothing() {
        let parent = tempfile::tempdir().unwrap();
        let before = std::fs::read_dir(parent.path()).unwrap().count();
        for pattern in ["../x-*", "/var/x-*", "a/b-*"] {
            let cfg = Config {
                temp_dir_prefix: pattern.into(),
                ..Config::default()
            };
            let err = EncodingSession::new_in(parent.path(), &cfg).unwrap_err();
            assert_eq!(err.msg, "failed to create temp directory", "{pattern}");
            assert!(
                err.to_string().ends_with("pattern contains path separator"),
                "{pattern}: {err}"
            );
        }
        // Nothing appeared in the parent, nor beside it.
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), before);
        let sibling = parent.path().parent().unwrap().join("x-");
        assert!(
            std::fs::read_dir(parent.path().parent().unwrap())
                .unwrap()
                .all(|e| !e.unwrap().path().starts_with(&sibling))
        );
    }

    // Go: CleanUp (no dedicated test)
    #[test]
    fn close_removes_the_directory_and_its_content() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        let dir = session.dir().to_path_buf();
        std::fs::write(session.x_path(), b"P5 2 2 65535\n").unwrap();
        session.close().unwrap();
        assert!(!dir.exists());
    }

    // Go: `defer common.CleanUp()` -- here the drop does it.
    #[test]
    fn dropping_the_session_removes_the_directory() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        let dir = session.dir().to_path_buf();
        std::fs::write(session.y_path(), b"x").unwrap();
        drop(session);
        assert!(!dir.exists());
    }

    // The case Go refused with "encoding session already active" is no case here.
    #[test]
    fn two_sessions_coexist_in_two_directories() {
        let a = EncodingSession::new(&Config::default()).unwrap();
        let b = EncodingSession::new(&Config::default()).unwrap();
        assert_ne!(a.dir(), b.dir());
        let a_dir = a.dir().to_path_buf();
        a.close().unwrap();
        assert!(!a_dir.exists());
        assert!(b.dir().is_dir());
    }

    // Go: TestSessionError_UnwrapKeepsCause, TestSessionErrorsAreTyped
    #[test]
    fn creation_failure_is_a_session_error_with_its_cause() {
        let parent = tempfile::tempdir().unwrap();
        let err =
            EncodingSession::new_in(&parent.path().join("absent"), &Config::default()).unwrap_err();
        assert_eq!(err.msg, "failed to create temp directory");
        assert!(err.source().is_some());
        assert!(
            err.to_string()
                .starts_with("session error: failed to create temp directory: ")
        );
    }

    // Go: TestSessionOutputSize_ReadsWhatGeneratePGMWrote
    #[test]
    fn output_size_reads_what_generate_pgm_wrote() {
        let source = video(1440, 1080);
        for squeeze in [false, true] {
            let session = EncodingSession::new(&Config::default()).unwrap();
            generate_pgm(&source, squeeze, session.dir()).unwrap();
            assert_eq!(
                session.output_size(),
                Some(remap_output_size(&source, squeeze)),
                "squeeze={squeeze}"
            );
        }
    }

    // Go: TestSessionOutputSize_SaysSoWithoutASession (no map yet, the nearest case)
    #[test]
    fn output_size_is_none_without_a_map() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        assert_eq!(session.output_size(), None);
    }

    // The rejections of sessionOutputSize that Go's Sscanf and size check give.
    #[test]
    fn output_size_rejects_what_it_cannot_read() {
        let session = EncodingSession::new(&Config::default()).unwrap();
        for (what, content) in [
            ("empty file", &b""[..]),
            ("wrong magic", b"P6 1920 1080 65535\n"),
            ("not numbers", b"P5 wide high 65535\n"),
            ("height missing", b"P5 1920\n"),
            ("zero width", b"P5 0 1080 65535\n"),
            ("negative height", b"P5 1920 -1 65535\n"),
        ] {
            std::fs::write(session.x_path(), content).unwrap();
            assert_eq!(session.output_size(), None, "{what}");
        }
        std::fs::write(session.x_path(), b"P5 1920 1080 65535\nbody").unwrap();
        assert_eq!(session.output_size(), Some((1920, 1080)));
    }
}
