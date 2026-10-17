//! What the window asks of the desktop: file pickers, the browser, the end of the event loop.
//! Behind the [`Host`](crate::app::Host) trait, so that the tests choose what a picker answers.
//!
//! The pickers are rfd's, for now (the `portal` path on Linux, the native box on Windows); the
//! fallbacks for a machine with no portal arrive with PR 6.4, in this file.

use std::path::PathBuf;
use std::process::Command;

use superview_core::process::background_command;

use crate::app::Host;
use crate::format::SUPPORTED_INPUT_EXTENSIONS;

/// The picker's extensions, without the dot, from the one list of what the product reads.
fn picker_extensions() -> Vec<&'static str> {
    SUPPORTED_INPUT_EXTENSIONS
        .iter()
        .map(|ext| ext.trim_start_matches('.'))
        .collect()
}

/// The command that opens `url` in the user's browser: one argument per element, never a shell
/// string (the address is a constant of the program, but the habit is the contract).
/// `explorer.exe` on Windows, where `cmd /c start` would hand the address to a shell;
/// `xdg-open` elsewhere.
fn browser_command(url: &str) -> Command {
    if cfg!(windows) {
        background_command("explorer.exe", [url])
    } else {
        background_command("xdg-open", [url])
    }
}

/// The real desktop.
pub struct SystemHost;

impl Host for SystemHost {
    /// Blocks until the picker closes: call it off the UI thread.
    fn pick_input(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Select input video")
            .add_filter("MP4 Video", &picker_extensions())
            .pick_file()
    }

    /// Blocks until the picker closes: call it off the UI thread.
    fn pick_output(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Save output video")
            .add_filter("MP4 Video", &picker_extensions())
            .set_file_name("output.mp4")
            .save_file()
    }

    fn open_link(&self, url: &str) {
        match browser_command(url).spawn() {
            // Reaped on a thread of its own, so that the opener does not stay a zombie.
            Ok(mut child) => {
                let reaper =
                    std::thread::Builder::new()
                        .name("open-link".into())
                        .spawn(move || {
                            if let Err(e) = child.wait() {
                                log::warn!("Cannot wait for the browser opener: error={e}");
                            }
                        });
                if let Err(e) = reaper {
                    log::warn!(
                        "Cannot start the thread that waits for the browser opener: error={e}"
                    );
                }
            }
            Err(e) => log::warn!("Cannot open the browser: url={url} error={e}"),
        }
    }

    fn quit(&self) {
        if let Err(e) = slint::quit_event_loop() {
            log::warn!("Cannot end the event loop: error={e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_filter_comes_from_the_supported_extensions() {
        assert_eq!(picker_extensions(), ["mp4", "MP4"]);
    }

    #[test]
    fn the_browser_gets_the_address_as_one_argument_and_no_shell() {
        let url = "https://example.com/a?b=1&c=2#d e";
        let command = browser_command(url);
        let program = command.get_program().to_string_lossy().into_owned();
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, [url], "the address must be a single argument");
        if cfg!(windows) {
            assert_eq!(program, "explorer.exe");
        } else {
            assert_eq!(program, "xdg-open");
        }
        for shell in ["sh", "cmd", "cmd.exe", "bash", "powershell"] {
            assert_ne!(program, shell, "the browser must not go through a shell");
        }
    }
}
