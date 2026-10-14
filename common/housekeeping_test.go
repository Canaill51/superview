package common

import (
	"errors"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// TestSummaryAndLogMetrics_NoRecursiveRLock guards E-22: Summary and LogMetrics
// held the read lock and called ElapsedTime, which took it again. With a writer
// queued between the two, neither side can proceed.
func TestSummaryAndLogMetrics_NoRecursiveRLock(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	log := slog.New(slog.NewTextHandler(io.Discard, nil))

	stop := make(chan struct{})
	writerDone := make(chan struct{})
	go func() {
		defer close(writerDone)
		for {
			select {
			case <-stop:
				return
			default:
				m.RecordProgress(10)
			}
		}
	}()

	done := make(chan struct{})
	go func() {
		defer close(done)
		for i := 0; i < 5000; i++ {
			_ = m.Summary()
			m.LogMetrics(log)
		}
	}()

	select {
	case <-done:
		close(stop)
		<-writerDone
	case <-time.After(5 * time.Second):
		t.Fatal("Summary/LogMetrics deadlocked against a concurrent RecordProgress")
	}
}

// TestFindWindowsToolBinary_IgnoresRelativeCandidates: with the variables unset,
// filepath.Join yields relative paths that os.Stat resolves in the working
// directory.
func TestFindWindowsToolBinary_IgnoresRelativeCandidates(t *testing.T) {
	t.Setenv("LOCALAPPDATA", "")
	t.Setenv("ProgramFiles", "")
	t.Setenv("USERPROFILE", "")

	tmp := t.TempDir()
	planted := filepath.Join(tmp, "Microsoft", "WinGet", "Links")
	if err := os.MkdirAll(planted, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(planted, "ffmpeg.exe"), []byte("x"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Chdir(tmp)

	if got := findWindowsToolBinary("ffmpeg"); got != "" {
		t.Errorf("findWindowsToolBinary = %q, want \"\" (a relative path was resolved against the working directory)", got)
	}
}

// TestCappedLogWriter_StaysUnderCap: the cap used to apply only at open.
func TestCappedLogWriter_StaysUnderCap(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	const limit = 1024

	w, path, err := OpenLogFile("superview-test", limit)
	if err != nil {
		t.Fatal(err)
	}
	record := []byte(strings.Repeat("r", 99) + "\n")
	for i := 0; i < 200; i++ {
		if _, err := w.Write(record); err != nil {
			t.Fatal(err)
		}
		info, err := os.Stat(path)
		if err != nil {
			t.Fatal(err)
		}
		if info.Size() > limit+int64(len(record)) {
			t.Fatalf("after %d writes the log is %d bytes, cap %d + one record", i+1, info.Size(), limit)
		}
	}
	if err := w.Close(); err != nil {
		t.Fatal(err)
	}
	info, _ := os.Stat(path)
	if info.Size() == 0 {
		t.Error("the log is empty after the last write; the newest record must survive a truncation")
	}
}

// TestCgroupLimitsAvailableMemory: in a container MemAvailable describes the host.
func TestCgroupLimitsAvailableMemory(t *testing.T) {
	dir := t.TempDir()
	write := func(name, content string) string {
		p := filepath.Join(dir, name)
		if err := os.WriteFile(p, []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
		return p
	}
	oldInfo, oldMax, oldCur := procMeminfoPath, cgroupMemoryMaxPath, cgroupMemoryCurPath
	t.Cleanup(func() { procMeminfoPath, cgroupMemoryMaxPath, cgroupMemoryCurPath = oldInfo, oldMax, oldCur })

	procMeminfoPath = write("meminfo", "MemAvailable:   8388608 kB\n") // 8 GiB
	cgroupMemoryCurPath = write("current", "629145600\n")              // 600 MiB

	cgroupMemoryMaxPath = write("max", "1073741824\n") // 1 GiB
	got, ok := availableMemoryBytes()
	if want := uint64(1073741824 - 629145600); !ok || got != want {
		t.Errorf("with a 1 GiB cgroup limit: got %d, %v; want %d", got, ok, want)
	}

	cgroupMemoryMaxPath = write("max", "max\n")
	got, ok = availableMemoryBytes()
	if want := uint64(8 << 30); !ok || got != want {
		t.Errorf("with no cgroup limit: got %d, %v; want MemAvailable %d", got, ok, want)
	}
}

// TestSessionErrorsAreTyped: E-23, the session's failures were bare errors.
func TestSessionErrorsAreTyped(t *testing.T) {
	if err := InitEncodingSession(nil); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = CleanUp() }()

	err := InitEncodingSession(nil)
	if !errors.As(err, new(*SessionError)) {
		t.Errorf("second InitEncodingSession: %T %v, want *SessionError", err, err)
	}

	_ = CleanUp()
	_, _, err = getSessionPaths()
	if !errors.As(err, new(*SessionError)) {
		t.Errorf("getSessionPaths without session: %T %v, want *SessionError", err, err)
	}
}

func TestSessionError_UnwrapKeepsCause(t *testing.T) {
	cause := os.ErrPermission
	err := &SessionError{Msg: "failed to create temp directory", Err: cause}
	if !errors.Is(err, os.ErrPermission) {
		t.Error("errors.Is cannot see the cause through SessionError")
	}
	if !strings.Contains(err.Error(), cause.Error()) {
		t.Errorf("Error() = %q, want it to include the cause", err.Error())
	}
}

// reAudioConfig is the default configuration with audio always re-encoded.
//
// The stand-in ffmpeg of the fallback tests refuses every run. In
// safe_performance mode (the default since DEC-33) a refused run is retried once
// with AAC audio, which doubles the launches those tests count. They are about
// the video fallback ladder, so they pin the mode that has no audio retry.
func reAudioConfig() *Config {
	cfg := DefaultConfig()
	cfg.PerformanceMode = "safe"
	return cfg
}

func TestNormalizePerformanceMode_EmptyIsSafePerformance(t *testing.T) {
	if got := normalizePerformanceMode(""); got != "safe_performance" {
		t.Errorf("normalizePerformanceMode(\"\") = %q, want safe_performance", got)
	}
	if got := normalizePerformanceMode("safe"); got != "safe" {
		t.Errorf("an explicit \"safe\" must stay accepted, got %q", got)
	}
}
