//! The file pickers, and what to do when the system has none (phase 6, PR 6.4 of DEC-43). The
//! decisions are here as plain functions over a [`Desktop`], so that the tests choose what every
//! step answers; [`SystemDesktop`] is the real one, used by `host.rs`.
//!
//! The order, on Linux:
//!
//! 1. rfd (the xdg portal, then zenity, both inside rfd);
//! 2. if rfd answered "nothing" **in less than [`PORTAL_FAILURE_WINDOW`]**, our own `zenity`
//!    then `kdialog` (ported from Go's `gui_native_dialog_linux.go` (removed in phase 8; last release with it: v0.2.9): same arguments, exit code 1
//!    is a cancel);
//! 3. if neither exists or works, [`Pick::NoPicker`]: the window shows a text field instead.
//!
//! Why a duration: rfd 0.17 returns `None` for a cancel and for "no portal and no zenity"
//! alike, and only logs the difference. Nobody cancels a dialog in 300 ms, so a quick `None` is
//! read as "the picker could not open". The price is that a user who cancels at once gets the
//! second picker; the gain is that a machine with kdialog only (KDE without a portal) is served.
//!
//! On Windows there is no fallback: rfd's native box alone, and `None` is a cancel (no
//! PowerShell, a deliberate gap with Go, see DEC-43).
//!
//! Every program is run with an argument list, never a shell string, and the only thing that
//! comes back from a picker is a path, which the window validates like any other.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use superview_core::process::background_command;

use crate::format::SUPPORTED_INPUT_EXTENSIONS;

pub const INPUT_TITLE: &str = "Select input video";
pub const OUTPUT_TITLE: &str = "Save output video";
pub const FILTER_NAME: &str = "MP4 Video";
pub const DEFAULT_OUTPUT_NAME: &str = "output.mp4";

/// A "nothing" from rfd that comes back sooner than this is not a user's cancel.
pub const PORTAL_FAILURE_WINDOW: Duration = Duration::from_millis(300);

/// Which file the picker is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Input,
    Output,
}

/// What a picker came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Chosen(PathBuf),
    Cancelled,
    /// No picker could be opened: the window asks for the path in a text field.
    NoPicker,
}

/// How a helper program (zenity, kdialog) ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRun {
    /// It ended normally; what it printed, as the raw bytes of a path, without the final line
    /// break (a path is not always UTF-8; the window refuses one that is not, with a message).
    Selected(OsString),
    /// Exit code 1: the user pressed Cancel or closed the box.
    Cancelled,
    /// Any other end (cannot start, crashed, another exit code): the next picker is tried.
    Failed(String),
}

/// What [`pick`] needs from the machine.
pub trait Desktop {
    /// rfd's picker. `None` is a cancel or a failure, rfd does not say which.
    fn portal(&self, kind: Kind) -> Option<PathBuf>;
    /// Whether an earlier quick `None` showed that rfd cannot open a picker here: it is then
    /// not asked again for the rest of the session.
    fn portal_unusable(&self) -> bool;
    /// Remembers that rfd cannot open a picker here.
    fn mark_portal_unusable(&self);
    /// Whether our own zenity and kdialog are tried after a quick `None` (Linux only).
    fn tools_fallback(&self) -> bool;
    /// Where `name` is, if it is installed.
    fn find_tool(&self, name: &str) -> Option<PathBuf>;
    /// Runs a picker program and waits for it to close.
    fn run_tool(&self, program: &Path, args: &[String]) -> ToolRun;
    /// A monotonic clock.
    fn elapsed(&self) -> Duration;
}

/// The helper pickers, in the order they are tried (Go: zenity, GTK and the most common, first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Zenity,
    Kdialog,
}

impl Tool {
    const ORDER: [Self; 2] = [Self::Zenity, Self::Kdialog];

    fn program(self) -> &'static str {
        match self {
            Self::Zenity => "zenity",
            Self::Kdialog => "kdialog",
        }
    }
}

/// The extensions rfd's filter takes, without the dot, from the one list of what the product
/// reads.
pub fn filter_extensions() -> Vec<&'static str> {
    SUPPORTED_INPUT_EXTENSIONS
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .collect()
}

/// `*.mp4 *.MP4`: the same list as glob patterns, for zenity and kdialog.
fn filter_globs() -> String {
    filter_extensions()
        .iter()
        .map(|ext| format!("*.{ext}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The arguments of `tool` for `kind` (Go: `chooseInputFileNative`, `chooseOutputFileNative`).
/// Kdialog's filter also lists `*.MP4`, which Go's left out: the filter is case-sensitive.
fn tool_args(tool: Tool, kind: Kind) -> Vec<String> {
    let globs = filter_globs();
    let args: Vec<String> = match (tool, kind) {
        (Tool::Zenity, Kind::Input) => vec![
            "--file-selection".into(),
            format!("--title={INPUT_TITLE}"),
            format!("--file-filter={FILTER_NAME} | {globs}"),
        ],
        (Tool::Zenity, Kind::Output) => vec![
            "--file-selection".into(),
            "--save".into(),
            format!("--title={OUTPUT_TITLE}"),
            format!("--filename={DEFAULT_OUTPUT_NAME}"),
            "--confirm-overwrite".into(),
        ],
        (Tool::Kdialog, Kind::Input) => vec![
            "--getopenfilename".into(),
            ".".into(),
            format!("{globs}|{FILTER_NAME}"),
        ],
        (Tool::Kdialog, Kind::Output) => vec![
            "--getsavefilename".into(),
            DEFAULT_OUTPUT_NAME.into(),
            format!("{globs}|{FILTER_NAME}"),
        ],
    };
    args
}

/// Opens the right picker for `kind` and returns what the user did.
///
/// Blocks until the picker closes: call it off the UI thread.
pub fn pick(desktop: &dyn Desktop, kind: Kind) -> Pick {
    // After one quick failure the fallback is sticky: rfd is not asked again, so a second
    // click does not wait for a portal that is known not to answer.
    if !desktop.portal_unusable() {
        let started = desktop.elapsed();
        if let Some(path) = desktop.portal(kind) {
            return Pick::Chosen(path);
        }
        if !desktop.tools_fallback() {
            return Pick::Cancelled;
        }
        let took = desktop.elapsed().saturating_sub(started);
        if took >= PORTAL_FAILURE_WINDOW {
            log::debug!("The file picker was closed: took_ms={}", took.as_millis());
            return Pick::Cancelled;
        }
        log::info!(
            "The file picker returned nothing at once, trying zenity and kdialog from now on: took_ms={}",
            took.as_millis()
        );
        desktop.mark_portal_unusable();
    }
    for tool in Tool::ORDER {
        let Some(program) = desktop.find_tool(tool.program()) else {
            continue;
        };
        match desktop.run_tool(&program, &tool_args(tool, kind)) {
            ToolRun::Selected(path) => {
                return if path.is_empty() {
                    Pick::Cancelled
                } else {
                    Pick::Chosen(PathBuf::from(path))
                };
            }
            ToolRun::Cancelled => return Pick::Cancelled,
            ToolRun::Failed(why) => {
                log::warn!(
                    "The file picker program failed: tool={} error={why}",
                    tool.program()
                );
            }
        }
    }
    log::warn!("No file picker could be opened, asking for the path in the window");
    Pick::NoPicker
}

/// What the user typed in the text field, or why it cannot be used. The messages are shown as
/// they are. Unlike a picker's answer, which is kept byte for byte, the typed text is trimmed:
/// the user types it, and a stray space or the newline of a paste is not part of the name.
pub fn parse_typed_path(text: &str) -> Result<PathBuf, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Please type the path of a file.");
    }
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err("Please type the full path of the file, from the root of the disk.");
    }
    Ok(path)
}

/// The first `name` found in the `path_var` directories that is a file one can run.
pub fn find_in_path(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let path_var = path_var?;
    std::env::split_paths(path_var)
        // A relative entry (or an empty one, which means "here") would run whatever program
        // of that name sits in the current directory.
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| is_runnable(candidate))
}

#[cfg(unix)]
fn is_runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

/// The bytes of a path as the system's own string type, untouched (lossy only where paths are
/// not bytes, on Windows, where the fallback is never used).
fn os_from_bytes(bytes: &[u8]) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes.to_vec())
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Reads how a picker program ended.
fn classify_exit(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> ToolRun {
    match code {
        Some(0) => {
            // Only the line break the program adds: a name may begin or end with a space.
            let end = stdout
                .iter()
                .rposition(|b| !matches!(b, b'\n' | b'\r'))
                .map_or(0, |last| last + 1);
            ToolRun::Selected(os_from_bytes(&stdout[..end]))
        }
        Some(1) => ToolRun::Cancelled,
        other => ToolRun::Failed(format!(
            "exit code {other:?}: {}",
            String::from_utf8_lossy(stderr).trim()
        )),
    }
}

/// Set by the first quick failure of rfd, for the rest of the session.
static PORTAL_UNUSABLE: AtomicBool = AtomicBool::new(false);

/// The real machine.
pub struct SystemDesktop;

impl Desktop for SystemDesktop {
    fn portal(&self, kind: Kind) -> Option<PathBuf> {
        let dialog = rfd::FileDialog::new().add_filter(FILTER_NAME, &filter_extensions());
        match kind {
            Kind::Input => dialog.set_title(INPUT_TITLE).pick_file(),
            Kind::Output => dialog
                .set_title(OUTPUT_TITLE)
                .set_file_name(DEFAULT_OUTPUT_NAME)
                .save_file(),
        }
    }

    fn portal_unusable(&self) -> bool {
        PORTAL_UNUSABLE.load(Ordering::Relaxed)
    }

    fn mark_portal_unusable(&self) {
        PORTAL_UNUSABLE.store(true, Ordering::Relaxed);
    }

    fn tools_fallback(&self) -> bool {
        cfg!(target_os = "linux")
    }

    fn find_tool(&self, name: &str) -> Option<PathBuf> {
        find_in_path(name, std::env::var_os("PATH").as_deref())
    }

    fn run_tool(&self, program: &Path, args: &[String]) -> ToolRun {
        let mut command = background_command(program, args);
        command.stdin(Stdio::null());
        match command.output() {
            Ok(output) => classify_exit(output.status.code(), &output.stdout, &output.stderr),
            Err(e) => ToolRun::Failed(format!("cannot start: {e}")),
        }
    }

    fn elapsed(&self) -> Duration {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        EPOCH.get_or_init(Instant::now).elapsed()
    }
}

#[cfg(test)]
mod tests;
