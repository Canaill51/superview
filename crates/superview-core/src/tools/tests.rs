use super::*;
use crate::process::tests::spawn_lock;
use std::sync::Arc;
use tempfile::TempDir;

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + Send + Sync + 'static {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |key| map.get(key).map(OsString::from)
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A resolver that believes the program runs from `exe_dir`, with `pairs`
/// as its whole environment.
fn resolver_in(exe_dir: &Path, pairs: &[(&str, &str)]) -> ToolResolver {
    ToolResolver::new(env_of(pairs), Some(exe_dir.to_path_buf()))
}

/// Drops a file where a tool binary would be and returns its path.
fn write_fake_tool(dir: &Path, tool: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(tool_file_name(tool));
    fs::write(&path, "stand-in").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

// The bundled copy is preferred over PATH -- the decision the
// whole bundling exists to make (U-03).
#[test]
fn resolve_prefers_the_bundled_copy_over_path() {
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let bundled = write_fake_tool(app.path(), "ffmpeg");
    write_fake_tool(on_path.path(), "ffmpeg");
    let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), bundled);
}

#[test]
fn resolve_finds_the_linux_install_layout() {
    let prefix = TempDir::new().unwrap();
    let app = prefix.path().join("bin");
    fs::create_dir_all(&app).unwrap();
    let bundled = write_fake_tool(&prefix.path().join("lib").join("superview"), "ffprobe");
    let empty = TempDir::new().unwrap();
    let resolver = resolver_in(&app, &[("PATH", s(empty.path()))]);
    assert_eq!(resolver.resolve("ffprobe"), bundled);
}

#[test]
fn resolve_override_wins_over_the_bundled_copy() {
    let (app, over) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    write_fake_tool(app.path(), "ffmpeg");
    let chosen = write_fake_tool(over.path(), "ffmpeg");
    let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, s(over.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), chosen);
}

// The variable is trimmed.
#[test]
fn resolve_trims_the_override() {
    let (app, over) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let chosen = write_fake_tool(over.path(), "ffmpeg");
    let padded = format!("  {}\t", s(over.path()));
    let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, &padded)]);
    assert_eq!(resolver.resolve("ffmpeg"), chosen);
}

#[test]
fn resolve_bogus_override_falls_through() {
    let app = TempDir::new().unwrap();
    let bundled = write_fake_tool(app.path(), "ffmpeg");
    let nothing = app.path().join("nothing-here");
    let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, s(&nothing))]);
    assert_eq!(resolver.resolve("ffmpeg"), bundled);
}

// Falls back to PATH with nothing bundled. Skipped on
// Windows: winget and scoop directories would answer first.
#[cfg(not(windows))]
#[test]
fn resolve_falls_back_to_path_with_nothing_bundled() {
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let found = write_fake_tool(on_path.path(), "ffmpeg");
    let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), found);
}

// A file without the execute
// bit on PATH is not a tool.
#[cfg(unix)]
#[test]
fn path_lookup_ignores_a_file_that_is_not_executable() {
    use std::os::unix::fs::PermissionsExt;
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let file = write_fake_tool(on_path.path(), "ffmpeg");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), PathBuf::from("ffmpeg"));
}

/// A relative spelling of `target` (an absolute path), reached from the
/// working directory by as many ".." as it is deep. No change of directory.
#[cfg(unix)]
fn relative_to_cwd(target: &Path) -> String {
    let depth = std::env::current_dir().unwrap().components().count();
    format!(
        "{}{}",
        "../".repeat(depth),
        s(target).trim_start_matches('/')
    )
}

// L-16: a relative entry on PATH is refused.
// Runs the command for real: a fake ffmpeg that would answer sits behind a
// relative PATH entry, and the spawner must not find it.
#[cfg(unix)]
#[test]
fn a_command_never_finds_a_tool_through_a_relative_path_entry() {
    let _guard = spawn_lock();
    let (app, planted) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let fake = write_fake_tool(planted.path(), "ffmpeg");
    fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
    let relative = relative_to_cwd(planted.path());
    assert!(
        Path::new(&relative).join("ffmpeg").is_file(),
        "setup: {relative}"
    );

    // Control: reached by its absolute path, the fake runs and succeeds.
    assert!(Command::new(&fake).status().unwrap().success());

    for path in [relative.clone(), format!(".:{relative}:"), String::new()] {
        let resolver = resolver_in(app.path(), &[("PATH", &path)]);
        let err = resolver
            .ffmpeg_command(["-version"])
            .status()
            .expect_err(&format!("PATH={path:?}: a tool was found through it"));
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}

// The filter on its own. Never empty, which execvp would read as
// "the current directory".
#[cfg(unix)]
#[test]
fn absolute_path_entries_keeps_only_absolute_ones() {
    let keep = |p: &str| absolute_path_entries(OsString::from(p));
    assert_eq!(keep("/a:rel:.::/b/c"), OsString::from("/a:/b/c"));
    for none in ["", ".", ":", "rel/bin:x"] {
        assert_eq!(
            keep(none),
            OsString::from("/superview-no-such-dir"),
            "{none:?}"
        );
    }
}

// A PATH entry that is not UTF-8 still searches, and so does
// an override (the values are OsString end to end).
#[cfg(unix)]
#[test]
fn non_utf8_environment_values_are_still_searched() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let (app, base) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let odd = base.path().join(OsStr::from_bytes(b"bin-\xff"));
    let found = write_fake_tool(&odd, "ffmpeg");

    let path = OsString::from_vec(odd.as_os_str().as_bytes().to_vec());
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone()),
        Some(app.path().to_path_buf()),
    );
    assert_eq!(resolver.resolve("ffmpeg"), found, "PATH");

    let dir = odd.clone().into_os_string();
    let resolver = ToolResolver::new(
        move |key| (key == TOOL_DIR_ENV).then(|| dir.clone()),
        Some(app.path().to_path_buf()),
    );
    assert_eq!(resolver.resolve("ffmpeg"), found, "override");
}

// A directory named like the tool on PATH is not the tool
// (it has the execute bit, which is how a directory is searched).
#[cfg(unix)]
#[test]
fn path_lookup_ignores_a_directory_named_like_the_tool() {
    let (app, decoy, real) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    fs::create_dir(decoy.path().join("ffmpeg")).unwrap();
    let found = write_fake_tool(real.path(), "ffmpeg");
    let path = format!("{}:{}", s(decoy.path()), s(real.path()));
    let resolver = resolver_in(app.path(), &[("PATH", &path)]);
    assert_eq!(resolver.resolve("ffmpeg"), found);
}

// A relative PATH entry is skipped, never resolved against the
// working directory (listed in the header).
#[cfg(not(windows))]
#[test]
fn path_lookup_skips_relative_entries() {
    // A real tool reached by a RELATIVE path: as many ".." as the working
    // directory is deep, then the absolute path. No change of directory.
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    write_fake_tool(on_path.path(), "ffmpeg");
    let depth = std::env::current_dir().unwrap().components().count();
    let relative = format!(
        "{}{}",
        "../".repeat(depth),
        s(on_path.path()).trim_start_matches('/')
    );
    assert!(
        Path::new(&relative).join("ffmpeg").is_file(),
        "the test's own setup: {relative}"
    );
    let path = format!("{relative}:.:");
    let resolver = resolver_in(app.path(), &[("PATH", &path)]);
    assert_eq!(resolver.resolve("ffmpeg"), PathBuf::from("ffmpeg"));
}

#[cfg(not(windows))]
#[test]
fn resolve_ignores_a_directory_named_like_the_tool() {
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    fs::create_dir(app.path().join(tool_file_name("ffmpeg"))).unwrap();
    let found = write_fake_tool(on_path.path(), "ffmpeg");
    let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), found);
}

// usable_tool_file is the regular-file check on its own.
#[test]
fn usable_tool_file_wants_a_regular_file() {
    let dir = TempDir::new().unwrap();
    let file = write_fake_tool(dir.path(), "ffmpeg");
    assert!(usable_tool_file(&file));
    assert!(!usable_tool_file(dir.path()));
    assert!(!usable_tool_file(&dir.path().join("missing")));
}

#[test]
fn find_bundled_survives_an_unknown_executable_path() {
    let resolver = ToolResolver::new(env_of(&[]), None);
    assert_eq!(resolver.find_bundled("ffmpeg"), None);
}

// A batch file would leave ffmpeg running when a
// cancellation kills `cmd.exe`, so `PATHEXT` is cut down to programs. Pure, so it runs
// on every OS; the Windows search uses it.
#[test]
fn path_search_keeps_only_program_extensions() {
    assert_eq!(
        program_extensions(Some(".COM;.EXE;.BAT;.CMD;.VBS"), false),
        [".COM", ".EXE"]
    );
    assert_eq!(program_extensions(Some(".bat;.Exe"), false), [".Exe"]);
    assert_eq!(
        program_extensions(Some(".BAT;.CMD"), false),
        Vec::<String>::new()
    );
    assert_eq!(program_extensions(None, false), [".COM", ".EXE"]);
    assert_eq!(program_extensions(Some(""), false), [".COM", ".EXE"]);
    assert_eq!(
        program_extensions(None, true),
        [".COM", ".EXE", ".BAT", ".CMD"]
    );
    assert_eq!(
        program_extensions(Some(".COM;.EXE;.BAT;.CMD"), true),
        [".COM", ".EXE", ".BAT", ".CMD"]
    );
}

// Failures are not cached. The test does not peek into
// the cache; it shows the effect: a tool installed afterwards is found.
#[test]
fn resolve_does_not_cache_failures() {
    let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
    const MISSING: &str = "superview-definitely-not-a-real-tool";
    assert_eq!(resolver.resolve(MISSING), PathBuf::from(MISSING));
    let installed = write_fake_tool(on_path.path(), MISSING);
    let found = resolver.resolve(MISSING);
    // PATHEXT supplies the extension as the user wrote it (`.EXE`), and NTFS
    // takes both spellings for the same file: compare the paths without case there.
    if cfg!(windows) {
        assert_eq!(
            found.to_string_lossy().to_lowercase(),
            installed.to_string_lossy().to_lowercase()
        );
    } else {
        assert_eq!(found, installed);
    }
}

// Successes are cached, on a stand-in rather than a real
// ffmpeg, plus reset_cache.
#[test]
fn resolve_caches_success_until_the_cache_is_reset() {
    let app = TempDir::new().unwrap();
    let bundled = write_fake_tool(app.path(), "ffmpeg");
    let empty = TempDir::new().unwrap();
    let resolver = resolver_in(app.path(), &[("PATH", s(empty.path()))]);
    assert_eq!(resolver.resolve("ffmpeg"), bundled);
    fs::remove_file(&bundled).unwrap();
    assert_eq!(resolver.resolve("ffmpeg"), bundled, "served from the cache");
    resolver.reset_cache();
    assert_eq!(resolver.resolve("ffmpeg"), PathBuf::from("ffmpeg"));
}

// The resolver is shared between threads (the GUI's and the
// encode's), and what it caches for one tool is not served for another.
#[test]
fn resolve_caches_per_tool_and_is_shareable() {
    let app = TempDir::new().unwrap();
    let ffmpeg = write_fake_tool(app.path(), "ffmpeg");
    let ffprobe = write_fake_tool(app.path(), "ffprobe");
    let resolver = Arc::new(resolver_in(app.path(), &[]));
    let handle = {
        let resolver = Arc::clone(&resolver);
        std::thread::spawn(move || resolver.resolve("ffmpeg"))
    };
    assert_eq!(handle.join().unwrap(), ffmpeg);
    assert_eq!(resolver.resolve("ffprobe"), ffprobe);
}

// A stand-in for the Windows layouts: points the three variables at fresh
// directories and returns the LOCALAPPDATA one. ProgramFiles and
// USERPROFILE get real, empty directories rather than "" (E-22).
struct Winget {
    local: TempDir,
    _program_files: TempDir,
    _profile: TempDir,
    env: HashMap<String, String>,
}

fn winget_layout() -> Winget {
    let (local, program_files, profile) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let env = HashMap::from([
        ("LOCALAPPDATA".to_string(), s(local.path()).to_string()),
        (
            "ProgramFiles".to_string(),
            s(program_files.path()).to_string(),
        ),
        ("USERPROFILE".to_string(), s(profile.path()).to_string()),
    ]);
    Winget {
        local,
        _program_files: program_files,
        _profile: profile,
        env,
    }
}

impl Winget {
    fn find(&self, tool: &str) -> Option<PathBuf> {
        let env = self.env.clone();
        find_windows_tool_binary(
            &move |k| env.get(k).map(OsString::from),
            &usable_tool_file,
            tool,
        )
    }

    fn write_tool(&self, tool: &str, parts: &[&str]) -> PathBuf {
        let mut dir = self.local.path().to_path_buf();
        dir.extend(parts);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{tool}.exe"));
        fs::write(&path, "stand-in").unwrap();
        // The lookup asks for the execute bit on Unix too (F-21); Windows has none.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }
}

const PACKAGES: [&str; 3] = ["Microsoft", "WinGet", "Packages"];

fn package_parts<'a>(rest: &[&'a str]) -> Vec<&'a str> {
    PACKAGES
        .iter()
        .copied()
        .chain(rest.iter().copied())
        .collect()
}

// A winget package tree.
#[test]
fn windows_finds_a_winget_packages_tree() {
    let layout = winget_layout();
    let want = layout.write_tool(
        "ffmpeg",
        &package_parts(&[
            "Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe",
            "ffmpeg-8.1.1-full_build",
            "bin",
        ]),
    );
    assert_eq!(layout.find("ffmpeg"), Some(want));
}

// A package not
// named after ffmpeg
#[test]
fn windows_walk_stays_inside_ffmpeg_packages() {
    let layout = winget_layout();
    layout.write_tool("ffmpeg", &package_parts(&["SomeVendor.MediaKit_x", "bin"]));
    assert_eq!(layout.find("ffmpeg"), None);
}

// The Links shim wins.
#[test]
fn windows_links_shim_wins_over_the_package_tree() {
    let layout = winget_layout();
    layout.write_tool("ffmpeg", &package_parts(&["Gyan.FFmpeg_x", "bin"]));
    let link = layout.write_tool("ffmpeg", &["Microsoft", "WinGet", "Links"]);
    assert_eq!(layout.find("ffmpeg"), Some(link));
}

// The walk matches the file name without regard to case.
#[test]
fn windows_walk_ignores_the_case_of_the_file_name() {
    let layout = winget_layout();
    let dir = layout
        .local
        .path()
        .join(package_parts(&["Gyan.FFmpeg_x", "bin"]).join("/"));
    fs::create_dir_all(&dir).unwrap();
    let want = dir.join("FFmpeg.EXE");
    fs::write(&want, "stand-in").unwrap();
    assert_eq!(layout.find("ffmpeg"), Some(want));
}

// Relative candidates are ignored. Rust tests must not change the
// process's state, so every candidate is shown to `usable` instead, and
// `usable` approves whatever it is shown: a relative path would be returned.
#[test]
fn windows_ignores_relative_candidates() {
    let env = env_of(&[
        ("LOCALAPPDATA", ""),
        ("ProgramFiles", ""),
        ("USERPROFILE", ""),
    ]);
    let found = find_windows_tool_binary(&env, &|_| true, "ffmpeg");
    assert_eq!(found, None);
}

// The same, with only one variable set: a candidate may only come from a
// variable that is.
#[test]
fn windows_builds_candidates_only_from_set_variables() {
    let env = env_of(&[("USERPROFILE", r"C:\Users\x")]);
    let seen = Mutex::new(Vec::new());
    let found = find_windows_tool_binary(
        &env,
        &|path| {
            seen.lock().unwrap().push(path.to_path_buf());
            false
        },
        "ffprobe",
    );
    assert_eq!(found, None);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].ends_with("ffprobe.exe") && seen[0].starts_with(r"C:\Users\x"));
}

// ---- parsing ----

#[test]
fn parse_ffmpeg_version_cases() {
    for (name, input, want) in [
        (
            "standard",
            "ffmpeg version 6.1.1 Copyright (c) 2000-2023\nbuilt with gcc",
            "6.1.1",
        ),
        (
            "distro build",
            "ffmpeg version n7.0.2-ubuntu1 Copyright",
            "n7.0.2-ubuntu1",
        ),
        ("truncated", "ffmpeg version", "unknown"),
        ("single token", "ffmpeg", "unknown"),
        ("empty", "", "unknown"),
        ("only newlines", "\n\n", "unknown"),
    ] {
        assert_eq!(parse_ffmpeg_version(input), want, "{name}");
    }
}

fn codecs(list: &[&str]) -> Vec<String> {
    list.iter().map(|c| (*c).to_string()).collect()
}

#[test]
fn parse_ffmpeg_encoders_keeps_the_matching_video_encoders() {
    let listing = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n \
V....D libx264              H.264 / AVC\n V....D libx265              H.265 / HEVC\n \
V....D h264_nvenc           NVIDIA NVENC H.264\n A....D aac                  AAC audio\n \
V....D libvpx-vp9           VP9 video\n";
    assert_eq!(
        parse_ffmpeg_encoders(listing, &codecs(&["264", "265", "hevc"])),
        ["libx264", "libx265", "h264_nvenc"]
    );
}

// Malformed input does not panic (each of these used
// to index past the end of a split slice).
#[test]
fn parse_ffmpeg_encoders_malformed_input_does_not_panic() {
    for input in [
        "",
        " V",
        " V\n",
        "------\n V",
        "no separator at all\n V....D libx264 H.264",
        "\r\n------\r\n V....D libx265 HEVC\r\n",
    ] {
        let _ = parse_ffmpeg_encoders(input, &codecs(&["264", "265"]));
    }
    // And the CRLF case still parses.
    assert_eq!(
        parse_ffmpeg_encoders("\r\n------\r\n V....D libx265 HEVC\r\n", &codecs(&["265"])),
        ["libx265"]
    );
}

#[test]
fn parse_ffmpeg_encoders_without_separator_still_parses() {
    let listing = " V....D libx264              H.264 / AVC\n V....D libx265              H.265\n";
    assert_eq!(
        parse_ffmpeg_encoders(listing, &codecs(&["264"])),
        ["libx264"]
    );
}

// The header is skipped whatever it holds, and CRLF line ends
// (Windows ffmpeg) leave no "\r" in a name.
#[test]
fn parse_ffmpeg_accels_skips_the_header_and_reads_crlf() {
    assert_eq!(
        parse_ffmpeg_accels("Hardware acceleration methods:\r\ncuda\r\nvaapi\r\n\r\n"),
        "cuda,vaapi"
    );
    assert_eq!(
        parse_ffmpeg_accels("Header:\ncuda\n\nvaapi\n"),
        "cuda,vaapi"
    );
    assert_eq!(parse_ffmpeg_accels("Hardware acceleration methods:\n"), "");
    assert_eq!(parse_ffmpeg_accels(""), "");
}

// What precedes the separator is a banner, not encoders, even
// if a line of it looks like a row.
#[test]
fn parse_ffmpeg_encoders_ignores_rows_before_the_separator() {
    let listing = "Encoders:\n V..... libx264_banner\n ------\n V....D libx265  HEVC\n";
    assert_eq!(
        parse_ffmpeg_encoders(listing, &codecs(&["264", "265"])),
        ["libx265"]
    );
}

// Only rows flagged "V" are video encoders, whatever the name.
#[test]
fn parse_ffmpeg_encoders_ignores_audio_rows() {
    let listing = " ------\n A....D audio264  AAC\n V....D libx264  H.264\n";
    assert_eq!(
        parse_ffmpeg_encoders(listing, &codecs(&["264"])),
        ["libx264"]
    );
}

// ---- check_ffmpeg ----

/// An ffmpeg on PATH that answers -version and -hwaccels like a real one,
/// lists libx264 for -encoders, and exits 1 on the one query named by
/// `failing`. The bundled lookup points at an empty directory and the
/// override is unset.
struct FakeFfmpeg {
    _dir: TempDir,
    _empty: TempDir,
    resolver: ToolResolver,
}

fn fake_check_ffmpeg(failing: &str) -> FakeFfmpeg {
    let (dir, empty) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let encoders = ["Encoders:", " ------", " V....D libx264              H.264"];
    let (name, script);
    #[cfg(not(windows))]
    {
        name = "ffmpeg";
        let sh = |query: &str, lines: &[&str]| {
            if query == failing {
                return "echo 'something went wrong' 1>&2; exit 1".to_string();
            }
            let echoes: String = lines.iter().map(|l| format!("echo '{l}'; ")).collect();
            format!("{echoes}exit 0")
        };
        script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  -version) {} ;;\n  -hwaccels) {} ;;\n  -encoders) {} ;;\nesac\nexit 1\n",
            sh("-version", &["ffmpeg version 9.9 Copyright"]),
            sh("-hwaccels", &["Hardware acceleration methods:", "cuda"]),
            sh("-encoders", &encoders),
        );
    }
    #[cfg(windows)]
    {
        name = "ffmpeg.bat";
        let bat = |query: &str, lines: &[&str]| {
            let head = format!(":{}\r\n", query.trim_start_matches('-'));
            if query == failing {
                return format!("{head}echo something went wrong 1>&2\r\nexit /b 1\r\n");
            }
            let echoes: String = lines.iter().map(|l| format!("echo {l}\r\n")).collect();
            format!("{head}{echoes}exit /b 0\r\n")
        };
        script = format!(
            "@echo off\r\nif \"%1\"==\"-version\" goto version\r\nif \"%1\"==\"-hwaccels\" goto hwaccels\r\nif \"%1\"==\"-encoders\" goto encoders\r\nexit /b 1\r\n{}{}{}",
            bat("-version", &["ffmpeg version 9.9 Copyright"]),
            bat("-hwaccels", &["Hardware acceleration methods:", "cuda"]),
            bat("-encoders", &encoders),
        );
    }
    let file = dir.path().join(name);
    fs::write(&file, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let real_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&real_path)),
    )
    .unwrap();
    let path = path.to_str().unwrap().to_string();
    let resolver = ToolResolver::new(env_of(&[("PATH", &path)]), Some(empty.path().to_path_buf()))
        .with_batch_stand_ins();
    FakeFfmpeg {
        _dir: dir,
        _empty: empty,
        resolver,
    }
}

// Control: the same
// stand-in, failing nothing, is read correctly, so the error cases cannot
// pass because the fake is broken.
#[test]
fn check_ffmpeg_reads_a_healthy_stand_in() {
    let _guard = spawn_lock();
    let fake = fake_check_ffmpeg("");
    let got = check_ffmpeg(&Config::default(), &fake.resolver).unwrap();
    assert_eq!(got["version"], "9.9");
    assert_eq!(got["accels"], "cuda");
    assert_eq!(got["encoders"], "libx264");
    assert_eq!(got.len(), 3, "the map keys are the recorded ones: {got:?}");
}

// "-encoders fails" and
// "-hwaccels fails". An ffmpeg that runs but cannot list its accelerators
// or encoders is reported, not turned into an empty encoder list.
#[test]
fn check_ffmpeg_reports_a_failing_query() {
    let _guard = spawn_lock();
    for (failing, want) in [
        ("-encoders", "encoders"),
        ("-hwaccels", "hardware accelerators"),
    ] {
        let fake = fake_check_ffmpeg(failing);
        let err = check_ffmpeg(&Config::default(), &fake.resolver)
            .expect_err(&format!("{failing} failed but no error came back"));
        let message = err.to_string();
        assert!(
            message.contains(want),
            "{message:?} should mention {want:?}"
        );
    }
}

// A failing -version query, which
// is the "ffmpeg missing" message the GUI turns into an install link.
#[test]
fn check_ffmpeg_reports_a_failing_version_query() {
    let _guard = spawn_lock();
    let fake = fake_check_ffmpeg("-version");
    let message = check_ffmpeg(&Config::default(), &fake.resolver)
        .unwrap_err()
        .to_string();
    assert!(message.starts_with("cannot find ffmpeg/ffprobe on your system\n"));
}

// Not on Windows, where
// winget directories PATH does not govern could still answer.
#[cfg(not(windows))]
#[test]
fn check_ffmpeg_reports_a_missing_ffmpeg() {
    let _guard = spawn_lock();
    let (app, empty) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let resolver = resolver_in(app.path(), &[("PATH", s(empty.path()))]);
    let message = check_ffmpeg(&Config::default(), &resolver)
        .unwrap_err()
        .to_string();
    assert!(
        message.starts_with("cannot find ffmpeg/ffprobe on your system\n"),
        "{message}"
    );
}

// The machine's own ffmpeg, as a whole. Skipped without ffmpeg unless
// SUPERVIEW_REQUIRE_FFMPEG=1, which CI sets.
#[test]
fn check_ffmpeg_reads_the_real_ffmpeg() {
    let _guard = spawn_lock();
    let resolver = ToolResolver::from_process();
    match check_ffmpeg(&Config::default(), &resolver) {
        Ok(got) => {
            assert_ne!(got["version"], "unknown");
            assert!(
                got["encoders"].contains("264"),
                "encoders: {:?}",
                got["encoders"]
            );
        }
        Err(err) => {
            assert!(
                std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() != Ok("1"),
                "no usable ffmpeg although SUPERVIEW_REQUIRE_FFMPEG=1: {err}"
            );
        }
    }
}

// The constructors hand the arguments over one by one.
#[cfg(unix)]
#[test]
fn commands_carry_their_arguments_and_the_injected_path() {
    let (app, empty) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let resolver = resolver_in(app.path(), &[("PATH", s(empty.path()))]);
    let cmd = resolver.ffprobe_command(["-i", "a b;c"]);
    let args: Vec<_> = cmd.get_args().collect();
    assert_eq!(args, ["-i", "a b;c"]);
    assert_eq!(cmd.get_program(), "ffprobe");
    let path = cmd
        .get_envs()
        .find(|(k, _)| *k == "PATH")
        .and_then(|(_, v)| v);
    assert_eq!(path, Some(empty.path().as_os_str()));
}

// F-17: `ffmpeg -version`, `-encoders` and `-hwaccels` are bounded too.
#[cfg(unix)]
#[test]
fn an_ffmpeg_query_that_never_answers_is_an_error() {
    let _guard = crate::process::tests::spawn_lock();
    let tools = crate::probe::tests::fake_tools(&[("ffmpeg", "exec sleep 30")]);
    let resolver = tools
        .resolver
        .with_query_timeout(std::time::Duration::from_secs(1));
    let started = std::time::Instant::now();
    let err = query_ffmpeg(&resolver, &["-version"]).unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

// F-21: every place a tool is looked for asks for the execute bit, as the PATH search does.
#[cfg(unix)]
#[test]
fn a_tool_file_without_the_execute_bit_is_not_used() {
    use std::os::unix::fs::PermissionsExt;
    let (bundled, over) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    for dir in [bundled.path(), over.path()] {
        let path = write_fake_tool(dir, "ffmpeg");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let resolver = resolver_in(
        &bundled.path().join("bin"),
        &[("SUPERVIEW_FFMPEG_DIR", over.path().to_str().unwrap())],
    );
    assert_eq!(resolver.find_bundled("ffmpeg"), None);
    assert_eq!(resolver.resolve("ffmpeg"), PathBuf::from("ffmpeg"));
}

// F-21 (E-22, L-16): a relative override would be the working directory.
#[cfg(unix)]
#[test]
fn a_relative_override_directory_is_ignored() {
    let dir = TempDir::new().unwrap();
    let empty = TempDir::new().unwrap();
    write_fake_tool(dir.path(), "ffmpeg");
    let relative = pathdiff_relative(dir.path());
    let resolver = resolver_in(empty.path(), &[("SUPERVIEW_FFMPEG_DIR", &relative)]);
    assert_eq!(resolver.resolve("ffmpeg"), PathBuf::from("ffmpeg"));
}

/// A relative spelling of `dir` from the working directory, which exists whatever it is.
#[cfg(unix)]
fn pathdiff_relative(dir: &Path) -> String {
    let cwd = std::env::current_dir().unwrap();
    let up = cwd.components().count() - 1;
    let mut text = "../".repeat(up);
    text.push_str(&dir.strip_prefix("/").unwrap_or(dir).to_string_lossy());
    text
}
