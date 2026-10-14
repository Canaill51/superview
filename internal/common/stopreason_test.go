package common

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"testing"
)

func TestParseMemAvailable_ReadsTheKernelEstimate(t *testing.T) {
	meminfo := []byte("MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\nBuffers:           12345 kB\n")

	available, ok := parseMemAvailable(meminfo)
	if !ok {
		t.Fatal("expected MemAvailable to be found")
	}
	// The kernel reports kB; everything downstream works in bytes.
	if want := uint64(3894132) * 1024; available != want {
		t.Fatalf("expected %d bytes, got %d", want, available)
	}
}

// TestParseMemAvailable_IsNotMemFree guards the choice of field. MemFree
// ignores the reclaimable page cache, so reading it reports a healthy desktop
// as being out of memory -- and would make the diagnosis this parser feeds fire
// on machines that are perfectly fine.
func TestParseMemAvailable_IsNotMemFree(t *testing.T) {
	meminfo := []byte("MemTotal:        8039284 kB\nMemFree:          204552 kB\nMemAvailable:    3894132 kB\n")

	available, ok := parseMemAvailable(meminfo)
	if !ok {
		t.Fatal("expected MemAvailable to be found")
	}
	if available == uint64(204552)*1024 {
		t.Fatal("MemFree was read instead of MemAvailable")
	}
}

func TestParseMemAvailable_RejectsWhatItCannotRead(t *testing.T) {
	cases := map[string]string{
		"field absent":      "MemTotal:        8039284 kB\nMemFree:          204552 kB\n",
		"value missing":     "MemAvailable:\n",
		"value not numeric": "MemAvailable:    plenty kB\n",
		"empty file":        "",
	}

	for name, meminfo := range cases {
		t.Run(name, func(t *testing.T) {
			if _, ok := parseMemAvailable([]byte(meminfo)); ok {
				t.Fatalf("expected %s to be rejected", name)
			}
		})
	}
}

// TestFormatAvailableMemory_AgreesWithTheHelper checks the rendering against the
// reading rather than against a hardcoded number, which would only be true on
// the machine that wrote it.
func TestFormatAvailableMemory_AgreesWithTheHelper(t *testing.T) {
	formatted := formatAvailableMemory()

	available, ok := availableMemoryBytes()
	if !ok {
		if formatted != unknownMemory {
			t.Fatalf("with no reading available, expected %q, got %q", unknownMemory, formatted)
		}
		return
	}

	if formatted == unknownMemory {
		t.Fatalf("a reading of %d bytes was available but formatAvailableMemory said %q", available, formatted)
	}
	if !strings.HasSuffix(formatted, " GB") {
		t.Fatalf("expected a GB figure, got %q", formatted)
	}
}

func TestAvailableMemoryBytes_MatchesProcMeminfo(t *testing.T) {
	data, err := os.ReadFile("/proc/meminfo")
	if err != nil {
		// Windows, and any system without procfs: the contract there is that
		// the reading is refused, not invented.
		if _, ok := availableMemoryBytes(); ok {
			t.Fatal("no /proc/meminfo, yet a reading was reported")
		}
		return
	}

	expected, ok := parseMemAvailable(data)
	if !ok {
		t.Skip("/proc/meminfo carries no MemAvailable line on this kernel")
	}

	available, got := availableMemoryBytes()
	if !got {
		t.Fatal("expected a reading on a machine that has /proc/meminfo")
	}
	// Memory moves between the two reads; agreement within 25% is enough to
	// prove the same field is being read, which is what this test is about.
	delta := float64(available) - float64(expected)
	if delta < 0 {
		delta = -delta
	}
	if delta > 0.25*float64(expected) {
		t.Fatalf("reading %d is not the MemAvailable of %d", available, expected)
	}
}

// TestClassifyFfmpegFailure_KeepsStderrForOrdinaryFailures pins what must not
// change: everything that is not a kill keeps the diagnosis it had, ffmpeg's
// own words included.
func TestClassifyFfmpegFailure_KeepsStderrForOrdinaryFailures(t *testing.T) {
	underlying := errors.New("exit status 1")

	err := classifyFfmpegFailure(underlying, "Unknown encoder 'nope'")
	if err == nil {
		t.Fatal("expected an error")
	}
	if !errors.Is(err, underlying) {
		t.Fatalf("the underlying error must stay wrapped, got %v", err)
	}
	if !strings.Contains(err.Error(), "Unknown encoder 'nope'") {
		t.Fatalf("ffmpeg's stderr must be kept, got %q", err.Error())
	}
	var encoderErr *EncoderError
	if errors.As(err, &encoderErr) {
		t.Fatalf("an ordinary failure must not be diagnosed as an encoder problem: %v", err)
	}
}

func TestClassifyFfmpegFailure_WithoutStderr(t *testing.T) {
	underlying := fmt.Errorf("exit status 8")

	err := classifyFfmpegFailure(underlying, "")
	if err == nil {
		t.Fatal("expected an error")
	}
	if !errors.Is(err, underlying) {
		t.Fatalf("the underlying error must stay wrapped, got %v", err)
	}
	if strings.Contains(err.Error(), "ffmpeg stderr") {
		t.Fatalf("no stderr was produced, so none should be announced: %q", err.Error())
	}
}
