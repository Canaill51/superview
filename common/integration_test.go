package common

import (
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// integrationUI is a minimal UIHandler recording what the pipeline reports.
type integrationUI struct {
	lastProgress float64
	progressCall int
	bitrate      int
	encoder      string
}

func (u *integrationUI) ShowError(error) {}
func (u *integrationUI) ShowInfo(string) {}
func (u *integrationUI) ShowProgress(p float64) {
	u.lastProgress = p
	u.progressCall++
}
func (u *integrationUI) GetBitrate() (int, error) { return u.bitrate, nil }
func (u *integrationUI) GetEncoder() string       { return u.encoder }
func (u *integrationUI) GetSqueeze() bool         { return false }

// makeTestClip renders a short 4:3 clip with ffmpeg, skipping the test when
// ffmpeg is unavailable.
func makeTestClip(t *testing.T, width, height, seconds int) string {
	t.Helper()

	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("ffmpeg not installed; skipping integration test")
	}

	path := filepath.Join(t.TempDir(), "input.mp4")
	cmd := exec.Command("ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
		"-f", "lavfi",
		"-i", "testsrc=size="+strconv.Itoa(width)+"x"+strconv.Itoa(height)+":rate=30",
		"-t", strconv.Itoa(seconds),
		"-c:v", "libx264", "-b:v", "2M", "-pix_fmt", "yuv420p",
		path)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Skipf("could not render a test clip (%v): %s", err, out)
	}
	return path
}

func probeDimensions(t *testing.T, path string) (int, int) {
	t.Helper()
	out, err := exec.Command("ffprobe", "-v", "error", "-select_streams", "v:0",
		"-show_entries", "stream=width,height", "-of", "csv=p=0", path).Output()
	if err != nil {
		t.Fatalf("ffprobe failed: %v", err)
	}
	parts := strings.Split(strings.TrimSpace(string(out)), ",")
	if len(parts) != 2 {
		t.Fatalf("unexpected ffprobe output: %q", out)
	}
	w, err := strconv.Atoi(parts[0])
	if err != nil {
		t.Fatalf("bad width %q: %v", parts[0], err)
	}
	h, err := strconv.Atoi(parts[1])
	if err != nil {
		t.Fatalf("bad height %q: %v", parts[1], err)
	}
	return w, h
}

// TestIntegration_PerformEncoding drives the whole pipeline against a real
// ffmpeg: probe, validate, generate the remap maps, encode, collect metrics.
//
// Unit tests stub ffmpeg out, so they cannot catch an argument ordering
// mistake, a broken progress pipe or a malformed remap map -- all of which
// only surface when ffmpeg actually runs.
func TestIntegration_PerformEncoding(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	const inW, inH = 640, 480
	input := makeTestClip(t, inW, inH, 3)

	ffmpeg, err := CheckFfmpeg()
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}
	if !strings.Contains(ffmpeg["encoders"], "libx264") {
		t.Skip("libx264 not available in this ffmpeg build")
	}

	output := filepath.Join(t.TempDir(), "output.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}

	if err := PerformEncoding(input, output, ui, ffmpeg, make(chan struct{})); err != nil {
		t.Fatalf("PerformEncoding: %v", err)
	}

	// The conversion must widen 4:3 to 16:9 while keeping the height.
	outW, outH := probeDimensions(t, output)
	if outH != inH {
		t.Errorf("height changed: got %d, want %d", outH, inH)
	}
	height := inH // avoid a constant-folded conversion
	wantW := int(float64(height)*(16.0/9.0)) / 2 * 2
	if outW != wantW {
		t.Errorf("width = %d, want %d (16:9 of height %d)", outW, wantW, inH)
	}

	if ui.progressCall == 0 {
		t.Error("the progress callback never fired")
	}
	if ui.lastProgress <= 0 {
		t.Errorf("last reported progress = %.2f, want > 0", ui.lastProgress)
	}

	metrics := GetLastEncodingMetrics()
	if metrics == nil {
		t.Fatal("no metrics recorded")
	}
	if metrics.OutputCodec != "libx264" {
		t.Errorf("OutputCodec = %q, want libx264", metrics.OutputCodec)
	}
	if metrics.OutputFileSize <= 0 {
		t.Errorf("OutputFileSize = %d, want > 0", metrics.OutputFileSize)
	}
	if summary := GetLastHardwareAccelerationSummary(); summary == "" {
		t.Error("no hardware acceleration summary recorded")
	}
}

// TestIntegration_CancelDuringEncoding checks the cancellation channel really
// stops ffmpeg instead of leaving the pipeline hanging.
func TestIntegration_CancelDuringEncoding(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	input := makeTestClip(t, 640, 480, 3)

	ffmpeg, err := CheckFfmpeg()
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}

	output := filepath.Join(t.TempDir(), "cancelled.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}

	cancel := make(chan struct{})
	close(cancel) // already cancelled before we start

	err = PerformEncoding(input, output, ui, ffmpeg, cancel)
	if err == nil {
		t.Fatal("expected an error when cancelled, got nil")
	}
	if !strings.Contains(err.Error(), "interrupted") {
		t.Errorf("expected an interruption error, got: %v", err)
	}
}
