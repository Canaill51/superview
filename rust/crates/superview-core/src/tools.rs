//! Finding and invoking ffmpeg/ffprobe, and reading what the binary says about
//! itself. Mirrors Go's `internal/common/tools.go` (removed in phase 8; last release with it: v0.2.9).
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
//!   tries each `PATHEXT` extension that names a program (default `.COM;.EXE`).
//! - The command constructors pass the injected `PATH` on to the child, so a
//!   tool left unresolved is looked up in the same `PATH` that was searched.
//! - The command constructors hand the child a `PATH` reduced to its absolute
//!   entries (never empty: see [`absolute_path_entries`]), so a bare name can
//!   never be found through a relative entry.
//! - Environment values are `OsString`, read with `var_os`: Go's strings are
//!   bytes too, and a non-UTF-8 `PATH` must not blind the search. Only the
//!   trimming of the override and `PATHEXT` need text; a non-UTF-8 override
//!   is used as it is, a non-UTF-8 `PATHEXT` is replaced by the default.
//! - `lookup_in_path` accepts only `.COM` and `.EXE` on Windows, where Go also
//!   takes `.BAT` and `.CMD`: a batch file runs through `cmd.exe`, so a
//!   cancellation would kill `cmd.exe` and leave ffmpeg running, against the
//!   "cancellation leaves nothing behind" contract. Decided by the user on
//!   2026-10-17. Only the tests' stand-ins on Windows are batch files, and
//!   they opt in with `ToolResolver::with_batch_stand_ins`.
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
    /// Windows only: also accept `.bat`/`.cmd` on `PATH`. Off, except for test stand-ins.
    batch_files: bool,
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
            batch_files: false,
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

    /// Lets the `PATH` search return `.bat`/`.cmd` files too. The Windows tests'
    /// stand-in tools are batch files; a real ffmpeg is an `.exe`, and a batch
    /// file would leave ffmpeg running on a cancellation (it dies with `cmd.exe`).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_batch_stand_ins(mut self) -> Self {
        self.batch_files = true;
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
        if let Some(path) = lookup_in_path(tool, &*self.env, self.batch_files) {
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

/// The `PATHEXT` extensions the search tries: those that name a program (`.com`,
/// `.exe`, in any case), plus `.bat`/`.cmd` when `batch_files`. An unset or empty
/// `PATHEXT` is `.COM;.EXE` (and `.BAT;.CMD` with `batch_files`).
fn program_extensions(pathext: Option<&str>, batch_files: bool) -> Vec<String> {
    pathext
        .filter(|e| !e.is_empty())
        .unwrap_or(if batch_files {
            ".COM;.EXE;.BAT;.CMD"
        } else {
            ".COM;.EXE"
        })
        .split(';')
        .filter(|e| !e.is_empty())
        .filter(|e| {
            ["com", "exe"]
                .iter()
                .chain(if batch_files {
                    &["bat", "cmd"][..]
                } else {
                    &[][..]
                })
                .any(|ok| {
                    e.strip_prefix('.')
                        .is_some_and(|x| x.eq_ignore_ascii_case(ok))
                })
        })
        .map(str::to_string)
        .collect()
}

/// `exec.LookPath` over the injected environment.
fn lookup_in_path(tool: &str, env: &EnvFn, batch_files: bool) -> Option<PathBuf> {
    let path = env("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        let pathext = env("PATHEXT").and_then(|e| e.into_string().ok());
        program_extensions(pathext.as_deref(), batch_files)
            .into_iter()
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
mod tests;
