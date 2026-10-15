//! Typed errors, mirroring `internal/common/errors.go`.
//!
//! The Display text of each variant is the Go `Error()` text, so a message can
//! be compared against the parity oracle as it stands.
//!
//! Deliberate differences from the Go code:
//! - The three sentinels of the fallback cascade (`ErrCancelled`, `ErrStoppedBySignal`,
//!   `errFailedAfterOutput`) are variants of [`Error`], not values compared with
//!   `errors.Is`. Go wraps the signal name into the message with `%w`, so the value is not
//!   the sentinel itself; here [`Error::StoppedBySignal`] carries the name. `errors.Is`
//!   looking through a wrapped error is reproduced only for [`Error::FailedAfterOutput`],
//!   the one variant that wraps another (`fmt.Errorf("%w: %w", ...)`); Go's `errors.Join`
//!   has no use in the port.
//! - The signal name is the Linux table of `process.rs` on every platform
//!   ([`signal_description`]): Go's `os.Interrupt` and `SIGTERM` read "interrupt" and
//!   "terminated" on Windows too. (Go's Windows table only names signals 1 to 15 and prints
//!   "signal 16" beyond; the port says "stack fault". No effect: only 2 and 15 are listened to.)

use std::error::Error as StdError;

/// Video metadata validation failed (`InvalidVideoError`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid video: {reason}")]
pub struct InvalidVideoError {
    pub reason: String,
}

/// Encoder selection or validation failed (`EncoderError`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("encoder error: {msg}")]
pub struct EncoderError {
    pub msg: String,
}

/// Session initialisation or cleanup failed (`SessionError`), with an
/// optional underlying cause.
#[derive(Debug, thiserror::Error)]
pub struct SessionError {
    pub msg: String,
    #[source]
    pub source: Option<Box<dyn StdError + Send + Sync>>,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(cause) => write!(f, "session error: {}: {cause}", self.msg),
            None => write!(f, "session error: {}", self.msg),
        }
    }
}

/// Every error the core can return.
///
/// `Plain` is Go's untyped `fmt.Errorf` error: callers must not match on its
/// text, which follows Go's standard library only loosely.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    InvalidVideo(#[from] InvalidVideoError),
    #[error(transparent)]
    Encoder(#[from] EncoderError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("{0}")]
    Plain(String),
    /// The user stopped the conversion (`ErrCancelled`).
    #[error("encoding interrupted by user")]
    Cancelled,
    /// The operating system asked the application to quit (`ErrStoppedBySignal`); the field
    /// is the signal's name as [`signal_description`] gives it. Never the same thing as
    /// [`Error::Cancelled`] (U-09): the log must tell them apart.
    #[error("encoding stopped because the system asked the application to quit (signal: {0})")]
    StoppedBySignal(String),
    /// An ffmpeg run failed after it had started producing frames (`errFailedAfterOutput`
    /// wrapped around the run's error): no other path would fare better.
    #[error(
        "ffmpeg failed after it had started producing frames, so no other encoding path was tried: {0}"
    )]
    FailedAfterOutput(#[source] Box<Error>),
}

impl Error {
    /// `isStopRequest`: the error means "stop", whoever asked. Looks through
    /// [`Error::FailedAfterOutput`], as `errors.Is` looks through `%w`.
    pub fn is_stop_request(&self) -> bool {
        match self {
            Self::Cancelled | Self::StoppedBySignal(_) => true,
            Self::FailedAfterOutput(cause) => cause.is_stop_request(),
            _ => false,
        }
    }

    /// `shouldNotRetry`: a failed run that must be returned as it is rather than handed to
    /// the next level of the fallback cascade.
    pub fn should_not_retry(&self) -> bool {
        self.is_stop_request() || matches!(self, Self::FailedAfterOutput(_))
    }
}

/// The name Go gives a signal (`os.Signal`'s `String()`), for [`Error::StoppedBySignal`]:
/// `2` reads "interrupt", `15` "terminated", an unknown number "signal N".
pub fn signal_description(signal: i32) -> String {
    crate::process::signal_name(signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_the_go_text() {
        let invalid = InvalidVideoError {
            reason: "no codec information".into(),
        };
        assert_eq!(invalid.to_string(), "invalid video: no codec information");
        assert_eq!(
            EncoderError { msg: "x".into() }.to_string(),
            "encoder error: x"
        );
        let bare = SessionError {
            msg: "boom".into(),
            source: None,
        };
        assert_eq!(bare.to_string(), "session error: boom");
        let cause = std::io::Error::other("disk full");
        let wrapped = SessionError {
            msg: "write".into(),
            source: Some(Box::new(cause)),
        };
        assert_eq!(wrapped.to_string(), "session error: write: disk full");
        assert!(wrapped.source().is_some());
    }

    // Go: the message of each sentinel, and the %w (signal: %v) form of EncodeVideo.
    #[test]
    fn the_stop_messages_match_the_go_text() {
        assert_eq!(Error::Cancelled.to_string(), "encoding interrupted by user");
        assert_eq!(
            Error::StoppedBySignal(signal_description(15)).to_string(),
            "encoding stopped because the system asked the application to quit (signal: terminated)"
        );
        assert_eq!(
            Error::StoppedBySignal(signal_description(2)).to_string(),
            "encoding stopped because the system asked the application to quit (signal: interrupt)"
        );
        let after = Error::FailedAfterOutput(Box::new(Error::Plain("ffmpeg failed: x".into())));
        assert_eq!(
            after.to_string(),
            "ffmpeg failed after it had started producing frames, so no other encoding path \
             was tried: ffmpeg failed: x"
        );
        assert!(after.source().is_some());
        assert_eq!(signal_description(999), "signal 999");
    }

    // Go: TestShouldNotRetry_NamesEveryStopReason. "wrapped system signal" (errors.Join in Go)
    // becomes the one wrapping the port has: a signal inside a failure-after-output.
    #[test]
    fn should_not_retry_names_every_stop_reason() {
        let signal = || Error::StoppedBySignal("terminated".into());
        let cases = [
            ("user cancellation", Error::Cancelled, true),
            ("system signal", signal(), true),
            (
                "signal inside a failure after output",
                Error::FailedAfterOutput(Box::new(signal())),
                true,
            ),
            (
                "failure after output",
                Error::FailedAfterOutput(Box::new(Error::Plain("disk full".into()))),
                true,
            ),
            (
                "ordinary encoder failure",
                Error::Plain("ffmpeg failed: exit status: 1".into()),
                false,
            ),
            (
                "typed encoder failure",
                Error::Encoder(EncoderError { msg: "x".into() }),
                false,
            ),
        ];
        for (name, err, want) in cases {
            assert_eq!(err.should_not_retry(), want, "{name}");
        }
    }

    // Go: isStopRequest -- a failure after output is not, by itself, a stop request.
    #[test]
    fn only_cancellation_and_signal_are_stop_requests() {
        assert!(Error::Cancelled.is_stop_request());
        assert!(Error::StoppedBySignal("interrupt".into()).is_stop_request());
        assert!(
            Error::FailedAfterOutput(Box::new(Error::Cancelled)).is_stop_request(),
            "errors.Is looks through %w"
        );
        let after = Error::FailedAfterOutput(Box::new(Error::Plain("x".into())));
        assert!(!after.is_stop_request());
        assert!(after.should_not_retry());
        assert!(!Error::Plain("x".into()).is_stop_request());
    }

    // U-09: a signal from the system is not a cancellation by the user.
    #[test]
    fn a_system_signal_is_not_a_user_cancellation() {
        let signal = Error::StoppedBySignal("terminated".into());
        assert!(!matches!(signal, Error::Cancelled));
        assert_ne!(signal.to_string(), Error::Cancelled.to_string());
        assert!(!Error::Cancelled.to_string().contains("signal"));
        assert!(signal.to_string().contains("signal: terminated"));
    }

    #[test]
    fn wrapping_keeps_the_type() {
        let err: Error = InvalidVideoError { reason: "r".into() }.into();
        assert!(matches!(err, Error::InvalidVideo(_)));
    }
}
