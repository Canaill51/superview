use super::*;
use std::fs;
use tempfile::TempDir;

// Declared first, dropped last: on Windows an open handle keeps the directory from
// being deleted, so each test drops its writer before the end.
fn cache() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn log_levels_parse_with_info_as_the_fallback() {
    use log::LevelFilter::*;
    let cases = [
        ("debug", Debug),
        ("DEBUG", Debug),
        (" Debug ", Debug),
        ("info", Info),
        ("warn", Warn),
        ("warning", Warn),
        ("error", Error),
        ("ERROR", Error),
        ("", Info),
        ("verbose", Info),
        ("trace", Info),
    ];
    for (input, want) in cases {
        assert_eq!(parse_log_level(input), want, "{input:?}");
    }
}

// user_cache_dir follows XDG, then HOME.
#[test]
#[cfg(unix)]
fn user_cache_dir_follows_xdg_then_home() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    };
    let got = user_cache_dir(&env(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/x")));
    let got = user_cache_dir(&env(&[("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/h/.cache")));
    let got = user_cache_dir(&env(&[("XDG_CACHE_HOME", ""), ("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/h/.cache")));
    let relative = env(&[("XDG_CACHE_HOME", "rel"), ("HOME", "/h")]);
    assert_eq!(user_cache_dir(&relative), None);
    assert_eq!(user_cache_dir(&env(&[])), None);
}

#[test]
#[cfg(windows)]
fn user_cache_dir_is_local_app_data() {
    let env = |key: &str| (key == "LocalAppData").then(|| r"C:\Users\u\AppData\Local".to_string());
    assert_eq!(
        user_cache_dir(&env),
        Some(PathBuf::from(r"C:\Users\u\AppData\Local"))
    );
    assert_eq!(user_cache_dir(&|_| None), None);
}

#[test]
fn the_file_is_created_and_reopening_appends() {
    let cache = cache();
    let (mut first, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
    first.write_all(b"first\n").unwrap();
    drop(first);
    assert_eq!(path, cache.path().join("superview-test/superview-test.log"));

    let (mut second, path2) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
    second.write_all(b"second\n").unwrap();
    drop(second);
    assert_eq!(path2, path, "the path changed between runs");
    assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");
}

#[test]
fn a_file_over_the_cap_is_emptied_at_open_and_zero_means_no_cap() {
    let cache = cache();
    let (mut writer, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
    writer.write_all(&[b'x'; 4096]).unwrap();
    drop(writer);

    // A cap the file already exceeds.
    drop(
        open_log_file("superview-test", 1024, cache.path())
            .unwrap()
            .0,
    );
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        0,
        "kept bytes past the cap"
    );

    // A cap of 0 (or less) means no cap and must not truncate.
    for no_cap in [0, -1] {
        let (mut writer, _) = open_log_file("superview-test", no_cap, cache.path()).unwrap();
        writer.write_all(&[b'k'; 4096]).unwrap();
        drop(writer);
        drop(
            open_log_file("superview-test", no_cap, cache.path())
                .unwrap()
                .0,
        );
    }
    assert_eq!(fs::metadata(&path).unwrap().len(), 8192);
}

#[test]
fn the_cap_holds_while_writing_and_the_newest_record_survives() {
    let cache = cache();
    const LIMIT: u64 = 1024;
    let (mut writer, path) = open_log_file("superview-test", LIMIT as i64, cache.path()).unwrap();
    let record = format!("{}\n", "r".repeat(99));
    let mut truncated = false;
    let mut previous = 0;
    for i in 0..200 {
        writer.write_all(record.as_bytes()).unwrap();
        let size = fs::metadata(&path).unwrap().len();
        assert!(
            size <= LIMIT + record.len() as u64,
            "after {} writes: {size}",
            i + 1
        );
        truncated |= size < previous;
        previous = size;
    }
    assert!(truncated, "the file was never emptied");
    assert!(previous > 0, "the log is empty after the last write");
    assert_eq!(
        fs::read_to_string(&path).unwrap().len() as u64 % record.len() as u64,
        0
    );
}

// "a single record larger than the cap is still written whole".
#[test]
fn a_record_larger_than_the_cap_is_written_whole() {
    let cache = cache();
    let (mut writer, path) = open_log_file("superview-test", 10, cache.path()).unwrap();
    writer.write_all(b"small\n").unwrap();
    writer.write_all(&[b'b'; 100]).unwrap();
    drop(writer);
    assert_eq!(fs::read(&path).unwrap(), vec![b'b'; 100]);
}

// The line is exact (a write that reaches the cap is kept, one that passes
// it starts over) and the count starts again after a truncation.
#[test]
fn the_cap_is_exact_and_the_count_restarts_after_a_truncation() {
    let cache = cache();
    let (mut writer, path) = open_log_file("superview-test", 10, cache.path()).unwrap();
    let read = || fs::read_to_string(&path).unwrap();
    writer.write_all(b"12345").unwrap();
    writer.write_all(b"67890").unwrap();
    assert_eq!(
        read(),
        "1234567890",
        "a write that reaches the cap was refused"
    );
    writer.write_all(b"a").unwrap();
    assert_eq!(read(), "a", "a write past the cap did not start over");
    writer.write_all(b"bcd").unwrap();
    writer.write_all(b"efg").unwrap();
    assert_eq!(
        read(),
        "abcdefg",
        "the count did not restart after the truncation"
    );
    drop(writer);
}

// The shared form: two threads through `&CappedLogWriter` never interleave a record.
#[test]
fn a_shared_writer_keeps_records_whole() {
    let cache = cache();
    let (writer, path) = open_log_file("superview-test", 0, cache.path()).unwrap();
    std::thread::scope(|scope| {
        for byte in *b"ab" {
            let writer = &writer;
            scope.spawn(move || {
                let mut line = vec![byte; 63];
                line.push(b'\n');
                for _ in 0..200 {
                    (&*writer).write_all(&line).unwrap();
                }
            });
        }
    });
    drop(writer);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 400);
    assert!(
        text.lines()
            .all(|l| l.len() == 63 && l.bytes().all(|b| b == l.as_bytes()[0]))
    );
}

#[cfg(unix)]
#[test]
fn the_log_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let cache = cache();
    let (writer, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600, "log file mode");
    assert_eq!(mode(path.parent().unwrap()), 0o700, "log directory mode");
    drop(writer);
}

// The error names what failed.
#[test]
fn an_impossible_directory_is_reported() {
    let cache = cache();
    let blocker = cache.path().join("file");
    fs::write(&blocker, "x").unwrap();
    let err = open_log_file("superview-test", 0, &blocker).err().unwrap();
    assert!(
        err.to_string().starts_with("cannot create log directory"),
        "{err}"
    );
}

// F-22: the modes given at creation do not tighten what already exists.
#[cfg(unix)]
#[test]
fn an_existing_loose_log_is_made_private() {
    use std::os::unix::fs::PermissionsExt;
    let cache = cache();
    let dir = cache.path().join("superview-test");
    fs::create_dir_all(&dir).unwrap();
    let log = dir.join("superview-test.log");
    fs::write(&log, b"old\n").unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).unwrap();
    let (writer, path) = open_log_file("superview-test", 1 << 20, cache.path()).unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600, "log file mode");
    assert_eq!(mode(&dir), 0o700, "log directory mode");
    drop(writer);
}
