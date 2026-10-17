//! The application window (phase 3b trial of DEC-43): one button that opens a
//! file picker, one field that shows the chosen file name. No conversion logic.
//! Unlike the Go window, a refused file shows no error dialog here, only the
//! status line.

// No console window next to the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The state, the texts and the preferences are written and tested ahead of the window that will
// use them (phase 6, PR 6.3 wires them); until then only part of them is reached from `main`.
// `format` and `prefs` are fully reached by their own tests, so their expectation holds for
// the real build only; `state` keeps dead code in both.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in PR 6.3"))]
mod format;
#[cfg_attr(not(test), expect(dead_code, reason = "wired in PR 6.3"))]
mod prefs;
#[expect(dead_code, reason = "wired in PR 6.3")]
mod state;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use format::SUPPORTED_INPUT_EXTENSIONS;
use state::{AppState, ChosenOutcome, Control};
use superview_core::error::Error;
use superview_core::video::VideoSpecs;

slint::include_modules!();

#[cfg(test)]
mod window_tests;

/// The picker's extensions, without the dot, from the one list of what the product reads.
fn picker_extensions() -> Vec<&'static str> {
    SUPPORTED_INPUT_EXTENSIONS
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .collect()
}

/// A poisoned lock (a panic elsewhere) does not stop the window from working.
fn locked(state: &Mutex<AppState>) -> MutexGuard<'_, AppState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Opens the native picker, MP4 only, and blocks until it closes.
/// Called off the UI thread: the picker holds the call for as long as it is open.
fn pick_mp4() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select input video")
        .add_filter("MP4 Video", &picker_extensions())
        .pick_file()
}

/// True when the file name ends with `.mp4`, in any case. Stands in for the ffprobe check of
/// the real window in this trial: the real input side does not filter by extension.
fn is_mp4(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().to_lowercase().ends_with(".mp4"))
}

/// The trial's video check: no ffprobe, an MP4 name is enough.
fn trial_check(path: &str) -> Result<VideoSpecs, Error> {
    if is_mp4(path) {
        Ok(VideoSpecs {
            file: path.to_string(),
            streams: Vec::new(),
        })
    } else {
        Err(Error::Plain(format!("not an MP4 file: {path}")))
    }
}

/// Connects the window to the state. `pick` is the blocking picker,
/// a parameter so that nothing here depends on rfd.
fn wire(window: &MainWindow, pick: fn() -> Option<PathBuf>) {
    let state = Arc::new(Mutex::new(AppState::default()));
    let weak = window.as_weak();
    window.on_choose_input(move || {
        let Some(window) = weak.upgrade() else { return };
        if !locked(&state).begin_pick(Control::Open) {
            return;
        }
        window.set_picking(true);
        let state = Arc::clone(&state);
        let weak = weak.clone();
        std::thread::spawn(move || {
            // A picker that panics counts as a cancel: the button must never
            // stay greyed out.
            let answer = std::panic::catch_unwind(pick).unwrap_or(None);
            // Back on the UI thread: widgets are only touched from there.
            // If the window is already gone, the update is dropped.
            let _ = weak.upgrade_in_event_loop(move |window| {
                let mut state = locked(&state);
                state.finish_pick(Control::Open);
                // A cancel changes nothing. Unlike Go, a refused file shows no error dialog
                // in this trial, only the status line.
                if let Some(path) = answer {
                    match state.apply_chosen_input(&path, trial_check) {
                        ChosenOutcome::Applied => {
                            window.set_input_path(state.view.input_name.as_str().into());
                            window.set_status(state.view.status.as_str().into());
                        }
                        ChosenOutcome::Rejected(_) => {
                            window.set_status(state.view.status.as_str().into());
                        }
                        ChosenOutcome::Ignored => {}
                    }
                }
                window.set_picking(false);
            });
        });
    });
}

fn main() -> Result<(), slint::PlatformError> {
    println!("superview {}", superview_core::version());
    let window = MainWindow::new()?;
    wire(&window, pick_mp4);
    window.run()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Clicks the button and runs the event loop until the picker is released.
    /// Fails after 5 s: a button stuck greyed out must not hang the suite.
    fn click_and_wait(window: &MainWindow) {
        window.invoke_choose_input();
        let weak = window.as_weak();
        let poll = slint::Timer::default();
        poll.start(slint::TimerMode::Repeated, Duration::from_millis(10), {
            move || {
                if !weak.unwrap().get_picking() {
                    slint::quit_event_loop().expect("quit");
                }
            }
        });
        let give_up = slint::Timer::default();
        give_up.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
            slint::quit_event_loop().expect("quit");
        });
        slint::run_event_loop_until_quit().expect("event loop");
    }

    fn window_with(pick: fn() -> Option<PathBuf>) -> MainWindow {
        let window = MainWindow::new().expect("window");
        wire(&window, pick);
        window
    }

    // The trial's is_mp4 tests.
    #[test]
    fn is_mp4_is_case_insensitive_and_takes_the_hidden_name() {
        for ok in [
            "a.mp4",
            "A.MP4",
            ".mp4",
            "/x/y/.MP4",
            "/x/y/clip.Mp4",
            "C:/Users/u/a.mp4",
        ] {
            assert!(is_mp4(ok), "{ok} should be accepted");
        }
    }

    #[test]
    fn is_mp4_refuses_other_names() {
        for bad in [
            "a.mkv",
            "a.mp4.txt",
            "x.mp4.txt",
            "mp4",
            ".mp4x",
            "amp4",
            "a.",
            "dir/",
            "",
        ] {
            assert!(!is_mp4(bad), "{bad:?} should be refused");
        }
    }

    #[test]
    fn the_picker_filter_comes_from_the_supported_extensions() {
        assert_eq!(picker_extensions(), ["mp4", "MP4"]);
    }

    // One test only: the testing backend can be initialised once per process, so every window
    // scenario lives in this function (the picker's here, the layout's in `window_tests`).
    // Each scenario names itself in its failure message.
    #[test]
    fn window_scenarios() {
        i_slint_backend_testing::init_integration_test_with_system_time();

        let window = window_with(|| None);
        assert_eq!(window.get_status(), "Status: Ready");
        assert_eq!(window.get_input_path(), "");
        assert!(!window.get_picking());

        // A chosen MP4: the field shows the name, the status says so.
        let window = window_with(|| Some("/videos/trip/clip.mp4".into()));
        click_and_wait(&window);
        assert!(!window.get_picking(), "the button must come back");
        assert_eq!(window.get_input_path(), "clip.mp4");
        assert_eq!(window.get_status(), "Status: Input loaded");

        // A cancel changes nothing.
        let window = window_with(|| None);
        click_and_wait(&window);
        assert!(!window.get_picking());
        assert_eq!(window.get_input_path(), "");
        assert_eq!(window.get_status(), "Status: Ready");

        // A refused file: invalid status, field still empty.
        let window = window_with(|| Some("/videos/clip.mkv".into()));
        click_and_wait(&window);
        assert!(!window.get_picking());
        assert_eq!(window.get_input_path(), "");
        assert_eq!(window.get_status(), "Status: Invalid input");

        // A cancel or a refusal after a good choice keeps that choice.
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn scripted() -> Option<PathBuf> {
            match CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 => Some("/videos/trip/clip.mp4".into()),
                1 => None,
                _ => Some("/videos/other.mkv".into()),
            }
        }
        let window = window_with(scripted);
        click_and_wait(&window);
        assert_eq!(window.get_input_path(), "clip.mp4");
        click_and_wait(&window); // cancelled
        assert_eq!(window.get_input_path(), "clip.mp4");
        assert_eq!(window.get_status(), "Status: Input loaded");
        click_and_wait(&window); // refused
        assert_eq!(
            window.get_input_path(),
            "clip.mp4",
            "a refused file replaced the input"
        );
        assert_eq!(window.get_status(), "Status: Invalid input");

        // A picker that panics must not leave the button greyed out.
        let window = window_with(|| panic!("picker blew up"));
        click_and_wait(&window);
        assert!(!window.get_picking(), "a panicking picker froze the button");
        assert_eq!(window.get_status(), "Status: Ready");

        // The real window's layout, labels, initial state and popups.
        window_tests::run();
    }
}
