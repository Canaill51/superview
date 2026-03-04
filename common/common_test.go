package common

import (
	"testing"
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
			bitrate: 5000000,  // 5M bytes/sec
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
