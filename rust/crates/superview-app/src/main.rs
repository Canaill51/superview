//! The application window (phase 3b trial of DEC-43): one button that opens a
//! file picker, one field that shows the chosen file name. No conversion logic.
//! Unlike the Go window, a refused file shows no error dialog here, only the
//! status line.

// No console window next to the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod selection;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use selection::Selection;

slint::include_modules!();

/// Opens the native picker, MP4 only, and blocks until it closes.
/// Called off the UI thread: the picker holds the call for as long as it is open.
fn pick_mp4() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select input video")
        .add_filter("MP4 Video", &["mp4", "MP4"])
        .pick_file()
}

/// Connects the window to the selection state. `pick` is the blocking picker,
/// a parameter so that nothing here depends on rfd.
fn wire(window: &MainWindow, pick: fn() -> Option<PathBuf>) {
    let state = Arc::new(Mutex::new(Selection::default()));
    let weak = window.as_weak();
    window.on_choose_input(move || {
        let Some(window) = weak.upgrade() else { return };
        if !state.lock().expect("selection lock").begin_pick() {
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
                let mut state = state.lock().expect("selection lock");
                let outcome = state.finish_pick(answer);
                window.set_input_path(state.input_text().into());
                if let Some(status) = Selection::status_for(&outcome) {
                    window.set_status(status.into());
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

    // One test only: the testing backend can be initialised once per process,
    // so every window scenario lives in this function.
    #[test]
    fn the_window_follows_the_picker() {
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

        // A picker that panics must not leave the button greyed out.
        let window = window_with(|| panic!("picker blew up"));
        click_and_wait(&window);
        assert!(!window.get_picking(), "a panicking picker froze the button");
        assert_eq!(window.get_status(), "Status: Ready");
    }
}
