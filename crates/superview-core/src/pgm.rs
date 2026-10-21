//! The remap maps: output geometry, the distortion formula, and the PGM writer.
//!
//! Port of Go's `internal/common/pgm.go` (removed in phase 8; last release with it: v0.2.9). Pinned byte for byte by the parity oracle
//! `tests/fixtures/parity/pgm.json` and the four golden fingerprints of
//! `pgm_golden_test.go` (Go, removed; now `tests/pgm_golden.rs`).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, InvalidVideoError};
use crate::video::VideoSpecs;

/// Dimensions of the remapped frame, which are also those of each remap map.
/// Shared by the writer and the space check so the two cannot drift apart.
///
/// # Panics
/// When `video` has no stream (Go indexes `Streams[0]` the same way); callers
/// validate first.
pub fn remap_output_size(video: &VideoSpecs, squeeze: bool) -> (i64, i64) {
    let stream = &video.streams[0];
    let out_x = if squeeze {
        stream.width
    } else {
        // Truncate, then round down to a multiple of 2 (integer division).
        (stream.height as f64 * (16.0 / 9.0)) as i64 / 2 * 2
    };
    (out_x, stream.height)
}

/// Refuses a frame the widening map cannot handle: past 9:8 the curve folds
/// back on itself at both edges. Squeeze mode is never refused.
pub fn check_geometry(video: &VideoSpecs, squeeze: bool) -> Result<(), InvalidVideoError> {
    if squeeze || video.streams.is_empty() {
        return Ok(());
    }
    let (width, height) = (video.streams[0].width, video.streams[0].height);
    let (out_x, _) = remap_output_size(video, squeeze);
    // Go wraps on overflow; saturating keeps the verdict sensible instead.
    if out_x <= width.saturating_mul(2) {
        return Ok(());
    }
    Err(InvalidVideoError {
        reason: format!(
            "the frame is {width}x{height}, taller than 9:8; widening it to 16:9 ({out_x}x{height}) would fold the image back on itself at both edges. Superview converts 4:3 footage; a portrait clip cannot be widened"
        ),
    })
}

fn io_err(what: &'static str) -> impl Fn(io::Error) -> Error {
    move |e| Error::Plain(format!("failed to {what}: {e}"))
}

fn header(out_x: i64, out_y: i64) -> String {
    format!("P5 {out_x} {out_y} 65535\n")
}

/// Bytes the two maps take: two files, one 16-bit sample per pixel, plus a
/// short ASCII header each. 0 when there is no stream. Go wraps on overflow;
/// here a size that does not fit in `i64` saturates to `i64::MAX`, which any
/// free-space check refuses.
pub fn remap_map_bytes(video: &VideoSpecs, squeeze: bool) -> i64 {
    if video.streams.is_empty() {
        return 0;
    }
    let (out_x, out_y) = remap_output_size(video, squeeze);
    let samples = out_x.checked_mul(out_y).and_then(|n| n.checked_mul(2));
    let per_map = samples.and_then(|n| n.checked_add(header(out_x, out_y).len() as i64));
    per_map.and_then(|n| n.checked_mul(2)).unwrap_or(i64::MAX)
}

/// One remap coordinate as the big-endian 16-bit sample PGM P5 mandates,
/// clamped to the sample range (a guard, see `putMapSample` in pgm.go).
fn put_map_sample(dst: &mut [u8], value: i64) {
    let value = value.clamp(0, 65535) as u16;
    dst[..2].copy_from_slice(&value.to_be_bytes());
}

/// Writes both maps to the given writers, header included.
///
/// Mirrors `GeneratePGM` in pgm.go: validation, geometry check, then the X map
/// (one row, row-invariant) repeated for every row, and the Y map (the row
/// index repeated).
pub fn write_maps<WX: Write, WY: Write>(
    video: &VideoSpecs,
    squeeze: bool,
    wx: &mut WX,
    wy: &mut WY,
) -> Result<(), Error> {
    video.validate()?;
    check_geometry(video, squeeze)?;

    let (out_x, out_y) = remap_output_size(video, squeeze);
    let width = video.streams[0].width;
    let head = header(out_x, out_y);

    wx.write_all(head.as_bytes())
        .map_err(io_err("write header to x.pgm"))?;
    wy.write_all(head.as_bytes())
        .map_err(io_err("write header to y.pgm"))?;

    let row_bytes = usize::try_from(out_x)
        .ok()
        .and_then(|n| n.checked_mul(2))
        .filter(|&n| isize::try_from(n).is_ok())
        .ok_or_else(|| Error::Plain(format!("remap map of width {out_x} is too large")))?;

    // The X map is row-invariant: sx, tx and offset depend on x alone.
    let mut buf_x = vec![0u8; row_bytes];
    for x in 0..out_x {
        let sx = x as f64 - (out_x - width) as f64 / 2.0; // x - width diff/2
        let tx = (x as f64 / out_x as f64 - 0.5) * 2.0; // (x/width - 0.5) * 2
        let slot = &mut buf_x[x as usize * 2..];

        if squeeze {
            let inv = 1.0 - tx.abs();

            // Same expression as pgm.go, which keeps it recognisable against
            // the reference implementation (DEC-13). The divisions are in
            // floating point on purpose: integer ones left a seam in the middle
            // of the frame. `powi(2)`, never `powf(2.0)`: math.Pow(x, 2) is
            // x*x, and powf depends on the platform's libm.
            let mut offset = inv * (out_x as f64 / 16.0 * 7.0 / 2.0)
                - ((inv / 16.0) * 7.0).powi(2) * (out_x as f64 / 7.0 * 16.0 / 2.0);
            if tx < 0.0 {
                offset *= -1.0;
            }
            put_map_sample(slot, (sx + offset) as i64);
        } else {
            let mut offset = tx.powi(2) * ((out_x - width) as f64 / 2.0); // tx^2 * width diff/2
            if tx < 0.0 {
                offset *= -1.0;
            }
            put_map_sample(slot, (sx - offset) as i64);
        }
    }

    let mut buf_y = vec![0u8; row_bytes];
    let mut sample = [0u8; 2];
    for y in 0..out_y {
        wx.write_all(&buf_x).map_err(io_err("write x.pgm"))?;

        // A Y row is the row index repeated; encode it once, through the same
        // function as the X map so the byte order lives in one place.
        put_map_sample(&mut sample, y);
        for pair in buf_y.as_chunks_mut::<2>().0 {
            *pair = sample;
        }
        wy.write_all(&buf_y).map_err(io_err("write y.pgm"))?;
    }

    wx.flush().map_err(io_err("flush x.pgm"))?;
    wy.flush().map_err(io_err("flush y.pgm"))?;
    Ok(())
}

/// Both maps in memory, X first.
pub fn generate_pgm_bytes(video: &VideoSpecs, squeeze: bool) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let mut x = Vec::new();
    let mut y = Vec::new();
    write_maps(video, squeeze, &mut x, &mut y)?;
    Ok((x, y))
}

/// Writes `x.pgm` and `y.pgm` into `dir` (which must exist) and returns their
/// paths. The session that owns the directory comes in a later phase.
///
/// On a write error, partial files may remain in `dir`: cleaning them up is the
/// session's job, as in Go. A close or flush failure is reported, like the
/// `Close()` errors of `GeneratePGM`.
pub fn generate_pgm(
    video: &VideoSpecs,
    squeeze: bool,
    dir: &Path,
) -> Result<(PathBuf, PathBuf), Error> {
    // Validate first so a refused video leaves no file behind.
    video.validate()?;
    check_geometry(video, squeeze)?;

    let x_path = dir.join("x.pgm");
    let y_path = dir.join("y.pgm");
    let create = |name: &str, path: &Path| {
        std::fs::File::create(path)
            .map(io::BufWriter::new)
            .map_err(|e| Error::Plain(format!("failed to create temp file {name}: {e}")))
    };
    let mut wx = create("x.pgm", &x_path)?;
    let mut wy = create("y.pgm", &y_path)?;
    write_maps(video, squeeze, &mut wx, &mut wy)?;
    // `Drop` would swallow a final write error: `into_inner` flushes and reports it. No
    // `sync_all`: only the ffmpeg that follows reads these files, and 66-113 MB forced to disk
    // would only slow the start (F-03).
    for (name, writer) in [("x.pgm", wx), ("y.pgm", wy)] {
        writer
            .into_inner()
            .map_err(|e| Error::Plain(format!("failed to flush {name}: {}", e.error())))?;
    }
    Ok((x_path, y_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::VideoStream;

    fn video(width: i64, height: i64) -> VideoSpecs {
        VideoSpecs {
            file: "t.mp4".into(),
            streams: vec![VideoStream {
                codec: "h264".into(),
                width,
                height,
                duration_float: 1.0,
                bitrate_int: 1,
                ..VideoStream::default()
            }],
        }
    }

    #[test]
    fn extreme_sizes_do_not_overflow() {
        let huge = video(i64::MAX, i64::MAX);
        // Debug builds panic on overflow: reaching the asserts is the test.
        assert!(check_geometry(&huge, false).is_ok());
        assert_eq!(remap_map_bytes(&huge, false), i64::MAX);
        assert_eq!(remap_map_bytes(&huge, true), i64::MAX);
        let tall = video(1, i64::MAX);
        assert!(check_geometry(&tall, false).is_err());
        assert_eq!(remap_map_bytes(&tall, true), i64::MAX);
        let mut sink = io::sink();
        let err = write_maps(&huge, true, &mut sink, &mut io::sink());
        assert!(
            err.is_err(),
            "a map that cannot be sized is refused, not allocated"
        );
    }

    // Go: TestCheckGeometry
    #[test]
    fn geometry_table() {
        // (width, height, squeeze, refused)
        let cases = [
            (640, 480, false, false),
            (1440, 1080, false, false),
            (1000, 1000, false, false), // ratio 1, below 9:8
            (800, 900, false, false),   // exactly 9:8, the limit itself
            (2560, 1080, false, false), // wider than 16:9
            (1080, 1920, false, true),
            (1200, 1600, false, true),
            (1080, 1920, true, false),
            (1200, 1600, true, false),
        ];
        for (w, h, squeeze, refused) in cases {
            let result = check_geometry(&video(w, h), squeeze);
            let name = format!("{w}x{h} squeeze={squeeze}");
            if !refused {
                assert!(result.is_ok(), "{name}: refused a geometry the map handles");
                continue;
            }
            // Go also asked GeneratePGM itself to refuse each of these.
            assert!(
                generate_pgm_bytes(&video(w, h), squeeze).is_err(),
                "{name}: the map generator accepted a refused geometry"
            );
            let reason = result.expect_err(&name).reason;
            assert!(
                reason.contains(&format!("{w}x{h}"))
                    && reason.contains("9:8")
                    && reason.contains("fold"),
                "{name}: reason {reason:?} should name the size, the 9:8 limit and the fold"
            );
        }
    }

    // validate() runs before check_geometry(): 0x1440 is a dimension error.
    #[test]
    fn validation_comes_before_the_geometry_check() {
        let zero = video(0, 1440);
        assert!(check_geometry(&zero, false).is_err());
        let wrote = |r: Result<(), Error>| match r.unwrap_err() {
            Error::InvalidVideo(e) => e.reason,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            wrote(write_maps(&zero, false, &mut io::sink(), &mut io::sink())),
            "invalid dimensions: 0x1440"
        );
        let dir = std::env::temp_dir();
        match generate_pgm(&zero, false, &dir).unwrap_err() {
            Error::InvalidVideo(e) => assert_eq!(e.reason, "invalid dimensions: 0x1440"),
            other => panic!("{other:?}"),
        }
        // A valid portrait gets the geometry error instead.
        let portrait = write_maps(&video(1080, 1440), false, &mut io::sink(), &mut io::sink());
        assert!(wrote(portrait).contains("taller than 9:8"));
    }
}
