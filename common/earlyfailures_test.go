package common

import (
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
)

// stageRecorder is an ObservabilityHandler keeping the "stage" attribute of
// every error PerformEncoding records.
type stageRecorder struct {
	mu     sync.Mutex
	stages []string
}

func (r *stageRecorder) OnEvent(*EncodingEvent)      {}
func (r *stageRecorder) OnProgress(float64, string)  {}
func (r *stageRecorder) OnComplete(*EncodingMetrics) {}
func (r *stageRecorder) OnError(_ error, context map[string]interface{}) {
	r.mu.Lock()
	defer r.mu.Unlock()
	stage, _ := context["stage"].(string)
	r.stages = append(r.stages, stage)
}

func (r *stageRecorder) recorded() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]string(nil), r.stages...)
}

// recordStages swaps in a fresh global recorder holding only a stageRecorder,
// and restores the previous recorder -- with whatever handler it had -- when
// the test ends.
func recordStages(t *testing.T) *stageRecorder {
	t.Helper()
	previous := globalEventRecorder
	globalEventRecorder = NewEventRecorder()
	t.Cleanup(func() { globalEventRecorder = previous })

	rec := &stageRecorder{}
	SetObservabilityHandler(rec)
	return rec
}

// TestPerformEncoding_EarlyFailuresAreRecorded walks every refusal
// PerformEncoding makes before a map is written, which together were most of
// what kept it at 70 % coverage (E-19).
//
// Each case must return an error, leave nothing at or beside the output path,
// and record the failure under the stage that names it: that attribute is
// what a log attached to a bug report is read by. A `return nil` after a
// recorded error, a refusal moved behind the work it is meant to prevent, or
// two labels swapped by a copy-paste would each pass every other test.
//
// Counter-proofs: swap the "encoder_sanitization" and "bitrate_validation"
// labels in PerformEncoding and two cases redden; replace `return err` after
// ValidateBitrate with nothing and the bitrate case reports a nil error.
func TestPerformEncoding_EarlyFailuresAreRecorded(t *testing.T) {
	notAVideo := filepath.Join(t.TempDir(), "notes.mp4")
	if err := os.WriteFile(notAVideo, []byte("this is not a video"), 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	// Only the cases past CheckVideo render a clip, each its own: a clip lives
	// in the TempDir of the subtest that rendered it. The cases before
	// CheckVideo run on any machine.
	var ffmpeg map[string]string
	realClip := func(t *testing.T) string {
		t.Helper()
		var clip string
		clip, ffmpeg = refusalSetup(t, 640, 480)
		return clip
	}

	cases := []struct {
		name string
		// input returns the input path; it may skip the test.
		input     func(t *testing.T) string
		outputDir func(t *testing.T) string
		ui        UIHandler
		stage     string
	}{
		{
			name:      "relative input path",
			input:     func(*testing.T) string { return "clip.mp4" },
			outputDir: func(t *testing.T) string { return t.TempDir() },
			ui:        &MockHandler{},
			stage:     "input_validation",
		},
		{
			name:      "input that does not exist",
			input:     func(t *testing.T) string { return filepath.Join(t.TempDir(), "gone.mp4") },
			outputDir: func(t *testing.T) string { return t.TempDir() },
			ui:        &MockHandler{},
			stage:     "input_validation",
		},
		{
			name:      "output in a missing directory",
			input:     func(*testing.T) string { return notAVideo },
			outputDir: func(t *testing.T) string { return filepath.Join(t.TempDir(), "missing") },
			ui:        &MockHandler{},
			stage:     "output_validation",
		},
		{
			name: "input that is not a video",
			input: func(t *testing.T) string {
				if _, err := exec.LookPath("ffprobe"); err != nil {
					skipWithoutFFmpeg(t, "ffprobe not installed")
				}
				return notAVideo
			},
			outputDir: func(t *testing.T) string { return t.TempDir() },
			ui:        &MockHandler{},
			stage:     "video_check",
		},
		{
			name:      "encoder not in the whitelist",
			input:     realClip,
			outputDir: func(t *testing.T) string { return t.TempDir() },
			ui:        &integrationUI{bitrate: 2_000_000, encoder: "bogus_encoder"},
			stage:     "encoder_sanitization",
		},
		{
			name:      "bitrate below the configured minimum",
			input:     realClip,
			outputDir: func(t *testing.T) string { return t.TempDir() },
			ui:        &integrationUI{bitrate: DefaultConfig().MinBitrate - 1, encoder: "libx264"},
			stage:     "bitrate_validation",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			input := tc.input(t)
			outputDir := tc.outputDir(t)
			output := filepath.Join(outputDir, "output.mp4")
			rec := recordStages(t)

			err := PerformEncoding(nil, input, output, tc.ui, ffmpeg, make(chan struct{}))
			if err == nil {
				t.Fatal("PerformEncoding returned nil for a run it must refuse")
			}

			if _, statErr := os.Stat(output); !os.IsNotExist(statErr) {
				t.Errorf("something exists at the output path after a refusal (stat: %v)", statErr)
			}
			if entries, readErr := os.ReadDir(outputDir); readErr == nil && len(entries) != 0 {
				t.Errorf("the refusal left %d file(s) in the destination, first %q", len(entries), entries[0].Name())
			}

			stages := rec.recorded()
			if len(stages) != 1 || stages[0] != tc.stage {
				t.Errorf("recorded stages = %q, want exactly [%q] (error: %v)", stages, tc.stage, err)
			}
		})
	}
}
