//! Where an encode writes: the guard against overwriting the source, the working file next
//! to the destination, and the check that the destination volume has room.
//!
//! Design notes:
//! - Same-file detection (`os.SameFile`) is device plus inode on Unix. std has no stable
//!   file identity on Windows, so there both files are compared through their canonical
//!   paths (`dunce`, no `\\?\` prefix, case-insensitive); a hard link to the source under
//!   another name is therefore not recognised on Windows. The risk is
//!   small (the rename of the partial file onto a hard link replaces the name, not the
//!   source's data) and no safe std API closes it.
//! - The fallback "absolute, cleaned" form uses `std::path::absolute` and the cleaning of
//!   `security`, which drops `.` and repeated separators but not `..`: paths reaching this
//!   point have already passed the traversal check.
//! - The working file's name comes from `std`'s `RandomState` (48 random bits, 12 hex
//!   digits) instead of `crypto/rand`, so no crate is added. `tempfile` was not used: it
//!   picks alphanumerics, and its mode would have to be forced and its file kept. The name
//!   only has to be unlikely to collide, and `create_new` (`O_EXCL`) is what makes a
//!   collision, or a planted symlink, a retry rather than a hazard.
//! - `Drop` ignores an error on closing the working file (a network share could report
//!   one), which is harmless: ffmpeg rewrites the file.
//! - `check_output_space` and `new_partial_output_path` take `&Path` and return
//!   [`Error::Plain`]; the cause inside a message is Rust's `io::Error` text. The
//!   free-space reading is a parameter; `disk::free_disk_gb` is the real one.

use std::collections::hash_map::RandomState;
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::security::{clean, path_equal};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: i64 = 1024 * 1024;

/// Covers what the video bitrate does not: the audio track and the container. Fifteen
/// percent is generous for AAC beside any video bitrate this pipeline accepts, and copied
/// audio rarely exceeds it either.
pub const OUTPUT_SPACE_ALLOWANCE: f64 = 1.15;

/// How many names [`new_partial_output_path`] tries before giving up.
const PARTIAL_NAME_ATTEMPTS: u32 = 100;

/// Resolves a path as far as the filesystem allows, so two spellings of the same location
/// compare equal. Falls back to an absolute, cleaned form when the file does not exist yet,
/// which is the normal case for a destination.
fn canonical_path(path: &Path) -> PathBuf {
    if let Ok(resolved) = dunce::canonicalize(path) {
        return clean(&resolved);
    }
    if let Ok(absolute) = std::path::absolute(path) {
        return clean(&absolute);
    }
    clean(path)
}

/// Unix: the two files are the same inode of the same device. `None` where std cannot say.
fn same_inode(a: &fs::Metadata, b: &fs::Metadata) -> Option<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        None
    }
}

/// Whether the destination is the source file.
///
/// FFmpeg used to catch this on its own: it refuses to read and write the same file and
/// exits 234, which left the source intact but told the user nothing useful. Now that the
/// encode goes to a temporary file first, FFmpeg no longer sees a conflict at all -- and the
/// final rename would happily destroy the source. This guard is what keeps that from
/// happening, and it can say something comprehensible while it is at it.
pub fn same_output_as_input(input: &Path, output: &Path) -> bool {
    if let (Ok(input_meta), Ok(output_meta)) = (fs::metadata(input), fs::metadata(output))
        && let Some(same) = same_inode(&input_meta, &output_meta)
    {
        return same;
    }
    let (a, b) = (canonical_path(input), canonical_path(output));
    path_equal(&a.to_string_lossy(), &b.to_string_lossy())
}

/// The directory that holds `output` (`.` for a bare name).
fn destination_dir(output: &Path) -> &Path {
    match output.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// Six random bytes as twelve hex digits.
fn random_suffix() -> String {
    // A `RandomState` is seeded randomly by the standard library; an empty hash of it is
    // a fresh random number, different for every call.
    let bits = RandomState::new().build_hasher().finish();
    format!("{:012x}", bits & 0xffff_ffff_ffff)
}

/// Creates the file ffmpeg writes into while it works, and returns its path.
///
/// It keeps the `.mp4` extension, because ffmpeg picks its muxer from it, and it sits in the
/// destination directory so the final rename stays on one filesystem and is therefore
/// atomic. The name is random and the open is exclusive: two conversions targeting the same
/// folder must not collide.
///
/// Not `tempfile`'s default, which creates with mode 0600. ffmpeg then truncates that inode
/// and the rename keeps its mode, so the finished video arrived unreadable by anyone but its
/// owner -- in a shared folder, a NAS export or a media server's library, that is the one
/// place the user wanted it readable. 0666 before umask is what ffmpeg itself would have
/// created, and is what the user got before the working file existed.
pub fn new_partial_output_path(output: &Path) -> Result<PathBuf, Error> {
    create_partial(output, random_suffix)
}

/// [`new_partial_output_path`] with the names' source as a parameter, so a test can make
/// names collide.
fn create_partial(output: &Path, mut suffix: impl FnMut() -> String) -> Result<PathBuf, Error> {
    let dir = destination_dir(output);
    let shown = output.display();
    for _ in 0..PARTIAL_NAME_ATTEMPTS {
        let name = dir.join(format!(".superview-partial-{}.mp4", suffix()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o666);
        match options.open(&name) {
            // Closed when the handle drops; a close error is ignored by Drop.
            Ok(_) => return Ok(name),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(Error::Plain(format!(
                    "cannot create a working file next to {shown}: {e}"
                )));
            }
        }
    }
    Err(Error::Plain(format!(
        "cannot create a working file next to {shown}: every name tried already exists"
    )))
}

/// Refuses to start when the destination volume cannot hold the file the encode is about to
/// write.
///
/// The output runs to bitrate x duration / 8 -- 100 Mb/s for thirty minutes is 22 GB -- and
/// nothing compared that with the target volume: the encode filled the disk twenty minutes in
/// and failed with an opaque write error.
///
/// A failed reading is not a refusal, the rule `check_temp_space_for_maps` follows: the
/// encode goes ahead and, at worst, fails later exactly as it used to.
pub fn check_output_space(
    output: &Path,
    bitrate: i64,
    duration_seconds: f64,
    free_gb: impl Fn(&Path) -> io::Result<f64>,
) -> Result<(), Error> {
    if bitrate <= 0 || duration_seconds <= 0.0 {
        return Ok(());
    }
    let dir = destination_dir(output);
    let needed = (bitrate as f64 * duration_seconds / 8.0 * OUTPUT_SPACE_ALLOWANCE) as i64;

    let free_gb = match free_gb(dir) {
        Ok(gb) => gb,
        Err(e) => {
            log::warn!(
                "Could not check free space at the destination: path={} error={e}",
                dir.display()
            );
            return Ok(());
        }
    };
    let free = (free_gb * GIB) as i64;
    if free >= needed {
        return Ok(());
    }
    Err(Error::Plain(format!(
        "not enough space in {}: the output needs about {} MB but only {} MB is free. Choose another destination or lower the bitrate",
        dir.display(),
        needed / MIB,
        free / MIB
    )))
}

#[cfg(test)]
mod tests;
