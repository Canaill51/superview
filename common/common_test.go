package common

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// ============================================================================
// Tests for ValidateBitrate
// ============================================================================

func TestValidateBitrate_ValidBitrate(t *testing.T) {
	tests := []struct {
		name    string
		bitrate int
		minBits int
		maxBits int
		wantErr bool
	}{
		{
			name:    "valid bitrate in range",
			bitrate: 5000000,  // 5 Mbps
			minBits: 100000,   // 100k (recommended minimum)
			maxBits: 50000000, // 50M (recommended maximum)
			wantErr: false,
		},
		{
			name:    "bitrate at minimum boundary",
			bitrate: 100000,
			minBits: 100000,
			maxBits: 50000000,
			wantErr: false,
		},
		{
			name:    "bitrate at maximum boundary",
			bitrate: 50000000,
			minBits: 100000,
			maxBits: 50000000,
			wantErr: false,
		},
		{
			name:    "bitrate below minimum",
			bitrate: 50000, // 50k < 100k minimum
			minBits: 100000,
			maxBits: 50000000,
			wantErr: true,
		},
		{
			name:    "bitrate above maximum",
			bitrate: 100000000, // 100M > 50M maximum
			minBits: 100000,
			maxBits: 50000000,
			wantErr: true,
		},
		{
			name:    "zero bitrate",
			bitrate: 0,
			minBits: 100000,
			maxBits: 50000000,
			wantErr: true,
		},
		{
			name:    "negative bitrate",
			bitrate: -1000000,
			minBits: 100000,
			maxBits: 50000000,
			wantErr: true,
		},
		{
			name:    "no min/max constraints",
			bitrate: 12345,
			minBits: 0,
			maxBits: 0,
			wantErr: false, // Only checks positive
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateBitrate(tt.bitrate, tt.minBits, tt.maxBits)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateBitrate(%d, %d, %d) error = %v, wantErr = %v",
					tt.bitrate, tt.minBits, tt.maxBits, err, tt.wantErr)
			}
		})
	}
}

// ============================================================================
// Tests for VideoSpecs.Validate
// ============================================================================

func TestVideoSpecs_ValidateValid(t *testing.T) {
	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				Duration:      "60.5",
				DurationFloat: 60.5,
				Bitrate:       "5000000",
				BitrateInt:    5000000,
			},
		},
	}

	err := video.Validate()
	if err != nil {
		t.Errorf("Valid video failed validation: %v", err)
	}
}

func TestVideoSpecs_ValidateNoStreams(t *testing.T) {
	video := &VideoSpecs{
		File:    "test.mp4",
		Streams: []VideoStream{},
	}

	err := video.Validate()
	if err == nil {
		t.Errorf("Video with no streams should fail validation")
	}

	if _, ok := err.(*InvalidVideoError); !ok {
		t.Errorf("Expected InvalidVideoError, got %T", err)
	}
}

func TestVideoSpecs_ValidateInvalidDimensions(t *testing.T) {
	tests := []struct {
		name   string
		width  int
		height int
	}{
		{"zero width", 0, 1080},
		{"zero height", 1920, 0},
		{"negative width", -100, 1080},
		{"negative height", 1920, -100},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			video := &VideoSpecs{
				File: "test.mp4",
				Streams: []VideoStream{
					{
						Codec:         "h264",
						Width:         tt.width,
						Height:        tt.height,
						Duration:      "60.5",
						DurationFloat: 60.5,
						Bitrate:       "5000000",
						BitrateInt:    5000000,
					},
				},
			}

			err := video.Validate()
			if err == nil {
				t.Errorf("Invalid dimensions (%d x %d) should fail validation", tt.width, tt.height)
			}
		})
	}
}

func TestVideoSpecs_ValidateInvalidDuration(t *testing.T) {
	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				Duration:      "0",
				DurationFloat: 0, // Invalid: must be > 0
				Bitrate:       "5000000",
				BitrateInt:    5000000,
			},
		},
	}

	err := video.Validate()
	if err == nil {
		t.Errorf("Video with invalid duration should fail validation")
	}
}

func TestVideoSpecs_ValidateInvalidBitrate(t *testing.T) {
	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				Duration:      "60.5",
				DurationFloat: 60.5,
				Bitrate:       "0",
				BitrateInt:    0, // Invalid: must be > 0
			},
		},
	}

	err := video.Validate()
	if err == nil {
		t.Errorf("Video with invalid bitrate should fail validation")
	}
}

func TestVideoSpecs_ValidateMissingCodec(t *testing.T) {
	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "", // Invalid: empty codec
				Width:         1920,
				Height:        1080,
				Duration:      "60.5",
				DurationFloat: 60.5,
				Bitrate:       "5000000",
				BitrateInt:    5000000,
			},
		},
	}

	err := video.Validate()
	if err == nil {
		t.Errorf("Video with missing codec should fail validation")
	}
}

// ============================================================================
// Tests for FindEncoder
// ============================================================================

func TestFindEncoder_UseInputCodec(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265,hevc",
		"accels":   "",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	// Empty codec means use input codec
	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with empty codec failed: %v", err)
	}

	if encoder != "libx264" {
		t.Errorf("Expected libx264, got %s", encoder)
	}
}

func TestFindEncoder_PreferHardwareWhenAvailable(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_nvenc,libx264,libx265",
		"accels":   "cuda",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with hardware option failed: %v", err)
	}

	if encoder != "h264_nvenc" {
		t.Errorf("Expected h264_nvenc, got %s", encoder)
	}
}

func TestFindEncoder_AllowsNVENCWithoutCUDAHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_nvenc,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with NVENC on Windows hardware failed: %v", err)
	}

	if encoder != "h264_nvenc" {
		t.Errorf("Expected h264_nvenc, got %s", encoder)
	}
}

func TestFindEncoder_AllowsAMFWithoutDedicatedHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_amf,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with AMF on Windows hardware failed: %v", err)
	}

	if encoder != "h264_amf" {
		t.Errorf("Expected h264_amf, got %s", encoder)
	}
}

func TestFindEncoder_AllowsQSVWithoutDedicatedHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_qsv,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with QSV on Windows hardware failed: %v", err)
	}

	if encoder != "h264_qsv" {
		t.Errorf("Expected h264_qsv, got %s", encoder)
	}
}

func TestFindEncoder_AllowsHEVCNVENCWithoutCUDAHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "hevc_nvenc,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "hevc",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with HEVC NVENC on Windows hardware failed: %v", err)
	}

	if encoder != "hevc_nvenc" {
		t.Errorf("Expected hevc_nvenc, got %s", encoder)
	}
}

func TestFindEncoder_AllowsHEVCAMFWithoutDedicatedHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "hevc_amf,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "hevc",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with HEVC AMF on Windows hardware failed: %v", err)
	}

	if encoder != "hevc_amf" {
		t.Errorf("Expected hevc_amf, got %s", encoder)
	}
}

func TestFindEncoder_AllowsHEVCQSVWithoutDedicatedHwAccel(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "hevc_qsv,libx264,libx265",
		"accels":   "d3d11va,dxva2",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "hevc",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with HEVC QSV on Windows hardware failed: %v", err)
	}

	if encoder != "hevc_qsv" {
		t.Errorf("Expected hevc_qsv, got %s", encoder)
	}
}

func TestFindEncoder_SelectValidEncoder(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265,hevc",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("libx265", ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder with valid encoder failed: %v", err)
	}

	if encoder != "libx265" {
		t.Errorf("Expected libx265, got %s", encoder)
	}
}

func TestBuildEncodeBaseArgs_Defaults(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx264", 2000000, "aac", 6, 0, "")

	joined := strings.Join(args, " ")
	// -re throttles the input to realtime; it must never appear for file output.
	if strings.Contains(joined, "-re") {
		t.Fatalf("-re must not be emitted, it caps the encode at realtime speed, args: %v", args)
	}
	if strings.Contains(joined, "-preset") {
		t.Fatalf("did not expect -preset by default, args: %v", args)
	}
	if strings.Contains(joined, "-filter_threads") {
		t.Fatalf("did not expect -filter_threads when set to 0, args: %v", args)
	}
	if !strings.Contains(joined, "-threads 6") {
		t.Fatalf("expected encoder threads to be applied, args: %v", args)
	}
	if !strings.Contains(joined, "-b:v 2000000") {
		t.Fatalf("expected bitrate to be applied, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_PerformanceWithPresetAndFilterThreads(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx264", 2000000, "copy", 8, 4, "fast")

	joined := strings.Join(args, " ")
	if strings.Contains(joined, "-re") {
		t.Fatalf("did not expect -re, args: %v", args)
	}
	if !strings.Contains(joined, "-preset fast") {
		t.Fatalf("expected preset to be applied, args: %v", args)
	}
	if !strings.Contains(joined, "-filter_threads 4") {
		t.Fatalf("expected filter threads to be applied, args: %v", args)
	}
	if !strings.Contains(joined, "-threads 8") {
		t.Fatalf("expected encoder threads to be applied, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_AMFMapsFastPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_amf", 2000000, "copy", 8, 4, "fast")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-preset speed") {
		t.Fatalf("expected AMF fast preset to map to speed, args: %v", args)
	}
	if strings.Contains(joined, "-preset fast") {
		t.Fatalf("did not expect unsupported AMF preset name to leak through, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_AMFMapsMediumPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "hevc_amf", 2000000, "copy", 8, 4, "medium")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-preset balanced") {
		t.Fatalf("expected AMF medium preset to map to balanced, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_AMFSuppressesUnsupportedPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_amf", 2000000, "copy", 8, 4, "p4")
	joined := strings.Join(args, " ")

	if strings.Contains(joined, "-preset") {
		t.Fatalf("did not expect unsupported AMF preset to be passed through, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_QSVKeepsSupportedPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_qsv", 2000000, "copy", 8, 4, "fast")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-preset fast") {
		t.Fatalf("expected QSV supported preset to be kept, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_QSVMapsUltrafastPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_qsv", 2000000, "copy", 8, 4, "ultrafast")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-preset veryfast") {
		t.Fatalf("expected QSV ultrafast preset to map to veryfast, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_QSVMapsPlaceboPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "hevc_qsv", 2000000, "copy", 8, 4, "placebo")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-preset veryslow") {
		t.Fatalf("expected QSV placebo preset to map to veryslow, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_QSVSuppressesUnsupportedPreset(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_qsv", 2000000, "copy", 8, 4, "p4")
	joined := strings.Join(args, " ")

	if strings.Contains(joined, "-preset") {
		t.Fatalf("did not expect unsupported QSV preset to be passed through, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_Libx265AddsQuietParams(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx265", 2000000, "aac", 4, 0, "slow")

	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "-x265-params log-level=error") {
		t.Fatalf("expected x265 params for libx265 encoder, args: %v", args)
	}
}

func TestEncodeVideo_InterruptedByUser(t *testing.T) {
	tempDir := t.TempDir()

	fakeHangingFFmpeg(t, tempDir)

	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("failed to init session: %v", err)
	}
	defer func() {
		if err := CleanUp(); err != nil {
			t.Errorf("failed to cleanup session: %v", err)
		}
	}()

	video := &VideoSpecs{
		File: filepath.Join(tempDir, "input.mp4"),
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

	oldNotify := signalNotify
	oldStop := signalStop
	defer func() {
		signalNotify = oldNotify
		signalStop = oldStop
	}()

	registered := make(chan chan<- os.Signal, 1)
	signalNotify = func(c chan<- os.Signal, _ ...os.Signal) {
		registered <- c
	}
	signalStop = func(c chan<- os.Signal) {}

	errCh := make(chan error, 1)
	go func() {
		errCh <- EncodeVideo(nil, video, "libx264", 2000000, filepath.Join(tempDir, "output.mp4"), map[string]string{}, func(float64) {}, make(chan struct{}))
	}()

	var sigTarget chan<- os.Signal
	select {
	case sigTarget = <-registered:
	case <-time.After(3 * time.Second):
		t.Fatal("timeout waiting for signal registration")
	}

	select {
	case sigTarget <- os.Interrupt:
	case <-time.After(2 * time.Second):
		t.Fatal("timeout sending interrupt to encoder")
	}

	select {
	case err := <-errCh:
		if err == nil {
			t.Fatal("expected interruption error, got nil")
		}
		if !strings.Contains(err.Error(), "encoding interrupted by user") {
			t.Fatalf("unexpected error: %v", err)
		}
	case <-time.After(8 * time.Second):
		t.Fatal("EncodeVideo did not return after interruption")
	}
}

func TestEncodeVideo_ReturnsStdoutPipeError(t *testing.T) {
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

	expectedErr := errors.New("stdout pipe unavailable")
	oldStdoutPipe := commandStdoutPipe
	defer func() {
		commandStdoutPipe = oldStdoutPipe
	}()

	commandStdoutPipe = func(_ *exec.Cmd) (io.ReadCloser, error) {
		return nil, expectedErr
	}

	err := EncodeVideo(nil, video, "libx264", 2000000, "output.mp4", map[string]string{}, func(float64) {}, make(chan struct{}))
	if err == nil {
		t.Fatal("expected stdout pipe error, got nil")
	}
	if !errors.Is(err, expectedErr) {
		t.Fatalf("expected error %q, got %v", expectedErr, err)
	}
}

func TestFindEncoder_InvalidEncoder(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    5000000,
				DurationFloat: 60.5,
			},
		},
	}

	encoder, err := FindEncoder("nonexistent", ffmpegInfo, video)
	if err == nil {
		t.Errorf("FindEncoder with invalid encoder should fail")
	}

	if encoder != "" {
		t.Errorf("Expected empty encoder, got %s", encoder)
	}

	if _, ok := err.(*EncoderError); !ok {
		t.Errorf("Expected EncoderError, got %T", err)
	}
}

func TestFindEncoder_NoStreams(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265",
	}

	video := &VideoSpecs{
		File:    "test.mp4",
		Streams: []VideoStream{}, // Empty streams
	}

	encoder, err := FindEncoder("libx264", ffmpegInfo, video)
	if err == nil {
		t.Errorf("FindEncoder with no streams should fail")
	}

	if encoder != "" {
		t.Errorf("Expected empty encoder, got %s", encoder)
	}

	if _, ok := err.(*InvalidVideoError); !ok {
		t.Errorf("Expected InvalidVideoError, got %T", err)
	}
}

// ============================================================================
// Tests for MockHandler (for UIHandler interface)
// ============================================================================

// MockHandler implements UIHandler for testing
type MockHandler struct {
	ErrorCalls      []error
	InfoCalls       []string
	ProgressCalls   []float64
	ErrorToReturn   error
	BitrateDuration int
	EncoderValue    string
	SqueezeValue    bool
}

func (m *MockHandler) ShowError(err error) {
	m.ErrorCalls = append(m.ErrorCalls, err)
}

func (m *MockHandler) ShowInfo(msg string) {
	m.InfoCalls = append(m.InfoCalls, msg)
}

func (m *MockHandler) ShowProgress(percent float64) {
	m.ProgressCalls = append(m.ProgressCalls, percent)
}

func (m *MockHandler) GetBitrate() (int, error) {
	if m.ErrorToReturn != nil {
		return 0, m.ErrorToReturn
	}
	return m.BitrateDuration, nil
}

func (m *MockHandler) GetEncoder() string {
	return m.EncoderValue
}

func (m *MockHandler) GetSqueeze() bool {
	return m.SqueezeValue
}

func TestMockHandler(t *testing.T) {
	handler := &MockHandler{
		BitrateDuration: 5000000,
		EncoderValue:    "libx265",
		SqueezeValue:    true,
	}

	// Test ShowError
	testErr := &InvalidVideoError{Reason: "test error"}
	handler.ShowError(testErr)
	if len(handler.ErrorCalls) != 1 || handler.ErrorCalls[0] != testErr {
		t.Errorf("ShowError didn't record error correctly")
	}

	// Test ShowInfo
	handler.ShowInfo("test info")
	if len(handler.InfoCalls) != 1 || handler.InfoCalls[0] != "test info" {
		t.Errorf("ShowInfo didn't record message correctly")
	}

	// Test ShowProgress
	handler.ShowProgress(50.5)
	if len(handler.ProgressCalls) != 1 || handler.ProgressCalls[0] != 50.5 {
		t.Errorf("ShowProgress didn't record percentage correctly")
	}

	// Test GetBitrate
	bitrate, err := handler.GetBitrate()
	if err != nil || bitrate != 5000000 {
		t.Errorf("GetBitrate returned unexpected value: %d, %v", bitrate, err)
	}

	// Test GetEncoder
	if handler.GetEncoder() != "libx265" {
		t.Errorf("GetEncoder returned unexpected value: %s", handler.GetEncoder())
	}

	// Test GetSqueeze
	if !handler.GetSqueeze() {
		t.Errorf("GetSqueeze returned false, expected true")
	}
}

// ============================================================================
// Tests for Custom Error Types
// ============================================================================

func TestErrorTypes(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		expected string
	}{
		{
			name:     "InvalidVideoError",
			err:      &InvalidVideoError{Reason: "test reason"},
			expected: "invalid video: test reason",
		},
		{
			name:     "EncoderError",
			err:      &EncoderError{Msg: "test encoder error"},
			expected: "encoder error: test encoder error",
		},
		{
			name:     "SessionError",
			err:      &SessionError{Msg: "test session error"},
			expected: "session error: test session error",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.err.Error() != tt.expected {
				t.Errorf("Expected %q, got %q", tt.expected, tt.err.Error())
			}
		})
	}
}

// ============================================================================
// Integration-like tests (using mock handler)
// ============================================================================

func TestUIHandlerInterface_WithMock(t *testing.T) {
	handler := &MockHandler{
		BitrateDuration: 8000000,
		EncoderValue:    "libx264",
		SqueezeValue:    false,
	}

	// Simulate a typical workflow
	bitrate, _ := handler.GetBitrate()
	encoder := handler.GetEncoder()
	_ = handler.GetSqueeze() // Get squeeze value (would be used for GeneratePGM in real scenario)

	// Validate obtained values
	if err := ValidateBitrate(bitrate, 100000, 50000000); err != nil {
		t.Errorf("Bitrate validation failed: %v", err)
	}

	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265",
	}

	video := &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{
			{
				Codec:         "h264",
				Width:         1920,
				Height:        1080,
				BitrateInt:    4000000,
				DurationFloat: 60.5,
			},
		},
	}

	selectedEncoder, err := FindEncoder(encoder, ffmpegInfo, video)
	if err != nil {
		t.Errorf("FindEncoder failed: %v", err)
	}

	handler.ShowInfo("Starting encoding")
	if len(handler.InfoCalls) != 1 {
		t.Errorf("Expected 1 info call, got %d", len(handler.InfoCalls))
	}

	// Simulate progress updates
	handler.ShowProgress(25.0)
	handler.ShowProgress(50.0)
	handler.ShowProgress(100.0)

	if len(handler.ProgressCalls) != 3 {
		t.Errorf("Expected 3 progress calls, got %d", len(handler.ProgressCalls))
	}

	handler.ShowInfo("Encoding complete")
	if len(handler.InfoCalls) != 2 {
		t.Errorf("Expected 2 info calls, got %d", len(handler.InfoCalls))
	}

	if selectedEncoder != encoder {
		t.Errorf("Selected encoder mismatch: got %s, expected %s", selectedEncoder, encoder)
	}

	if err := video.Validate(); err != nil {
		t.Errorf("Video validation failed: %v", err)
	}
}

// errReader returns a non-EOF error on every Read. It reproduces the condition
// of a broken progress pipe (regression guard for the infinite read loop).
type errReader struct{ err error }

func (r *errReader) Read(_ []byte) (int, error) { return 0, r.err }
func (r *errReader) Close() error               { return nil }

func TestEncodeVideo_ProgressReaderNonEOFErrorDoesNotHang(t *testing.T) {
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

	oldStdoutPipe := commandStdoutPipe
	defer func() {
		commandStdoutPipe = oldStdoutPipe
	}()

	// Anything other than io.EOF used to make the reader goroutine spin
	// forever, so readDone was never closed and EncodeVideo blocked for good.
	commandStdoutPipe = func(_ *exec.Cmd) (io.ReadCloser, error) {
		return &errReader{err: errors.New("pipe closed unexpectedly")}, nil
	}

	done := make(chan error, 1)
	go func() {
		done <- EncodeVideo(nil, video, "libx264", 2000000, filepath.Join(t.TempDir(), "out.mp4"), map[string]string{}, func(float64) {}, make(chan struct{}))
	}()

	select {
	case <-done:
		// Returning at all is the point; ffmpeg itself fails on the fake input.
	case <-time.After(20 * time.Second):
		t.Fatal("EncodeVideo hung on a non-EOF progress read error")
	}
}

func TestParseFFmpegVersion(t *testing.T) {
	cases := []struct {
		name, input, want string
	}{
		{"standard", "ffmpeg version 6.1.1 Copyright (c) 2000-2023\nbuilt with gcc", "6.1.1"},
		{"distro build", "ffmpeg version n7.0.2-ubuntu1 Copyright", "n7.0.2-ubuntu1"},
		{"truncated", "ffmpeg version", "unknown"},
		{"single token", "ffmpeg", "unknown"},
		{"empty", "", "unknown"},
		{"only newlines", "\n\n", "unknown"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := parseFFmpegVersion(c.input); got != c.want {
				t.Errorf("parseFFmpegVersion(%q) = %q, want %q", c.input, got, c.want)
			}
		})
	}
}

func TestParseFFmpegEncoders(t *testing.T) {
	listing := `Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libx264              H.264 / AVC
 V....D libx265              H.265 / HEVC
 V....D h264_nvenc           NVIDIA NVENC H.264
 A....D aac                  AAC audio
 V....D libvpx-vp9           VP9 video
`
	got := parseFFmpegEncoders(listing, []string{"264", "265", "hevc"})
	want := []string{"libx264", "libx265", "h264_nvenc"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("got %v, want %v", got, want)
	}
}

func TestParseFFmpegEncoders_MalformedInputDoesNotPanic(t *testing.T) {
	// Each of these used to index past the end of a split slice.
	for _, input := range []string{
		"",
		" V",
		" V\n",
		"------\n V",
		"no separator at all\n V....D libx264 H.264",
		"\r\n------\r\n V....D libx265 HEVC\r\n",
	} {
		func() {
			defer func() {
				if r := recover(); r != nil {
					t.Errorf("panic on input %q: %v", input, r)
				}
			}()
			parseFFmpegEncoders(input, []string{"264", "265"})
		}()
	}
}

func TestParseFFmpegEncoders_NoSeparatorStillParses(t *testing.T) {
	// A build whose banner is not 10 lines long, and with no "------" marker.
	listing := " V....D libx264              H.264 / AVC\n V....D libx265              H.265\n"
	got := parseFFmpegEncoders(listing, []string{"264"})
	if len(got) != 1 || got[0] != "libx264" {
		t.Errorf("got %v, want [libx264]", got)
	}
}

func TestResolveToolBinary_DoesNotCacheFailures(t *testing.T) {
	ResetToolResolutionCache()
	defer ResetToolResolutionCache()

	const missing = "superview-definitely-not-a-real-tool"

	if got := resolveToolBinary(missing); got != missing {
		t.Fatalf("expected the bare name back, got %q", got)
	}
	// A failure must not be memoised: the user typically installs ffmpeg while
	// the app is running, and a cached miss would require a restart.
	if _, cached := toolResolveCache.Load(missing); cached {
		t.Error("a failed resolution must not be cached")
	}
}

func TestResolveToolBinary_CachesSuccess(t *testing.T) {
	ResetToolResolutionCache()
	defer ResetToolResolutionCache()

	if _, err := exec.LookPath("ffmpeg"); err != nil {
		skipWithoutFFmpeg(t, "ffmpeg not installed")
	}
	first := resolveToolBinary("ffmpeg")
	if !filepath.IsAbs(first) {
		t.Fatalf("expected an absolute path, got %q", first)
	}
	if _, cached := toolResolveCache.Load("ffmpeg"); !cached {
		t.Error("a successful resolution should be cached")
	}
	if second := resolveToolBinary("ffmpeg"); second != first {
		t.Errorf("cached lookup returned %q, want %q", second, first)
	}
}

func TestIsHighBitDepth(t *testing.T) {
	cases := []struct {
		pixFmt string
		want   bool
	}{
		{"yuv420p", false},
		{"yuvj420p", false},
		{"", false},
		// No endianness suffix means a single-byte sample, whatever digits the
		// name happens to contain.
		{"nv12", false},
		{"nv16", false},
		{"nv21", false},
		{"rgb24", false},
		{"gray", false},
		// The trap: yuv410p is a genuine 8-bit 4:1:0 format whose name contains
		// "10". A substring test would read it as 10-bit and ask an encoder for
		// a depth the source does not have.
		{"yuv410p", false},
		{"yuv411p", false},
		{"yuv420p10le", true},
		{"YUV420P10LE", true},
		{"  yuv422p10be  ", true},
		{"p010le", true},
		{"yuv420p12le", true},
		{"yuv420p9le", true},
		{"gray16be", true},
		{"p016le", true},
		// Packed formats carry a byte order but their digits are a channel
		// layout, not a depth.
		{"rgb565le", false},
		{"rgb444le", false},
	}

	for _, tc := range cases {
		if got := isHighBitDepth(tc.pixFmt); got != tc.want {
			t.Errorf("isHighBitDepth(%q) = %v, want %v", tc.pixFmt, got, tc.want)
		}
	}
}

func TestIsHEVCEncoder(t *testing.T) {
	for _, enc := range []string{"libx265", "hevc_nvenc", "hevc_qsv", "hevc_vaapi", "HEVC_AMF"} {
		if !isHEVCEncoder(enc) {
			t.Errorf("isHEVCEncoder(%q) = false, want true", enc)
		}
	}
	for _, enc := range []string{"libx264", "h264_nvenc", "h264_amf", ""} {
		if isHEVCEncoder(enc) {
			t.Errorf("isHEVCEncoder(%q) = true, want false", enc)
		}
	}
}

func TestRemapFilterChain(t *testing.T) {
	cases := []struct {
		name    string
		pixFmt  string
		encoder string
		want    string
	}{
		{
			name: "8-bit source stays 8-bit", pixFmt: "yuv420p", encoder: "libx265",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p[v]",
		},
		{
			name:   "10-bit source with an HEVC encoder keeps its depth",
			pixFmt: "yuv420p10le", encoder: "libx265",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p10le,format=yuv420p10le[v]",
		},
		{
			name:   "10-bit source with hardware HEVC keeps its depth",
			pixFmt: "yuv420p10le", encoder: "hevc_nvenc",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p10le,format=yuv420p10le[v]",
		},
		{
			// h264_nvenc cannot encode 10-bit at all; asking for it would turn a
			// working conversion into a hard failure.
			name:   "10-bit source with an H.264 encoder falls back to 8-bit",
			pixFmt: "yuv420p10le", encoder: "h264_nvenc",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p[v]",
		},
		{
			// Old ffprobe builds do not report pix_fmt; 8-bit is the safe read.
			name:   "unknown pixel format is treated as 8-bit",
			pixFmt: "", encoder: "libx265",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p[v]",
		},
		{
			// remap is a CPU filter, so the frames are in system memory whatever
			// the encoder. Vulkan cannot take them from there.
			name:   "a Vulkan encoder gets the frames uploaded",
			pixFmt: "yuv420p", encoder: "h264_vulkan",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p,format=nv12,hwupload[v]",
		},
		{
			name:   "a VAAPI encoder gets the frames uploaded",
			pixFmt: "yuv420p", encoder: "h264_vaapi",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p,format=nv12,hwupload[v]",
		},
		{
			// Hardware frame pools are semi-planar, so the upload format is not
			// the chain's output format: 10-bit goes up as p010, not yuv420p10le.
			name:   "a 10-bit upload goes through p010, not the planar format",
			pixFmt: "yuv420p10le", encoder: "hevc_vulkan",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p10le,format=yuv420p10le,format=p010,hwupload[v]",
		},
		{
			// The encoders that upload for themselves must not be given an
			// upload step: it would need a device the machine may not have.
			name:   "NVENC is left to upload for itself",
			pixFmt: "yuv420p", encoder: "h264_nvenc",
			want: "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p[v]",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := remapFilterChain(tc.pixFmt, tc.encoder); got != tc.want {
				t.Errorf("remapFilterChain(%q, %q) =\n  %s\nwant\n  %s", tc.pixFmt, tc.encoder, got, tc.want)
			}
		})
	}
}

// TestBuildEncodeBaseArgs_DeviceOptionsComeBeforeTheInput pins where the
// device setup goes.
//
// -init_hw_device is a global option. After -i it configures nothing the filter
// graph can use, and the upload the chain performs then fails looking for a
// device that was never created -- a failure that names the filter, not the
// misplaced option.
func TestBuildEncodeBaseArgs_DeviceOptionsComeBeforeTheInput(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}

	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_vulkan", 2000000, "aac", 6, 0, "")

	deviceAt, inputAt := -1, -1
	for i, arg := range args {
		if arg == "-init_hw_device" && deviceAt < 0 {
			deviceAt = i
		}
		if arg == "-i" && inputAt < 0 {
			inputAt = i
		}
	}

	if deviceAt < 0 {
		t.Fatalf("a Vulkan encode carries no -init_hw_device: %v", args)
	}
	if inputAt < 0 {
		t.Fatalf("no -i in the arguments at all: %v", args)
	}
	if deviceAt > inputAt {
		t.Errorf("-init_hw_device is at %d, after the input at %d: it would configure nothing", deviceAt, inputAt)
	}

	// And nothing of the sort for an encoder that uploads for itself.
	cpu := strings.Join(buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx264", 2000000, "aac", 6, 0, ""), " ")
	if strings.Contains(cpu, "-init_hw_device") {
		t.Errorf("a CPU encode must not create a hardware device: %s", cpu)
	}
}

func TestBuildEncodeBaseArgs_MapsStreamsAndMetadata(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4", Streams: []VideoStream{{
		Codec: "hevc", Width: 1440, Height: 1080, PixFmt: "yuv420p10le",
		Duration: "1", DurationFloat: 1, Bitrate: "2000000", BitrateInt: 2000000,
	}}}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx265", 2000000, "aac", 6, 0, "")

	joined := strings.Join(args, " ")

	// Without these three, ffmpeg keeps one audio track and no global metadata.
	if !strings.Contains(joined, "-map [v] -map 0:a?") {
		t.Errorf("expected explicit stream maps, args: %v", args)
	}
	if !strings.Contains(joined, "-map_metadata 0") {
		t.Errorf("expected -map_metadata 0 so the recording date survives, args: %v", args)
	}
	// The "?" is what makes a clip with no audio work rather than fail.
	if strings.Contains(joined, "-map 0:a ") {
		t.Errorf("the audio map must be optional (0:a?), args: %v", args)
	}
	if !strings.Contains(joined, "format=yuv420p10le[v]") {
		t.Errorf("expected the 10-bit chain for a 10-bit source on libx265, args: %v", args)
	}
}

func TestBuildEncodeBaseArgs_NoStreamsFallsBackToEightBit(t *testing.T) {
	// sourcePixelFormat must not panic on a VideoSpecs without streams.
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx265", 2000000, "aac", 6, 0, "")

	if joined := strings.Join(args, " "); !strings.Contains(joined, "format=yuv444p,format=yuv420p[v]") {
		t.Errorf("expected the 8-bit chain when the pixel format is unknown, args: %v", args)
	}
}

func TestClampEncoderThreads(t *testing.T) {
	// libx265 maps -threads onto --frame-threads and refuses more than 16,
	// failing to open the encoder at all. runtime.NumCPU() routinely exceeds
	// that, so without the clamp every H.265 encode died on a big machine.
	if got := clampEncoderThreads("libx265", 24); got != x265MaxFrameThreads {
		t.Errorf("clampEncoderThreads(libx265, 24) = %d, want %d", got, x265MaxFrameThreads)
	}
	if got := clampEncoderThreads("libx265", 8); got != 8 {
		t.Errorf("clampEncoderThreads(libx265, 8) = %d, want 8 (below the cap, leave it alone)", got)
	}
	// Everything else clamps internally instead of failing; do not second-guess it.
	if got := clampEncoderThreads("libx264", 24); got != 24 {
		t.Errorf("clampEncoderThreads(libx264, 24) = %d, want 24", got)
	}
	if got := clampEncoderThreads("hevc_nvenc", 24); got != 24 {
		t.Errorf("clampEncoderThreads(hevc_nvenc, 24) = %d, want 24", got)
	}
}

func TestBuildEncodeBaseArgs_Libx265ThreadsAreCapped(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx265", 2000000, "aac", 32, 0, "")

	if joined := strings.Join(args, " "); !strings.Contains(joined, "-threads 16") {
		t.Errorf("expected libx265 threads capped at 16, args: %v", args)
	}
}

// fakeHangingFFmpeg installs a stand-in ffmpeg on PATH that reports progress
// and then runs until it is killed.
//
// Windows gets a .bat, because LookPath honours PATHEXT and so still resolves
// the bare name "ffmpeg" to it. This used to skip on Windows instead, which
// left every test built on the helper running on Linux only -- on a platform
// that has a published build and, since the runner started installing ffmpeg,
// nothing left stopping it. TestEncodeVideo_InterruptedByUser had been using
// the same .bat successfully all along.
func fakeHangingFFmpeg(t *testing.T, dir string) {
	t.Helper()

	name, mode := "ffmpeg", os.FileMode(0o755)
	script := "#!/bin/sh\n" +
		"while true; do\n" +
		"  echo out_time_ms=1000\n" +
		"  sleep 0.2\n" +
		"done\n"
	if runtime.GOOS == "windows" {
		// cmd.exe has no sleep builtin; pinging the loopback twice is the
		// customary stand-in for a short pause in a .bat.
		name, mode = "ffmpeg.bat", os.FileMode(0o644)
		script = "@echo off\r\n" +
			":loop\r\n" +
			"echo out_time_ms=1000\r\n" +
			"ping -n 2 127.0.0.1 >nul\r\n" +
			"goto loop\r\n"
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), mode); err != nil {
		t.Fatalf("failed to create the stand-in ffmpeg: %v", err)
	}

	// t.Setenv rather than os.Setenv: it restores PATH even when the test
	// panics, and it makes the framework reject a t.Parallel() here instead of
	// letting one test rewrite another's PATH mid-run.
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Cleanup(func() { toolResolveCache.Delete("ffmpeg") })
	toolResolveCache.Delete("ffmpeg")
}

// countFFmpegLaunches counts how many times EncodeVideo starts an ffmpeg
// process, by hooking the stdout-pipe wrapper it calls once per launch.
//
// Counting from inside the stand-in process instead looks simpler and does not
// work: once the cancel channel is closed, each subsequent process is killed
// within microseconds of Start, usually before the shell has run a single line.
// The count would then read 1 whether the cascade stopped or not -- a test that
// passes for the wrong reason.
func countFFmpegLaunches(t *testing.T) *atomic.Int32 {
	t.Helper()

	var launches atomic.Int32
	previous := commandStdoutPipe
	commandStdoutPipe = func(cmd *exec.Cmd) (io.ReadCloser, error) {
		launches.Add(1)
		return previous(cmd)
	}
	t.Cleanup(func() { commandStdoutPipe = previous })
	return &launches
}

// TestEncodeVideo_CancellationStopsTheFallbackCascade pins P-03.
//
// EncodeVideo retries a failed encode on safer paths: CPU decode, then the
// equivalent CPU encoder. It decides by looking at the error, and a
// cancellation used to be indistinguishable from an encoder failure -- so
// asking to stop launched the whole cascade instead of stopping it. With a
// hardware encoder and a matching decode path available, that is three ffmpeg
// processes started after the user pressed Cancel. It must be one.
func TestEncodeVideo_CancellationStopsTheFallbackCascade(t *testing.T) {
	tempDir := t.TempDir()
	fakeHangingFFmpeg(t, tempDir)
	launches := countFFmpegLaunches(t)

	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("InitEncodingSession: %v", err)
	}
	defer func() { _ = CleanUp() }()

	video := &VideoSpecs{File: filepath.Join(tempDir, "input.mp4"), Streams: []VideoStream{{
		Codec: "h264", Width: 1920, Height: 1080,
		Duration: "60", DurationFloat: 60, Bitrate: "5000000", BitrateInt: 5000000,
	}}}

	// A hardware encoder with a matching decode accel: this is what opens the
	// three-level cascade in the first place.
	ffmpeg := map[string]string{"accels": "cuda", "encoders": "h264_nvenc,libx264"}

	// Cancel from the progress callback rather than up front, so the first
	// ffmpeg is provably running when the request lands.
	cancel := make(chan struct{})
	var cancelOnce sync.Once
	progress := func(float64) {
		cancelOnce.Do(func() { close(cancel) })
	}

	err := EncodeVideo(nil, video, "h264_nvenc", 2000000,
		filepath.Join(tempDir, "output.mp4"), ffmpeg, progress, cancel)

	if !errors.Is(err, ErrCancelled) {
		t.Fatalf("error = %v, want ErrCancelled", err)
	}
	if got := launches.Load(); got != 1 {
		t.Errorf("ffmpeg was launched %d time(s), want 1: the fallback cascade did not stop on cancellation", got)
	}
}

func TestPerformEncoding_RefusesOutputEqualToInput(t *testing.T) {
	dir := t.TempDir()
	clip := filepath.Join(dir, "clip.mp4")
	if err := os.WriteFile(clip, []byte("pretend this is a video"), 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	handler := &MockHandler{}
	err := PerformEncoding(nil, clip, clip, handler, map[string]string{}, make(chan struct{}))

	if err == nil {
		t.Fatal("expected an error when the output is the input, got nil")
	}
	if !strings.Contains(err.Error(), "input file") {
		t.Errorf("error should name the problem in plain words, got: %v", err)
	}

	// The guard exists because the encode now goes through a working file:
	// ffmpeg no longer sees a conflict, so nothing else would stop the final
	// rename from destroying the source.
	data, readErr := os.ReadFile(clip)
	if readErr != nil {
		t.Fatalf("the source file is gone: %v", readErr)
	}
	if string(data) != "pretend this is a video" {
		t.Error("the source file was modified")
	}
}

func TestParseFrameRate(t *testing.T) {
	cases := []struct {
		in   string
		want float64
	}{
		{"30/1", 30},
		{"60/1", 60},
		{"25/1", 25},
		{" 120/1 ", 120},
		{"30", 30},
		// NTSC rates are the reason this is a rational and not a number.
		{"30000/1001", 30000.0 / 1001.0},
		{"24000/1001", 24000.0 / 1001.0},
		// ffprobe reports 0/0 when it cannot determine the rate; every caller
		// reads 0 as "unknown" and none of them substitutes a default.
		{"0/0", 0},
		{"0/1", 0},
		{"30/0", 0},
		{"", 0},
		{"N/A", 0},
		{"abc/def", 0},
	}

	for _, tc := range cases {
		got := parseFrameRate(tc.in)
		if (tc.want == 0 && got != 0) || (tc.want != 0 && (got < tc.want-0.001 || got > tc.want+0.001)) {
			t.Errorf("parseFrameRate(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}

func TestBuildEncodeBaseArgs_ConstrainsTheBitrateCeiling(t *testing.T) {
	// -b:v alone is an average with no ceiling: measured +83% overshoot on
	// incompressible content. The VBV constraint brings it to +24%.
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "libx264", 8_000_000, "aac", 6, 0, "")

	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "-b:v 8000000") {
		t.Errorf("expected the requested bitrate, args: %v", args)
	}
	if !strings.Contains(joined, "-maxrate 8000000") {
		t.Errorf("expected -maxrate to cap the bitrate, args: %v", args)
	}
	if !strings.Contains(joined, "-bufsize 16000000") {
		t.Errorf("expected -bufsize at twice the target, args: %v", args)
	}
}

func TestEncoderThreadArgs(t *testing.T) {
	// Software encoders get the value, capped where the encoder needs it.
	if got := strings.Join(encoderThreadArgs("libx264", 24), " "); got != "-threads 24" {
		t.Errorf("libx264 thread args = %q, want \"-threads 24\"", got)
	}
	if got := strings.Join(encoderThreadArgs("libx265", 24), " "); got != "-threads 16" {
		t.Errorf("libx265 thread args = %q, want \"-threads 16\" (its ceiling)", got)
	}

	// Hardware encoders run on a dedicated block; the host core count says
	// nothing about them, so the option is simply not emitted.
	for _, enc := range []string{"h264_nvenc", "hevc_nvenc", "h264_amf", "hevc_qsv", "h264_vaapi"} {
		if args := encoderThreadArgs(enc, 24); len(args) != 0 {
			t.Errorf("%s should get no -threads option, got %v", enc, args)
		}
	}
}

func TestBuildEncodeBaseArgs_NoThreadsForHardwareEncoders(t *testing.T) {
	video := &VideoSpecs{File: "input.mp4"}
	args := buildEncodeBaseArgs(video, "x.pgm", "y.pgm", "h264_nvenc", 2000000, "aac", 24, 0, "")

	if joined := strings.Join(args, " "); strings.Contains(joined, "-threads") {
		t.Errorf("did not expect -threads for a hardware encoder, args: %v", args)
	}
}
