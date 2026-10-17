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
mod tests {
    use super::*;

    fn sample() -> Preferences {
        Preferences {
            quality_profile: "Fast".into(),
            encoder_selection: "hevc_nvenc encoder".into(),
            squeeze_source: true,
        }
    }

    #[test]
    fn a_saved_file_loads_back_identical() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("superview").join(FILE_NAME);
        // The directory does not exist yet: save creates it.
        save(&path, &sample()).expect("save");
        assert_eq!(load(&path), sample());
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(
            left,
            [std::ffi::OsString::from(FILE_NAME)],
            "the temporary file must be gone after the rename"
        );
    }

    #[test]
    fn the_temporary_file_is_unique_to_the_process() {
        // A directory squatting on the name another process would use must not matter, and the
        // name of ours carries our pid.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        std::fs::create_dir(dir.path().join("preferences.json.tmp")).expect("mkdir");
        save(&path, &sample()).expect("save");
        assert_eq!(load(&path), sample());

        // And when saving fails, the temporary file of this process is cleaned up.
        let blocked = dir.path().join("sub");
        std::fs::create_dir(&blocked).expect("mkdir");
        let target = blocked.join(FILE_NAME);
        std::fs::create_dir(&target).expect("a directory where the file goes");
        assert!(save(&target, &sample()).is_err());
        let tmp = format!("{FILE_NAME}.{}.tmp", std::process::id());
        assert!(
            !blocked.join(tmp).exists(),
            "a failed save left its temporary file"
        );
    }

    #[test]
    fn saving_over_an_existing_file_replaces_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        save(&path, &sample()).expect("first save");
        save(&path, &Preferences::default()).expect("second save");
        assert_eq!(load(&path), Preferences::default());
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let prefs = load(&dir.path().join("absent").join(FILE_NAME));
        assert_eq!(prefs, Preferences::default());
        assert_eq!(prefs.quality_profile, "Balanced");
        assert_eq!(prefs.encoder_selection, "");
        assert!(!prefs.squeeze_source);
    }

    #[test]
    fn a_corrupted_file_gives_the_defaults_without_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        for garbage in [
            &b"{ not json"[..],
            b"",
            b"\xff\xfe\x00binary",
            b"[1, 2, 3]",
            // Well-formed, wrongly typed.
            br#"{"squeeze_source": "yes"}"#,
        ] {
            std::fs::write(&path, garbage).expect("write");
            assert_eq!(load(&path), Preferences::default(), "{garbage:?}");
        }
    }

    #[test]
    fn an_unreadable_path_gives_the_defaults() {
        // A directory where the file should be: reading it fails, which is not "absent".
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(load(dir.path()), Preferences::default());
    }

    #[test]
    fn missing_keys_keep_their_defaults_and_unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, r#"{"squeeze_source": true, "from_the_future": 1}"#).expect("write");
        let prefs = load(&path);
        assert!(prefs.squeeze_source);
        assert_eq!(prefs.quality_profile, "Balanced");
    }

    // The dropdown's old rule: any value it does not offer reads as Balanced.
    #[test]
    fn an_unknown_quality_profile_reads_as_balanced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, r#"{"quality_profile": "Turbo"}"#).expect("write");
        assert_eq!(load(&path).quality_profile, "Balanced");
        std::fs::write(&path, r#"{"quality_profile": "Fast"}"#).expect("write");
        assert_eq!(load(&path).quality_profile, "Fast");
    }

    #[test]
    fn saving_where_no_file_can_be_written_reports_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The parent "directory" is a file.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").expect("write");
        assert!(save(&blocker.join(FILE_NAME), &sample()).is_err());
    }

    #[test]
    fn the_default_path_is_under_the_injected_user_config_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().to_string_lossy().into_owned();
        // Both keys: AppData on Windows, XDG_CONFIG_HOME elsewhere.
        let env = move |key: &str| match key {
            "AppData" | "XDG_CONFIG_HOME" => Some(base.clone()),
            _ => None,
        };
        assert_eq!(
            default_path(&env),
            Some(dir.path().join("superview").join(FILE_NAME))
        );
        assert_eq!(default_path(&|_| None), None);
    }
}
