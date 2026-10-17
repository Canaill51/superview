//! What the window asks of the desktop: file pickers, the browser, the end of the event loop.
//! Behind the [`Host`](crate::app::Host) trait, so that the tests choose what a picker answers.
//!
//! The pickers are `dialogs.rs`'s: rfd, then zenity and kdialog on a Linux machine with no
//! portal, then a text field in the window (PR 6.4).

use std::process::Command;

use superview_core::process::background_command;

use crate::app::Host;
use crate::dialogs::{self, Kind, Pick, SystemDesktop};

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
    fn pick_input(&self) -> Pick {
        dialogs::pick(&SystemDesktop, Kind::Input)
    }

    /// Blocks until the picker closes: call it off the UI thread.
    fn pick_output(&self) -> Pick {
        dialogs::pick(&SystemDesktop, Kind::Output)
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
