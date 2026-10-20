use super::*;

fn err_text(result: Result<(), Error>) -> String {
    result.expect_err("expected a refusal").to_string()
}

fn write(dir: &Path, name: &str) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, b"x").unwrap();
    file
}

/// Creates a symlink, or `None` where the OS or its privileges forbid it
/// (Go's `t.Skipf`).
fn try_symlink(target: &Path, link: &Path) -> Option<()> {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    };
    made.ok()
}

// TestIsValidInputPath
#[test]
fn input_path_table() {
    for (name, path) in [
        ("empty path", ""),
        ("relative path", "video.mp4"),
        ("path traversal with ..", "/home/user/../../../etc/passwd"),
        ("directory instead of file", "/tmp"),
        ("nonexistent file", "/nonexistent/file/path.mp4"),
    ] {
        assert!(is_valid_input_path(path).is_err(), "{name}: {path:?}");
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(is_valid_input_path(dir.path()).is_err(), "a real directory");
    assert!(is_valid_input_path(write(dir.path(), "clip.mp4")).is_ok());
    // The reasons, since each of these fails some other way without its check:
    // Cargo.toml exists relative to the crate directory the tests run in.
    assert!(err_text(is_valid_input_path("")).contains("cannot be empty"));
    assert!(err_text(is_valid_input_path("Cargo.toml")).contains("must be absolute"));
    assert!(err_text(is_valid_input_path(dir.path())).contains("is a directory"));
}

// TestIsValidOutputPath. Go's tmpDir/../../../etc case went through
// filepath.Join, which erases the ".."; the literal string is the stronger
// test.
#[test]
fn output_path_table() {
    let dir = tempfile::tempdir().unwrap();
    let literal = format!("{}/../../etc/output.mp4", dir.path().display());
    for (name, path) in [
        ("empty path", String::new()),
        ("relative path", "output.mp4".into()),
        ("path traversal", literal),
        (
            "nonexistent parent directory",
            "/nonexistent/directory/output.mp4".into(),
        ),
    ] {
        assert!(is_valid_output_path(&path).is_err(), "{name}: {path:?}");
    }
    assert!(is_valid_output_path(dir.path().join("output.mp4")).is_ok());
    assert!(err_text(is_valid_output_path("")).contains("cannot be empty"));
    assert!(err_text(is_valid_output_path("output.mp4")).contains("must be absolute"));
    let missing = dir.path().join("absent").join("out.mp4");
    assert!(err_text(is_valid_output_path(missing)).contains("cannot access output directory"));
}

// TestSanitizeEncoderInput
#[test]
fn sanitize_encoder_input_table() {
    let available = "libx264,libx265,h264_nvenc,hevc_nvenc";
    assert_eq!(sanitize_encoder_input("", available).unwrap(), "");
    assert_eq!(
        sanitize_encoder_input("libx264", available).unwrap(),
        "libx264"
    );
    assert_eq!(
        sanitize_encoder_input("libx265", available).unwrap(),
        "libx265"
    );
    let err = sanitize_encoder_input("libx264 -ssof /etc/passwd", available).unwrap_err();
    assert_eq!(
        err.to_string(),
        "encoder \"libx264 -ssof /etc/passwd\" not in approved list"
    );
    assert!(sanitize_encoder_input("mpeg4", available).is_err());
    // Go trims each approved entry, and matches exactly.
    assert_eq!(sanitize_encoder_input("b", "a, b ,c").unwrap(), "b");
    assert!(sanitize_encoder_input("libx26", available).is_err());
}

// TestPathTraversalPrevention
#[test]
fn traversal_attempts_are_refused() {
    for attempt in [
        "/home/user/../../../etc/passwd",
        "/home/user/./../../etc/passwd",
        "/tmp/video/../../sensitive/file.txt",
        "/var/www/uploads/../../config.php",
        "/home/user/video/../../../etc/shadow",
    ] {
        let text = err_text(is_valid_input_path(attempt));
        assert!(
            text.contains("traversal") || text.contains("cannot access"),
            "{attempt}: {text}"
        );
    }
}

// TestSymlinkResolution_AcceptedWhenTargetIsValid
#[test]
fn symlink_to_a_regular_file_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let target = write(dir.path(), "targetfile.mp4");
    let link = dir.path().join("symlink.mp4");
    if try_symlink(&target, &link).is_none() {
        eprintln!("skipped: cannot create a symlink here");
        return;
    }
    assert!(is_valid_input_path(&link).is_ok());
}

// TestSymlinkResolution_RejectedWhenTargetIsMissing
#[test]
fn dangling_symlink_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("dangling.mp4");
    if try_symlink(&dir.path().join("does-not-exist.mp4"), &link).is_none() {
        eprintln!("skipped: cannot create a symlink here");
        return;
    }
    assert!(is_valid_input_path(&link).is_err());
}

// TestSymlinkResolution_RejectedWhenTargetIsDirectory
#[test]
fn symlink_to_a_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("somedir");
    fs::create_dir(&target).unwrap();
    let link = dir.path().join("dirlink");
    if try_symlink(&target, &link).is_none() {
        eprintln!("skipped: cannot create a symlink here");
        return;
    }
    assert!(is_valid_input_path(&link).is_err());
}

// TestIsValidInputPath_RejectsNonRegularFile (/dev/zero would make ffmpeg
// read forever)
#[test]
#[cfg(unix)]
fn character_device_is_refused() {
    assert!(Path::new("/dev/zero").exists());
    assert!(err_text(is_valid_input_path("/dev/zero")).contains("not a regular file"));
}

// TestHasTraversalComponent
#[test]
fn traversal_component_table() {
    for (path, want) in [
        ("/home/user/../../etc/passwd", true),
        ("/home/user/..", true),
        ("../relative.mp4", true),
        (r"C:\Users\..\Windows\x.mp4", true),
        ("/home/user/Videos/vacances..2024.mp4", false),
        ("/home/user/GoPro..raw/clip.mp4", false),
        ("/home/user/a..b..c.mp4", false),
        ("/home/user/...hidden.mp4", false),
        ("/home/user/normal.mp4", false),
    ] {
        assert_eq!(has_traversal_component(path), want, "{path}");
    }
}

// TestIsValidInputPath_AcceptsDoubleDotInFilename
#[test]
fn double_dot_inside_a_filename_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    assert!(is_valid_input_path(write(dir.path(), "vacances..2024.mp4")).is_ok());
}

// TestIsValidInputPath_StillRejectsTraversal
#[test]
fn traversal_is_still_refused() {
    assert!(err_text(is_valid_input_path("/home/user/../../etc/passwd")).contains("traversal"));
    assert!(err_text(is_valid_output_path("/home/user/../../etc/x.mp4")).contains("traversal"));
}

// TestIsValidOutputPath_LeavesNoProbeFileBehind
#[test]
fn output_check_leaves_no_probe_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    is_valid_output_path(dir.path().join("out.mp4")).unwrap();
    let left: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert!(left.is_empty(), "probe file left behind: {left:?}");
}

// TestIsValidOutputPath_RejectsADirectory (E-04)
#[test]
fn output_path_that_is_a_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("clips");
    fs::create_dir(&target).unwrap();
    assert!(err_text(is_valid_output_path(&target)).contains("is a directory"));
    assert!(is_valid_output_path(dir.path().join("clip.mp4")).is_ok());
}

// Beyond Go: the parent must be a directory, and an existing file may be
// overwritten.
#[test]
fn output_parent_must_be_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "plain.txt");
    let text = err_text(is_valid_output_path(file.join("out.mp4")));
    assert!(text.contains("output parent is not a directory"), "{text}");
    assert!(is_valid_output_path(&file).is_ok());
}

// TestIsSensitiveSystemPath (E-17, E-19)
#[test]
fn sensitive_path_covers_each_directory_and_its_children_only() {
    let sep = MAIN_SEPARATOR;
    for dir in system_directories() {
        assert!(is_sensitive_system_path(&dir), "{dir} itself");
        assert!(
            is_sensitive_system_path(&format!("{dir}{sep}x.mp4")),
            "{dir} child"
        );
        // A sibling that merely shares the prefix is somewhere else.
        assert!(
            !is_sensitive_system_path(&format!("{dir}etera{sep}x.mp4")),
            "{dir} sibling"
        );
    }
}

// TestIsSensitiveSystemPath_UnixLiterals
#[test]
#[cfg(unix)]
fn sensitive_path_unix_literals() {
    for (path, want) in [
        ("/etc/x.mp4", true),
        ("/etc", true),
        ("/etcetera/x.mp4", false),
        ("/home/x.mp4", false),
    ] {
        assert_eq!(is_sensitive_system_path(path), want, "{path}");
    }
    // Refused as written, not only after resolving the parent.
    let text = err_text(is_valid_output_path("/etc/x.mp4"));
    assert!(
        text.contains("protected system directory") && !text.contains("resolves to"),
        "{text}"
    );
}

// The Windows list is built from the environment, with literal fallbacks.
#[test]
#[cfg(windows)]
fn windows_directories_come_from_the_environment() {
    let env = |key: &str| (key == "SystemRoot").then(|| r"D:\WinNT".to_string());
    let dirs = sensitive_system_directories(&env);
    assert!(dirs.iter().any(|d| d == r"D:\WinNT"));
    assert!(dirs.iter().any(|d| d == r"C:\Program Files"));
    assert!(is_sensitive_in(r"d:\winnt\system32\x.mp4", &dirs));
    assert!(!is_sensitive_in(r"D:\WinNTx\x.mp4", &dirs));
}

// Spellings of a protected directory that the string check on the path as
// written cannot see: only the second check, on the resolved parent, refuses
// them. Not in Go's tests; the verdict follows from Go's two-step design.
#[test]
#[cfg(windows)]
fn windows_alternate_spellings_are_refused_after_resolution() {
    let mut forms = vec![
        r"\\?\C:\Windows\x.mp4".to_string(),
        r"\\.\C:\Windows\x.mp4".to_string(),
        r"C:\Windows.\x.mp4".to_string(),
    ];
    // The 8.3 short name exists only where the machine generates them.
    if Path::new(r"C:\PROGRA~1").exists() {
        forms.push(r"C:\PROGRA~1\x.mp4".to_string());
    }
    for form in forms {
        let text = err_text(is_valid_output_path(&form));
        assert!(
            text.contains("protected system directory"),
            "{form}: {text}"
        );
    }
}

// TestIsValidOutputPath_SymlinkedParentIntoSystemDirectory (E-17). The
// assertion is on the reason, so the test means the same thing as root.
#[test]
#[cfg(unix)]
fn symlinked_parent_into_a_system_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("out");
    std::os::unix::fs::symlink("/usr", &link).unwrap();
    let result = is_valid_output_path(link.join("clip.mp4"));
    assert!(err_text(result).contains("protected system directory"));
}
