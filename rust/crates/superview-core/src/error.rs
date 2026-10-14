//! Typed errors, mirroring `internal/common/errors.go`.
//!
//! The Display text of each variant is the Go `Error()` text, so a message can
//! be compared against the parity oracle as it stands.

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

    #[test]
    fn wrapping_keeps_the_type() {
        let err: Error = InvalidVideoError { reason: "r".into() }.into();
        assert!(matches!(err, Error::InvalidVideo(_)));
    }
}
