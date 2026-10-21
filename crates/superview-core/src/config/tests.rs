use super::*;
use std::fs;

fn write_config(dir: &tempfile::TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("superview.yaml");
    fs::write(&path, content).unwrap();
    path
}

fn load_str(content: &str) -> Result<Config, Error> {
    let dir = tempfile::tempdir().unwrap();
    load_config(Some(&write_config(&dir, content)))
}

fn codecs(list: &[&str]) -> Vec<String> {
    list.iter().map(|c| (*c).into()).collect()
}

#[test]
fn no_path_yields_the_defaults() {
    let cfg = load_config(None).unwrap();
    assert_eq!(cfg, Config::default());
    assert_eq!(cfg.min_bitrate, 102_400);
    assert_eq!(cfg.max_bitrate, 209_715_200);
    assert_eq!(cfg.temp_dir_prefix, "superview-*");
    assert_eq!(cfg.encoder_codecs, codecs(&["264", "265", "hevc"]));
    assert_eq!(cfg.log_level, "info");
    assert_eq!(cfg.performance_mode, "safe_performance");
    assert_eq!(cfg.video_preset, "");
    assert_eq!((cfg.filter_threads, cfg.encoder_threads), (0, 0));
}

#[test]
fn missing_file_yields_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = load_config(Some(&dir.path().join("absent.yaml"))).unwrap();
    assert_eq!(cfg, Config::default());
}

#[test]
fn values_come_from_the_file() {
    let cfg = load_str(
        "min_bitrate: 256000\nmax_bitrate: 20000000\nlog_level: debug\n\
             temp_dir_prefix: \"custom-*\"\nencoder_codecs:\n  - \"264\"\n",
    )
    .unwrap();
    assert_eq!(cfg.min_bitrate, 256_000);
    assert_eq!(cfg.max_bitrate, 20_000_000);
    assert_eq!(cfg.log_level, "debug");
    assert_eq!(cfg.temp_dir_prefix, "custom-*");
    assert_eq!(cfg.encoder_codecs, codecs(&["264"]));
    // Keys the file does not set keep their default.
    assert_eq!(cfg.performance_mode, "safe_performance");
}

// E-21
#[test]
fn unknown_key_is_an_error_naming_it() {
    let err = load_str("video-preset: slow\n").unwrap_err();
    assert!(err.to_string().contains("video-preset"), "{err}");
    assert!(err.to_string().contains("failed to parse config file"));
}

// The migration
// of retired keys is gone, so the v0.1.6 keys are unknown keys like others.
#[test]
fn retired_keys_are_no_longer_tolerated() {
    for key in ["quality_preset", "min_video_width", "min_video_height"] {
        let err = load_str(&format!("{key}: 1\nvideo_preset: slow\n")).unwrap_err();
        assert!(err.to_string().contains(key), "{err}");
    }
}

// Beyond the basic cases: what a strict decoder also refuses.
#[test]
fn duplicate_key_and_wrong_type_are_errors() {
    assert!(load_str("min_bitrate: 1\nmin_bitrate: 2\n").is_err());
    assert!(load_str("min_bitrate: abc\n").is_err());
    assert!(load_str("- a\n- b\n").is_err());
}

// yaml.v3 took 1e6 and truncated 1.5 to 1; both are refused here.
#[test]
fn decimals_in_integer_fields_are_refused() {
    assert!(load_str("min_bitrate: 1e6\n").is_err());
    assert!(load_str("min_bitrate: 1.5\n").is_err());
    assert!(load_str("min_bitrate: \"5\"\n").is_ok());
    assert!(load_str("min_bitrate: 5\n---\nlog_level: x\n").is_err());
}

fn load_bytes(bytes: &[u8]) -> Result<Config, Error> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("superview.yaml");
    fs::write(&path, bytes).unwrap();
    load_config(Some(&path))
}

#[test]
fn utf16_and_utf8_bom_files_load() {
    let text = "min_bitrate: 5\nlog_level: déb\n";
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut le = vec![0xFF, 0xFE];
    le.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    let mut be = vec![0xFE, 0xFF];
    be.extend(units.iter().flat_map(|u| u.to_be_bytes()));
    let mut bom8 = vec![0xEF, 0xBB, 0xBF];
    bom8.extend(text.as_bytes());
    for bytes in [le, be, bom8, text.as_bytes().to_vec()] {
        let cfg = load_bytes(&bytes).unwrap();
        assert_eq!((cfg.min_bitrate, cfg.log_level.as_str()), (5, "déb"));
    }
    // Odd length UTF-16, and UTF-8 that is not.
    assert!(load_bytes(&[0xFF, 0xFE, 0x41]).is_err());
    let err = load_bytes(b"log_level: \xff\xfe\n").unwrap_err();
    assert!(err.to_string().contains("invalid UTF-8"), "{err}");
}

// yaml.v3 leaves a field alone on `null`, so the default survives.
#[test]
fn null_keeps_the_default() {
    let cfg = load_str("min_bitrate: ~\nlog_level:\n").unwrap();
    assert_eq!(cfg, Config::default());
}

#[test]
fn min_above_max_is_an_error() {
    let err = load_str("min_bitrate: 10\nmax_bitrate: 5\n").unwrap_err();
    assert!(
        err.to_string()
            .contains("min_bitrate (10) is above max_bitrate (5)")
    );
    // A bound of 0 or less means "unset" and is not compared.
    assert!(load_str("min_bitrate: 10\nmax_bitrate: 0\n").is_ok());
    // Equal bounds are fine.
    assert!(load_str("min_bitrate: 5\nmax_bitrate: 5\n").is_ok());
    // Only one bound set: the default of the other is compared.
    assert!(load_str("min_bitrate: 300000000\n").is_err());
}

// The env variable is gone; the
// YAML list gets the same normalisation.
#[test]
fn encoder_codecs_are_trimmed() {
    let cfg = load_str("encoder_codecs: [\"264\", \" 265 \", \"\"]\n").unwrap();
    assert_eq!(cfg.encoder_codecs, codecs(&["264", "265"]));
}

#[test]
fn empty_encoder_codecs_fall_back_to_the_defaults() {
    let defaults = codecs(&["264", "265", "hevc"]);
    assert_eq!(
        load_str("encoder_codecs: []\n").unwrap().encoder_codecs,
        defaults
    );
    assert_eq!(
        load_str("encoder_codecs: [\" \", \"\"]\n")
            .unwrap()
            .encoder_codecs,
        defaults
    );
}

#[test]
fn shipped_file_still_loads() {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../superview.yaml");
    // A missing file loads as defaults: prove the path is right first.
    assert!(shipped.is_file(), "{} not found", shipped.display());
    assert_eq!(load_config(Some(&shipped)).unwrap(), Config::default());
    // A comment-only (or empty) file is an empty document, not an error.
    assert_eq!(load_str("# nothing set\n").unwrap(), Config::default());
    assert_eq!(load_str("").unwrap(), Config::default());
}

// Via the file.
#[test]
fn performance_mode_is_normalised() {
    for (written, want) in [
        ("turbo", "safe"),
        ("safe", "safe"),
        (" SAFE ", "safe"),
        (" Performance ", "safe_performance"),
        ("performance", "safe_performance"),
        ("safe_performance", "safe_performance"),
        ("", "safe_performance"),
    ] {
        let cfg = load_str(&format!("performance_mode: \"{written}\"\n")).unwrap();
        assert_eq!(cfg.performance_mode, want, "for {written:?}");
    }
}

// Via the file.
#[test]
fn tuning_values_are_normalised() {
    let cfg = load_str("video_preset: turbo\nfilter_threads: -1\nencoder_threads: -2\n").unwrap();
    assert_eq!(cfg.video_preset, "");
    assert_eq!((cfg.filter_threads, cfg.encoder_threads), (0, 0));

    let cfg =
        load_str("video_preset: \" Slow \"\nfilter_threads: 4\nencoder_threads: 8\n").unwrap();
    assert_eq!(cfg.video_preset, "slow");
    assert_eq!((cfg.filter_threads, cfg.encoder_threads), (4, 8));
    // Zero is "auto", not invalid.
    assert_eq!(load_str("filter_threads: 0\n").unwrap().filter_threads, 0);
}

#[test]
fn every_preset_is_accepted() {
    for preset in [
        "ultrafast",
        "superfast",
        "veryfast",
        "faster",
        "fast",
        "medium",
        "slow",
        "slower",
        "veryslow",
        "placebo",
    ] {
        let cfg = load_str(&format!("video_preset: {preset}\n")).unwrap();
        assert_eq!(cfg.video_preset, preset);
    }
}

#[test]
fn safe_performance_mode_follows_the_normalised_mode() {
    let mut cfg = Config::default();
    assert!(cfg.is_safe_performance_mode());
    cfg.performance_mode = "safe".into();
    assert!(!cfg.is_safe_performance_mode());
    cfg.performance_mode = "performance".into();
    assert!(cfg.is_safe_performance_mode());
}

#[test]
fn display_lists_the_settings() {
    let text = Config::default().to_string();
    assert!(text.starts_with("Configuration:\n"));
    assert!(text.contains("  Min Bitrate: 102400 bits/sec (0.10 Mbps)\n"));
    assert!(text.contains("  Max Bitrate: 209715200 bits/sec (209.72 Mbps)\n"));
    assert!(text.contains("  Encoder Codecs: 264,265,hevc\n"));
    assert!(text.contains("  Log Level: info\n"));
    assert!(text.contains("  Performance Mode: safe_performance\n"));
    assert!(text.ends_with("  Encoder Threads: 0\n"));
}

fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
    }
}

// SUPERVIEW_CONFIG is gone.
#[test]
fn candidates_are_ordered_exe_user_cwd() {
    let exe = Path::new("/opt/superview/bin/superview");
    let user = Path::new("/home/u/.config");
    let got = config_candidate_paths(Some(exe), Some(user));
    assert_eq!(
        got,
        vec![
            PathBuf::from("/opt/superview/bin").join("superview.yaml"),
            PathBuf::from("/home/u/.config/superview").join("superview.yaml"),
            PathBuf::from("superview.yaml"),
        ]
    );
    // Without an executable or user directory only the working directory is left.
    assert_eq!(
        config_candidate_paths(None, None),
        vec![PathBuf::from("superview.yaml")]
    );
}

#[test]
#[cfg(unix)]
fn user_config_dir_follows_xdg_then_home() {
    let got = user_config_dir(&env_of(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/x")));
    let got = user_config_dir(&env_of(&[("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/h/.config")));
    // Empty counts as unset; a relative XDG_CONFIG_HOME is refused, not
    // replaced by $HOME.
    let got = user_config_dir(&env_of(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]));
    assert_eq!(got, Some(PathBuf::from("/h/.config")));
    assert_eq!(
        user_config_dir(&env_of(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/h")])),
        None
    );
    assert_eq!(user_config_dir(&env_of(&[])), None);
}

#[test]
#[cfg(windows)]
fn user_config_dir_is_appdata() {
    let got = user_config_dir(&env_of(&[("AppData", r"C:\Users\u\AppData\Roaming")]));
    assert_eq!(got, Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")));
    assert_eq!(user_config_dir(&env_of(&[])), None);
}

// The first existing file wins.
#[test]
fn resolve_takes_the_first_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = write_config(&dir, "log_level: debug\n");
    let candidates = vec![
        dir.path().join("absent.yaml"),
        first.clone(),
        PathBuf::from("x"),
    ];
    assert_eq!(resolve_config_path(&candidates), Some(first));
}

#[test]
fn resolve_ignores_directories_and_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let candidates = vec![dir.path().to_path_buf(), dir.path().join("absent.yaml")];
    assert_eq!(resolve_config_path(&candidates), None);
    assert_eq!(resolve_config_path(&[]), None);
}

// F-20: a negative bitrate is "unset", with a warning, like a negative thread count.
#[test]
fn negative_bitrates_are_treated_as_unset() {
    let cfg = load_str("min_bitrate: -5\nmax_bitrate: -1\n").unwrap();
    assert_eq!((cfg.min_bitrate, cfg.max_bitrate), (0, 0));
    let cfg = load_str("min_bitrate: 1000\nmax_bitrate: 2000\n").unwrap();
    assert_eq!((cfg.min_bitrate, cfg.max_bitrate), (1000, 2000));
}
