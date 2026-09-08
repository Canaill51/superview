package common

import (
	"bytes"
	"io"
	"log/slog"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// scriptedProgress replays a canned ffmpeg -progress stream.
type scriptedProgress struct{ *strings.Reader }

func (s *scriptedProgress) Close() error { return nil }

// TestEncodeVideo_ProgressNotAvailableIsNotAWarning covers the noise a user's
// log actually carried: 45 lines of
//
//	WARN Failed to parse progress value raw_value=N/A
//
// in 81 seconds, for a conversion that was working. ffmpeg answers N/A until the
// first frame leaves the filter graph, so on a large frame that is the normal
// state at the start -- and this is the log the README asks people to attach to
// a bug report.
func TestEncodeVideo_ProgressNotAvailableIsNotAWarning(t *testing.T) {
	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("failed to init session: %v", err)
	}
	defer func() {
		if err := CleanUp(); err != nil {
			t.Errorf("failed to cleanup session: %v", err)
		}
	}()

	video := &VideoSpecs{
		File: "input.mp4",
		Streams: []VideoStream{{
			Codec:         "h264",
			Width:         1920,
			Height:        1080,
			Duration:      "60",
			DurationFloat: 60,
			Bitrate:       "5000000",
			BitrateInt:    5000000,
		}},
	}

	captured := new(bytes.Buffer)
	previousLogger := logger
	SetLogger(slog.New(slog.NewTextHandler(captured, &slog.HandlerOptions{Level: slog.LevelDebug})))
	t.Cleanup(func() { SetLogger(previousLogger) })

	// Two N/A ticks, then a real one: the shape of every large-frame encode.
	oldStdoutPipe := commandStdoutPipe
	t.Cleanup(func() { commandStdoutPipe = oldStdoutPipe })
	commandStdoutPipe = func(_ *exec.Cmd) (io.ReadCloser, error) {
		return &scriptedProgress{strings.NewReader(
			"out_time_ms=N/A\nout_time_ms=N/A\nout_time_ms=600000000\n")}, nil
	}

	progress := make(chan float64, 8)
	done := make(chan error, 1)
	go func() {
		done <- EncodeVideo(nil, video, "libx264", 2000000,
			filepath.Join(t.TempDir(), "out.mp4"), map[string]string{},
			func(p float64) { progress <- p }, make(chan struct{}))
	}()

	select {
	case <-done:
	case <-time.After(20 * time.Second):
		t.Fatal("EncodeVideo did not return")
	}

	logged := captured.String()
	if strings.Contains(logged, "Failed to parse progress value") {
		t.Errorf("N/A is ffmpeg's normal answer before the first frame, not a parse failure:\n%s", logged)
	}
	if !strings.Contains(logged, "ffmpeg has not produced a frame yet") {
		t.Errorf("the wait for the first frame should still be traceable at debug level:\n%s", logged)
	}

	// The real value must still get through: silencing N/A must not silence
	// progress itself.
	select {
	case p := <-progress:
		if p <= 0 {
			t.Errorf("expected a real progress value, got %v", p)
		}
	default:
		t.Error("the real progress value never reached the callback")
	}
}

// TestEncodeVideo_UnparsableProgressIsStillAWarning is the other half: a value
// that is neither N/A nor a number means ffmpeg's progress format changed under
// us, and that is worth saying out loud.
func TestEncodeVideo_UnparsableProgressIsStillAWarning(t *testing.T) {
	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("failed to init session: %v", err)
	}
	defer func() {
		if err := CleanUp(); err != nil {
			t.Errorf("failed to cleanup session: %v", err)
		}
	}()

	video := &VideoSpecs{
		File: "input.mp4",
		Streams: []VideoStream{{
			Codec:         "h264",
			Width:         1920,
			Height:        1080,
			Duration:      "60",
			DurationFloat: 60,
			Bitrate:       "5000000",
			BitrateInt:    5000000,
		}},
	}

	captured := new(bytes.Buffer)
	previousLogger := logger
	SetLogger(slog.New(slog.NewTextHandler(captured, &slog.HandlerOptions{Level: slog.LevelDebug})))
	t.Cleanup(func() { SetLogger(previousLogger) })

	oldStdoutPipe := commandStdoutPipe
	t.Cleanup(func() { commandStdoutPipe = oldStdoutPipe })
	commandStdoutPipe = func(_ *exec.Cmd) (io.ReadCloser, error) {
		return &scriptedProgress{strings.NewReader("out_time_ms=soon\n")}, nil
	}

	done := make(chan error, 1)
	go func() {
		done <- EncodeVideo(nil, video, "libx264", 2000000,
			filepath.Join(t.TempDir(), "out.mp4"), map[string]string{},
			func(float64) {}, make(chan struct{}))
	}()

	select {
	case <-done:
	case <-time.After(20 * time.Second):
		t.Fatal("EncodeVideo did not return")
	}

	if !strings.Contains(captured.String(), "Failed to parse progress value") {
		t.Errorf("a value that is not a number and not N/A must still warn:\n%s", captured.String())
	}
}

// TestCheckDiskSpaceHealth_ThresholdFitsWhatTheDirectoryIsFor pins the number
// that reported UNHEALTHY for the life of a healthy machine.
//
// The temp directory holds the remap maps and nothing else -- 113 MB for the
// largest frame this pipeline handles. The threshold was 10 GB, which a tmpfs
// cannot satisfy: current distributions size /tmp at half the RAM, so an 8 GiB
// laptop showed 3.8 GB free and a permanent failure, while holding thirty times
// what the conversion needed.
func TestCheckDiskSpaceHealth_ThresholdFitsWhatTheDirectoryIsFor(t *testing.T) {
	if minimumTempFreeGB > 2 {
		t.Fatalf("a threshold of %d GB cannot be met by a tmpfs on a small machine, "+
			"and the maps need about 0.11 GB", minimumTempFreeGB)
	}

	result := checkDiskSpaceHealth(time.Now().Unix())
	if result.Name != "disk_space" {
		t.Fatalf("unexpected check name %q", result.Name)
	}
	if !result.Healthy {
		t.Logf("this machine genuinely has little space in the temp directory: %s", result.Message)
	}
}
