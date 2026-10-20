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
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A desktop that answers from a script and moves its own clock.
    struct Fake {
        /// What rfd answers, and how long it takes.
        portal_answer: Option<PathBuf>,
        portal_takes: Duration,
        fallback: bool,
        /// The tools that are installed, with what each does when run.
        installed: Vec<(&'static str, ToolRun)>,
        now: Cell<Duration>,
        /// Every program run, with its arguments, in order.
        ran: RefCell<Vec<(String, Vec<String>)>>,
        portal_calls: Cell<u32>,
        portal_unusable: Cell<bool>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                portal_answer: None,
                portal_takes: Duration::ZERO,
                fallback: true,
                installed: Vec::new(),
                now: Cell::new(Duration::from_secs(1000)),
                ran: RefCell::new(Vec::new()),
                portal_calls: Cell::new(0),
                portal_unusable: Cell::new(false),
            }
        }

        fn with_tool(mut self, name: &'static str, run: ToolRun) -> Self {
            self.installed.push((name, run));
            self
        }

        fn after(mut self, ms: u64) -> Self {
            self.portal_takes = Duration::from_millis(ms);
            self
        }

        fn ran_names(&self) -> Vec<String> {
            self.ran.borrow().iter().map(|(n, _)| n.clone()).collect()
        }
    }

    impl Desktop for Fake {
        fn portal(&self, _: Kind) -> Option<PathBuf> {
            self.portal_calls.set(self.portal_calls.get() + 1);
            self.now.set(self.now.get() + self.portal_takes);
            self.portal_answer.clone()
        }

        fn portal_unusable(&self) -> bool {
            self.portal_unusable.get()
        }

        fn mark_portal_unusable(&self) {
            self.portal_unusable.set(true);
        }

        fn tools_fallback(&self) -> bool {
            self.fallback
        }

        fn find_tool(&self, name: &str) -> Option<PathBuf> {
            self.installed
                .iter()
                .any(|(n, _)| *n == name)
                .then(|| PathBuf::from("/usr/bin").join(name))
        }

        fn run_tool(&self, program: &Path, args: &[String]) -> ToolRun {
            let name = program.file_name().unwrap().to_string_lossy().into_owned();
            self.ran.borrow_mut().push((name.clone(), args.to_vec()));
            self.installed
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, run)| run.clone())
                .expect("only an installed tool is run")
        }

        fn elapsed(&self) -> Duration {
            self.now.get()
        }
    }

    fn chosen(path: &str) -> ToolRun {
        ToolRun::Selected(OsString::from(path))
    }

    #[test]
    fn the_filter_comes_from_the_supported_extensions() {
        assert_eq!(filter_extensions(), ["mp4", "MP4"]);
        assert_eq!(filter_globs(), "*.mp4 *.MP4");
    }

    #[test]
    fn an_answer_from_the_portal_is_taken_without_asking_anything_else() {
        let mut desktop = Fake::new().with_tool("zenity", chosen("/z.mp4"));
        desktop.portal_answer = Some(PathBuf::from("/p.mp4"));
        assert_eq!(
            pick(&desktop, Kind::Input),
            Pick::Chosen(PathBuf::from("/p.mp4"))
        );
        assert!(desktop.ran_names().is_empty());
    }

    #[test]
    fn a_nothing_after_a_long_time_is_a_cancel_and_opens_no_second_picker() {
        for ms in [300, 301, 5000] {
            let desktop = Fake::new().after(ms).with_tool("zenity", chosen("/z.mp4"));
            assert_eq!(
                pick(&desktop, Kind::Input),
                Pick::Cancelled,
                "after {ms} ms"
            );
            assert!(desktop.ran_names().is_empty(), "after {ms} ms");
            assert_eq!(desktop.portal_calls.get(), 1);
        }
    }

    #[test]
    fn a_nothing_in_under_300_ms_means_the_picker_could_not_open() {
        for ms in [0, 1, 299] {
            let desktop = Fake::new().after(ms).with_tool("zenity", chosen("/z.mp4"));
            assert_eq!(
                pick(&desktop, Kind::Input),
                Pick::Chosen(PathBuf::from("/z.mp4")),
                "after {ms} ms"
            );
        }
    }

    #[test]
    fn zenity_comes_before_kdialog_and_kdialog_serves_when_zenity_is_absent() {
        let both = Fake::new()
            .with_tool("kdialog", chosen("/k.mp4"))
            .with_tool("zenity", chosen("/z.mp4"));
        assert_eq!(
            pick(&both, Kind::Input),
            Pick::Chosen(PathBuf::from("/z.mp4"))
        );
        assert_eq!(both.ran_names(), ["zenity"]);

        let only_kdialog = Fake::new().with_tool("kdialog", chosen("/k.mp4"));
        assert_eq!(
            pick(&only_kdialog, Kind::Output),
            Pick::Chosen(PathBuf::from("/k.mp4"))
        );
        assert_eq!(only_kdialog.ran_names(), ["kdialog"]);
    }

    #[test]
    fn a_tool_that_fails_hands_over_to_the_next_and_then_to_the_text_field() {
        let fails = || ToolRun::Failed("exit code Some(255)".into());
        let second = Fake::new()
            .with_tool("zenity", fails())
            .with_tool("kdialog", chosen("/k.mp4"));
        assert_eq!(
            pick(&second, Kind::Input),
            Pick::Chosen(PathBuf::from("/k.mp4"))
        );
        assert_eq!(second.ran_names(), ["zenity", "kdialog"]);

        let none = Fake::new()
            .with_tool("zenity", fails())
            .with_tool("kdialog", fails());
        assert_eq!(pick(&none, Kind::Input), Pick::NoPicker);
        assert_eq!(pick(&Fake::new(), Kind::Output), Pick::NoPicker);
    }

    #[test]
    fn exit_code_1_is_a_cancel_and_stops_the_search() {
        let desktop = Fake::new()
            .with_tool("zenity", ToolRun::Cancelled)
            .with_tool("kdialog", chosen("/k.mp4"));
        assert_eq!(pick(&desktop, Kind::Input), Pick::Cancelled);
        assert_eq!(desktop.ran_names(), ["zenity"]);
    }

    #[test]
    fn an_empty_answer_is_a_cancel_and_spaces_in_a_name_are_kept() {
        let empty = Fake::new().with_tool("zenity", ToolRun::Selected(OsString::new()));
        assert_eq!(pick(&empty, Kind::Input), Pick::Cancelled);
        let spaced = Fake::new().with_tool("zenity", ToolRun::Selected("  /a b.mp4 ".into()));
        assert_eq!(
            pick(&spaced, Kind::Input),
            Pick::Chosen(PathBuf::from("  /a b.mp4 "))
        );
    }

    #[test]
    fn once_the_portal_failed_quickly_it_is_not_asked_again() {
        let desktop = Fake::new().with_tool("zenity", chosen("/z.mp4"));
        pick(&desktop, Kind::Input);
        pick(&desktop, Kind::Output);
        assert_eq!(desktop.portal_calls.get(), 1, "rfd was asked a second time");
        assert_eq!(desktop.ran_names(), ["zenity", "zenity"]);
        // And with nothing installed, every later click goes straight to the text field.
        let bare = Fake::new();
        assert_eq!(pick(&bare, Kind::Input), Pick::NoPicker);
        assert_eq!(pick(&bare, Kind::Input), Pick::NoPicker);
        assert_eq!(bare.portal_calls.get(), 1);
    }

    #[test]
    fn a_slow_nothing_does_not_make_the_fallback_sticky() {
        let desktop = Fake::new()
            .after(5000)
            .with_tool("zenity", chosen("/z.mp4"));
        pick(&desktop, Kind::Input);
        pick(&desktop, Kind::Input);
        assert_eq!(desktop.portal_calls.get(), 2);
    }

    #[test]
    fn without_the_fallback_a_quick_nothing_is_a_cancel() {
        let mut desktop = Fake::new().with_tool("zenity", chosen("/z.mp4"));
        desktop.fallback = false;
        assert_eq!(pick(&desktop, Kind::Input), Pick::Cancelled);
        assert!(desktop.ran_names().is_empty());
    }

    #[test]
    fn the_arguments_are_go_s_with_the_title_filter_and_default_name() {
        assert_eq!(
            tool_args(Tool::Zenity, Kind::Input),
            [
                "--file-selection",
                "--title=Select input video",
                "--file-filter=MP4 Video | *.mp4 *.MP4"
            ]
        );
        assert_eq!(
            tool_args(Tool::Zenity, Kind::Output),
            [
                "--file-selection",
                "--save",
                "--title=Save output video",
                "--filename=output.mp4",
                "--confirm-overwrite"
            ]
        );
        assert_eq!(
            tool_args(Tool::Kdialog, Kind::Input),
            ["--getopenfilename", ".", "*.mp4 *.MP4|MP4 Video"]
        );
        assert_eq!(
            tool_args(Tool::Kdialog, Kind::Output),
            ["--getsavefilename", "output.mp4", "*.mp4 *.MP4|MP4 Video"]
        );
    }

    #[test]
    fn the_arguments_reach_the_tool_unchanged() {
        let desktop = Fake::new().with_tool("zenity", ToolRun::Cancelled);
        pick(&desktop, Kind::Output);
        assert_eq!(
            desktop.ran.borrow()[0].1,
            tool_args(Tool::Zenity, Kind::Output)
        );
    }

    #[test]
    fn exit_codes_are_read_as_go_reads_them() {
        assert_eq!(
            classify_exit(Some(0), b"/a.mp4\n", b""),
            ToolRun::Selected("/a.mp4".into())
        );
        // Only the final line break goes; the rest of the name is kept as it is.
        assert_eq!(
            classify_exit(Some(0), b" /a b \r\n\n", b""),
            ToolRun::Selected(" /a b ".into())
        );
        assert_eq!(
            classify_exit(Some(0), b"\n", b""),
            ToolRun::Selected(OsString::new())
        );
        assert_eq!(classify_exit(Some(1), b"", b""), ToolRun::Cancelled);
        // 255 (a broken zenity), 5 (timeout) and a death by signal are failures, not cancels.
        for code in [Some(255), Some(5), Some(-1), None] {
            assert!(
                matches!(classify_exit(code, b"", b"boom"), ToolRun::Failed(_)),
                "{code:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_real_process_exits_with_the_code_that_is_read() {
        let run = |script: &str| {
            SystemDesktop.run_tool(Path::new("sh"), &["-c".to_string(), script.to_string()])
        };
        assert_eq!(run("echo /x.mp4"), ToolRun::Selected("/x.mp4".into()));
        assert_eq!(run("exit 1"), ToolRun::Cancelled);
        assert!(matches!(run("exit 255"), ToolRun::Failed(_)));
        let missing = SystemDesktop.run_tool(Path::new("/nonexistent/picker"), &[]);
        assert!(matches!(missing, ToolRun::Failed(_)));
    }

    #[test]
    fn a_typed_path_is_trimmed_and_must_be_full() {
        let full = if cfg!(windows) {
            "C:\\v\\a.mp4"
        } else {
            "/v/a.mp4"
        };
        assert_eq!(
            parse_typed_path(&format!("  {full}\n")),
            Ok(PathBuf::from(full))
        );
        assert!(parse_typed_path("").unwrap_err().contains("type the path"));
        assert!(parse_typed_path("  \t ").is_err());
        assert!(parse_typed_path("a.mp4").unwrap_err().contains("full path"));
    }

    #[cfg(unix)]
    #[test]
    fn the_search_finds_an_executable_and_skips_the_rest() {
        use std::os::unix::fs::PermissionsExt;
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        // In the first directory: present but not executable, and a directory of that name.
        std::fs::write(first.path().join("zenity"), "").unwrap();
        std::fs::create_dir(first.path().join("kdialog")).unwrap();
        let real = second.path().join("zenity");
        std::fs::write(&real, "").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();

        let var = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(find_in_path("zenity", Some(&var)), Some(real));
        assert_eq!(find_in_path("kdialog", Some(&var)), None);
        assert_eq!(find_in_path("absent", Some(&var)), None);
        assert_eq!(find_in_path("zenity", None), None);
        assert_eq!(find_in_path("zenity", Some(OsStr::new(""))), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_reaches_the_window_byte_for_byte() {
        use std::os::unix::ffi::OsStrExt;
        let run = classify_exit(Some(0), b"/a\xe9.mp4\n", b"");
        let ToolRun::Selected(raw) = &run else {
            panic!("{run:?}");
        };
        assert_eq!(raw.as_bytes(), b"/a\xe9.mp4");
        let desktop = Fake::new().with_tool("zenity", run);
        let Pick::Chosen(path) = pick(&desktop, Kind::Input) else {
            panic!("a path was chosen");
        };
        assert_eq!(path.as_os_str().as_bytes(), b"/a\xe9.mp4");
        assert!(
            path.to_str().is_none(),
            "the window refuses it later, by its own message"
        );
    }

    #[cfg(unix)]
    #[test]
    fn relative_and_empty_entries_of_the_search_path_are_ignored() {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Component;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("zenity");
        std::fs::write(&fake, "").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // The same directory, written relative to where the test runs.
        let up: PathBuf = std::env::current_dir()
            .unwrap()
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .map(|_| "..")
            .collect();
        let relative = up.join(dir.path().strip_prefix("/").unwrap());
        assert!(relative.is_relative() && relative.join("zenity").is_file());

        let var = std::env::join_paths([relative.as_path(), Path::new("")]).unwrap();
        assert_eq!(find_in_path("zenity", Some(&var)), None);
        // The same directory as an absolute entry is found.
        assert_eq!(
            find_in_path("zenity", Some(dir.path().as_os_str())),
            Some(fake)
        );
    }
}
