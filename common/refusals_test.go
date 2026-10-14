package common

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

// withFreeDisk hands checkOutputSpace a destination volume of the given size,
// or a reading that fails.
func withFreeDisk(t *testing.T, freeGB float64, err error) {
	t.Helper()
	previous := readFreeDiskGB
	readFreeDiskGB = func(string) (float64, error) { return freeGB, err }
	t.Cleanup(func() { readFreeDiskGB = previous })
}

// TestCheckOutputSpace_RefusesAFullDestination pins E-16: 100 Mb/s for thirty
// minutes is 22 GB, and a 1 GB volume used to be found full twenty minutes in.
func TestCheckOutputSpace_RefusesAFullDestination(t *testing.T) {
	withFreeDisk(t, 1, nil)
	dir := t.TempDir()

	err := checkOutputSpace(filepath.Join(dir, "out.mp4"), 100_000_000, 30*60)
	if err == nil {
		t.Fatal("a 22 GB output was allowed onto a 1 GB volume")
	}
	for _, want := range []string{dir, "MB", "1024 MB is free"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("error %q should mention %q", err, want)
		}
	}

	// And a volume with room is not refused: the check must not become a
	// blanket refusal.
	withFreeDisk(t, 100, nil)
	if err := checkOutputSpace(filepath.Join(dir, "out.mp4"), 100_000_000, 30*60); err != nil {
		t.Errorf("a 22 GB output was refused on a 100 GB volume: %v", err)
	}
}

// TestCheckOutputSpace_UnreadableFreeSpaceAllowsTheEncode follows
// checkTempSpaceForMaps: a reading that fails is not a reason to refuse.
func TestCheckOutputSpace_UnreadableFreeSpaceAllowsTheEncode(t *testing.T) {
	withFreeDisk(t, 0, errors.New("statfs: operation not supported"))
	if err := checkOutputSpace(filepath.Join(t.TempDir(), "out.mp4"), 100_000_000, 30*60); err != nil {
		t.Errorf("an unreadable free space refused the encode: %v", err)
	}
}

// TestEncodingMetrics_EstimateStartsAtFirstProgress pins the other half of
// E-16: StartTime is taken before validation, the maps and the wait for the
// first frame, and the estimate measured from it read hours at one percent.
func TestEncodingMetrics_EstimateStartsAtFirstProgress(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.StartTime = time.Now().Add(-time.Hour)

	m.RecordProgress(1)
	if got := m.Remaining(); got != 0 {
		t.Errorf("an estimate was given at the first progress report: %v", got)
	}

	time.Sleep(20 * time.Millisecond)
	m.RecordProgress(4)
	got := m.Remaining()
	if got <= 0 {
		t.Fatalf("no estimate after three percent: %v", got)
	}
	if got >= time.Minute {
		t.Errorf("remaining = %v after 3%% in 20 ms: the hour before the first frame is still counted", got)
	}
}

// TestEncodingMetrics_EstimateRestartsWithTheEncode covers the fallback: a new
// encoder starts again at 0%, and must not inherit the first one's origin.
func TestEncodingMetrics_EstimateRestartsWithTheEncode(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordProgress(1)
	m.RecordProgress(40)
	m.RecordProgress(0)
	if got := m.Remaining(); got != 0 {
		t.Errorf("the estimate survived a restart at 0%%: %v", got)
	}
	m.RecordProgress(0.5)
	m.RecordProgress(1)
	if got := m.Remaining(); got != 0 {
		t.Errorf("an estimate was given half a percent after the restart: %v", got)
	}
}

func writeConfig(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "superview.yaml")
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write config: %v", err)
	}
	return path
}

// TestLoadConfig_UnknownKeyIsAnError pins E-21: a misspelt key was dropped in
// silence, the way a user's video_preset was already lost once.
func TestLoadConfig_UnknownKeyIsAnError(t *testing.T) {
	_, err := LoadConfig(writeConfig(t, "video-preset: slow\n"))
	if err == nil {
		t.Fatal("an unknown key was accepted")
	}
	if !strings.Contains(err.Error(), "video-preset") {
		t.Errorf("error %q does not name the unknown key", err)
	}
}

func TestLoadConfig_MinAboveMaxIsAnError(t *testing.T) {
	if _, err := LoadConfig(writeConfig(t, "min_bitrate: 10\nmax_bitrate: 5\n")); err == nil {
		t.Error("min_bitrate above max_bitrate was accepted")
	}
	// The environment can produce the same inversion over a sane file.
	t.Setenv("SUPERVIEW_MIN_BITRATE", "300000000")
	if _, err := LoadConfig(""); err == nil {
		t.Error("SUPERVIEW_MIN_BITRATE above the default maximum was accepted")
	}
}

func TestLoadConfig_EncoderCodecsAreTrimmed(t *testing.T) {
	t.Setenv("SUPERVIEW_ENCODER_CODECS", "264, 265")
	cfg, err := LoadConfig("")
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	if want := []string{"264", "265"}; !reflect.DeepEqual(cfg.EncoderCodecs, want) {
		t.Errorf("EncoderCodecs = %q, want %q", cfg.EncoderCodecs, want)
	}
}

func TestLoadConfig_EmptyEncoderCodecsFallBackToDefaults(t *testing.T) {
	cfg, err := LoadConfig(writeConfig(t, "encoder_codecs: []\n"))
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	if !reflect.DeepEqual(cfg.EncoderCodecs, defaultConfig.EncoderCodecs) {
		t.Errorf("EncoderCodecs = %q, want the defaults %q", cfg.EncoderCodecs, defaultConfig.EncoderCodecs)
	}

	t.Setenv("SUPERVIEW_ENCODER_CODECS", " , ,")
	cfg, err = LoadConfig("")
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	if !reflect.DeepEqual(cfg.EncoderCodecs, defaultConfig.EncoderCodecs) {
		t.Errorf("EncoderCodecs = %q from commas alone, want the defaults", cfg.EncoderCodecs)
	}
}

// TestLoadConfig_ShippedFileStillLoads guards the strict decoder against the
// one file every user starts from.
func TestLoadConfig_ShippedFileStillLoads(t *testing.T) {
	if _, err := LoadConfig(filepath.Join("..", "superview.yaml")); err != nil {
		t.Errorf("the superview.yaml shipped with the application no longer loads: %v", err)
	}
	// A file holding only comments is an empty document, not an error.
	if _, err := LoadConfig(writeConfig(t, "# nothing set\n")); err != nil {
		t.Errorf("a comment-only file was refused: %v", err)
	}
}

// TestIsSensitiveSystemPath exercises the system-directory refusal itself
// (E-17, E-19): no test made it return true, and a destination under /etc was
// only ever caught by the ".." check before it.
func TestIsSensitiveSystemPath(t *testing.T) {
	sep := string(os.PathSeparator)
	for _, dir := range sensitiveSystemDirectories() {
		if !isSensitiveSystemPath(dir) {
			t.Errorf("%s itself is not refused", dir)
		}
		if child := dir + sep + "x.mp4"; !isSensitiveSystemPath(child) {
			t.Errorf("%s is not refused", child)
		}
		// A sibling that merely shares the prefix is somewhere else.
		if sibling := dir + "etera" + sep + "x.mp4"; isSensitiveSystemPath(sibling) {
			t.Errorf("%s is refused, but only shares a prefix with %s", sibling, dir)
		}
	}
}

// refusalSetup renders a clip and returns what PerformEncoding needs, skipping
// when ffmpeg or libx264 is missing.
func refusalSetup(t *testing.T, width, height int) (input string, ffmpeg map[string]string) {
	t.Helper()
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}
	input = makeTestClip(t, width, height, 1)
	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "CheckFfmpeg failed: %v", err)
	}
	if !strings.Contains(ffmpeg["encoders"], "libx264") {
		skipWithoutFFmpeg(t, "libx264 not available in this ffmpeg build")
	}
	return input, ffmpeg
}

// TestIntegration_PortraitSourceIsRefusedUpFront proves the geometry check is
// wired into PerformEncoding, ahead of the memory check, and not merely left to
// the defensive copy in GeneratePGM: the memory reading must never be taken.
func TestIntegration_PortraitSourceIsRefusedUpFront(t *testing.T) {
	input, ffmpeg := refusalSetup(t, 360, 640)

	previous := readAvailableMemory
	readAvailableMemory = func() (uint64, bool) {
		t.Error("the memory check ran: the portrait frame was not refused before it")
		return 0, false
	}
	t.Cleanup(func() { readAvailableMemory = previous })

	output := filepath.Join(t.TempDir(), "output.mp4")
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}
	err := PerformEncoding(nil, input, output, ui, ffmpeg, make(chan struct{}))

	var invalid *InvalidVideoError
	if !errors.As(err, &invalid) || !strings.Contains(err.Error(), "9:8") {
		t.Fatalf("a 360x640 source was not refused for its geometry: %v", err)
	}
	if ui.progressCall != 0 {
		t.Error("ffmpeg was started for a frame that folds")
	}
}

// TestIntegration_FullDestinationIsRefusedUpFront proves the output-space check
// is wired into PerformEncoding before anything is encoded.
func TestIntegration_FullDestinationIsRefusedUpFront(t *testing.T) {
	input, ffmpeg := refusalSetup(t, 640, 480)
	withFreeDisk(t, 0, nil)

	outputDir := t.TempDir()
	ui := &integrationUI{bitrate: 2_000_000, encoder: "libx264"}
	err := PerformEncoding(nil, input, filepath.Join(outputDir, "output.mp4"), ui, ffmpeg, make(chan struct{}))
	if err == nil || !strings.Contains(err.Error(), "not enough space in "+outputDir) {
		t.Fatalf("a destination with no room was not refused: %v", err)
	}
	if ui.progressCall != 0 {
		t.Error("ffmpeg was started towards a full destination")
	}
}

// TestLoadConfig_AcceptsTheFileReleasesUsedToShip pins the review finding on
// E-21: v0.1.6 shipped superview.yaml with quality_preset, min_video_width and
// min_video_height. The strict decoder refused that whole file, so a user who
// had copied and tuned it lost every setting on upgrade, with one log line to
// say so. Retired keys are dropped with a warning; other unknown keys are still
// errors.
//
// Counter-proof: empty retiredConfigKeys and the first case fails.
func TestLoadConfig_AcceptsTheFileReleasesUsedToShip(t *testing.T) {
	dir := t.TempDir()
	legacy := filepath.Join(dir, "superview.yaml")
	if err := os.WriteFile(legacy, []byte(`min_bitrate: 102400
max_bitrate: 209715200
quality_preset: balanced
temp_dir_prefix: "superview-*"
encoder_codecs:
  - "264"
  - "265"
log_level: info
performance_mode: safe
video_preset: "slow"
filter_threads: 0
encoder_threads: 0
min_video_width: 320
min_video_height: 240
`), 0o600); err != nil {
		t.Fatal(err)
	}
	cfg, err := LoadConfig(legacy)
	if err != nil {
		t.Fatalf("the file v0.1.6 shipped must still load, got: %v", err)
	}
	if cfg.VideoPreset != "slow" || cfg.PerformanceMode != "safe" {
		t.Fatalf("the user's own settings must survive: preset=%q mode=%q", cfg.VideoPreset, cfg.PerformanceMode)
	}

	typo := filepath.Join(dir, "typo.yaml")
	if err := os.WriteFile(typo, []byte("quality_preset: balanced\nvideo-preset: slow\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadConfig(typo); err == nil || !strings.Contains(err.Error(), "video-preset") {
		t.Fatalf("an unknown key that was never shipped must still be an error naming it, got: %v", err)
	}
}
