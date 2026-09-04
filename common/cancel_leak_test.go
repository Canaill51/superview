//go:build linux

package common

import (
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

// zombieChildren counts child processes of this test binary left in the "Z"
// (defunct) state, by reading /proc directly so the test carries no dependency
// on ps or any external tool.
func zombieChildren(t *testing.T) int {
	t.Helper()

	entries, err := os.ReadDir("/proc")
	if err != nil {
		t.Skipf("cannot read /proc: %v", err)
	}
	self := os.Getpid()

	count := 0
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue // not a process directory
		}
		data, err := os.ReadFile(filepath.Join("/proc", e.Name(), "stat"))
		if err != nil {
			continue // process exited between ReadDir and here
		}
		// Format: pid (comm) state ppid ... — comm may contain spaces and
		// parentheses, so parse after the final ')'.
		close := strings.LastIndex(string(data), ")")
		if close < 0 {
			continue
		}
		fields := strings.Fields(string(data)[close+1:])
		if len(fields) < 2 {
			continue
		}
		state, ppid := fields[0], fields[1]
		if state == "Z" && ppid == strconv.Itoa(self) && pid != self {
			count++
		}
	}
	return count
}

// TestEncodeVideo_CancelReapsProcess guards against leaking a zombie ffmpeg on
// every cancellation.
//
// The cancel path killed the process and returned without calling cmd.Wait, so
// the child was never reaped and the goroutine os/exec spawns to drain stderr
// was never released. Measured before the fix: five cancellations left five
// zombies, which persisted for the lifetime of the process.
func TestEncodeVideo_CancelReapsProcess(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping process-level test in short mode")
	}
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("ffmpeg not installed")
	}

	input := makeTestClip(t, 640, 480, 20)
	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		t.Skipf("CheckFfmpeg failed: %v", err)
	}

	before := zombieChildren(t)

	const rounds = 3
	for i := 0; i < rounds; i++ {
		video, err := CheckVideo(input)
		if err != nil {
			t.Fatalf("CheckVideo: %v", err)
		}
		if err := InitEncodingSession(nil); err != nil {
			t.Fatalf("InitEncodingSession: %v", err)
		}
		if err := GeneratePGM(video, false); err != nil {
			t.Fatalf("GeneratePGM: %v", err)
		}

		cancel := make(chan struct{})
		done := make(chan error, 1)
		go func() {
			done <- EncodeVideo(nil, video, "libx264", 2_000_000,
				filepath.Join(t.TempDir(), "out.mp4"), ffmpeg, func(float64) {}, cancel)
		}()

		time.Sleep(250 * time.Millisecond) // let ffmpeg actually start
		close(cancel)

		if err := <-done; err == nil {
			t.Fatal("expected an interruption error")
		}
		if err := CleanUp(); err != nil {
			t.Errorf("CleanUp: %v", err)
		}
	}

	// Reaping is synchronous in EncodeVideo, so no settling delay is needed.
	if leaked := zombieChildren(t) - before; leaked > 0 {
		t.Errorf("%d cancellations leaked %d zombie process(es); cmd.Wait is not being called on the cancel path",
			rounds, leaked)
	}
}
