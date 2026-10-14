package common

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// readFixture returns a recorded ffprobe response from common/testdata/ffprobe.
func readFixture(t *testing.T, name string) []byte {
	t.Helper()

	out, err := os.ReadFile(filepath.Join("testdata", "ffprobe", name))
	if err != nil {
		t.Fatalf("reading fixture %s: %v", name, err)
	}
	return out
}

// TestParseVideoSpecs_Accepts covers what a usable ffprobe response looks like,
// including the two shapes that must not be rejected: a source recorded at a
// non-integer frame rate, and one from an ffprobe old enough to report neither
// pix_fmt nor r_frame_rate.
func TestParseVideoSpecs_Accepts(t *testing.T) {
	cases := []struct {
		name      string
		fixture   string
		codec     string
		width     int
		height    int
		duration  float64
		bitrate   int
		pixFmt    string
		frameRate float64
	}{
		{
			name:      "h264 4:3, the ordinary case",
			fixture:   "valid-h264-4x3.json",
			codec:     "h264",
			width:     1440,
			height:    1080,
			duration:  1.0,
			bitrate:   192648,
			pixFmt:    "yuv420p",
			frameRate: 30,
		},
		{
			// HERO 10 and later record 10-bit, and the filter chain has to know
			// so the output does not silently drop to 8.
			name:      "hevc 10-bit at 29.97",
			fixture:   "valid-hevc-10bit.json",
			codec:     "hevc",
			width:     4000,
			height:    3000,
			duration:  12.5125,
			bitrate:   78451200,
			pixFmt:    "yuv420p10le",
			frameRate: 30000.0 / 1001.0,
		},
		{
			// A missing frame rate costs the encoding-speed metric and nothing
			// else. Rejecting the file over it would be a regression.
			name:      "no pix_fmt and no r_frame_rate",
			fixture:   "frame-rate-and-pixfmt-absent.json",
			codec:     "h264",
			width:     1440,
			height:    1080,
			duration:  8.4,
			bitrate:   192648,
			pixFmt:    "",
			frameRate: 0,
		},
		{
			name:      "r_frame_rate with a zero denominator",
			fixture:   "frame-rate-zero-denominator.json",
			codec:     "h264",
			width:     1440,
			height:    1080,
			duration:  8.4,
			bitrate:   192648,
			pixFmt:    "yuv420p",
			frameRate: 0,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			specs, err := parseVideoSpecs("input.mp4", readFixture(t, tc.fixture))
			if err != nil {
				t.Fatalf("parseVideoSpecs: %v", err)
			}

			if specs.File != "input.mp4" {
				t.Errorf("File = %q, want input.mp4", specs.File)
			}
			if len(specs.Streams) != 1 {
				t.Fatalf("got %d streams, want 1", len(specs.Streams))
			}

			s := specs.Streams[0]
			if s.Codec != tc.codec {
				t.Errorf("Codec = %q, want %q", s.Codec, tc.codec)
			}
			if s.Width != tc.width || s.Height != tc.height {
				t.Errorf("dimensions = %dx%d, want %dx%d", s.Width, s.Height, tc.width, tc.height)
			}
			if s.DurationFloat != tc.duration {
				t.Errorf("DurationFloat = %v, want %v", s.DurationFloat, tc.duration)
			}
			if s.BitrateInt != tc.bitrate {
				t.Errorf("BitrateInt = %d, want %d", s.BitrateInt, tc.bitrate)
			}
			if s.PixFmt != tc.pixFmt {
				t.Errorf("PixFmt = %q, want %q", s.PixFmt, tc.pixFmt)
			}
			// Frame rate is derived, so compare with a tolerance rather than
			// pinning 29.970029970029973.
			if diff := s.FrameRate - tc.frameRate; diff > 1e-9 || diff < -1e-9 {
				t.Errorf("FrameRate = %v, want %v", s.FrameRate, tc.frameRate)
			}
		})
	}
}

// TestParseVideoSpecs_Rejects walks every path that refuses a file.
//
// Each of these was previously reachable only by finding a real video that
// provoked it. The point of asserting the message is that this text reaches the
// user in a dialog: "invalid bitrate value 'N/A'" says what to look at,
// "failed to parse video metadata" alone does not.
func TestParseVideoSpecs_Rejects(t *testing.T) {
	cases := []struct {
		name    string
		fixture string
		// wantInvalidVideo distinguishes a file we understand and refuse from
		// output we could not read at all.
		wantInvalidVideo bool
		wantMessage      string
	}{
		{
			name:        "ffprobe did not return JSON",
			fixture:     "not-json.txt",
			wantMessage: "failed to parse video metadata",
		},
		{
			name:        "JSON cut short",
			fixture:     "truncated.json",
			wantMessage: "failed to parse video metadata",
		},
		{
			name:             "no video stream in the file",
			fixture:          "no-streams.json",
			wantInvalidVideo: true,
			wantMessage:      "no video streams in file",
		},
		{
			name:        "duration is N/A",
			fixture:     "duration-not-numeric.json",
			wantMessage: "invalid duration value 'N/A'",
		},
		{
			name:             "bit_rate absent from the stream",
			fixture:          "bitrate-absent.json",
			wantInvalidVideo: true,
			wantMessage:      "bitrate information not available",
		},
		{
			name:        "bit_rate is N/A",
			fixture:     "bitrate-not-numeric.json",
			wantMessage: "invalid bitrate value 'N/A'",
		},
		{
			// Caught by VideoSpecs.Validate, after parsing succeeds.
			name:             "zero dimensions",
			fixture:          "dimensions-zero.json",
			wantInvalidVideo: true,
			wantMessage:      "invalid dimensions: 0x0",
		},
		{
			name:             "codec absent",
			fixture:          "codec-absent.json",
			wantInvalidVideo: true,
			wantMessage:      "no codec information",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			specs, err := parseVideoSpecs("input.mp4", readFixture(t, tc.fixture))
			if err == nil {
				t.Fatalf("parseVideoSpecs accepted %s and returned %+v", tc.fixture, specs)
			}
			if specs != nil {
				t.Errorf("specs = %+v on error, want nil", specs)
			}

			var invalid *InvalidVideoError
			if got := errors.As(err, &invalid); got != tc.wantInvalidVideo {
				t.Errorf("errors.As(*InvalidVideoError) = %v, want %v (err: %v)",
					got, tc.wantInvalidVideo, err)
			}

			if !strings.Contains(err.Error(), tc.wantMessage) {
				t.Errorf("error = %q, want it to mention %q", err, tc.wantMessage)
			}
		})
	}
}
