package common

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// cascadeVideo is a plain 1080p H.264 source for the cascade tests below.
func cascadeVideo(dir string) *VideoSpecs {
	return &VideoSpecs{File: filepath.Join(dir, "input.mp4"), Streams: []VideoStream{{
		Codec: "h264", Width: 1920, Height: 1080,
		Duration: "60", DurationFloat: 60, Bitrate: "5000000", BitrateInt: 5000000,
	}}}
}

// cascadeMachine advertises a hardware encoder with a matching decode accel,
// which is what opens all three levels of the fallback cascade.
var cascadeMachine = map[string]string{"accels": "cuda", "encoders": "h264_nvenc,libx264"}

// TestEncodeVideo_SystemSignalStopsTheFallbackCascade pins E-02.
//
// The existing signal test runs libx264 with no decode accel under the default
// audio mode: the one configuration with no retry in it. Under the shipped
// configuration (safe_performance) with a hardware encoder, one SIGTERM used to
// start six ffmpeg processes -- the AAC retry, the CPU-decode fallback, the
// CPU-encoder fallback, each with its audio retry -- and if the last one
// finished, the signal was lost and the window said "Transform complete".
//
// Counter-proof: make the cascade gates test ErrCancelled alone and this
// reports six launches.
func TestEncodeVideo_SystemSignalStopsTheFallbackCascade(t *testing.T) {
	tempDir := t.TempDir()
	fakeHangingFFmpeg(t, tempDir)
	launches := countFFmpegLaunches(t)

	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("InitEncodingSession: %v", err)
	}
	defer func() { _ = CleanUp() }()

	oldNotify, oldStop := signalNotify, signalStop
	defer func() { signalNotify, signalStop = oldNotify, oldStop }()

	registered := make(chan chan<- os.Signal, 16)
	signalNotify = func(c chan<- os.Signal, _ ...os.Signal) { registered <- c }
	signalStop = func(c chan<- os.Signal) {}

	cfg := &Config{PerformanceMode: "safe_performance"}
	errCh := make(chan error, 1)
	go func() {
		errCh <- EncodeVideo(cfg, cascadeVideo(tempDir), "h264_nvenc", 2_000_000,
			filepath.Join(tempDir, "output.mp4"), cascadeMachine, func(float64) {}, make(chan struct{}))
	}()

	// One signal for the conversion. If the cascade relaunches ffmpeg, each new
	// process registers its own channel and would hang until signalled too, so
	// every registration after the first is signalled as well -- that keeps the
	// test from hanging on the defect while the launch count still exposes it.
	deadline := time.After(20 * time.Second)
	for {
		select {
		case c := <-registered:
			c <- os.Interrupt
		case err := <-errCh:
			if !errors.Is(err, ErrStoppedBySignal) {
				t.Fatalf("error = %v, want ErrStoppedBySignal", err)
			}
			if got := launches.Load(); got != 1 {
				t.Fatalf("ffmpeg was launched %d time(s) after one system signal, want 1: the cascade did not stop", got)
			}
			return
		case <-deadline:
			t.Fatalf("EncodeVideo did not return; ffmpeg launched %d time(s) so far", launches.Load())
		}
	}
}

// fakeLateFailingFFmpeg stands in for an ffmpeg that produces frames and then
// fails -- a destination disk filling up, a corrupt tail in the source.
func fakeLateFailingFFmpeg(t *testing.T, dir string) {
	t.Helper()

	name, mode := "ffmpeg", os.FileMode(0o755)
	script := "#!/bin/sh\n" +
		"echo out_time_ms=500000\n" +
		"sleep 0.2\n" +
		"echo 'No space left on device' 1>&2\n" +
		"exit 1\n"
	if runtime.GOOS == "windows" {
		name, mode = "ffmpeg.bat", os.FileMode(0o644)
		script = "@echo off\r\n" +
			"echo out_time_ms=500000\r\n" +
			"ping -n 2 127.0.0.1 >nul\r\n" +
			"echo No space left on device 1>&2\r\n" +
			"exit /b 1\r\n"
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), mode); err != nil {
		t.Fatalf("failed to create the stand-in ffmpeg: %v", err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Cleanup(func() { toolResolveCache.Delete("ffmpeg") })
	toolResolveCache.Delete("ffmpeg")
}

// TestEncodeVideo_FailureAfterOutputIsNotRetried pins E-13.
//
// The cascade is for encoders that cannot open. A failure that arrives after
// frames have been written fails the same way on every other path, and each
// retry is a whole encode: on a twenty-minute 4K clip that was an hour and a
// half between the fault and the message. The progress callback must have
// fired, so the test knows the failure was a late one and not an open failure
// that the cascade is right to retry.
//
// Counter-proof: drop the errFailedAfterOutput wrapping in run() and this
// reports six launches.
func TestEncodeVideo_FailureAfterOutputIsNotRetried(t *testing.T) {
	tempDir := t.TempDir()
	fakeLateFailingFFmpeg(t, tempDir)
	launches := countFFmpegLaunches(t)

	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("InitEncodingSession: %v", err)
	}
	defer func() { _ = CleanUp() }()

	var progressed sync.Once
	progressSeen := make(chan struct{})
	progress := func(float64) { progressed.Do(func() { close(progressSeen) }) }

	cfg := &Config{PerformanceMode: "safe_performance"}
	err := EncodeVideo(cfg, cascadeVideo(tempDir), "h264_nvenc", 2_000_000,
		filepath.Join(tempDir, "output.mp4"), cascadeMachine, progress, make(chan struct{}))

	select {
	case <-progressSeen:
	default:
		t.Fatal("the stand-in never reported progress, so this test cannot tell a late failure from an early one")
	}
	if err == nil {
		t.Fatal("the stand-in ffmpeg fails every time, so an error was expected")
	}
	if !errors.Is(err, errFailedAfterOutput) {
		t.Fatalf("error = %v, want it marked as a failure after output", err)
	}
	if !strings.Contains(err.Error(), "No space left on device") {
		t.Fatalf("ffmpeg's own words must survive in the error, got: %v", err)
	}
	if got := launches.Load(); got != 1 {
		t.Fatalf("ffmpeg was launched %d time(s) after a failure that came after frames were written, want 1", got)
	}
}

// TestShouldNotRetry_NamesEveryStopReason is the table the three cascade gates
// share. A fourth reason added to one gate and not the others is exactly how
// E-02 happened.
func TestShouldNotRetry_NamesEveryStopReason(t *testing.T) {
	cases := []struct {
		name string
		err  error
		want bool
	}{
		{"user cancellation", ErrCancelled, true},
		{"system signal", ErrStoppedBySignal, true},
		{"wrapped system signal", errors.Join(errors.New("ctx"), ErrStoppedBySignal), true},
		{"failure after output", errFailedAfterOutput, true},
		{"ordinary encoder failure", errors.New("ffmpeg failed: exit status 1"), false},
		{"nil", nil, false},
	}
	for _, tc := range cases {
		if got := shouldNotRetry(tc.err); got != tc.want {
			t.Errorf("%s: shouldNotRetry = %v, want %v", tc.name, got, tc.want)
		}
	}
}

// TestIsValidOutputPath_RejectsADirectory pins the early half of E-04: a
// destination that is a directory used to pass validation and fail at the
// final rename, after the whole encode.
func TestIsValidOutputPath_RejectsADirectory(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "clips")
	if err := os.Mkdir(target, 0o755); err != nil {
		t.Fatal(err)
	}

	err := isValidOutputPath(target)
	if err == nil || !strings.Contains(err.Error(), "is a directory") {
		t.Fatalf("a directory as output must be refused up front, got: %v", err)
	}
	if err := isValidOutputPath(filepath.Join(dir, "clip.mp4")); err != nil {
		t.Fatalf("a plain file path next to it must still pass: %v", err)
	}
}

// fakeAudioCopyRefusingFFmpeg stands in for an ffmpeg that cannot copy the
// source's audio into MP4 -- a PCM track, a codec the muxer has no tag for --
// and converts fine once asked for AAC. Every call appends its argv to ledger.
//
// With afterOutput, the copy run reports a frame before failing: the shape of a
// failure that is not about opening the audio stream at all, and that the
// audio retry must not replay any more than the cascade does (E-13).
func fakeAudioCopyRefusingFFmpeg(t *testing.T, dir, ledger string, afterOutput bool) {
	t.Helper()

	name, mode := "ffmpeg", os.FileMode(0o755)
	copyRun := "echo 'Could not find tag for codec pcm_s16le in stream #1' 1>&2; exit 1"
	if afterOutput {
		copyRun = "echo out_time_ms=500000; sleep 0.2; echo 'No space left on device' 1>&2; exit 1"
	}
	script := "#!/bin/sh\n" +
		"echo \"call $*\" >> '" + ledger + "'\n" +
		"case \" $* \" in *\" -c:a copy \"*) " + copyRun + " ;; esac\n" +
		"echo out_time_ms=1000000\n" +
		"echo progress=end\n" +
		"exit 0\n"
	if runtime.GOOS == "windows" {
		name, mode = "ffmpeg.bat", os.FileMode(0o644)
		copyRun = "echo Could not find tag for codec pcm_s16le in stream #1 1>&2\r\n" +
			"exit /b 1\r\n"
		if afterOutput {
			copyRun = "echo out_time_ms=500000\r\n" +
				"ping -n 2 127.0.0.1 >nul\r\n" +
				"echo No space left on device 1>&2\r\n" +
				"exit /b 1\r\n"
		}
		script = "@echo off\r\n" +
			"echo call %* >> \"" + ledger + "\"\r\n" +
			"echo %* | findstr /C:\"-c:a copy\" >nul && goto copyrun\r\n" +
			"echo out_time_ms=1000000\r\n" +
			"echo progress=end\r\n" +
			"exit /b 0\r\n" +
			":copyrun\r\n" + copyRun
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), mode); err != nil {
		t.Fatalf("failed to create the stand-in ffmpeg: %v", err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Cleanup(func() { toolResolveCache.Delete("ffmpeg") })
	toolResolveCache.Delete("ffmpeg")
}

// ledgerCalls returns the argv lines the stand-in ffmpeg appended to ledger.
func ledgerCalls(t *testing.T, ledger string) []string {
	t.Helper()
	data, err := os.ReadFile(ledger)
	if err != nil {
		t.Fatalf("the stand-in ffmpeg never ran: %v", err)
	}
	return strings.Split(strings.TrimSpace(strings.ReplaceAll(string(data), "\r\n", "\n")), "\n")
}

// TestEncodeVideo_SafePerformanceRetriesAudioAsAAC pins the audio retry of the
// configuration that actually ships (E-19).
//
// config.yaml sets safe_performance, which copies the audio stream and falls
// back to AAC when the copy fails. Every other test passes a nil config, which
// means "aac" from the start, so the copy attempt and its retry were 0 %
// covered: deleting the retry, or letting it fire on a stop, would have passed
// the whole suite while every clip with a PCM track failed for real users.
//
// Counter-proofs: delete the `return run(hwaccel, "aac")` branch in
// runWithAudioFallback and the first subtest reports one launch and an error;
// delete its `shouldNotRetry` check and the other two report two launches.
func TestEncodeVideo_SafePerformanceRetriesAudioAsAAC(t *testing.T) {
	cfg := &Config{PerformanceMode: "safe_performance"}
	cpuOnly := map[string]string{"accels": "", "encoders": "libx264"}

	t.Run("copy refused, AAC accepted", func(t *testing.T) {
		tempDir := t.TempDir()
		ledger := filepath.Join(tempDir, "calls.txt")
		fakeAudioCopyRefusingFFmpeg(t, tempDir, ledger, false)
		launches := countFFmpegLaunches(t)

		if err := InitEncodingSession(nil); err != nil {
			t.Fatalf("InitEncodingSession: %v", err)
		}
		defer func() { _ = CleanUp() }()

		err := EncodeVideo(cfg, cascadeVideo(tempDir), "libx264", 2_000_000,
			filepath.Join(tempDir, "output.mp4"), cpuOnly, func(float64) {}, make(chan struct{}))
		if err != nil {
			t.Fatalf("the AAC retry should have rescued the conversion, got: %v", err)
		}
		if got := launches.Load(); got != 2 {
			t.Fatalf("ffmpeg was launched %d time(s), want 2: the copy, then the AAC retry", got)
		}
		calls := ledgerCalls(t, ledger)
		if len(calls) != 2 {
			t.Fatalf("ledger holds %d call(s), want 2: %q", len(calls), calls)
		}
		if !strings.Contains(calls[0], "-c:a copy") {
			t.Errorf("safe_performance must try copying the audio first, first call: %s", calls[0])
		}
		if !strings.Contains(calls[1], "-c:a aac") {
			t.Errorf("the retry must ask for AAC, second call: %s", calls[1])
		}
	})

	t.Run("copy fails after output: no retry", func(t *testing.T) {
		tempDir := t.TempDir()
		ledger := filepath.Join(tempDir, "calls.txt")
		fakeAudioCopyRefusingFFmpeg(t, tempDir, ledger, true)
		launches := countFFmpegLaunches(t)

		if err := InitEncodingSession(nil); err != nil {
			t.Fatalf("InitEncodingSession: %v", err)
		}
		defer func() { _ = CleanUp() }()

		var progressed atomic.Bool
		err := EncodeVideo(cfg, cascadeVideo(tempDir), "libx264", 2_000_000,
			filepath.Join(tempDir, "output.mp4"), cpuOnly,
			func(float64) { progressed.Store(true) }, make(chan struct{}))

		if !progressed.Load() {
			t.Fatal("the stand-in never reported progress, so this test cannot tell a late failure from an early one")
		}
		if !errors.Is(err, errFailedAfterOutput) {
			t.Fatalf("error = %v, want it marked as a failure after output", err)
		}
		if got := launches.Load(); got != 1 {
			t.Fatalf("ffmpeg was launched %d time(s) after a failure that came after frames were written, want 1: the audio retry replayed it", got)
		}
	})

	t.Run("cancelled during the copy run: no retry", func(t *testing.T) {
		tempDir := t.TempDir()
		fakeHangingFFmpeg(t, tempDir)
		launches := countFFmpegLaunches(t)

		if err := InitEncodingSession(nil); err != nil {
			t.Fatalf("InitEncodingSession: %v", err)
		}
		defer func() { _ = CleanUp() }()

		cancel := make(chan struct{})
		var cancelOnce sync.Once
		progress := func(float64) { cancelOnce.Do(func() { close(cancel) }) }

		err := EncodeVideo(cfg, cascadeVideo(tempDir), "libx264", 2_000_000,
			filepath.Join(tempDir, "output.mp4"), cpuOnly, progress, cancel)
		if !errors.Is(err, ErrCancelled) {
			t.Fatalf("error = %v, want ErrCancelled", err)
		}
		if got := launches.Load(); got != 1 {
			t.Fatalf("ffmpeg was launched %d time(s) after a cancellation, want 1: the audio retry ran anyway", got)
		}
	})
}
