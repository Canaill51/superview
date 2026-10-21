//! Turning a failed ffmpeg run into an error that says what happened.
//!
//! The case worth naming is a kill signal nobody here sent. Everything this program sends
//! ffmpeg is accounted for before the call gets here (a cancellation and a system signal
//! both return their own reason), so a process killed from outside is the machine
//! intervening, and on Linux that is almost always the out-of-memory killer. The stderr of
//! an ordinary failure is attached unchanged: a kill leaves none, every other failure
//! needs it.
//!
//! Design notes (exhaustive):
//! - The memory reading is a parameter of [`classify_ffmpeg_failure`];
//!   [`classify_ffmpeg_failure_now`] is the one that takes the reading. Nothing here reads the environment, so tests need no machine to be short
//!   of memory.
//! - An ordinary failure reads "exit status: 1" on Unix and "exit code: 1" on Windows
//!   (Rust's [`ExitStatus`] text; v0.2.9 read "exit status 1"), and the underlying error is
//!   not wrapped: Rust's `ExitStatus` is a value, not an error to keep reachable. Only the
//!   leading text is comparable.
//! - The kill is detected by [`killed_by_signal`] on the [`ExitStatus`], so on Windows no
//!   run is ever diagnosed as a kill (see `process.rs`).
//! - The log line is a `log` record with `key=value` text.

use std::process::ExitStatus;

use crate::error::{EncoderError, Error};
use crate::memory::{UNKNOWN_MEMORY, format_available_memory};
use crate::process::killed_by_signal;

/// `classifyFfmpegFailure`, with the memory reading ([`format_available_memory`]'s text, or
/// [`UNKNOWN_MEMORY`]) given by the caller.
pub fn classify_ffmpeg_failure(status: &ExitStatus, stderr: &str, memory_available: &str) -> Error {
    let Some(signal) = killed_by_signal(status) else {
        return Error::Plain(if stderr.is_empty() {
            format!("ffmpeg failed: {status}")
        } else {
            format!("ffmpeg failed: {status}\nffmpeg stderr:\n{stderr}")
        });
    };

    log::error!(
        "ffmpeg was killed by the system: signal={signal} memory_available={memory_available}"
    );

    let memory = if memory_available == UNKNOWN_MEMORY {
        String::new()
    } else {
        format!(" Memory available when it stopped: {memory_available}.")
    };

    Error::Encoder(EncoderError {
        msg: format!(
            "ffmpeg was killed by the system (signal: {signal}). Nothing in Superview sent that \
             signal, which on this platform almost always means the machine ran out of memory.\
             {memory} Close other applications, choose a hardware encoder if this machine offers \
             one, or convert a video with fewer pixels."
        ),
    })
}

/// [`classify_ffmpeg_failure`] with the reading taken now.
pub fn classify_ffmpeg_failure_now(status: &ExitStatus, stderr: &str) -> Error {
    classify_ffmpeg_failure(status, stderr, &format_available_memory())
}

#[cfg(test)]
mod tests;
