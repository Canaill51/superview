//! Where an encode writes: the guard against overwriting the source, the working file next
//! to the destination, and the check that the destination volume has room.
//! Port of `canonicalPath`, `sameOutputAsInput`, `newPartialOutputPath`, `checkOutputSpace`
//! and `outputSpaceAllowance` (`pipeline.go`).
//!
//! Deliberate differences from the Go code:
//! - Same-file detection (`os.SameFile`) is device plus inode on Unix. std has no stable
//!   file identity on Windows, so there both files are compared through their canonical
//!   paths (`dunce`, no `\\?\` prefix, case-insensitive); a hard link to the source under
//!   another name is therefore not recognised on Windows. Go recognises it. The risk is
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
//! - Go removes the working file if closing it fails; here `Drop` ignores a close error
//!   (a network share could report one), which is harmless: ffmpeg rewrites the file.
//! - `check_output_space` and `new_partial_output_path` take `&Path` and return
//!   [`Error::Plain`] where Go has `fmt.Errorf`; the cause inside a message is Rust's
//!   `io::Error` text. The free-space reading is a parameter (Go swaps a variable in
//!   tests); `disk::free_disk_gb` is the real one.

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

/// The directory that holds `output` (Go: `filepath.Dir`, which says `.` for a bare name).
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn clip(dir: &Path) -> PathBuf {
        let path = dir.join("clip.mp4");
        fs::write(&path, "pretend this is a video").unwrap();
        path
    }

    // ---- same_output_as_input (Go: TestPerformEncoding_RefusesOutputEqualToInput) ----

    #[test]
    fn the_same_path_is_the_same_file() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        assert!(same_output_as_input(&clip, &clip));
    }

    #[test]
    fn another_spelling_of_the_path_is_the_same_file() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        let roundabout = dir.path().join(".").join("clip.mp4");
        assert!(same_output_as_input(&clip, &roundabout));
    }

    #[test]
    fn different_files_are_not_the_same() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        let other = dir.path().join("other.mp4");
        fs::write(&other, "pretend this is a video").unwrap();
        assert!(!same_output_as_input(&clip, &other));
    }

    // The normal case: the destination does not exist yet.
    #[test]
    fn a_destination_that_does_not_exist_is_not_the_source() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        assert!(!same_output_as_input(&clip, &dir.path().join("out.mp4")));
    }

    // Neither exists: the comparison falls back to the cleaned paths.
    #[test]
    fn missing_files_compare_by_their_cleaned_paths() {
        let dir = TempDir::new().unwrap();
        let (a, b) = (dir.path().join("a.mp4"), dir.path().join("b.mp4"));
        assert!(same_output_as_input(
            &a,
            &dir.path().join(".").join("a.mp4")
        ));
        assert!(!same_output_as_input(&a, &b));
    }

    // A symlink to the source, as the destination, and a hard link on Unix.
    #[cfg(unix)]
    #[test]
    fn a_symlink_or_hard_link_to_the_source_is_the_source() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        let symlink = dir.path().join("symlink.mp4");
        std::os::unix::fs::symlink(&clip, &symlink).unwrap();
        assert!(same_output_as_input(&clip, &symlink));
        assert!(same_output_as_input(&symlink, &clip));
        let hard = dir.path().join("hard.mp4");
        fs::hard_link(&clip, &hard).unwrap();
        assert!(same_output_as_input(&clip, &hard));
    }

    // A dangling symlink as the destination does not exist for `metadata`; it is not the
    // source either.
    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_not_the_source() {
        let dir = TempDir::new().unwrap();
        let clip = clip(dir.path());
        let dangling = dir.path().join("dangling.mp4");
        std::os::unix::fs::symlink(dir.path().join("nowhere.mp4"), &dangling).unwrap();
        assert!(!same_output_as_input(&clip, &dangling));
    }

    // Windows reads the same case-insensitively (pathEqual); on Unix names differ by case.
    #[test]
    fn case_matters_only_on_unix() {
        let dir = TempDir::new().unwrap();
        let (lower, upper) = (dir.path().join("a.mp4"), dir.path().join("A.MP4"));
        assert_eq!(same_output_as_input(&lower, &upper), cfg!(windows));
    }

    // ---- new_partial_output_path ----

    // Go: TestNewPartialOutputPath_HonoursUmask (E-10). Go sets the umask to 022; changing
    // it here needs `unsafe` or a process-wide effect that would reach parallel tests. So
    // the mode is compared with that of a file created plainly with 0666, whatever the
    // umask of the test process: 0600 (a `tempfile`-style creation) differs from it under
    // any umask that leaves a group or other bit, 022 included. Under umask 077 the test
    // cannot tell the two apart.
    #[cfg(unix)]
    #[test]
    fn the_working_file_is_created_with_mode_0666_before_umask() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let dir = TempDir::new().unwrap();
        let mode_of = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let partial = new_partial_output_path(&dir.path().join("out.mp4")).unwrap();
        let plain = dir.path().join("plain");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o666)
            .open(&plain)
            .unwrap();
        assert!(
            mode_of(&plain) & 0o066 != 0,
            "umask 077: this test cannot tell 0600 from 0666"
        );
        assert_eq!(mode_of(&partial), mode_of(&plain));
    }

    #[test]
    fn the_working_file_sits_next_to_the_destination_with_the_mp4_extension() {
        let dir = TempDir::new().unwrap();
        let partial = new_partial_output_path(&dir.path().join("out.mp4")).unwrap();
        assert!(partial.is_file(), "the working file was not created");
        assert_eq!(partial.parent(), Some(dir.path()));
        assert_eq!(partial.extension().unwrap(), "mp4");
        let name = partial.file_name().unwrap().to_string_lossy().into_owned();
        let hex = name
            .strip_prefix(".superview-partial-")
            .and_then(|n| n.strip_suffix(".mp4"))
            .unwrap_or_else(|| panic!("unexpected name {name}"));
        assert!(
            hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
            "{name}"
        );
    }

    #[test]
    fn two_working_files_for_one_destination_get_two_names() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out.mp4");
        let first = new_partial_output_path(&out).unwrap();
        let second = new_partial_output_path(&out).unwrap();
        assert_ne!(first, second);
        assert!(first.is_file() && second.is_file());
    }

    #[test]
    fn a_missing_destination_directory_is_an_error_naming_the_output() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("no-such-dir").join("out.mp4");
        let err = new_partial_output_path(&out).expect_err("created a file in a missing folder");
        let message = err.to_string();
        assert!(
            message.starts_with(&format!(
                "cannot create a working file next to {}: ",
                out.display()
            )),
            "{message}"
        );
    }

    // The open is exclusive: a name already taken is skipped and its file left untouched.
    #[test]
    fn a_name_already_taken_is_skipped_and_its_file_left_alone() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out.mp4");
        let taken = dir.path().join(".superview-partial-aaaaaaaaaaaa.mp4");
        fs::write(&taken, "someone else's").unwrap();
        let mut names = ["aaaaaaaaaaaa", "bbbbbbbbbbbb"]
            .into_iter()
            .map(String::from);
        let partial = create_partial(&out, || names.next().unwrap()).unwrap();
        assert_eq!(
            partial,
            dir.path().join(".superview-partial-bbbbbbbbbbbb.mp4")
        );
        assert_eq!(fs::read_to_string(&taken).unwrap(), "someone else's");
    }

    #[test]
    fn after_a_hundred_names_taken_it_gives_up() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out.mp4");
        fs::write(dir.path().join(".superview-partial-aaaaaaaaaaaa.mp4"), "x").unwrap();
        let mut tries = 0;
        let err = create_partial(&out, || {
            tries += 1;
            "aaaaaaaaaaaa".into()
        })
        .unwrap_err();
        assert_eq!(tries, 100);
        assert_eq!(
            err.to_string(),
            format!(
                "cannot create a working file next to {}: every name tried already exists",
                out.display()
            )
        );
    }

    #[test]
    fn the_name_is_random_enough_not_to_repeat() {
        let names: std::collections::HashSet<_> = (0..200).map(|_| random_suffix()).collect();
        assert_eq!(names.len(), 200);
    }

    // ---- check_output_space ----

    // Go: TestCheckOutputSpace_RefusesAFullDestination (E-16)
    #[test]
    fn a_22_gb_output_is_refused_on_a_1_gb_volume_and_allowed_on_100_gb() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("out.mp4");
        let err = check_output_space(&out, 100_000_000, 30.0 * 60.0, |_| Ok(1.0))
            .expect_err("a 22 GB output was allowed onto a 1 GB volume");
        assert_eq!(
            err.to_string(),
            format!(
                "not enough space in {}: the output needs about 24676 MB but only 1024 MB is free. Choose another destination or lower the bitrate",
                dir.path().display()
            )
        );
        check_output_space(&out, 100_000_000, 30.0 * 60.0, |_| Ok(100.0))
            .expect("a 22 GB output was refused on a 100 GB volume");
    }

    // Go: TestCheckOutputSpace_UnreadableFreeSpaceAllowsTheEncode
    #[test]
    fn an_unreadable_free_space_allows_the_encode() {
        let out = Path::new("/videos/out.mp4");
        let unreadable = |_: &Path| Err(io::Error::other("statfs: operation not supported"));
        check_output_space(out, 100_000_000, 30.0 * 60.0, unreadable)
            .expect("an unreadable free space refused the encode");
    }

    // The line is video bytes plus 15%, and the comparison is `>=`. 2 Mb/s over 8 s is
    // 2 MB of video, 2_300_000 bytes with the allowance.
    #[test]
    fn the_line_is_the_video_plus_fifteen_percent() {
        let out = Path::new("/videos/out.mp4");
        let at = |bytes: i64| move |_: &Path| Ok(bytes as f64 / GIB);
        let check = |free: i64| check_output_space(out, 2_000_000, 8.0, at(free));
        check(2_300_000).expect("exactly the line");
        check(2_299_999).expect_err("one byte under the line");
        // The video alone fits, with no room for the audio and the container.
        check(2_100_000).expect_err("the video alone fits but not with its allowance");
    }

    #[test]
    fn nothing_to_estimate_is_never_refused() {
        let out = Path::new("/videos/out.mp4");
        let empty = |_: &Path| Ok(0.0);
        let never_asked = |_: &Path| -> io::Result<f64> { panic!("the volume was measured") };
        check_output_space(out, 0, 100.0, never_asked).unwrap();
        check_output_space(out, -5, 100.0, never_asked).unwrap();
        check_output_space(out, 1_000_000, 0.0, never_asked).unwrap();
        check_output_space(out, 1_000_000, -1.0, never_asked).unwrap();
        check_output_space(out, 0, 100.0, empty).unwrap();
        check_output_space(out, -5, 100.0, empty).unwrap();
        check_output_space(out, 1_000_000, 0.0, empty).unwrap();
        check_output_space(out, 1_000_000, -1.0, empty).unwrap();
    }

    // The volume asked about is the destination folder (and `.` for a bare file name).
    #[test]
    fn the_destination_folder_is_the_one_measured() {
        use std::cell::RefCell;
        let asked = RefCell::new(Vec::new());
        let record = |p: &Path| {
            asked.borrow_mut().push(p.to_path_buf());
            Ok(1000.0)
        };
        check_output_space(Path::new("/videos/out.mp4"), 1_000_000, 10.0, record).unwrap();
        check_output_space(Path::new("out.mp4"), 1_000_000, 10.0, record).unwrap();
        assert_eq!(
            *asked.borrow(),
            [PathBuf::from("/videos"), PathBuf::from(".")]
        );
    }
}
