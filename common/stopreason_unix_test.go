//go:build !windows

package common

import (
	"errors"
	"os/exec"
	"strings"
	"syscall"
	"testing"
)

// killedHelperProcess starts a process, kills it the way the kernel's
// out-of-memory killer does, and hands back the wait error that produces.
//
// A real process rather than a fabricated exec.ExitError: the wait status is
// filled in by the operating system, and a hand-built one would prove only that
// the test and the code agree on a struct we both invented.
func killedHelperProcess(t *testing.T) error {
	t.Helper()

	cmd := exec.Command("sleep", "30")
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot start a helper process on this machine: %v", err)
	}
	if err := cmd.Process.Signal(syscall.SIGKILL); err != nil {
		t.Fatalf("cannot kill the helper process: %v", err)
	}

	err := cmd.Wait()
	if err == nil {
		t.Fatal("a killed process must report a failure")
	}
	return err
}

func TestKilledBySignal_NamesTheSignal(t *testing.T) {
	sig, killed := killedBySignal(killedHelperProcess(t))
	if !killed {
		t.Fatal("a process killed with SIGKILL must be reported as killed by a signal")
	}
	if sig != "killed" {
		t.Fatalf("expected the signal to be named \"killed\", got %q", sig)
	}
}

// TestKilledBySignal_IgnoresAnOrdinaryFailure is the other half of the
// contract: without it, the memory diagnosis would be attached to every failed
// encode, and the message would be wrong far more often than it was right.
func TestKilledBySignal_IgnoresAnOrdinaryFailure(t *testing.T) {
	err := exec.Command("sh", "-c", "exit 3").Run()
	if err == nil {
		t.Fatal("expected a non-zero exit to report a failure")
	}

	if sig, killed := killedBySignal(err); killed {
		t.Fatalf("an exit status is not a signal, yet %q was reported", sig)
	}
	if _, killed := killedBySignal(errors.New("not an exit error at all")); killed {
		t.Fatal("an error that is not an exec.ExitError cannot name a signal")
	}
}

// TestClassifyFfmpegFailure_DiagnosesAKillAsMemoryExhaustion is the test for the
// failure the user actually hit: the kernel took ffmpeg, and the application
// reported "ffmpeg failed: signal: killed" without the word memory anywhere.
func TestClassifyFfmpegFailure_DiagnosesAKillAsMemoryExhaustion(t *testing.T) {
	err := classifyFfmpegFailure(killedHelperProcess(t), "")

	var encoderErr *EncoderError
	if !errors.As(err, &encoderErr) {
		t.Fatalf("a kill must be diagnosed as an encoder error, got %T: %v", err, err)
	}

	message := err.Error()
	for _, want := range []string{"killed by the system", "signal: killed", "ran out of memory"} {
		if !strings.Contains(message, want) {
			t.Fatalf("expected the diagnosis to contain %q, got %q", want, message)
		}
	}
}

// TestClassifyFfmpegFailure_ReportsTheMemoryReadingWhenThereIsOne ties the
// message to the machine it is printed on. The number is what turns "out of
// memory" from a guess the user has to believe into something they can check.
func TestClassifyFfmpegFailure_ReportsTheMemoryReadingWhenThereIsOne(t *testing.T) {
	if _, ok := availableMemoryBytes(); !ok {
		t.Skip("no memory reading available on this machine")
	}

	message := classifyFfmpegFailure(killedHelperProcess(t), "").Error()
	if !strings.Contains(message, "Memory available when it stopped:") {
		t.Fatalf("expected the memory reading in the diagnosis, got %q", message)
	}
	if strings.Contains(message, unknownMemory) {
		t.Fatalf("a reading was available, so the message must not say %q: %s", unknownMemory, message)
	}
}
