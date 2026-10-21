//! What ffprobe says about the input, and its validation.
//!
//! Verdicts are pinned by the parity oracle `tests/fixtures/parity/specs.json`.
//!
//! When ffprobe cannot even be started, `check_video`'s message has the cause (the
//! `io::Error`) in place of ffprobe's output, so the user learns that ffprobe is missing.

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, InvalidVideoError};
use crate::process::split_output_within;
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
/// `i64`: the range v0.2.9 read them into, which the parity fixtures assume.
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

/// Float parsing with the rules the parity fixtures recorded, those of Go's
/// `strconv.ParseFloat(s, 64)` for the decimal and `inf`/`nan` forms, measured rather
/// than assumed:
///
/// - a finite literal that overflows (`1e999`) is an error there, infinity in Rust:
///   refused here;
/// - a signed NaN (`-nan`, `+nan`) is an error there, accepted by Rust: refused;
/// - underscores are accepted without any base prefix (`1_0` is 10) as long as each one
///   sits between two digits: reproduced.
///
/// Known differences, not reproduced because ffprobe never prints such text:
/// hexadecimal floats (`0x1p-2`), which the Go parser accepts and this refuses; JSON keys,
/// which the Go decoder matches without regard to case and serde matches exactly; invalid
/// UTF-8 in the JSON, which the Go decoder replaces with U+FFFD and serde rejects.
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

/// For decimal text, an underscore must have a digit on each side.
fn underscores_ok(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().all(|(i, &b)| {
        b != b'_'
            || (i > 0
                && bytes[i - 1].is_ascii_digit()
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
    })
}

/// `v <= 0`. Kept as a function so callers can negate it: with
/// NaN, `!at_most_zero(v)` is true while `v > 0.0` is false.
fn at_most_zero(v: f64) -> bool {
    v <= 0.0
}

/// Turns ffprobe's rational frame rate into frames per second.
///
/// Returns 0 for anything it cannot make sense of, including the "0/0" ffprobe
/// reports when the rate is unknown. 0 means "unknown" to every caller. A NaN is not
/// caught by the `<= 0` tests and comes out as NaN.
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
        // DEC-53: not finite is refused too. v0.2.9's bare `<= 0` let NaN through, and inf became
        // a demand for 8 TB of free space.
        if !stream.duration_float.is_finite() || stream.duration_float <= 0.0 {
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
        // DEC-52: MP4 in. Without this ffprobe guesses the format from the content, and a
        // text file named .mp4 read as a concat script sends it to other files.
        "-f",
        "mov",
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
/// of the first video stream. `InvalidVideo` if required metadata is
/// missing or invalid; `Plain` "ffprobe failed, output:\n..." if ffprobe fails.
pub fn check_video(resolver: &ToolResolver, file: &str) -> Result<VideoSpecs, Error> {
    // Only stdout is JSON: a warning ffprobe prints on stderr must not be parsed with it.
    let output = match split_output_within(
        check_video_command(resolver, file),
        resolver.query_timeout(),
    ) {
        Ok((status, stdout, _)) if status.success() => stdout,
        Ok((_, stdout, stderr)) => {
            let gap = if stderr.is_empty() || stderr.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            return Err(Error::Plain(format!(
                "ffprobe failed, output:\n{stderr}{gap}{stdout}"
            )));
        }
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
mod tests;
