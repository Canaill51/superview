//! The harness binary against the machine's real ffmpeg. No fake tool here: a tool that is
//! not found is not used. Without ffmpeg the tests skip, unless `SUPERVIEW_REQUIRE_FFMPEG=1`
//! (CI), where its absence fails them. They start processes, so they run one at a time.
//!
//! Not covered: the Windows stop path (`ctrlc`, which stores 2 without being told the
//! signal). No test sends a Ctrl+C there; the signal tests below are Unix only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard};

use serde_json::Value;
use superview_core::tools::ToolResolver;
use tempfile::TempDir;

const HARNESS: &str = env!("CARGO_BIN_EXE_superview-harness");

static SERIAL: Mutex<()> = Mutex::new(());

/// Takes the lock, and is `None` (test skipped) when there is no usable ffmpeg. The tools
/// are found the way the harness finds them (`ToolResolver::from_process`).
fn real_ffmpeg() -> Option<MutexGuard<'static, ()>> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
    let resolver = ToolResolver::from_process();
    let has = |mut cmd: Command| {
        cmd.arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if has(resolver.ffmpeg_command(Vec::<&str>::new()))
        && has(resolver.ffprobe_command(Vec::<&str>::new()))
    {
        Some(guard)
    } else {
        assert!(
            !require,
            "no ffmpeg/ffprobe although SUPERVIEW_REQUIRE_FFMPEG=1"
        );
        eprintln!("skipped: no ffmpeg");
        None
    }
}

/// A 4:3 clip made by ffmpeg itself.
fn clip(dir: &Path, size: &str, seconds: u32) -> PathBuf {
    let file = dir.join("clip.mp4");
    let out = ToolResolver::from_process()
        .ffmpeg_command(Vec::<&str>::new())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!("testsrc=size={size}:rate=30"))
        .args(["-t", &seconds.to_string()])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-b:v",
            "2M",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&file)
        .output()
        .expect("ffmpeg starts");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    file
}

fn dimensions(file: &Path) -> String {
    let out = ToolResolver::from_process()
        .ffprobe_command(Vec::<&str>::new())
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .arg("stream=width,height")
        .args(["-of", "csv=s=x:p=0"])
        .arg(file)
        .output()
        .expect("ffprobe starts");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run(args: &[&str]) -> Output {
    Command::new(HARNESS)
        .args(args)
        .output()
        .expect("harness starts")
}

/// Stdout must be exactly one JSON value.
fn json_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not pure JSON ({e}): {:?} / stderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn convert_widens_to_16_9() {
    let Some(_lock) = real_ffmpeg() else { return };
    let dir = TempDir::new().unwrap();
    let input = clip(dir.path(), "640x480", 3);
    let output = dir.path().join("out.mp4");
    let out = run(&["convert", input.to_str().unwrap(), output.to_str().unwrap()]);
    let json = json_of(&out);
    assert_eq!(out.status.code(), Some(0), "{json}");
    assert_eq!(json["ok"], true, "{json}");
    assert!(
        json["encoder"].is_string() && json["bitrate"].is_number(),
        "{json}"
    );
    assert!(json["hardware_acceleration"].is_string(), "{json}");
    assert!(
        json["metrics"]["output_file_size"].as_i64().unwrap() > 0,
        "{json}"
    );
    assert_eq!(dimensions(&output), "852x480");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.lines().any(|l| l.starts_with("progress ")),
        "stderr: {stderr}"
    );
}

#[test]
fn convert_squeeze_keeps_the_width() {
    let Some(_lock) = real_ffmpeg() else { return };
    let dir = TempDir::new().unwrap();
    let input = clip(dir.path(), "640x480", 3);
    let output = dir.path().join("out.mp4");
    let out = run(&[
        "convert",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--squeeze",
    ]);
    assert_eq!(json_of(&out)["ok"], true);
    assert_eq!(dimensions(&output), "640x480");
}

#[test]
fn convert_failure_is_json_with_a_kind() {
    let Some(_lock) = real_ffmpeg() else { return };
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("not-a-video.mp4");
    std::fs::write(&input, b"this is not a video").unwrap();
    let output = dir.path().join("out.mp4");
    let out = run(&["convert", input.to_str().unwrap(), output.to_str().unwrap()]);
    let json = json_of(&out);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(json["ok"], false);
    // The core wraps the ffprobe failure into a plain error.
    assert_eq!(json["kind"], "other", "{json}");
    assert!(json["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert!(!output.exists());
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        left,
        ["not-a-video.mp4"],
        "a file was left beside the input"
    );
}

#[test]
fn probe_announces_libx264() {
    let Some(_lock) = real_ffmpeg() else { return };
    let out = run(&["probe"]);
    let json = json_of(&out);
    assert_eq!(out.status.code(), Some(0), "{json}");
    let encoders = json["ffmpeg"]["encoders"].as_array().unwrap();
    assert!(encoders.iter().any(|e| e == "libx264"), "{json}");
    assert!(json["probe"].is_array(), "{json}");
}

#[test]
fn wrong_arguments_exit_with_2() {
    for args in [
        &[][..],
        &["frobnicate"],
        &["probe", "extra"],
        &["convert"],
        &["convert", "a.mp4", "b.mp4", "--nope"],
        &["convert", "a.mp4", "b.mp4", "--bitrate", "abc"],
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(!out.stderr.is_empty(), "{args:?}");
    }
}

/// A running harness that cannot outlive a failing test: dropping it kills it (and its whole
/// process group when it has one) and reaps it. Stderr and stdout are read by threads, so
/// that no pipe fills up and every wait has a limit.
#[cfg(unix)]
struct Running {
    child: std::process::Child,
    group: bool,
    stderr: std::sync::mpsc::Receiver<String>,
    stdout: std::sync::mpsc::Receiver<String>,
}

#[cfg(unix)]
impl Drop for Running {
    fn drop(&mut self) {
        if self.group {
            let _ = Command::new("kill")
                .args(["-KILL", "--"])
                .arg(format!("-{}", self.child.id()))
                .stderr(std::process::Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
impl Running {
    /// `group`: the harness leads a process group of its own, like a program started from a
    /// terminal. `the environment pairs are given to the child only; the test's own environment is never
    /// changed (L-114).
    fn spawn(args: &[&str], env: &[(&str, &Path)], group: bool) -> Self {
        use std::io::{BufRead, BufReader, Read};
        use std::os::unix::process::CommandExt;
        use std::process::Stdio;
        use std::sync::mpsc;

        let mut cmd = Command::new(HARNESS);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (key, value) in env {
            cmd.env(key, value);
        }
        if group {
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().expect("harness starts");
        let (err_tx, stderr) = mpsc::channel();
        let pipe = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if err_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let (out_tx, stdout) = mpsc::channel();
        let mut pipe = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            let _ = out_tx.send(text);
        });
        Self {
            child,
            group,
            stderr,
            stdout,
        }
    }

    /// Waits (at most 60 s) for the first stderr line that starts with `prefix`.
    fn wait_for_line(&self, prefix: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.stderr.recv_timeout(left) {
                Ok(line) if line.starts_with(prefix) => return,
                Ok(_) => {}
                Err(e) => panic!("no stderr line starting with {prefix:?} within 60 s ({e})"),
            }
        }
    }

    /// Sends `signal` ("TERM", "INT") to the harness, or to its whole group.
    fn send(&self, signal: &str, to_group: bool) {
        let pid = self.child.id();
        let target = if to_group {
            format!("-{pid}")
        } else {
            pid.to_string()
        };
        let sent = Command::new("kill")
            .arg(format!("-{signal}"))
            .arg("--")
            .arg(target)
            .status()
            .expect("kill starts");
        assert!(sent.success());
    }

    /// Waits (at most 30 s) for the end; returns the exit code and what stdout held. A
    /// harness that does not end is killed and the test fails with a message that says so.
    fn finish(mut self) -> (Option<i32>, String) {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "the harness did not end within 30 s of the signal (killed by the test)"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        let stdout = self
            .stdout
            .recv_timeout(Duration::from_secs(10))
            .expect("stdout was not closed after the harness ended");
        (status.code(), stdout)
    }
}

/// What a stopped conversion must leave: nothing.
#[cfg(unix)]
fn assert_nothing_left(output: &Path, out_dir: &Path, tmp: &Path) {
    assert!(!output.exists(), "the output was left behind");
    let names = |dir: &Path| -> Vec<_> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect()
    };
    assert!(
        names(out_dir).is_empty(),
        "partial file left: {:?}",
        names(out_dir)
    );
    assert!(
        names(tmp).is_empty(),
        "session directory left: {:?}",
        names(tmp)
    );
    // The session paths are in ffmpeg's argv, so a survivor shows up under the temp path.
    let alive = Command::new("pgrep")
        .arg("-f")
        .arg(tmp)
        .output()
        .expect("pgrep starts");
    assert!(
        alive.stdout.is_empty(),
        "an ffmpeg survived: {}",
        String::from_utf8_lossy(&alive.stdout)
    );
}

/// Sends `signal` while a long conversion runs and checks that nothing is left behind. The
/// child's temporary directory is the test's own, given through its environment. With
/// `ctrl_c_like` the harness leads its own process group and the signal goes to the whole
/// group, ffmpeg included, as a terminal's Ctrl+C does.
#[cfg(unix)]
fn stop_with_signal(signal: &str, expected_word: &str, ctrl_c_like: bool) {
    let Some(_lock) = real_ffmpeg() else { return };
    let work = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let input = clip(work.path(), "1600x1200", 20);
    let output = out_dir.path().join("out.mp4");

    let running = Running::spawn(
        // The CPU encoder is forced: a GPU would finish the clip in two seconds and race the
        // signal, where libx264 on 1600x1200 keeps the run long on any machine.
        &[
            "convert",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--encoder",
            "libx264",
        ],
        &[("TMPDIR", tmp.path())],
        ctrl_c_like,
    );
    running.wait_for_line("progress ");
    running.send(signal, ctrl_c_like);
    let (code, stdout) = running.finish();

    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
    assert_eq!(code, Some(1), "{json}");
    assert_eq!(json["ok"], false, "{json}");
    assert_eq!(json["kind"], "signal", "{json}");
    assert!(
        json["error"].as_str().unwrap().contains(expected_word),
        "{json}"
    );
    assert_nothing_left(&output, out_dir.path(), tmp.path());
}

#[cfg(unix)]
#[test]
fn sigterm_stops_cleanly() {
    stop_with_signal("TERM", "terminated", false);
}

#[cfg(unix)]
#[test]
fn sigint_stops_cleanly() {
    stop_with_signal("INT", "interrupt", false);
}

#[cfg(unix)]
#[test]
fn ctrl_c_on_the_whole_process_group_stops_cleanly() {
    stop_with_signal("INT", "interrupt", true);
}

/// A signal during `probe` must never end in `ok:true`. Determinism: a directory first in
/// PATH holds an `ffmpeg` that only waits one second, then runs the real one, so the probe
/// is certainly still running when the signal comes; the marker file says the harness has
/// started (its handlers are installed before it runs any tool).
#[cfg(unix)]
#[test]
fn sigterm_during_probe_is_a_signal_failure() {
    use std::os::unix::fs::PermissionsExt;

    let Some(_lock) = real_ffmpeg() else { return };
    let real = ToolResolver::from_process()
        .ffmpeg_command(Vec::<&str>::new())
        .get_program()
        .to_owned();
    let shim_dir = TempDir::new().unwrap();
    let marker = shim_dir.path().join("started");
    let shim = shim_dir.path().join("ffmpeg");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\ntouch '{}'\nsleep 1\nexec '{}' \"$@\"\n",
            marker.display(),
            real.to_string_lossy()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        shim_dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let running = Running::spawn(&["probe"], &[("PATH", Path::new(&path))], false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !marker.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the probe never ran ffmpeg"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    running.send("TERM", false);
    let (code, stdout) = running.finish();
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
    assert_eq!(code, Some(1), "{json}");
    assert_eq!(json["ok"], false, "{json}");
    assert_eq!(json["kind"], "signal", "{json}");
    assert!(
        json["error"].as_str().unwrap().contains("terminated"),
        "{json}"
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_argument_exits_with_2() {
    use std::os::unix::ffi::OsStrExt;
    let out = Command::new(HARNESS)
        .arg("convert")
        .arg(std::ffi::OsStr::from_bytes(b"in\xff.mp4"))
        .arg("out.mp4")
        .output()
        .expect("harness starts");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("UTF-8"));
}
