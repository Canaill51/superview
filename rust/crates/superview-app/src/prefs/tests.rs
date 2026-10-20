use super::*;

fn sample() -> Preferences {
    Preferences {
        quality_profile: "Fast".into(),
        encoder_selection: "hevc_nvenc encoder".into(),
        squeeze_source: true,
    }
}

#[test]
fn a_saved_file_loads_back_identical() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("superview").join(FILE_NAME);
    // The directory does not exist yet: save creates it.
    save(&path, &sample()).expect("save");
    assert_eq!(load(&path), sample());
    let left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(
        left,
        [std::ffi::OsString::from(FILE_NAME)],
        "the temporary file must be gone after the rename"
    );
}

#[test]
fn the_temporary_file_is_unique_to_the_process() {
    // A directory squatting on the name another process would use must not matter, and the
    // name of ours carries our pid.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FILE_NAME);
    std::fs::create_dir(dir.path().join("preferences.json.tmp")).expect("mkdir");
    save(&path, &sample()).expect("save");
    assert_eq!(load(&path), sample());

    // And when saving fails, the temporary file of this process is cleaned up.
    let blocked = dir.path().join("sub");
    std::fs::create_dir(&blocked).expect("mkdir");
    let target = blocked.join(FILE_NAME);
    std::fs::create_dir(&target).expect("a directory where the file goes");
    assert!(save(&target, &sample()).is_err());
    let tmp = format!("{FILE_NAME}.{}.tmp", std::process::id());
    assert!(
        !blocked.join(tmp).exists(),
        "a failed save left its temporary file"
    );
}

#[test]
fn saving_over_an_existing_file_replaces_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FILE_NAME);
    save(&path, &sample()).expect("first save");
    save(&path, &Preferences::default()).expect("second save");
    assert_eq!(load(&path), Preferences::default());
}

#[test]
fn a_missing_file_gives_the_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let prefs = load(&dir.path().join("absent").join(FILE_NAME));
    assert_eq!(prefs, Preferences::default());
    assert_eq!(prefs.quality_profile, "Balanced");
    assert_eq!(prefs.encoder_selection, "");
    assert!(!prefs.squeeze_source);
}

#[test]
fn a_corrupted_file_gives_the_defaults_without_panicking() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FILE_NAME);
    for garbage in [
        &b"{ not json"[..],
        b"",
        b"\xff\xfe\x00binary",
        b"[1, 2, 3]",
        // Well-formed, wrongly typed.
        br#"{"squeeze_source": "yes"}"#,
    ] {
        std::fs::write(&path, garbage).expect("write");
        assert_eq!(load(&path), Preferences::default(), "{garbage:?}");
    }
}

#[test]
fn an_unreadable_path_gives_the_defaults() {
    // A directory where the file should be: reading it fails, which is not "absent".
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(load(dir.path()), Preferences::default());
}

#[test]
fn missing_keys_keep_their_defaults_and_unknown_keys_are_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, r#"{"squeeze_source": true, "from_the_future": 1}"#).expect("write");
    let prefs = load(&path);
    assert!(prefs.squeeze_source);
    assert_eq!(prefs.quality_profile, "Balanced");
}

// The dropdown's old rule: any value it does not offer reads as Balanced.
#[test]
fn an_unknown_quality_profile_reads_as_balanced() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, r#"{"quality_profile": "Turbo"}"#).expect("write");
    assert_eq!(load(&path).quality_profile, "Balanced");
    std::fs::write(&path, r#"{"quality_profile": "Fast"}"#).expect("write");
    assert_eq!(load(&path).quality_profile, "Fast");
}

#[test]
fn saving_where_no_file_can_be_written_reports_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The parent "directory" is a file.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").expect("write");
    assert!(save(&blocker.join(FILE_NAME), &sample()).is_err());
}

#[test]
fn the_default_path_is_under_the_injected_user_config_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().into_owned();
    // Both keys: AppData on Windows, XDG_CONFIG_HOME elsewhere.
    let env = move |key: &str| match key {
        "AppData" | "XDG_CONFIG_HOME" => Some(base.clone()),
        _ => None,
    };
    assert_eq!(
        default_path(&env),
        Some(dir.path().join("superview").join(FILE_NAME))
    );
    assert_eq!(default_path(&|_| None), None);
}
