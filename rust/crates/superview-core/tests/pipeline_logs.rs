//! What a conversion says in the log, which users attach to bug reports.
//!
//! The things only the log shows: the line saying the finished video was kept under its
//! working name, the success line, the warning when the session directory cannot be removed,
//! and the stage timings, which come last in every case (after the cleanup). Read here, in a
//! test binary of its own because the logger is process-wide.
//!
//! Unix only: the stand-in ffmpeg and ffprobe are shell scripts.
#![cfg(unix)]

use std::cell::Cell;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32};

use log::{Level, Log, Metadata, Record};
use superview_core::encode::run::StopControl;
use superview_core::error::Error;
use superview_core::pipeline::{PipelineServices, PipelineUi, perform_encoding};
use superview_core::probe::Prober;
use superview_core::tools::ToolResolver;

struct Capture(Mutex<Vec<(Level, String)>>);

impl Log for Capture {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

fn take_log() -> Vec<(Level, String)> {
    CAPTURE
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect()
}

struct Ui<'a> {
    on_progress: Box<dyn Fn() + 'a>,
    shown: Cell<usize>,
}

impl PipelineUi for Ui<'_> {
    fn get_encoder(&self) -> String {
        "libx264".into()
    }
    fn get_bitrate(&self) -> Result<i64, Error> {
        Ok(2_000_000)
    }
    fn get_squeeze(&self) -> bool {
        false
    }
    fn show_progress(&self, _: f64) {
        self.shown.set(self.shown.get() + 1);
        (self.on_progress)();
    }
}

fn script(dir: &Path, name: &str, body: &str) {
    let file = dir.join(name);
    fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
}

const PROBE: &str = "cat <<'EOF'\n{\"streams\":[{\"codec_name\":\"h264\",\"width\":640,\"height\":480,\"duration\":\"3.0\",\"bit_rate\":\"2500000\",\"pix_fmt\":\"yuv420p\",\"r_frame_rate\":\"30/1\"}]}\nEOF";
const OK: &str = "echo out_time_ms=1000000; echo progress=end; exit 0";

struct Scene {
    _root: tempfile::TempDir,
    tools: tempfile::TempDir,
    empty: tempfile::TempDir,
    input: PathBuf,
    out_dir: PathBuf,
    tmp: PathBuf,
}

impl Scene {
    fn new(ffprobe: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let (tools, empty) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        script(tools.path(), "ffprobe", ffprobe);
        script(tools.path(), "ffmpeg", OK);
        let out_dir = root.path().join("out");
        let tmp = root.path().join("tmp");
        fs::create_dir(&out_dir).unwrap();
        fs::create_dir(&tmp).unwrap();
        let input = root.path().join("input.mp4");
        fs::write(&input, "pretend this is a video").unwrap();
        Self {
            _root: root,
            tools,
            empty,
            input,
            out_dir,
            tmp,
        }
    }

    fn resolver(&self) -> ToolResolver {
        let path = std::env::join_paths(
            std::iter::once(self.tools.path().to_path_buf())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        ToolResolver::new(
            move |key| (key == "PATH").then(|| path.clone()),
            Some(self.empty.path().to_path_buf()),
        )
    }

    /// Runs a conversion to `output`, `on_progress` being called at the first progress report.
    fn run(
        &self,
        output: &Path,
        on_progress: impl Fn(),
    ) -> (Result<(), Error>, Vec<(Level, String)>) {
        let resolver = self.resolver();
        let prober = Prober::new(&resolver);
        let caps: HashMap<String, String> = [("encoders", "libx264,aac"), ("accels", "")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let services = PipelineServices {
            resolver: &resolver,
            prober: &prober,
            cpu_count: 4,
            temp_dir: self.tmp.clone(),
            free_gb: &|_| Ok(1000.0),
            available_memory: &|| Some(1 << 40),
        };
        let (cancel, signal) = (AtomicBool::new(false), AtomicI32::new(0));
        let ui = Ui {
            on_progress: Box::new(on_progress),
            shown: Cell::new(0),
        };
        take_log();
        let result = perform_encoding(
            None,
            &self.input,
            output,
            &ui,
            &caps,
            &services,
            &StopControl {
                cancel: &cancel,
                signal: &signal,
            },
        );
        (result.map(|_| ()), take_log())
    }
}

fn find<'a>(log: &'a [(Level, String)], starts: &str) -> Option<&'a (Level, String)> {
    log.iter().find(|(_, text)| text.starts_with(starts))
}

fn assert_timings_last(log: &[(Level, String)], what: &str) {
    let last = log
        .last()
        .unwrap_or_else(|| panic!("{what}: nothing was logged"));
    assert!(
        last.1
            .starts_with("Encoding stage timings: video_check_ms="),
        "{what}: the timings are not the last line:\n{log:#?}"
    );
    assert_eq!(last.0, Level::Info);
    for key in ["pgm_generation_ms=", "encoding_ms=", "cleanup_ms="] {
        assert!(last.1.contains(key), "{what}: {}", last.1);
    }
    assert_eq!(
        log.iter()
            .filter(|(_, t)| t.starts_with("Encoding stage timings"))
            .count(),
        1,
        "{what}: the timings are said once"
    );
}

// One test, because the scenarios share the logger and must not interleave.
#[test]
fn the_pipeline_log_tells_what_happened_and_ends_with_the_timings() {
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Trace);

    // ---- success ----
    let scene = Scene::new(PROBE);
    let output = scene.out_dir.join("output.mp4");
    let (result, log) = scene.run(&output, || {});
    result.expect("the conversion");
    let (level, done) = find(&log, "Encoding completed successfully").expect("the success line");
    assert_eq!(*level, Level::Info);
    for part in [
        "output_file=output.mp4",
        "encoder=libx264",
        "bitrate_bits_sec=2000000",
        "elapsed_time=",
    ] {
        assert!(done.contains(part), "{done}");
    }
    let profile = find(&log, "Machine profile analyzed").expect("the machine profile");
    assert!(profile.1.contains("cpu_cores=4") && profile.1.contains("selected_encoder=libx264"));
    assert_timings_last(&log, "success");

    // ---- a failure before the session: the timings are still said ----
    let (result, log) = scene.run(&PathBuf::from("relative.mp4"), || {});
    assert!(result.is_err());
    assert!(
        find(&log, "Encoding failed: stage=output_validation").is_some(),
        "{log:#?}"
    );
    assert_timings_last(&log, "early failure");

    // ---- a rename that fails ----
    let scene = Scene::new(PROBE);
    let output = scene.out_dir.join("output.mp4");
    let (result, log) = scene.run(&output, || {
        let _ = fs::create_dir(&output);
    });
    let err = result.expect_err("renaming onto a directory must fail");
    let (level, kept) = find(
        &log,
        "The finished video could not be moved into place; it is kept under its working name",
    )
    .expect("the error line");
    assert_eq!(*level, Level::Error);
    assert!(
        kept.contains("partial=") && kept.contains("destination="),
        "{kept}"
    );
    assert!(err.to_string().contains("The finished video is kept at "));
    assert!(find(&log, "Encoding failed: stage=finalize").is_some());
    assert!(find(&log, "Encoding completed successfully").is_none());
    assert_timings_last(&log, "failed rename");

    // ---- a session directory that cannot be removed: a warning, then the timings ----
    let scene = Scene::new(PROBE);
    let output = scene.out_dir.join("output.mp4");
    let tmp = scene.tmp.clone();
    let (result, log) = scene.run(&output, || {
        // The session exists now. Without write access to its parent it cannot be removed.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o500)).unwrap();
    });
    fs::set_permissions(&scene.tmp, fs::Permissions::from_mode(0o700)).unwrap();
    let removable = fs::read_dir(&scene.tmp).unwrap().count() == 0;
    if removable {
        // Running as an account that may remove it anyway (root): nothing to observe.
        assert!(result.is_ok());
    } else {
        // The conversion itself succeeded: a leftover temporary directory is not its error.
        result.expect("a session that cannot be removed does not fail the conversion");
        let (level, text) = find(&log, "Failed to cleanup encoding session").expect("the warning");
        assert_eq!(*level, Level::Warn);
        assert!(text.contains("error="), "{text}");
        assert_timings_last(&log, "session left behind");
    }
}
