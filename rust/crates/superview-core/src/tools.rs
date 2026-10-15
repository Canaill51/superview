//! Finding and invoking ffmpeg/ffprobe, and reading what the binary says about
//! itself. Mirrors `internal/common/tools.go`.
//!
//! The search order is the contract (CONTRATS.md): `SUPERVIEW_FFMPEG_DIR`, the
//! copy bundled next to the executable or under `../lib/superview`, `PATH`,
//! then on Windows the winget and scoop directories. The bundled copy beats
//! `PATH` on purpose: the FFmpeg a machine has installed is the variable this
//! program cannot control (U-03).
//!
//! Deliberate differences from the Go code (exhaustive):
//! - There is no global cache. [`ToolResolver`] owns its cache and receives the
//!   environment, the executable's directory and the "usable file" test, so
//!   tests run in parallel without touching the process environment (L-114).
//!   [`ToolResolver::reset_cache`] is `ResetToolResolutionCache`.
//! - `PATH` is searched here, by [`lookup_in_path`], not by `exec.LookPath`:
//!   the environment is injected. It ignores a relative `PATH` entry and goes
//!   on, where Go's `LookPath` stops with an error on one. On Windows it
//!   tries each `PATHEXT` extension (default `.COM;.EXE;.BAT;.CMD`).
//! - The command constructors pass the injected `PATH` on to the child, so a
//!   tool left unresolved is looked up in the same `PATH` that was searched.
//! - The command constructors hand the child a `PATH` reduced to its absolute
//!   entries (never empty: see [`absolute_path_entries`]), so a bare name can
//!   never be found through a relative entry.
//! - Environment values are `OsString`, read with `var_os`: Go's strings are
//!   bytes too, and a non-UTF-8 `PATH` must not blind the search. Only the
//!   trimming of the override and `PATHEXT` need text; a non-UTF-8 override
//!   is used as it is, a non-UTF-8 `PATHEXT` is replaced by the default.
//! - TODO PR 5: `lookup_in_path` accepts `.BAT` and `.CMD` on Windows, as Go
//!   does, and spawning a batch file goes through `cmd.exe`, which may leave
//!   an orphan on cancellation and re-parses arguments. Restrict to
//!   `.exe`/`.com`? Not decided here.
//! - `usable_tool_file` answers with a `bool`, not the path or `""`.
//! - Unlike the Go test helper, a failing query reports "exit status: N" (Rust's
//!   `ExitStatus` text); only the leading text of a message is comparable.
//! - `newFFmpegCommandContext` (the encoder probe's deadline) arrives with the
//!   probe, phase 4 PR 5.
//! - The Windows branch of the lookup is a pure function over the injected
//!   environment, like Go's, so it is tested on every OS.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::config::Config;
use crate::error::Error;
use crate::process::{background_command, combined_output};

/// Names a directory holding ffmpeg and ffprobe, and wins over everything else.
///
/// The escape hatch for the bundled build: if the pinned FFmpeg is wrong for a
/// machine, the user needs a way out that does not wait for a release.
pub const TOOL_DIR_ENV: &str = "SUPERVIEW_FFMPEG_DIR";

/// An environment lookup (`std::env::var_os(key)` for the real one): values
/// are `OsString`, so a non-UTF-8 `PATH` or override does not blind the search.
pub type EnvFn = dyn Fn(&str) -> Option<OsString> + Send + Sync;

/// Finds ffmpeg and ffprobe and remembers what it found.
pub struct ToolResolver {
    env: Box<EnvFn>,
    exe_dir: Option<PathBuf>,
    usable: fn(&Path) -> bool,
    cache: Mutex<HashMap<String, PathBuf>>,
}

impl ToolResolver {
    /// `env` is the environment lookup; `exe_dir` the directory of the running
    /// executable (`None` when it cannot be worked out: nothing is bundled).
    pub fn new(
        env: impl Fn(&str) -> Option<OsString> + Send + Sync + 'static,
        exe_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            env: Box::new(env),
            exe_dir,
            usable: usable_tool_file,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The real process: its environment and its executable's directory.
    pub fn from_process() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::new(|key| std::env::var_os(key), exe_dir)
    }

    /// Replaces the "is this file usable" test.
    #[must_use]
    pub fn with_usable(mut self, usable: fn(&Path) -> bool) -> Self {
        self.usable = usable;
        self
    }

    /// Forgets the resolved paths so the next call searches again. Call it
    /// when the user asks to retry after installing the tools.
    pub fn reset_cache(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn remember(&self, tool: &str, path: PathBuf) -> PathBuf {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(tool.to_string(), path.clone());
        path
    }

    /// Finds `tool` ("ffmpeg" or "ffprobe") in the contract's order. When
    /// nothing is found the bare name comes back, deliberately not cached: the
    /// GUI shows an install link, so the user often installs it while the app
    /// runs, and a cached miss would force a restart.
    pub fn resolve(&self, tool: &str) -> PathBuf {
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(tool)
        {
            return cached.clone();
        }

        if let Some(dir) = (self.env)(TOOL_DIR_ENV)
            .map(|d| match d.to_str() {
                Some(text) => PathBuf::from(text.trim()),
                None => PathBuf::from(d),
            })
            .filter(|d| !d.as_os_str().is_empty())
        {
            let path = dir.join(tool_file_name(tool));
            if (self.usable)(&path) {
                return self.remember(tool, path);
            }
            // Not cached and not fatal: an override pointing at nothing is a
            // mistake worth reporting, not a reason to refuse to work.
            log::warn!(
                "ignoring {TOOL_DIR_ENV}: it does not contain the tool (dir={}, tool={tool})",
                dir.display()
            );
        }

        if let Some(path) = self.find_bundled(tool) {
            return self.remember(tool, path);
        }
        if let Some(path) = lookup_in_path(tool, &*self.env) {
            return self.remember(tool, path);
        }
        if cfg!(windows)
            && let Some(path) = find_windows_tool_binary(&*self.env, &self.usable, tool)
        {
            return self.remember(tool, path);
        }
        PathBuf::from(tool)
    }

    /// The copy shipped with the release: next to the executable (Windows, a
    /// hand-unpacked tarball) or under `../lib/superview` (the Linux package,
    /// which cannot put an `ffmpeg` in `/usr/local/bin` without shadowing the
    /// system's for every program).
    pub fn find_bundled(&self, tool: &str) -> Option<PathBuf> {
        let dir = self.exe_dir.as_deref()?;
        let name = tool_file_name(tool);
        // `..` is taken lexically (`parent`), as Go's filepath.Join cleans it.
        let lib = dir.parent().unwrap_or(dir).join("lib").join("superview");
        [dir.join(&name), lib.join(&name)]
            .into_iter()
            .find(|candidate| (self.usable)(candidate))
    }

    /// An ffmpeg command, resolved now.
    pub fn ffmpeg_command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command("ffmpeg", args)
    }

    /// An ffprobe command, resolved now.
    pub fn ffprobe_command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command("ffprobe", args)
    }

    fn command<I, S>(&self, tool: &str, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = background_command(self.resolve(tool), args);
        cmd.env(
            "PATH",
            absolute_path_entries((self.env)("PATH").unwrap_or_default()),
        );
        cmd
    }
}

/// The on-disk name of a tool for this platform.
pub fn tool_file_name(tool: &str) -> String {
    if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    }
}

/// Whether `path` names a regular file (anything but a directory).
///
/// Not ceremony: a directory called "ffmpeg" next to the executable would
/// otherwise be handed to the spawner, which fails at encoding time with a
/// permission error rather than here with a fallback.
pub fn usable_tool_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| !m.is_dir())
}

/// A regular file the system would run (the execute bit, on Unix).
fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

/// `path` reduced to its absolute entries, for the child's `PATH`.
///
/// A tool left unresolved is a bare name, which the spawner looks up in the
/// child's `PATH`; a relative entry (`.`, an empty one, `x/bin`) would find a
/// planted program in the working directory, the lookup Go's `LookPath` refuses
/// with `ErrDot` (L-16). When nothing absolute is left the result is a
/// directory that does not exist, never `""`: `execvp` reads an empty `PATH` as
/// "the current directory".
fn absolute_path_entries(path: OsString) -> OsString {
    let kept: Vec<PathBuf> = std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .collect();
    let nowhere = if cfg!(windows) {
        r"C:\superview-no-such-dir"
    } else {
        "/superview-no-such-dir"
    };
    std::env::join_paths(kept)
        .ok()
        .filter(|joined| !joined.is_empty())
        .unwrap_or_else(|| OsString::from(nowhere))
}

/// `exec.LookPath` over the injected environment.
fn lookup_in_path(tool: &str, env: &EnvFn) -> Option<PathBuf> {
    let path = env("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        let ext = env("PATHEXT")
            .and_then(|e| e.into_string().ok())
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
        ext.split(';')
            .filter(|e| !e.is_empty())
            .map(|e| format!("{tool}{e}"))
            .collect()
    } else {
        vec![tool.to_string()]
    };
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| is_executable_file(candidate))
}

/// The directories winget, scoop and manual installs use, then a walk of
/// winget's `Packages` tree. Reads only the injected environment and has no
/// OS gate of its own, so it runs under test on every OS.
///
/// A candidate is built only from variables that are set: joining an empty one
/// yields a RELATIVE path, which would resolve against the working directory
/// -- a way to run a planted `ffmpeg.exe` (E-22).
pub fn find_windows_tool_binary(
    env: &EnvFn,
    usable: &dyn Fn(&Path) -> bool,
    tool: &str,
) -> Option<PathBuf> {
    let exe = format!("{tool}.exe");
    let var = |key: &str| env(key).filter(|v| !v.is_empty());
    let local_app_data = var("LOCALAPPDATA");
    let program_files = var("ProgramFiles");
    let user_profile = var("USERPROFILE");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(local) = &local_app_data {
        candidates.push(
            Path::new(local)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join(&exe),
        );
    }
    if let Some(files) = &program_files {
        candidates.push(Path::new(files).join("ffmpeg").join("bin").join(&exe));
        candidates.push(Path::new(files).join("FFmpeg").join("bin").join(&exe));
    }
    if let Some(profile) = &user_profile {
        let mut scoop = PathBuf::from(profile);
        scoop.extend(["scoop", "apps", "ffmpeg", "current", "bin"]);
        candidates.push(scoop.join(&exe));
    }
    if let Some(found) = candidates.into_iter().find(|c| usable(c)) {
        return Some(found);
    }

    // Winget often extracts under LOCALAPPDATA/Microsoft/WinGet/Packages.
    let package_root = Path::new(&local_app_data?)
        .join("Microsoft")
        .join("WinGet")
        .join("Packages");
    let lower_exe = exe.to_lowercase();
    sorted_entries(&package_root)
        .into_iter()
        .filter(|(_, is_dir)| *is_dir)
        .filter(|(path, _)| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("ffmpeg"))
        })
        .find_map(|(root, _)| walk_for(&root, &lower_exe))
}

/// The entries of `dir` by name, with whether each is a directory (symlinks
/// are not followed, as in Go's `WalkDir`). Empty when unreadable.
fn sorted_entries(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(PathBuf, bool)> = read
        .filter_map(Result::ok)
        .map(|e| {
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            (e.path(), is_dir)
        })
        .collect();
    entries.sort();
    entries
}

/// The first file named `lower_exe` (case-insensitively) under `root`,
/// depth-first in name order.
fn walk_for(root: &Path, lower_exe: &str) -> Option<PathBuf> {
    for (path, is_dir) in sorted_entries(root) {
        if is_dir {
            if let Some(found) = walk_for(&path, lower_exe) {
                return Some(found);
            }
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase() == lower_exe)
        {
            return Some(path);
        }
    }
    None
}

/// Runs `args` on the resolved ffmpeg; `Err` carries the cause's text.
fn query_ffmpeg(resolver: &ToolResolver, args: &[&str]) -> Result<String, String> {
    let (status, output) =
        combined_output(resolver.ffmpeg_command(args)).map_err(|e| e.to_string())?;
    if status.success() {
        Ok(output)
    } else {
        Err(status.to_string())
    }
}

/// Discovers the installed ffmpeg version, hardware accelerators and the
/// H.264/H.265 encoders listed in `cfg.encoder_codecs`.
///
/// Returns a map with the keys "version", "accels" (comma-separated) and
/// "encoders" (comma-separated); the last two are present, possibly empty.
/// Call it before encoding to verify ffmpeg is installed.
pub fn check_ffmpeg(
    cfg: &Config,
    resolver: &ToolResolver,
) -> Result<HashMap<String, String>, Error> {
    let mut ret = HashMap::new();

    let version = query_ffmpeg(resolver, &["-version"]).map_err(|_| {
        Error::Plain(
            "cannot find ffmpeg/ffprobe on your system\nmake sure to install it first: https://github.com/Canaill51/superview?tab=readme-ov-file#download-and-install"
                .to_string(),
        )
    })?;
    ret.insert("version".to_string(), parse_ffmpeg_version(&version));

    // Skip the first line, the header.
    let accels = query_ffmpeg(resolver, &["-hwaccels", "-hide_banner"]).map_err(|cause| {
        Error::Plain(format!(
            "failed to query ffmpeg hardware accelerators: {cause}"
        ))
    })?;
    ret.insert("accels".to_string(), parse_ffmpeg_accels(&accels));

    let encoders = query_ffmpeg(resolver, &["-encoders", "-hide_banner"])
        .map_err(|cause| Error::Plain(format!("failed to query ffmpeg encoders: {cause}")))?;
    ret.insert(
        "encoders".to_string(),
        parse_ffmpeg_encoders(&encoders, &cfg.encoder_codecs).join(","),
    );
    Ok(ret)
}

/// The accelerator names of `ffmpeg -hwaccels`, comma-separated: every
/// non-empty line after the first, which is the header.
pub fn parse_ffmpeg_accels(output: &str) -> String {
    let output = output.replace("\r\n", "\n");
    let lines: Vec<&str> = output
        .split('\n')
        .skip(1)
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(",").trim_matches(',').to_string()
}

/// The version token of "ffmpeg version N.N.N ...".
///
/// Never panics on an unexpected layout: a custom, patched or localised build
/// must degrade to "unknown" rather than take the app down at startup.
pub fn parse_ffmpeg_version(output: &str) -> String {
    let line = output.split('\n').next().unwrap_or("");
    line.split_whitespace()
        .nth(2)
        .unwrap_or("unknown")
        .to_string()
}

/// The video encoder names, in listing order, matching any of the `codecs`
/// fragments in the output of `ffmpeg -encoders`.
///
/// The listing is "<header>\n ------\n <flags> <name> <description>". This
/// scans for the "------" separator rather than skipping a fixed number of
/// header lines, and accepts every well-formed entry when it is absent.
pub fn parse_ffmpeg_encoders(output: &str, codecs: &[String]) -> Vec<String> {
    let output = output.replace("\r\n", "\n");
    let lines: Vec<&str> = output.split('\n').collect();
    let start = lines
        .iter()
        .position(|line| line.contains("------"))
        .map_or(0, |i| i + 1);

    lines[start..]
        .iter()
        // A video encoder row starts with a space then the "V" capability flag.
        .filter(|line| line.starts_with(" V"))
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|name| codecs.iter().any(|codec| name.contains(codec.as_str())))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
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
    /// as its whole environment. Go: withFakeExecutableIn.
    fn resolver_in(exe_dir: &Path, pairs: &[(&str, &str)]) -> ToolResolver {
        ToolResolver::new(env_of(pairs), Some(exe_dir.to_path_buf()))
    }

    /// Drops a file where a tool binary would be and returns its path.
    /// Go: writeFakeTool.
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

    // Go: TestResolveToolBinary_BundledIsPreferredOverPath -- the decision the
    // whole bundling exists to make (U-03).
    #[test]
    fn resolve_prefers_the_bundled_copy_over_path() {
        let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let bundled = write_fake_tool(app.path(), "ffmpeg");
        write_fake_tool(on_path.path(), "ffmpeg");
        let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
        assert_eq!(resolver.resolve("ffmpeg"), bundled);
    }

    // Go: TestResolveToolBinary_FindsTheLinuxInstallLayout
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

    // Go: TestResolveToolBinary_OverrideWinsOverTheBundledCopy
    #[test]
    fn resolve_override_wins_over_the_bundled_copy() {
        let (app, over) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        write_fake_tool(app.path(), "ffmpeg");
        let chosen = write_fake_tool(over.path(), "ffmpeg");
        let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, s(over.path()))]);
        assert_eq!(resolver.resolve("ffmpeg"), chosen);
    }

    // No Go test: the variable is trimmed, as Go trims it.
    #[test]
    fn resolve_trims_the_override() {
        let (app, over) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let chosen = write_fake_tool(over.path(), "ffmpeg");
        let padded = format!("  {}\t", s(over.path()));
        let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, &padded)]);
        assert_eq!(resolver.resolve("ffmpeg"), chosen);
    }

    // Go: TestResolveToolBinary_BogusOverrideFallsThrough
    #[test]
    fn resolve_bogus_override_falls_through() {
        let app = TempDir::new().unwrap();
        let bundled = write_fake_tool(app.path(), "ffmpeg");
        let nothing = app.path().join("nothing-here");
        let resolver = resolver_in(app.path(), &[(TOOL_DIR_ENV, s(&nothing))]);
        assert_eq!(resolver.resolve("ffmpeg"), bundled);
    }

    // Go: TestResolveToolBinary_FallsBackToPathWithNothingBundled. Skipped on
    // Windows like Go: winget and scoop directories would answer first.
    #[cfg(not(windows))]
    #[test]
    fn resolve_falls_back_to_path_with_nothing_bundled() {
        let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let found = write_fake_tool(on_path.path(), "ffmpeg");
        let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
        assert_eq!(resolver.resolve("ffmpeg"), found);
    }

    // No Go test (Go leaves it to exec.LookPath): a file without the execute
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

    // No Go test (L-16: Go's LookPath refuses a relative entry with ErrDot).
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

    // No Go test: the filter on its own. Never empty, which execvp would read as
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

    // No Go test: a PATH entry that is not UTF-8 still searches, and so does
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

    // No Go test: a directory named like the tool on PATH is not the tool
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

    // No Go test: a relative PATH entry is skipped, never resolved against the
    // working directory (the divergence from LookPath, listed in the header).
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

    // Go: TestResolveToolBinary_IgnoresADirectoryNamedLikeTheTool
    #[cfg(not(windows))]
    #[test]
    fn resolve_ignores_a_directory_named_like_the_tool() {
        let (app, on_path) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        fs::create_dir(app.path().join(tool_file_name("ffmpeg"))).unwrap();
        let found = write_fake_tool(on_path.path(), "ffmpeg");
        let resolver = resolver_in(app.path(), &[("PATH", s(on_path.path()))]);
        assert_eq!(resolver.resolve("ffmpeg"), found);
    }

    // No Go test: usable_tool_file is the regular-file check on its own.
    #[test]
    fn usable_tool_file_wants_a_regular_file() {
        let dir = TempDir::new().unwrap();
        let file = write_fake_tool(dir.path(), "ffmpeg");
        assert!(usable_tool_file(&file));
        assert!(!usable_tool_file(dir.path()));
        assert!(!usable_tool_file(&dir.path().join("missing")));
    }

    // Go: TestFindBundledToolBinary_SurvivesAnUnknownExecutablePath
    #[test]
    fn find_bundled_survives_an_unknown_executable_path() {
        let resolver = ToolResolver::new(env_of(&[]), None);
        assert_eq!(resolver.find_bundled("ffmpeg"), None);
    }

    // Go: TestResolveToolBinary_DoesNotCacheFailures. The Go test peeks into
    // the cache; this one shows the effect: a tool installed afterwards is found.
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

    // Go: TestResolveToolBinary_CachesSuccess, on a stand-in rather than a real
    // ffmpeg, plus reset_cache (ResetToolResolutionCache).
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

    // No Go test: the resolver is shared between threads (the GUI's and the
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
    // Go: wingetLayout.
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

        /// Go: writeWindowsTool.
        fn write_tool(&self, tool: &str, parts: &[&str]) -> PathBuf {
            let mut dir = self.local.path().to_path_buf();
            dir.extend(parts);
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join(format!("{tool}.exe"));
            fs::write(&path, "stand-in").unwrap();
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

    // Go: TestFindWindowsToolBinary_FindsAWingetPackagesTree / package tree
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

    // Go: TestFindWindowsToolBinary_FindsAWingetPackagesTree / package not
    // named after ffmpeg
    #[test]
    fn windows_walk_stays_inside_ffmpeg_packages() {
        let layout = winget_layout();
        layout.write_tool("ffmpeg", &package_parts(&["SomeVendor.MediaKit_x", "bin"]));
        assert_eq!(layout.find("ffmpeg"), None);
    }

    // Go: TestFindWindowsToolBinary_FindsAWingetPackagesTree / Links shim wins
    #[test]
    fn windows_links_shim_wins_over_the_package_tree() {
        let layout = winget_layout();
        layout.write_tool("ffmpeg", &package_parts(&["Gyan.FFmpeg_x", "bin"]));
        let link = layout.write_tool("ffmpeg", &["Microsoft", "WinGet", "Links"]);
        assert_eq!(layout.find("ffmpeg"), Some(link));
    }

    // No Go test: the walk matches the file name without regard to case.
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

    // Go: TestFindWindowsToolBinary_IgnoresRelativeCandidates. Go plants a file
    // in the working directory and chdirs; Rust tests must not change the
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

    // Go: TestParseFFmpegVersion
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

    // Go: TestParseFFmpegEncoders
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

    // Go: TestParseFFmpegEncoders_MalformedInputDoesNotPanic (each of these used
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

    // Go: TestParseFFmpegEncoders_NoSeparatorStillParses
    #[test]
    fn parse_ffmpeg_encoders_without_separator_still_parses() {
        let listing =
            " V....D libx264              H.264 / AVC\n V....D libx265              H.265\n";
        assert_eq!(
            parse_ffmpeg_encoders(listing, &codecs(&["264"])),
            ["libx264"]
        );
    }

    // No Go test: the header is skipped whatever it holds, and CRLF line ends
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

    // No Go test: what precedes the separator is a banner, not encoders, even
    // if a line of it looks like a row.
    #[test]
    fn parse_ffmpeg_encoders_ignores_rows_before_the_separator() {
        let listing = "Encoders:\n V..... libx264_banner\n ------\n V....D libx265  HEVC\n";
        assert_eq!(
            parse_ffmpeg_encoders(listing, &codecs(&["264", "265"])),
            ["libx265"]
        );
    }

    // No Go test: only rows flagged "V" are video encoders, whatever the name.
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
    /// override is unset. Go: fakeCheckFFmpeg.
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
        let resolver =
            ToolResolver::new(env_of(&[("PATH", &path)]), Some(empty.path().to_path_buf()));
        FakeFfmpeg {
            _dir: dir,
            _empty: empty,
            resolver,
        }
    }

    // Go: TestCheckFfmpeg_ReportsAFailingEncodersQuery / control: the same
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
        assert_eq!(got.len(), 3, "the map keys are the Go ones: {got:?}");
    }

    // Go: TestCheckFfmpeg_ReportsAFailingEncodersQuery / "-encoders fails" and
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

    // Go: TestCheckFfmpeg_ReportsAFailingEncodersQuery, -version half, which
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

    // Go: withoutFFmpegOnPath (health_report_test.go). Not on Windows, where
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

    // No Go test of this exact shape (Go's integration tests call CheckFfmpeg
    // on the machine's ffmpeg). Skipped without ffmpeg unless
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

    // No Go test: the constructors hand the arguments over one by one.
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
}
