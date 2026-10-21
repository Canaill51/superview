//! One picture of the input video, for the window's file card.
//!
//! ffmpeg decodes a single frame into a raw RGBA file in a private temporary directory, which is
//! read back and removed with the directory when the function returns, whatever way it ends. Raw
//! `rgba` rather than PNG: the window needs pixels, and nothing here has to decode an image.
//! The input is read as MP4 (DEC-52), like every other ffmpeg and ffprobe run on it, and the run is
//! bounded by a deadline and the cancel flag ([`run_with_deadline`]), so a stuck decoder or a file
//! the user has replaced cannot hold the window.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::error::Error;
use crate::process::{RunEnd, run_with_deadline};
use crate::tools::ToolResolver;

/// The picture's size: the 4:3 frame the card shows. Another shape is fitted inside it, with bars.
pub const THUMBNAIL_WIDTH: u32 = 192;
pub const THUMBNAIL_HEIGHT: u32 = 144;

/// What one frame may take to decode. A frame is milliseconds; this is for a file on a stalled
/// network share or a damaged index.
const DEADLINE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, row by row, red first.
    pub rgba: Vec<u8>,
}

/// The ffmpeg arguments that write the frame at `seconds` of `file` to `out`.
pub fn thumbnail_args(file: &str, seconds: f64, out: &Path) -> Vec<String> {
    let fit = format!(
        "scale={THUMBNAIL_WIDTH}:{THUMBNAIL_HEIGHT}:force_original_aspect_ratio=decrease,\
         pad={THUMBNAIL_WIDTH}:{THUMBNAIL_HEIGHT}:(ow-iw)/2:(oh-ih)/2"
    );
    [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        // DEC-52: the source is read as MP4, never as what its content looks like.
        "-f",
        "mov",
        "-ss",
        &format!("{:.3}", seconds.max(0.0)),
        "-i",
        file,
        "-frames:v",
        "1",
        "-vf",
        &fit,
        "-pix_fmt",
        "rgba",
        "-f",
        "rawvideo",
        // `file:` so that a directory name with a colon is a path, not a protocol.
        &format!("file:{}", out.to_string_lossy()),
    ]
    .map(String::from)
    .to_vec()
}

/// Decodes the frame at `seconds` of `file` (a path already validated). `Error::Cancelled` when
/// `cancel` is raised; any other failure, a timeout included, is a plain error the caller may
/// ignore: the card then shows no picture.
pub fn extract_thumbnail(
    resolver: &ToolResolver,
    file: &str,
    seconds: f64,
    cancel: &AtomicBool,
) -> Result<Thumbnail, Error> {
    let dir = tempfile::Builder::new()
        .prefix("superview-thumbnail-")
        .tempdir()
        .map_err(|e| Error::Plain(format!("cannot create the thumbnail directory: {e}")))?;
    let out = dir.path().join("frame.rgba");
    let cmd = resolver.ffmpeg_command(thumbnail_args(file, seconds, &out));
    let done = run_with_deadline(cmd, DEADLINE, Some(cancel))
        .map_err(|e| Error::Plain(format!("cannot run ffmpeg for the thumbnail: {e}")))?;
    match done.end {
        RunEnd::Cancelled => return Err(Error::Cancelled),
        RunEnd::TimedOut => {
            return Err(Error::Plain(
                "ffmpeg took too long to decode a frame".into(),
            ));
        }
        RunEnd::Finished => {}
    }
    if !done.status.is_some_and(|status| status.success()) {
        let last = done.output.lines().last().unwrap_or("no message");
        return Err(Error::Plain(format!(
            "ffmpeg could not decode a frame: {last}"
        )));
    }
    let rgba = std::fs::read(&out)
        .map_err(|e| Error::Plain(format!("cannot read the decoded frame: {e}")))?;
    let expected = THUMBNAIL_WIDTH as usize * THUMBNAIL_HEIGHT as usize * 4;
    if rgba.len() != expected {
        // A `-ss` past the end of the file decodes nothing and still exits cleanly.
        return Err(Error::Plain(format!(
            "ffmpeg decoded no frame (read {} bytes, expected {expected})",
            rgba.len()
        )));
    }
    Ok(Thumbnail {
        width: THUMBNAIL_WIDTH,
        height: THUMBNAIL_HEIGHT,
        rgba,
    })
}

#[cfg(test)]
mod tests;
