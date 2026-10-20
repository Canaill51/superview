//! Free space of the temporary filesystem, and the check that the remap maps fit in it.
//! Port of `health_disk_unix.go`, `health_disk_windows.go` and `checkTempSpaceForMaps`
//! (`pgm.go`), all in Go's `internal/common/` (removed in phase 8; last release with it: v0.2.9).
//!
//! The system call comes from the `fs4` crate. `rustix` alone would give a safe `statvfs`
//! on Unix, but Windows has no safe equivalent short of calling `GetDiskFreeSpaceExW`
//! through `windows-sys`, which is `unsafe`. `fs4` wraps both and adds no crate to the
//! tree (it sits on the `rustix` and `windows-sys` that `tempfile` already pulls in), so
//! this crate stays free of `unsafe`, with no `sysinfo`-sized dependency for one number.
//!
//! Deliberate differences from the Go code:
//! - The free space is read in bytes by `fs4` and converted to GB here. On Unix `fs4`
//!   multiplies the available blocks by `f_frsize` (the POSIX fragment size) where Go
//!   multiplies by `Bsize`; they are the same on the filesystems that matter, and
//!   `f_frsize` is the one the blocks are counted in. On Windows `fs4` asks for the
//!   free bytes available to the calling user (quotas honoured), like Go.
//! - On Windows Go calls `GetDiskFreeSpaceEx` on the path itself, which fails for a
//!   directory that does not exist; `fs4` first reduces the path to its volume root
//!   (`GetVolumePathNameW`) and measures that, so a missing directory still reads. No
//!   consequence for `temp_dir`, which exists.
//! - `check_temp_space_for_maps` receives the directory and the reading as parameters
//!   (Go reads `os.TempDir()` and a variable swapped by tests); `free_disk_gb` is the
//!   real reading.
//! - The headroom sum saturates: `remap_map_bytes` itself saturates at `i64::MAX`.

use std::io;
use std::path::Path;

use crate::error::SessionError;
use crate::pgm::remap_map_bytes;
use crate::video::VideoSpecs;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: i64 = 1024 * 1024;

/// Gigabytes the calling user may still write on the filesystem holding `path`: the
/// non-privileged free space on Unix (`Bavail`, not `Bfree`) and the free bytes available
/// to the user on Windows (`GetDiskFreeSpaceExW`, which honours per-user quotas).
pub fn free_disk_gb(path: &Path) -> io::Result<f64> {
    fs4::available_space(path).map(|bytes| bytes as f64 / GIB)
}

/// Refuses to start when the temporary filesystem cannot hold the remap maps.
///
/// A 12 Mpx GoPro frame needs 66 MB of maps, and on most current distributions the
/// temporary directory is a tmpfs -- which is to say RAM. Without this the shortfall
/// surfaced halfway through as an opaque write error from a stage the user has no reason
/// to know exists.
///
/// A failed probe is not a refusal: if the free space cannot be read, the encode goes
/// ahead and fails later at worst, exactly as it did before.
pub fn check_temp_space_for_maps(
    video: &VideoSpecs,
    squeeze: bool,
    temp_dir: &Path,
    free_gb: impl Fn(&Path) -> io::Result<f64>,
) -> Result<(), SessionError> {
    let required = remap_map_bytes(video, squeeze);
    if required <= 0 {
        return Ok(());
    }

    let free_gb = match free_gb(temp_dir) {
        Ok(gb) => gb,
        Err(e) => {
            log::warn!(
                "Could not check free space before encoding: path={} error={e}",
                temp_dir.display()
            );
            return Ok(());
        }
    };

    let free = (free_gb * GIB) as i64;
    // A fifth of headroom: the maps are not the only thing written there while ffmpeg
    // runs, and landing exactly at the limit is its own kind of failure.
    let needed = required.saturating_add(required / 5);
    if free >= needed {
        return Ok(());
    }

    Err(SessionError {
        msg: format!(
            "not enough space in {}: the remap maps need {} MB ({} MB with headroom) but only {} MB is free. On many systems this directory lives in RAM; set TMPDIR to a location with more room",
            temp_dir.display(),
            required / MIB,
            needed / MIB,
            free / MIB
        ),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::VideoStream;

    fn video(width: i64, height: i64) -> VideoSpecs {
        VideoSpecs {
            file: "big.mp4".into(),
            streams: vec![VideoStream {
                codec: "h264".into(),
                width,
                height,
                duration: "60".into(),
                duration_float: 60.0,
                bitrate: "5000000".into(),
                bitrate_int: 5_000_000,
                ..VideoStream::default()
            }],
        }
    }

    // Go: TestCheckTempSpaceForMaps_PassesWhenThereIsRoom (on the real filesystem)
    #[test]
    fn passes_when_there_is_room() {
        let dir = std::env::temp_dir();
        check_temp_space_for_maps(&video(640, 480), false, &dir, free_disk_gb)
            .expect("a 640x480 job was refused for lack of space");
    }

    #[test]
    fn the_real_reading_is_a_positive_figure() {
        assert!(free_disk_gb(&std::env::temp_dir()).unwrap() > 0.0);
    }

    // Unix only: Windows measures the volume root of any well-formed path (see the header).
    #[cfg(unix)]
    #[test]
    fn the_real_reading_fails_on_a_missing_path() {
        let missing = std::env::temp_dir().join("superview-no-such-dir/none");
        assert!(free_disk_gb(&missing).is_err());
    }

    // Go: TestCheckTempSpaceForMaps_RefusesWhenTmpIsFull
    #[test]
    fn refuses_when_tmp_is_full_and_names_tmpdir() {
        let dir = Path::new("/tmp/x");
        // About 1 MB free, for maps of 7 MB (9 MB with headroom).
        let err = check_temp_space_for_maps(&video(1440, 1080), false, dir, |_| Ok(0.001))
            .expect_err("maps that cannot fit in the temporary directory were not refused");
        assert!(err.msg.contains("TMPDIR"), "{err}");
        assert_eq!(
            err.msg,
            "not enough space in /tmp/x: the remap maps need 7 MB (9 MB with headroom) but only 1 MB is free. On many systems this directory lives in RAM; set TMPDIR to a location with more room"
        );
    }

    #[test]
    fn an_unreadable_free_space_is_not_a_refusal() {
        let reading = |_: &Path| Err(io::Error::other("statfs: operation not supported"));
        check_temp_space_for_maps(&video(1440, 1080), false, Path::new("/tmp"), reading)
            .expect("an unreadable free space refused the encode");
    }

    // The line is the maps plus a fifth, and the comparison is `>=`.
    #[test]
    fn the_margin_is_a_fifth_of_the_maps() {
        let source = video(1440, 1080);
        let required = remap_map_bytes(&source, false);
        let at = |bytes: i64| move |_: &Path| Ok(bytes as f64 / GIB);
        let dir = Path::new("/tmp");
        // The line itself is exact in f64 (GIB is a power of two), and `>=` lets it through.
        let needed = required + required / 5;
        check_temp_space_for_maps(&source, false, dir, at(needed)).expect("exactly the line");
        check_temp_space_for_maps(&source, false, dir, at(needed - 1))
            .expect_err("one byte under the line");
        // 22% above the maps clears a fifth of headroom (and not a quarter); 18% does not.
        check_temp_space_for_maps(&source, false, dir, at(required + required * 22 / 100))
            .expect("the maps and a fifth fit");
        check_temp_space_for_maps(&source, false, dir, at(required + required * 18 / 100))
            .expect_err("the maps alone fit but not with their headroom");
    }

    #[test]
    fn nothing_to_store_is_never_refused() {
        let empty = VideoSpecs {
            file: "none.mp4".into(),
            streams: vec![],
        };
        check_temp_space_for_maps(&empty, false, Path::new("/tmp"), |_| Ok(0.0)).unwrap();
    }
}
