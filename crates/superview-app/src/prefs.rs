//! The user's three choices, kept between sessions: the quality profile, the codec dropdown
//! and the "source already stretched" box.
//!
//! A new file (`<user config dir>/superview/preferences.json`), not a reading of what the
//! Go/Fyne window saved: the port starts clean (decision of 2026-10-17). Loading never fails:
//! a missing, unreadable or invalid file gives the defaults, and the last two leave a warning
//! in the log. Saving is atomic (a temporary file, then a rename), so a crash cannot leave half
//! a file.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use superview_core::config::user_config_dir;

pub const FILE_NAME: &str = "preferences.json";

/// The quality profiles the dropdown offers; the first is not the default.
pub const QUALITY_PROFILES: [&str; 2] = ["Fast", "Balanced"];
pub const DEFAULT_QUALITY_PROFILE: &str = "Balanced";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// "Fast" or "Balanced".
    pub quality_profile: String,
    /// The codec dropdown's entry, for example "hevc_nvenc encoder"; "" = none saved yet.
    pub encoder_selection: String,
    pub squeeze_source: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            quality_profile: DEFAULT_QUALITY_PROFILE.to_string(),
            encoder_selection: String::new(),
            squeeze_source: false,
        }
    }
}

impl Preferences {
    /// A quality profile the dropdown does not offer becomes the default one.
    pub(crate) fn sanitized(mut self) -> Self {
        if !QUALITY_PROFILES.contains(&self.quality_profile.as_str()) {
            self.quality_profile = DEFAULT_QUALITY_PROFILE.to_string();
        }
        self
    }
}

/// Where the preferences live: `<user config dir>/superview/preferences.json`. `env` is the
/// environment lookup (never read directly, so that no test touches the process environment).
pub fn default_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    user_config_dir(env).map(|dir| dir.join("superview").join(FILE_NAME))
}

/// Reads the preferences. Never fails and never panics: whatever goes wrong, the defaults.
pub fn load(path: &Path) -> Preferences {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Preferences::default(),
        Err(e) => {
            log::warn!(
                "Cannot read the preferences, using the defaults: path={} error={e}",
                path.display()
            );
            return Preferences::default();
        }
    };
    match serde_json::from_slice::<Preferences>(&bytes) {
        Ok(prefs) => prefs.sanitized(),
        Err(e) => {
            log::warn!(
                "Invalid preferences file, using the defaults: path={} error={e}",
                path.display()
            );
            Preferences::default()
        }
    }
}

/// Writes the preferences atomically, creating the directory if needed.
pub fn save(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Unique per process, so two instances saving at once do not write the same file.
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.tmp", std::process::id()));
    let temporary = PathBuf::from(temporary);

    let written = (|| {
        let mut json = serde_json::to_vec_pretty(prefs).map_err(std::io::Error::other)?;
        json.push(b'\n');
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests;
