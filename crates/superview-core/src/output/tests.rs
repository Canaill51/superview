use super::*;
use tempfile::TempDir;

fn clip(dir: &Path) -> PathBuf {
    let path = dir.join("clip.mp4");
    fs::write(&path, "pretend this is a video").unwrap();
    path
}

// ---- same_output_as_input ----

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

// E-10: the umask is not set to 022 here; changing
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

// A full destination is refused (E-16).
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

// An unreadable free-space reading allows the encode.
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
