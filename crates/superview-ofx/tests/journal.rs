//! The plugin's log, as `Load` installs it. A logger is set once per process, so this file is its
//! own test binary and does everything in one test.

use std::fs;

use superview_ofx::journal::{MAX_LOG_FILE_BYTES, install};

#[test]
fn the_log_is_installed_once_keeps_its_cap_and_drops_the_fork_chatter() {
    let first = tempfile::TempDir::new().unwrap();
    let other = tempfile::TempDir::new().unwrap();

    let path = install(Some(first.path())).expect("a writable cache directory gives a log");
    assert_eq!(
        path,
        first.path().join("superview-ofx").join("superview-ofx.log")
    );
    log::info!(target: "superview_ofx::plugin", "before the second load");

    // A second `Load` (or a second plugin load in the same process) opens nothing new and keeps
    // what was written.
    assert_eq!(install(Some(other.path())), Some(path.clone()));
    assert_eq!(install(None), Some(path.clone()));
    assert!(!other.path().join("superview-ofx").exists());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("msg=before the second load"), "{text}");

    // The fork logs every host message at info: none of it reaches the file.
    log::info!(target: "ofx::registry", "every frame");
    log::debug!(target: "superview_ofx::plugin", "per frame");
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("every frame") && !text.contains("per frame"),
        "{text}"
    );

    // Twice the cap in records: the file stays under it and the newest record is there.
    let filler = "x".repeat(1000);
    for i in 0..2 * MAX_LOG_FILE_BYTES / 1000 {
        log::info!(target: "superview_ofx::plugin", "record={i} {filler}");
    }
    log::info!(target: "superview_ofx::plugin", "the last record");
    let size = i64::try_from(fs::metadata(&path).unwrap().len()).unwrap();
    assert!(size <= MAX_LOG_FILE_BYTES, "{size} bytes");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.ends_with("msg=the last record\n"));
}
