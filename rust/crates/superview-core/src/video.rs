//! What ffprobe says about the input, and its validation.
//!
//! Port of `internal/common/video.go` (`VideoSpecs`, `parseFrameRate`,
//! `Validate`, `parseVideoSpecs`, `CheckVideo`). Verdicts are pinned by the parity oracle
//! `testdata/parity/specs.json`.
//!
//! `check_video` differs from `CheckVideo` in one respect: when ffprobe cannot even be
//! started, Go's message ends at "output:\n" with nothing after it; here the cause (the
//! `io::Error`) takes the place of the output, so the user learns that ffprobe is missing.

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, InvalidVideoError};
use crate::process::combined_output;
use crate::tools::ToolResolver;

/// Metadata about a video file extracted by ffprobe.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoSpecs {
    pub file: String,
    pub streams: Vec<VideoStream>,
}

/// Metadata about one video stream. The `Deserialize` side matches ffprobe's
/// JSON; the parsed fields (`duration_float`, `bitrate_int`, `frame_rate`) are
/// filled by [`parse_video_specs`].
///
/// Go's `int` is 64 bits wide, hence `i64`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct VideoStream {
    #[serde(rename = "codec_name", deserialize_with = "null_default")]
    pub codec: String,
    #[serde(deserialize_with = "null_default")]
    pub width: i64,
    #[serde(deserialize_with = "null_default")]
    pub height: i64,
    #[serde(deserialize_with = "null_default")]
    pub duration: String,
    #[serde(skip)]
    pub duration_float: f64,
    #[serde(rename = "bit_rate", deserialize_with = "null_default")]
    pub bitrate: String,
    #[serde(skip)]
    pub bitrate_int: i64,
    /// ffprobe's pixel format name; empty on very old ffprobe builds.
    #[serde(deserialize_with = "null_default")]
    pub pix_fmt: String,
    /// ffprobe's frame rate as a rational string, e.g. "30000/1001".
    #[serde(deserialize_with = "null_default")]
    pub r_frame_rate: String,
    /// `r_frame_rate` parsed, or 0 when missing or unusable.
    #[serde(skip)]
    pub frame_rate: f64,
}

/// A JSON `null` leaves the zero value, as encoding/json does.
fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// `strconv.ParseFloat(s, 64)` for the decimal and `inf`/`nan` forms, measured
/// against Go rather than assumed:
///
/// - a finite literal that overflows (`1e999`) is an error in Go, infinity in
///   Rust: refused here;
/// - a signed NaN (`-nan`, `+nan`) is an error in Go, accepted by Rust: refused;
/// - underscores are accepted by Go without any base prefix (`1_0` is 10) as
///   long as each one sits between two digits: reproduced.
///
/// Known differences, not reproduced because ffprobe never prints such text:
/// hexadecimal floats (`0x1p-2`), which Go accepts and this refuses; JSON keys,
/// which Go matches without regard to case and serde matches exactly; invalid
/// UTF-8 in the JSON, which Go replaces with U+FFFD and serde rejects.
pub(crate) fn parse_go_float(text: &str) -> Option<f64> {
    if !underscores_ok(text) {
        return None;
    }
    let cleaned = text.replace('_', "");
    let unsigned = cleaned.strip_prefix(['+', '-']);
    if let Some(rest) = unsigned
        && rest.eq_ignore_ascii_case("nan")
    {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    let named = unsigned.unwrap_or(&cleaned).to_ascii_lowercase();
    if value.is_infinite() && !named.starts_with("inf") {
        return None;
    }
    Some(value)
}

/// Go's `underscoreOK` for decimal text: an underscore must have a digit on
/// each side.
fn underscores_ok(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().all(|(i, &b)| {
        b != b'_'
            || (i > 0
                && bytes[i - 1].is_ascii_digit()
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
    })
}

/// `v <= 0` as Go writes it. Kept as a function so callers can negate it: with
/// NaN, `!at_most_zero(v)` is true while `v > 0.0` is false.
fn at_most_zero(v: f64) -> bool {
    v <= 0.0
}

/// Turns ffprobe's rational frame rate into frames per second.
///
/// Returns 0 for anything it cannot make sense of, including the "0/0" ffprobe
/// reports when the rate is unknown. 0 means "unknown" to every caller. Like
/// Go's, a NaN is not caught by the `<= 0` tests and comes out as NaN.
pub fn parse_frame_rate(rational: &str) -> f64 {
    let trimmed = rational.trim();
    let Some((num_text, den_text)) = trimmed.split_once('/') else {
        return match parse_go_float(trimmed) {
            Some(value) if !at_most_zero(value) => value,
            _ => 0.0,
        };
    };
    match (
        parse_go_float(num_text.trim()),
        parse_go_float(den_text.trim()),
    ) {
        (Some(num), Some(den)) if den != 0.0 && !at_most_zero(num) => num / den,
        _ => 0.0,
    }
}

impl VideoSpecs {
    /// Checks that the specs hold everything the pipeline needs.
    pub fn validate(&self) -> Result<(), InvalidVideoError> {
        let invalid = |reason: String| Err(InvalidVideoError { reason });
        let Some(stream) = self.streams.first() else {
            return invalid("no video streams found".into());
        };
        if stream.width <= 0 || stream.height <= 0 {
            return invalid(format!(
                "invalid dimensions: {}x{}",
                stream.width, stream.height
            ));
        }
        // Written as in Go: a NaN duration is not `<= 0` and passes.
        if stream.duration_float <= 0.0 {
            return invalid("invalid or missing duration".into());
        }
        if stream.bitrate_int <= 0 {
            return invalid("invalid or missing bitrate".into());
        }
        if stream.codec.is_empty() {
            return invalid("no codec information".into());
        }
        Ok(())
    }
}

/// The ffprobe command that reads the first video stream of `file` as JSON.
fn check_video_command(resolver: &ToolResolver, file: &str) -> std::process::Command {
    resolver.ffprobe_command([
        "-i",
        file,
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=codec_name,width,height,duration,bit_rate,pix_fmt,r_frame_rate",
        "-print_format",
        "json",
    ])
}

/// Loads and validates video metadata using ffprobe: codec, dimensions, duration and bitrate
/// of the first video stream (Go: `CheckVideo`). `InvalidVideo` if required metadata is
/// missing or invalid; `Plain` "ffprobe failed, output:\n..." if ffprobe fails.
pub fn check_video(resolver: &ToolResolver, file: &str) -> Result<VideoSpecs, Error> {
    let output = match combined_output(check_video_command(resolver, file)) {
        Ok((status, output)) if status.success() => output,
        Ok((_, output)) => return Err(Error::Plain(format!("ffprobe failed, output:\n{output}"))),
        Err(e) => return Err(Error::Plain(format!("ffprobe failed, output:\n{e}"))),
    };
    parse_video_specs(file, output.as_bytes())
}

/// Turns ffprobe's JSON into validated [`VideoSpecs`].
pub fn parse_video_specs(file: &str, out: &[u8]) -> Result<VideoSpecs, Error> {
    let plain = |msg: String| Error::Plain(msg);

    let root: Value = serde_json::from_slice(out)
        .map_err(|e| plain(format!("failed to parse video metadata: {e}")))?;
    let mut streams =
        streams_of(root).map_err(|e| plain(format!("failed to parse video metadata: {e}")))?;

    if streams.is_empty() {
        return Err(InvalidVideoError {
            reason: "no video streams in file".into(),
        }
        .into());
    }

    let first = &mut streams[0];
    first.duration_float = parse_go_float(&first.duration)
        .ok_or_else(|| plain(format!("invalid duration value '{}'", first.duration)))?;

    if first.bitrate.is_empty() {
        return Err(InvalidVideoError {
            reason: "bitrate information not available".into(),
        }
        .into());
    }
    first.bitrate_int = first
        .bitrate
        .parse::<i64>()
        .map_err(|_| plain(format!("invalid bitrate value '{}'", first.bitrate)))?;

    // A missing frame rate is not a reason to reject a file.
    first.frame_rate = parse_frame_rate(&first.r_frame_rate);

    let specs = VideoSpecs {
        file: file.to_owned(),
        streams,
    };
    specs.validate()?;
    Ok(specs)
}

/// Extracts `streams` the way encoding/json fills a struct: the top level must
/// be an object or null, `streams` an array or null, each element an object or
/// null. serde alone would also accept arrays standing in for structs.
fn streams_of(root: Value) -> Result<Vec<VideoStream>, String> {
    let mut object = match root {
        Value::Null => return Ok(Vec::new()),
        Value::Object(object) => object,
        _ => return Err("top level is not an object".into()),
    };
    let items = match object.remove("streams") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => return Err("streams is not an array".into()),
    };
    items
        .into_iter()
        .map(|item| match item {
            Value::Null => Ok(VideoStream::default()),
            Value::Object(_) => serde_json::from_value(item).map_err(|e| e.to_string()),
            _ => Err("stream is not an object".into()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // Go: TestVideoSpecs_ValidateValid
    #[test]
    fn validate_accepts_a_valid_video() {
        assert_eq!(specs(stream()).validate(), Ok(()));
    }

    // Go: TestVideoSpecs_ValidateNoStreams
    #[test]
    fn validate_refuses_no_streams() {
        let none = VideoSpecs {
            file: "test.mp4".into(),
            streams: vec![],
        };
        assert_eq!(reason(&none), "no video streams found");
    }

    // Go: TestVideoSpecs_ValidateInvalidDimensions
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

    // Go: TestVideoSpecs_ValidateInvalidDuration (and the "0" and NaN limits)
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
        // As in Go, NaN is not `<= 0` and passes.
        assert_eq!(with(f64::NAN).validate(), Ok(()));
    }

    // Go: TestVideoSpecs_ValidateInvalidBitrate
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

    // Go: TestVideoSpecs_ValidateMissingCodec
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

    // Go: TestParseFrameRate
    #[test]
    fn frame_rate_go_cases() {
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
    fn frame_rate_nan_is_not_caught_as_in_go() {
        for text in ["nan", "NaN", "nan/1", "30/nan", "NAN/NAN"] {
            assert!(parse_frame_rate(text).is_nan(), "{text}");
        }
        // A signed NaN does not parse in Go, hence 0.
        for text in ["-nan", "+nan", "-NaN/1"] {
            assert_eq!(parse_frame_rate(text), 0.0, "{text}");
        }
    }

    #[test]
    fn go_float_underscores_and_signs() {
        // Measured with strconv.ParseFloat.
        assert_eq!(parse_go_float("1_0"), Some(10.0));
        assert_eq!(parse_go_float("1_0.5"), Some(10.5));
        for bad in ["_1", "1_", "1__0", "1_.5", "1._5", "in_f", "1e_5"] {
            assert_eq!(parse_go_float(bad), None, "{bad}");
        }
        assert_eq!(parse_go_float("+Inf"), Some(f64::INFINITY));
        assert_eq!(parse_go_float("-infinity"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_go_float("1e999"), None);
        assert_eq!(parse_go_float("-1e999"), None);
        assert_eq!(parse_go_float("-nan"), None);
        assert_eq!(parse_go_float("+NaN"), None);
        assert!(parse_go_float("nan").unwrap().is_nan());
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
            .join("../../../internal/common/testdata/ffprobe")
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

    // No Go test of CheckVideo against a recorded output (the Go tests run the real ffprobe).
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
        // On Windows a child that does not find its program in its own PATH is also looked
        // for in the parent's, where CI has a real ffprobe: that one would start and fail
        // on clip.mp4, naming it. The io::Error of a program that cannot start never does.
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

    // Go: the integration tests' CheckVideo on a missing file.
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

    // Go: makeTestClip + CheckVideo (cancel_leak_test.go, integration tests).
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
}
