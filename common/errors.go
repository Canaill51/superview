// Typed errors, sentinels and the package logger.

package common

import (
	"errors"
	"fmt"
	"log/slog"
)

// Global logger instance
var logger = slog.Default()

// InvalidVideoError is returned when video metadata validation fails.
// It indicates issues with video dimensions, codec, duration, or bitrate information.
type InvalidVideoError struct {
	Reason string
}

func (e *InvalidVideoError) Error() string {
	return fmt.Sprintf("invalid video: %s", e.Reason)
}

// EncoderError is returned when encoder selection or validation fails.
// It indicates that the requested encoder is not available or cannot be used.
type EncoderError struct {
	Msg string
}

func (e *EncoderError) Error() string {
	return fmt.Sprintf("encoder error: %s", e.Msg)
}

// ErrCancelled is returned when the user stops a conversion, either through the
// Cancel button or with Ctrl+C.
//
// It has to be a sentinel rather than a bare errors.New string. EncodeVideo
// retries a failed encode on progressively safer paths -- CPU decode, then the
// equivalent CPU encoder -- and it decides by looking at the error. With an
// untyped error a cancellation was indistinguishable from an encoder failure,
// so asking to stop started up to three more ffmpeg processes instead of none.
// Callers should test it with errors.Is, never by matching the message.
var ErrCancelled = errors.New("encoding interrupted by user")

// ErrStoppedBySignal is returned when the operating system asked the
// application to quit, which is not the same thing as a user cancelling.
//
// The distinction is not academic, and it cost a full investigation to
// establish. On a machine that ran out of memory, the kernel's OOM killer took
// ffmpeg; systemd then applied its default OOMPolicy and stopped the whole
// application unit, which arrives here as a SIGTERM. That SIGTERM was reported
// as ErrCancelled, so the log read "encoding interrupted by user" for a
// conversion nobody had touched, and the real cause -- memory exhaustion --
// appeared in no line of it. Anything that reads this error apart from
// ErrCancelled must keep reading it apart: the message is the only trace the
// user gets of a decision taken outside the application.
//
// Callers should test it with errors.Is: the signal name is wrapped into the
// message, so the value returned is not this sentinel itself.
var ErrStoppedBySignal = errors.New("encoding stopped because the system asked the application to quit")

// SessionError is returned when encoding session initialization or cleanup fails.
// It indicates problems with temporary directory management.
type SessionError struct {
	Msg string
}

func (e *SessionError) Error() string {
	return fmt.Sprintf("session error: %s", e.Msg)
}

// SetLogger sets the global logger instance used throughout the encoding pipeline.
// If nil is passed, the current logger is unchanged.
// This allows customization of log output format, level, and destination.
func SetLogger(l *slog.Logger) {
	if l != nil {
		logger = l
	}
}

// GetLogger returns the current global logger instance.
// Use this in handlers and UI code to log encoding progress and diagnostic information.
func GetLogger() *slog.Logger {
	return logger
}

// exitCodeUnavailable is recorded when a stage fails before ffmpeg ever ran,
// so there is no process exit code to report.
const exitCodeUnavailable = -1
