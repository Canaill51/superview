//! A cache directory that cannot be written: no log, and nothing fails. Its own test binary, so
//! that no logger is set before it.

use superview_ofx::journal::install;

#[test]
fn a_cache_that_cannot_be_written_means_no_log_and_no_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    // A regular file where the directory should go: creating the log directory under it fails on
    // Windows and Linux alike, whoever runs the test.
    let not_a_dir = dir.path().join("a-file");
    std::fs::write(&not_a_dir, b"").unwrap();

    assert_eq!(install(Some(&not_a_dir)), None);
    // Logging then goes nowhere, without a panic.
    log::error!(target: "superview_ofx::plugin", "nowhere");
    // The answer is kept: a later load does not retry.
    assert_eq!(install(Some(dir.path())), None);
}
