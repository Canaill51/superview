use super::*;
use crate::process::combined_output;

fn stream() -> VideoStream {
    VideoStream {
        codec: "h264".into(),
        width: 1920,
        height: 1080,
        duration: "60.5".into(),
        duration_float: 60.5,
        bitrate: "5000000".into(),
        bitrate_int: 5_000_000,
        ..VideoStream::default()
    }
}

fn specs(stream: VideoStream) -> VideoSpecs {
    VideoSpecs {
        file: "test.mp4".into(),
        streams: vec![stream],
    }
}

fn reason(specs: &VideoSpecs) -> String {
    specs.validate().unwrap_err().reason
}

#[test]
fn validate_accepts_a_valid_video() {
    assert_eq!(specs(stream()).validate(), Ok(()));
}

#[test]
fn validate_refuses_no_streams() {
    let none = VideoSpecs {
        file: "test.mp4".into(),
        streams: vec![],
    };
    assert_eq!(reason(&none), "no video streams found");
}

#[test]
fn validate_refuses_bad_dimensions() {
    for (w, h) in [(0, 1080), (1920, 0), (-100, 1080), (1920, -100)] {
        let s = specs(VideoStream {
            width: w,
            height: h,
            ..stream()
        });
        assert_eq!(reason(&s), format!("invalid dimensions: {w}x{h}"));
    }
}

// Includes the "0" and NaN limits.
#[test]
fn validate_duration_limits() {
    let with = |d: f64| {
        specs(VideoStream {
            duration: d.to_string(),
            duration_float: d,
            ..stream()
        })
    };
    for bad in [0.0, -1.0] {
        assert_eq!(reason(&with(bad)), "invalid or missing duration");
    }
    assert_eq!(with(f64::MIN_POSITIVE).validate(), Ok(()));
    // DEC-53: a duration that is not a finite number is refused, where v0.2.9 let NaN through
    // (`NaN <= 0` is false) and inf became a demand for 8 TB of free space.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(reason(&with(bad)), "invalid or missing duration", "{bad}");
    }
}

// DEC-53 from the other end: what ffprobe prints reaches the same refusal.
#[test]
fn a_duration_of_inf_or_nan_in_ffprobes_json_is_refused() {
    for text in ["inf", "Inf", "+inf", "nan", "NaN"] {
        let json = format!(
            r#"{{"streams":[{{"codec_name":"h264","width":1440,"height":1080,"duration":"{text}","bit_rate":"8000000","pix_fmt":"yuv420p","r_frame_rate":"30/1"}}]}}"#
        );
        let err = parse_video_specs("clip.mp4", json.as_bytes())
            .and_then(|specs| specs.validate().map(|()| specs).map_err(Error::from))
            .unwrap_err();
        assert!(
            matches!(&err, Error::InvalidVideo(v) if v.reason == "invalid or missing duration"),
            "{text}: {err:?}"
        );
    }
}

#[test]
fn validate_bitrate_limits() {
    for bad in [0, -1] {
        let s = specs(VideoStream {
            bitrate: bad.to_string(),
            bitrate_int: bad,
            ..stream()
        });
        assert_eq!(reason(&s), "invalid or missing bitrate");
    }
    let one = specs(VideoStream {
        bitrate: "1".into(),
        bitrate_int: 1,
        ..stream()
    });
    assert_eq!(one.validate(), Ok(()));
}

#[test]
fn validate_refuses_a_missing_codec() {
    let s = specs(VideoStream {
        codec: String::new(),
        ..stream()
    });
    assert_eq!(reason(&s), "no codec information");
}

// The checks run in a fixed order: dimensions, duration, bitrate, codec.
#[test]
fn validate_reports_the_first_defect() {
    let all = VideoStream {
        codec: String::new(),
        width: 0,
        duration_float: 0.0,
        bitrate_int: 0,
        ..stream()
    };
    assert_eq!(reason(&specs(all.clone())), "invalid dimensions: 0x1080");
    let no_dims = VideoStream {
        width: 1920,
        ..all.clone()
    };
    assert_eq!(reason(&specs(no_dims)), "invalid or missing duration");
    let no_dur = VideoStream {
        width: 1920,
        duration_float: 1.0,
        ..all.clone()
    };
    assert_eq!(reason(&specs(no_dur)), "invalid or missing bitrate");
    let only_codec = VideoStream {
        width: 1920,
        duration_float: 1.0,
        bitrate_int: 1,
        ..all
    };
    assert_eq!(reason(&specs(only_codec)), "no codec information");
}

#[test]
fn frame_rate_cases() {
    for (text, want) in [
        ("60/1", 60.0),
        ("120/1", 120.0),
        ("24000/1001", 24000.0 / 1001.0),
        ("0/1", 0.0),
        ("N/A", 0.0),
        ("abc/def", 0.0),
        ("30", 30.0),
    ] {
        assert!((parse_frame_rate(text) - want).abs() < 0.001, "{text}");
    }
}

#[test]
fn frame_rate_nan_is_not_caught_as_in_v0_2_9() {
    for text in ["nan", "NaN", "nan/1", "30/nan", "NAN/NAN"] {
        assert!(parse_frame_rate(text).is_nan(), "{text}");
    }
    // A signed NaN does not parse, hence 0.
    for text in ["-nan", "+nan", "-NaN/1"] {
        assert_eq!(parse_frame_rate(text), 0.0, "{text}");
    }
}

#[test]
fn float_underscores_and_signs() {
    // Measured with strconv.ParseFloat.
    assert_eq!(parse_ffprobe_float("1_0"), Some(10.0));
    assert_eq!(parse_ffprobe_float("1_0.5"), Some(10.5));
    for bad in ["_1", "1_", "1__0", "1_.5", "1._5", "in_f", "1e_5"] {
        assert_eq!(parse_ffprobe_float(bad), None, "{bad}");
    }
    assert_eq!(parse_ffprobe_float("+Inf"), Some(f64::INFINITY));
    assert_eq!(parse_ffprobe_float("-infinity"), Some(f64::NEG_INFINITY));
    assert_eq!(parse_ffprobe_float("1e999"), None);
    assert_eq!(parse_ffprobe_float("-1e999"), None);
    assert_eq!(parse_ffprobe_float("-nan"), None);
    assert_eq!(parse_ffprobe_float("+NaN"), None);
    assert!(parse_ffprobe_float("nan").unwrap().is_nan());
    assert_eq!(parse_frame_rate("1_0/1"), 10.0);
}

#[test]
fn frame_rate_forms() {
    assert_eq!(parse_frame_rate("30/1"), 30.0);
    assert_eq!(parse_frame_rate(" 25 "), 25.0);
    assert_eq!(parse_frame_rate("0/0"), 0.0);
    assert_eq!(parse_frame_rate("30/0"), 0.0);
    assert_eq!(parse_frame_rate("-5/1"), 0.0);
    assert_eq!(parse_frame_rate(""), 0.0);
    assert_eq!(parse_frame_rate("abc/1"), 0.0);
    assert_eq!(parse_frame_rate("1e999"), 0.0);
    assert!((parse_frame_rate("30000/1001") - 29.97002997002997).abs() < 1e-12);
}

#[test]
fn json_shapes_encoding_json_would_reject() {
    for bad in [
        &b"[]"[..],
        br#"{"streams":{}}"#,
        br#"{"streams":[[]]}"#,
        br#"{"streams":[1]}"#,
    ] {
        assert!(
            matches!(parse_video_specs("f", bad), Err(Error::Plain(_))),
            "{bad:?}"
        );
    }
    for empty in [&b"null"[..], br#"{"streams":null}"#, b"{}"] {
        assert!(matches!(
            parse_video_specs("f", empty),
            Err(Error::InvalidVideo(_))
        ));
    }
}

// ---- check_video ----

use crate::probe::tests::fake_tools;
use crate::process::tests::spawn_lock;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ffprobe")
        .join(name)
}

/// A stand-in ffprobe printing the content of `fixture`, copied next to a canonical
/// path so the script quotes a plain Windows path too.
fn ffprobe_printing(fixture_name: &str) -> (crate::probe::tests::FakeTools, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let copy = dir.path().join("out.json");
    std::fs::copy(fixture(fixture_name), &copy).unwrap();
    let copy = dunce::canonicalize(copy).unwrap();
    #[cfg(unix)]
    let body = format!("cat '{}'", copy.display());
    #[cfg(windows)]
    let body = format!("type \"{}\"", copy.display());
    (fake_tools(&[("ffprobe", &body)]), dir)
}

// check_video against a recorded output (the real ffprobe runs in the integration tests).
#[test]
fn check_video_parses_what_ffprobe_prints() {
    let _guard = spawn_lock();
    let (tools, _dir) = ffprobe_printing("valid-h264-4x3.json");
    let specs = check_video(&tools.resolver, "clip.mp4").unwrap();
    assert_eq!(specs.file, "clip.mp4");
    let stream = &specs.streams[0];
    assert_eq!(
        (stream.codec.as_str(), stream.width, stream.height),
        ("h264", 1440, 1080)
    );
    assert_eq!(stream.bitrate_int, 192_648);
    assert_eq!(stream.duration_float, 1.0);
    assert_eq!(stream.frame_rate, 30.0);
}

#[test]
fn check_video_hands_a_bad_stream_to_the_validation() {
    let _guard = spawn_lock();
    let (tools, _dir) = ffprobe_printing("no-streams.json");
    let err = check_video(&tools.resolver, "clip.mp4").unwrap_err();
    assert!(
        matches!(&err, Error::InvalidVideo(v) if v.reason == "no video streams in file"),
        "{err:?}"
    );
}

#[test]
fn check_video_reports_a_failing_ffprobe_with_its_output() {
    let _guard = spawn_lock();
    #[cfg(unix)]
    let body = "echo 'broken file' 1>&2\nexit 1";
    #[cfg(windows)]
    let body = ">&2 echo broken file\r\nexit /b 1";
    let tools = fake_tools(&[("ffprobe", body)]);
    let err = check_video(&tools.resolver, "clip.mp4").unwrap_err();
    assert!(matches!(err, Error::Plain(_)), "{err:?}");
    // A batch file adds a carriage return.
    assert_eq!(
        err.to_string().trim_end(),
        "ffprobe failed, output:\nbroken file"
    );
}

// F-05: a warning on stderr must not be read as JSON; only stdout is parsed.
#[test]
fn check_video_parses_stdout_alone_when_ffprobe_warns_on_stderr() {
    let _guard = spawn_lock();
    let json = r#"{"streams":[{"codec_name":"h264","width":1440,"height":1080,"duration":"10.0","bit_rate":"8000000","pix_fmt":"yuv420p","r_frame_rate":"30/1"}]}"#;
    #[cfg(unix)]
    let body = format!("echo 'deprecated pixel format used' 1>&2\necho '{json}'");
    #[cfg(windows)]
    let body = format!(">&2 echo deprecated pixel format used\r\necho {json}");
    let tools = fake_tools(&[("ffprobe", &body)]);
    let specs = check_video(&tools.resolver, "clip.mp4").expect("stdout holds valid JSON");
    assert_eq!(specs.streams[0].codec, "h264");
}

// Unix only: on Windows a child that does not find its program in its own PATH is also
// looked for in the parent's, where CI has a real ffprobe. That one starts and fails on
// clip.mp4, so "ffprobe is missing" cannot be simulated there.
#[cfg(unix)]
#[test]
fn check_video_reports_a_missing_ffprobe() {
    let _guard = spawn_lock();
    let empty = tempfile::TempDir::new().unwrap();
    let path = empty.path().to_path_buf();
    let resolver = ToolResolver::new(
        move |key| (key == "PATH").then(|| path.clone().into_os_string()),
        Some(empty.path().to_path_buf()),
    );
    let err = check_video(&resolver, "clip.mp4").unwrap_err();
    // The io::Error of a program that cannot start never names the file.
    assert!(
        matches!(&err, Error::Plain(m) if m.starts_with("ffprobe failed, output:\n")
                && m.len() > 24 && !m.contains("clip.mp4")),
        "{err:?}"
    );
}

// The argv is part of the contract with ffprobe: the file goes through `-i`, whatever
// its name, and the JSON carries exactly the fields parse_video_specs reads.
#[test]
fn check_video_asks_ffprobe_for_the_first_video_stream_as_json() {
    let resolver = ToolResolver::new(|_| None, None);
    let cmd = check_video_command(&resolver, "-weird name.mp4");
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            // DEC-52: the demuxer is named, not guessed from the content.
            "-f",
            "mov",
            "-i",
            "-weird name.mp4",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,duration,bit_rate,pix_fmt,r_frame_rate",
            "-print_format",
            "json"
        ]
    );
}

/// The machine's ffprobe, or `None` where there is none -- unless
/// `SUPERVIEW_REQUIRE_FFMPEG=1`, which CI sets, where its absence fails the test.
fn real_ffprobe() -> Option<ToolResolver> {
    let resolver = ToolResolver::from_process();
    let works = combined_output(resolver.ffprobe_command(["-version"]))
        .is_ok_and(|(status, _)| status.success());
    let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
    assert!(
        works || !require,
        "no usable ffprobe although SUPERVIEW_REQUIRE_FFMPEG=1"
    );
    works.then_some(resolver)
}

// check_video on a missing file.
#[test]
fn check_video_refuses_a_file_that_does_not_exist() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffprobe() else {
        return;
    };
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("missing.mp4");
    let err = check_video(&resolver, &missing.to_string_lossy()).unwrap_err();
    assert!(
        matches!(&err, Error::Plain(m) if m.starts_with("ffprobe failed, output:\n")),
        "{err:?}"
    );
}

// A generated test clip, then check_video.
#[test]
fn check_video_reads_a_real_clip() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffprobe() else {
        return;
    };
    let dir = tempfile::TempDir::new().unwrap();
    let clip = dir.path().join("clip.mp4");
    let mut make = resolver.ffmpeg_command([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=640x480:rate=10:duration=2",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-y",
    ]);
    make.arg(&clip);
    let (status, output) = combined_output(make).unwrap();
    if !status.success() {
        assert!(
            std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() != Ok("1"),
            "cannot make a test clip although SUPERVIEW_REQUIRE_FFMPEG=1: {output}"
        );
        return;
    }
    let specs = check_video(&resolver, &clip.to_string_lossy()).unwrap();
    let stream = &specs.streams[0];
    assert_eq!(
        (stream.codec.as_str(), stream.width, stream.height),
        ("h264", 640, 480)
    );
    assert_eq!(stream.frame_rate, 10.0);
    assert!(
        stream.duration_float > 1.0 && stream.bitrate_int > 0,
        "{stream:?}"
    );
}

// DEC-52: a text file named .mp4 that ffprobe would read as a concat script, and so follow to
// another file, is refused; the real clip beside it is still read.
#[test]
fn check_video_refuses_a_concat_script_named_mp4() {
    let _guard = spawn_lock();
    let Some(resolver) = real_ffprobe() else {
        return;
    };
    let dir = tempfile::TempDir::new().unwrap();
    let clip = dir.path().join("clip.mp4");
    let mut make = resolver.ffmpeg_command([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x240:rate=10:duration=1",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-y",
    ]);
    make.arg(&clip);
    let (status, output) = combined_output(make).unwrap();
    if !status.success() {
        assert!(
            std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() != Ok("1"),
            "cannot make a test clip although SUPERVIEW_REQUIRE_FFMPEG=1: {output}"
        );
        return;
    }
    let script = dir.path().join("script.mp4");
    std::fs::write(&script, "ffconcat version 1.0\nfile clip.mp4\n").unwrap();
    // Without the forced demuxer this is the very file the test would otherwise pass on.
    assert!(check_video(&resolver, &clip.to_string_lossy()).is_ok());
    let err = check_video(&resolver, &script.to_string_lossy()).unwrap_err();
    assert!(
        matches!(&err, Error::Plain(m) if m.starts_with("ffprobe failed, output:\n")),
        "{err:?}"
    );
}

// F-17: an ffprobe stuck on a frozen share ends in an error, not a hung thread.
#[cfg(unix)]
#[test]
fn check_video_gives_up_on_an_ffprobe_that_never_answers() {
    let _guard = spawn_lock();
    let tools = fake_tools(&[("ffprobe", "exec sleep 30")]);
    let resolver = tools
        .resolver
        .with_query_timeout(std::time::Duration::from_secs(1));
    let started = std::time::Instant::now();
    let err = check_video(&resolver, "clip.mp4").unwrap_err();
    assert!(
        matches!(&err, Error::Plain(m) if m.starts_with("ffprobe failed, output:\n")),
        "{err:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}
