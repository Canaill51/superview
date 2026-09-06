package common

import (
	"bytes"
	"errors"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// newCapturingLogger returns a debug-level text logger writing into buf, so
// every level the handler can emit is visible.
func newCapturingLogger(buf *bytes.Buffer) *slog.Logger {
	return slog.New(slog.NewTextHandler(buf, &slog.HandlerOptions{Level: slog.LevelDebug}))
}

// TestDefaultObservabilityHandler_LevelPerEventType pins the mapping from
// event type to log level.
//
// It decides what survives at the default info level, which is what the user
// actually attaches to a bug report: progress events are debug and must not
// drown it, errors must be there. Until now only EventRecorder was tested, and
// with a mock handler -- the production handler never ran.
func TestDefaultObservabilityHandler_LevelPerEventType(t *testing.T) {
	cases := []struct {
		eventType string
		wantLevel string
	}{
		{"start", "INFO"},
		{"complete", "INFO"},
		{"progress", "DEBUG"},
		{"warning", "WARN"},
		{"error", "ERROR"},
		{"something_new", "INFO"}, // unknown types must not vanish
	}

	for _, tc := range cases {
		t.Run(tc.eventType, func(t *testing.T) {
			var buf bytes.Buffer
			h := NewDefaultObservabilityHandler(newCapturingLogger(&buf))

			h.OnEvent(&EncodingEvent{
				EventType:  tc.eventType,
				Timestamp:  time.Now(),
				Message:    "a message",
				InputFile:  "in.mp4",
				OutputFile: "out.mp4",
				Attributes: map[string]interface{}{"encoder": "libx264"},
			})

			got := buf.String()
			if !strings.Contains(got, "level="+tc.wantLevel) {
				t.Errorf("level = ?, want %s:\n%s", tc.wantLevel, got)
			}
			for _, want := range []string{
				"encoding_event", "event_type=" + tc.eventType, "a message",
				"input_file=in.mp4", "output_file=out.mp4", "encoder=libx264",
			} {
				if !strings.Contains(got, want) {
					t.Errorf("log omits %q:\n%s", want, got)
				}
			}
		})
	}
}

// TestDefaultObservabilityHandler_ProgressStaysBelowInfo is the reason
// progress is mapped to debug: it fires several times a second.
func TestDefaultObservabilityHandler_ProgressStaysBelowInfo(t *testing.T) {
	var buf bytes.Buffer
	h := NewDefaultObservabilityHandler(
		slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelInfo})))

	h.OnEvent(&EncodingEvent{EventType: "progress", Message: "42%", Timestamp: time.Now()})
	h.OnProgress(42.0, "encoding")

	if buf.Len() != 0 {
		t.Errorf("progress reached an info-level log:\n%s", buf.String())
	}
}

func TestDefaultObservabilityHandler_ProgressAndError(t *testing.T) {
	var buf bytes.Buffer
	h := NewDefaultObservabilityHandler(newCapturingLogger(&buf))

	h.OnProgress(42.5, "encoding")
	if got := buf.String(); !strings.Contains(got, "progress_percent=42.5") ||
		!strings.Contains(got, "encoding") {
		t.Errorf("OnProgress log = %q", got)
	}

	buf.Reset()
	h.OnError(errors.New("ffmpeg exited 1"), map[string]interface{}{"stage": "encode"})
	got := buf.String()
	if !strings.Contains(got, "level=ERROR") {
		t.Errorf("OnError did not log at error level:\n%s", got)
	}
	for _, want := range []string{"encoding failed", "ffmpeg exited 1", "stage=encode"} {
		if !strings.Contains(got, want) {
			t.Errorf("OnError log omits %q:\n%s", want, got)
		}
	}
}

func TestDefaultObservabilityHandler_Complete(t *testing.T) {
	var buf bytes.Buffer
	h := NewDefaultObservabilityHandler(newCapturingLogger(&buf))

	// No metrics is the cancellation path, and must still say something.
	h.OnComplete(nil)
	if !strings.Contains(buf.String(), "encoding completed") {
		t.Errorf("OnComplete(nil) logged %q", buf.String())
	}

	buf.Reset()
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordOutputMetadata(4_000_000, "libx265")
	m.RecordCompletion(1234)
	h.OnComplete(m)

	got := buf.String()
	if got == "" {
		t.Fatal("OnComplete with metrics logged nothing")
	}
	for _, want := range []string{
		"Encoding completed successfully", "output_file=out.mp4", "output_size_bytes=1234",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("OnComplete log omits %q:\n%s", want, got)
		}
	}

	// A run that failed takes the other branch, and this is the line a bug
	// report is read for: it has to carry ffmpeg's exit code and message.
	buf.Reset()
	failed := NewEncodingMetrics("in.mp4", "out.mp4")
	failed.RecordError(1, "ffmpeg: Invalid data found when processing input")
	h.OnComplete(failed)

	got = buf.String()
	if !strings.Contains(got, "level=ERROR") {
		t.Errorf("a failed encode did not log at error level:\n%s", got)
	}
	for _, want := range []string{"Encoding failed", "exit_code=1", "Invalid data found"} {
		if !strings.Contains(got, want) {
			t.Errorf("failed-encode log omits %q:\n%s", want, got)
		}
	}
}

// TestDefaultObservabilityHandler_NilInputs: the handler runs on the encoding
// path, where a panic costs the user their conversion.
func TestDefaultObservabilityHandler_NilInputs(t *testing.T) {
	var buf bytes.Buffer
	h := NewDefaultObservabilityHandler(newCapturingLogger(&buf))
	h.OnEvent(nil)
	if buf.Len() != 0 {
		t.Errorf("OnEvent(nil) logged %q", buf.String())
	}

	// A nil logger must fall back rather than panic.
	NewDefaultObservabilityHandler(nil).OnProgress(1, "x")
}

func TestParseLogLevel(t *testing.T) {
	cases := map[string]slog.Level{
		"debug":   slog.LevelDebug,
		"DEBUG":   slog.LevelDebug,
		" Debug ": slog.LevelDebug,
		"info":    slog.LevelInfo,
		"warn":    slog.LevelWarn,
		"warning": slog.LevelWarn,
		"error":   slog.LevelError,
		"ERROR":   slog.LevelError,
		// Anything unrecognised, including the empty string, is info: a typo in
		// superview.yaml must not silence the log.
		"":        slog.LevelInfo,
		"verbose": slog.LevelInfo,
		"trace":   slog.LevelInfo,
	}

	for in, want := range cases {
		if got := ParseLogLevel(in); got != want {
			t.Errorf("ParseLogLevel(%q) = %v, want %v", in, got, want)
		}
	}
}

// TestOpenLogFile_CreatesAppends covers the file the README asks users to
// attach: it must be created, and reopening must not lose what is in it.
func TestOpenLogFile_CreatesAppends(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())

	f, path, err := OpenLogFile("superview-test", 1<<20)
	if err != nil {
		t.Fatalf("OpenLogFile: %v", err)
	}
	if _, err := f.WriteString("first\n"); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	if filepath.Base(path) != "superview-test.log" {
		t.Errorf("path = %q, want it to end in superview-test.log", path)
	}

	f2, path2, err := OpenLogFile("superview-test", 1<<20)
	if err != nil {
		t.Fatalf("second OpenLogFile: %v", err)
	}
	if _, err := f2.WriteString("second\n"); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := f2.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}
	if path2 != path {
		t.Errorf("path changed between runs: %q then %q", path, path2)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != "first\nsecond\n" {
		t.Errorf("log = %q, want the first run's line kept and the second appended", got)
	}
}

// TestOpenLogFile_TruncatesOversized covers the size cap.
//
// This is the only thing standing between a long-running install and a log file
// that grows without bound, and it had never been exercised.
func TestOpenLogFile_TruncatesOversized(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())

	f, path, err := OpenLogFile("superview-test", 1<<20)
	if err != nil {
		t.Fatalf("OpenLogFile: %v", err)
	}
	if _, err := f.WriteString(strings.Repeat("x", 4096)); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	// Reopen with a cap the file already exceeds.
	f2, _, err := OpenLogFile("superview-test", 1024)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	if err := f2.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}

	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat: %v", err)
	}
	if info.Size() != 0 {
		t.Errorf("log kept %d bytes past a 1024-byte cap, want it truncated", info.Size())
	}

	// A cap of 0 means "no cap" and must not truncate.
	f3, _, err := OpenLogFile("superview-test", 0)
	if err != nil {
		t.Fatalf("third open: %v", err)
	}
	if _, err := f3.WriteString("kept\n"); err != nil {
		t.Fatalf("write: %v", err)
	}
	if err := f3.Close(); err != nil {
		t.Fatalf("close: %v", err)
	}
	if _, _, err := OpenLogFile("superview-test", 0); err != nil {
		t.Fatalf("fourth open: %v", err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != "kept\n" {
		t.Errorf("log = %q with maxLogBytes=0, want the content kept", got)
	}
}
