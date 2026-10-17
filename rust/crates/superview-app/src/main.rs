//! The application window: starts the services and hands them to [`app::App`], which wires the
//! Slint window to the logic (phase 6 of DEC-43).

// No console window next to the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod format;
mod host;
mod journal;
mod prefs;
mod signals;
mod state;

use std::sync::Arc;

use app::{App, Host, Setup};
use engine::SystemEngine;
use host::SystemHost;
use superview_core::config::find_config_path;
use superview_core::logfile::parse_log_level;

slint::include_modules!();

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod window_tests;

fn main() -> Result<(), slint::PlatformError> {
    // Records go to the log file from the start, at the default level; the configured level
    // applies as soon as the configuration is read.
    let log_path = journal::install(log::LevelFilter::Info);
    let config_path = find_config_path();
    let cfg = format::load_startup_config(config_path.as_deref());
    log::set_max_level(parse_log_level(&cfg.log_level));

    // The git stamp of the build arrives with the release work (phase 7): until then every
    // build is "dev".
    let identity = format::build_identity(superview_core::version(), false, None);
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
