//! The application window: starts the services and hands them to [`app::App`], which wires the
//! Slint window to the logic (phase 6 of DEC-43).

// No console window next to the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(test)]
#[path = "../build_support.rs"]
mod build_support;
mod dialogs;
mod engine;
mod format;
mod host;
mod journal;
mod prefs;
mod report;
mod signals;
mod state;

use std::sync::Arc;

use app::{App, Host, Setup};
use engine::SystemEngine;
use host::SystemHost;
use superview_core::config::find_config_path;
use superview_core::logfile::parse_log_level;

slint::include_modules!();

/// The application ID the compositor matches to a desktop file of the same name (Wayland
/// `app_id`). `packaging/linux/build_appimage.sh` names its `.desktop` file after the same ID;
/// a test pins the two together.
const APP_ID: &str = "com.canaill51.superview";

/// What the build recorded, in plain bytes of the binary: the release checks a built file for
/// `modified=true` with `grep -a SUPERVIEW-BUILD`, without running it. `#[used]` only keeps the
/// static, which is a pointer: the MSVC linker still dropped the text it points to (measured, CI
/// of #127). `main` therefore reads it through `black_box`, which keeps the bytes everywhere.
#[used]
static BUILD_MARKER: &str = concat!(
    "SUPERVIEW-BUILD version=",
    env!("SUPERVIEW_BUILD_VERSION"),
    " revision=",
    env!("SUPERVIEW_BUILD_REVISION"),
    " modified=",
    env!("SUPERVIEW_BUILD_MODIFIED"),
    // The linker packs string literals back to back; the NUL ends the line for `grep -a`.
    "\0",
);

/// The identity line of the log and the Diagnostic, from the values `build.rs` recorded.
fn build_identity() -> String {
    let revision = env!("SUPERVIEW_BUILD_REVISION");
    let vcs = format::Vcs {
        revision,
        modified: env!("SUPERVIEW_BUILD_MODIFIED") == "true",
    };
    format::build_identity(
        env!("SUPERVIEW_BUILD_VERSION"),
        env!("SUPERVIEW_BUILD_STAMPED") == "true",
        (!revision.is_empty()).then_some(&vcs),
    )
}

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod repo_guards;
#[cfg(test)]
mod window_tests;

fn main() -> Result<(), slint::PlatformError> {
    std::hint::black_box(BUILD_MARKER);
    // Records go to the log file from the start, at the default level; the configured level
    // applies as soon as the configuration is read.
    let log_path = journal::install(log::LevelFilter::Info);
    journal::install_panic_hook();
    let config_path = find_config_path();
    let cfg = format::load_startup_config(config_path.as_deref());
    log::set_max_level(parse_log_level(&cfg.log_level));

    let identity = build_identity();
    log::info!(
        "Superview starting: build={identity} log_file={} config_file={} log_level={}",
        log_path
            .as_deref()
            .map_or(String::new(), |p| p.display().to_string()),
        config_path
            .as_deref()
            .map_or(String::new(), |p| p.display().to_string()),
        cfg.log_level
    );

    let env = |key: &str| std::env::var(key).ok();
    let prefs_path = prefs::default_path(&env);
    let prefs = prefs_path.as_deref().map(prefs::load).unwrap_or_default();

    let window = MainWindow::new()?;
    // Set once the window exists (before that Slint has no platform yet and refuses), and before
    // it is shown, which is when winit reads the ID. It only matters on Wayland and X11. A
    // refusal costs the taskbar icon, not the program.
    if let Err(err) = slint::set_xdg_app_id(APP_ID) {
        log::warn!("could not set the application ID {APP_ID}: {err}");
    }
    // A system stop request ends the event loop; a conversion that runs sees the number
    // through the flag and unwinds on its own.
    let signal = signals::install(|| SystemHost.quit());
    let app = App::wire(
        &window,
        Setup {
            engine: Arc::new(SystemEngine::for_process(cfg.clone(), signal)),
            host: Arc::new(SystemHost),
            cfg,
            prefs,
            prefs_path,
            log_path,
            identity,
            temp_dir: clean_dir(&std::env::temp_dir()),
        },
    );
    // A stop request that came in before the loop could run ends it before it starts.
    let result = if signal.load(std::sync::atomic::Ordering::Acquire) == 0 {
        window.run()
    } else {
        Ok(())
    };
    // Every way out waits for a running conversion to unwind; after a Quit or a window close
    // there is nothing left to wait for and this returns at once (Go: the line after
    // `ShowAndRun`).
    app.shutdown("event loop stopped");
    result
}

/// The path without a trailing separator (`std::env::temp_dir()` keeps one on Windows, and the
/// disk messages of the Diagnostic would differ from Go's by that character).
fn clean_dir(path: &std::path::Path) -> std::path::PathBuf {
    path.components().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Wayland the compositor finds the taskbar icon through a desktop file named after the
    /// window's app_id; the AppImage script must build the same ID (Go:
    /// `TestAppImageDesktopFileIsNamedAfterTheAppID`).
    #[test]
    fn the_appimage_desktop_file_is_named_after_the_app_id() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packaging/linux/build_appimage.sh");
        let script = std::fs::read_to_string(&script).unwrap_or_else(|e| panic!("{script:?}: {e}"));
        let want = format!("APP_ID=\"{APP_ID}\"");
        assert!(
            script.lines().any(|line| line.trim_end() == want),
            "build_appimage.sh must set {want} so its desktop file is named after the window's app ID"
        );
    }

    // A test of shape only: that the bytes stay in the linked binary (the MSVC linker
    // drops it unless it is read) is checked on the built files by `rust.yml`.
    #[test]
    fn shape_test_the_build_marker_has_the_agreed_shape() {
        assert!(
            BUILD_MARKER.starts_with("SUPERVIEW-BUILD version="),
            "{BUILD_MARKER}"
        );
        assert!(
            BUILD_MARKER.ends_with(" modified=true\0")
                || BUILD_MARKER.ends_with(" modified=false\0"),
            "{BUILD_MARKER}"
        );
    }

    #[test]
    fn a_trailing_separator_is_dropped() {
        let cleaned = clean_dir(std::path::Path::new(if cfg!(windows) {
            "C:\\Users\\u\\Temp\\"
        } else {
            "/tmp/x/"
        }));
        let text = cleaned.display().to_string();
        assert!(!text.ends_with(['/', '\\']), "{text}");
        assert!(
            text.ends_with(if cfg!(windows) { "Temp" } else { "x" }),
            "{text}"
        );
    }
}
