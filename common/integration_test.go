package common

import (
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

// integrationUI is a minimal UIHandler recording what the pipeline reports.
type integrationUI struct {
	lastProgress float64
	progressCall int
	bitrate      int
	encoder      string
	// onProgress, when set, runs on every progress update. Tests that need to
	// interrupt a *running* encode hook it here: it is the only point that
	// proves ffmpeg has actually started producing output.
	onProgress func(float64)

	// detailCalls counts ShowProgressDetail dispatches; lastRemaining keeps the
	// last estimate handed over. They verify that the pipeline really picks up
	// the optional ProgressDetailHandler, which nothing else would catch: the
	// dispatch is a type assertion, so dropping the method compiles fine and
	// only makes the estimate quietly disappear.
	detailCalls   int
	lastRemaining time.Duration
}

func (u *integrationUI) ShowProgressDetail(_ float64, remaining time.Duration) {
	u.detailCalls++
	u.lastRemaining = remaining
}

func (u *integrationUI) ShowError(error) {}
func (u *integrationUI) ShowInfo(string) {}
func (u *integrationUI) ShowProgress(p float64) {
	u.lastProgress = p
	u.progressCall++
	if u.onProgress != nil {
		u.onProgress(p)
	}
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

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}
	if !strings.Contains(ffmpeg["encoders"], "libx264") {
		t.Skip("libx264 not available in this ffmpeg build")
	}

	output := filepath.Join(t.TempDir(), "output.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}

	if err := PerformEncoding(nil, input, output, ui, ffmpeg, make(chan struct{})); err != nil {
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
	// The clip is rendered at rate=30. The frame rate used to be assumed rather
	// than read, which made the encoding-speed metric wrong on any other rate.
	if metrics.InputFrameRate < 29.9 || metrics.InputFrameRate > 30.1 {
		t.Errorf("InputFrameRate = %v, want ~30 as rendered", metrics.InputFrameRate)
	}
	if metrics.EncodingSpeed <= 0 {
		t.Error("EncodingSpeed = 0 although the frame rate is known")
	}

	if ui.detailCalls == 0 {
		t.Error("ShowProgressDetail was never called: the pipeline is not picking up ProgressDetailHandler")
	}
	if ui.detailCalls != ui.progressCall {
		t.Errorf("ShowProgressDetail fired %d time(s) for %d progress updates, want one per update",
			ui.detailCalls, ui.progressCall)
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

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}

	output := filepath.Join(t.TempDir(), "cancelled.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}

	cancel := make(chan struct{})
	close(cancel) // already cancelled before we start

	err = PerformEncoding(nil, input, output, ui, ffmpeg, cancel)
	if err == nil {
		t.Fatal("expected an error when cancelled, got nil")
	}
	if !strings.Contains(err.Error(), "interrupted") {
		t.Errorf("expected an interruption error, got: %v", err)
	}
}

// makeRichTestClip renders a 4:3 clip that carries everything the pipeline used
// to throw away: a 10-bit pixel format, two audio tracks and a recording date.
func makeRichTestClip(t *testing.T, width, height int) string {
	t.Helper()

	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("ffmpeg not installed; skipping integration test")
	}

	path := filepath.Join(t.TempDir(), "rich.mp4")
	size := strconv.Itoa(width) + "x" + strconv.Itoa(height)
	cmd := exec.Command("ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
		"-f", "lavfi", "-i", "testsrc=size="+size+":rate=30",
		"-f", "lavfi", "-i", "sine=frequency=440",
		"-f", "lavfi", "-i", "sine=frequency=880",
		"-map", "0:v", "-map", "1:a", "-map", "2:a", "-t", "1",
		"-c:v", "libx265", "-x265-params", "log-level=error",
		"-pix_fmt", "yuv420p10le", "-b:v", "2M",
		"-c:a", "aac",
		"-metadata", "creation_time="+richClipCreationTime,
		path)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Skipf("could not render a 10-bit multi-track clip (%v): %s", err, out)
	}
	return path
}

const richClipCreationTime = "2026-01-15T10:30:00Z"

func probeEntry(t *testing.T, path string, args ...string) string {
	t.Helper()
	full := append([]string{"-v", "error"}, args...)
	full = append(full, "-of", "default=nw=1:nk=1", path)
	out, err := exec.Command("ffprobe", full...).Output()
	if err != nil {
		t.Fatalf("ffprobe %v failed: %v", args, err)
	}
	return strings.TrimSpace(string(out))
}

// TestIntegration_PreservesDepthTracksAndMetadata pins the three silent losses
// the filter chain used to cause, measured on a real encode:
//
//   - a 10-bit source came out 8-bit, because the chain was pinned to yuv420p;
//   - a second audio track was dropped, because nothing mapped the streams;
//   - creation_time vanished, because with three inputs ffmpeg inherits global
//     metadata from none of them.
//
// Each assertion below fails on the pre-fix argument list, so this test is what
// keeps the three from creeping back in one at a time.
func TestIntegration_PreservesDepthTracksAndMetadata(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}
	if !strings.Contains(ffmpeg["encoders"], "libx265") {
		t.Skip("libx265 not available in this ffmpeg build")
	}

	input := makeRichTestClip(t, 320, 240)

	// Guard the fixture itself: if ffmpeg did not actually produce a 10-bit,
	// two-track, dated clip, the assertions below would pass vacuously.
	if got := probeEntry(t, input, "-select_streams", "v:0", "-show_entries", "stream=pix_fmt"); got != "yuv420p10le" {
		t.Skipf("fixture is not 10-bit (pix_fmt=%q); nothing to assert", got)
	}
	if got := probeEntry(t, input, "-select_streams", "a", "-show_entries", "stream=index"); len(strings.Fields(got)) != 2 {
		t.Skipf("fixture does not carry two audio tracks (indices %q)", got)
	}

	output := filepath.Join(t.TempDir(), "rich-out.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx265"}

	if err := PerformEncoding(nil, input, output, ui, ffmpeg, make(chan struct{})); err != nil {
		t.Fatalf("PerformEncoding: %v", err)
	}

	if got := probeEntry(t, output, "-select_streams", "v:0", "-show_entries", "stream=pix_fmt"); got != "yuv420p10le" {
		t.Errorf("pix_fmt = %q, want yuv420p10le: the 10-bit source was flattened to 8 bits", got)
	}

	audio := strings.Fields(probeEntry(t, output, "-select_streams", "a", "-show_entries", "stream=index"))
	if len(audio) != 2 {
		t.Errorf("output carries %d audio track(s), want 2: a track was dropped", len(audio))
	}

	created := probeEntry(t, output, "-show_entries", "format_tags=creation_time")
	if created == "" {
		t.Error("creation_time is absent from the output: the recording date was lost")
	} else if !strings.HasPrefix(created, "2026-01-15T10:30:00") {
		t.Errorf("creation_time = %q, want the source's %q", created, richClipCreationTime)
	}
}

// TestIntegration_CancelLeavesNoPartialOutput pins P-04.
//
// ffmpeg runs with -y and used to write straight to the destination, so
// cancelling left a truncated .mp4 exactly where the user expected a finished
// one -- and, when overwriting, destroyed the file that was already there
// before producing anything. The encode now goes to a working file that is
// renamed into place only on success.
func TestIntegration_CancelLeavesNoPartialOutput(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}

	// Long enough that the encode is still running when the first progress
	// update arrives.
	input := makeTestClip(t, 640, 480, 10)
	dir := t.TempDir()

	// cancelOnFirstProgress interrupts the encode only once ffmpeg is provably
	// producing output. Closing the channel up front instead would kill ffmpeg
	// before it even creates the destination file, and the assertions below
	// would then pass whether the fix is present or not.
	cancelOnFirstProgress := func() (*integrationUI, chan struct{}) {
		cancel := make(chan struct{})
		var once sync.Once
		return &integrationUI{
			bitrate: 2_000_000, encoder: "libx264",
			onProgress: func(float64) { once.Do(func() { close(cancel) }) },
		}, cancel
	}

	t.Run("no output file is left behind", func(t *testing.T) {
		output := filepath.Join(dir, "cancelled.mp4")
		ui, cancel := cancelOnFirstProgress()

		if err := PerformEncoding(nil, input, output, ui, ffmpeg, cancel); err == nil {
			t.Fatal("expected an error when cancelled, got nil")
		}
		if ui.progressCall == 0 {
			t.Fatal("the encode never reported progress, so it was never really running")
		}
		if _, statErr := os.Stat(output); !os.IsNotExist(statErr) {
			t.Errorf("a partial %s was left on disk after cancellation", filepath.Base(output))
		}
	})

	t.Run("an existing destination survives", func(t *testing.T) {
		output := filepath.Join(dir, "existing.mp4")
		const previous = "the file the user already had"
		if err := os.WriteFile(output, []byte(previous), 0o600); err != nil {
			t.Fatalf("write fixture: %v", err)
		}

		ui, cancel := cancelOnFirstProgress()

		if err := PerformEncoding(nil, input, output, ui, ffmpeg, cancel); err == nil {
			t.Fatal("expected an error when cancelled, got nil")
		}
		if ui.progressCall == 0 {
			t.Fatal("the encode never reported progress, so it was never really running")
		}

		data, readErr := os.ReadFile(output)
		if readErr != nil {
			t.Fatalf("the previous output file is gone: %v", readErr)
		}
		if string(data) != previous {
			t.Error("the previous output file was clobbered by a run that produced nothing")
		}
	})

	t.Run("no working file is left in the directory", func(t *testing.T) {
		entries, err := os.ReadDir(dir)
		if err != nil {
			t.Fatalf("read dir: %v", err)
		}
		for _, e := range entries {
			if strings.Contains(e.Name(), "superview-partial") {
				t.Errorf("working file %s was not cleaned up", e.Name())
			}
		}
	})
}
