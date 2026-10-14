package common

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"testing"
)

// TestCheckGeometry pins E-05: widening folds any frame taller than 9:8, so
// such a frame is refused in widening mode and accepted in squeeze mode, which
// keeps the source width.
func TestCheckGeometry(t *testing.T) {
	for _, tc := range []struct {
		width, height int
		squeeze       bool
		refused       bool
	}{
		{640, 480, false, false},
		{1440, 1080, false, false},
		{1000, 1000, false, false}, // ratio 1, below 9:8
		{800, 900, false, false},   // exactly 9:8, the limit itself
		{2560, 1080, false, false}, // wider than 16:9
		{1080, 1920, false, true},
		{1200, 1600, false, true},
		{1080, 1920, true, false},
		{1200, 1600, true, false},
	} {
		t.Run(fmt.Sprintf("%dx%d_squeeze=%v", tc.width, tc.height, tc.squeeze), func(t *testing.T) {
			video := &VideoSpecs{File: "geometry.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
			}}}
			err := checkGeometry(video, tc.squeeze)
			if !tc.refused {
				if err != nil {
					t.Fatalf("refused a geometry the map handles: %v", err)
				}
				return
			}
			var invalid *InvalidVideoError
			if !errors.As(err, &invalid) {
				t.Fatalf("checkGeometry = %v, want an *InvalidVideoError", err)
			}
			dims := fmt.Sprintf("%dx%d", tc.width, tc.height)
			if !strings.Contains(invalid.Reason, dims) || !strings.Contains(invalid.Reason, "9:8") ||
				!strings.Contains(invalid.Reason, "fold") {
				t.Errorf("reason %q should name %s, the 9:8 limit and the fold", invalid.Reason, dims)
			}
			// GeneratePGM must refuse it too, before writing anything. A session
			// is open, so the check is the only thing that can stop it.
			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()
			if genErr := GeneratePGM(video, tc.squeeze); !errors.As(genErr, &invalid) {
				t.Errorf("GeneratePGM wrote a map for %s: %v", dims, genErr)
			}
		})
	}
}

// readMapSamples returns the declared geometry of a P5 map and a sampler over
// its pixel data.
func readMapSamples(t *testing.T, path string) (width, height int, sample func(x, y int) int) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	_, body, width, height := parsePGMP5(t, data)
	return width, height, func(x, y int) int {
		i := (y*width + x) * 2
		return int(body[i])<<8 | int(body[i+1])
	}
}

// TestGeneratePGM_WidenMapStaysWellFormed is the widening twin of
// TestGeneratePGM_SqueezeMapStaysWellFormed. The comment on putMapSample used
// to claim every input size landed inside the source row; a 1080x1920 frame
// sampled up to column 1164 and folded at both edges (E-05). For every
// geometry checkGeometry accepts, the curve must stay inside the row, never
// step backwards, start at 0, end at the last column and leave the centre in
// place.
func TestGeneratePGM_WidenMapStaysWellFormed(t *testing.T) {
	for _, tc := range []struct{ width, height int }{
		{640, 480}, {641, 481}, {1440, 1080}, {1000, 1000}, {800, 900}, {2560, 1080},
	} {
		t.Run(fmt.Sprintf("%dx%d", tc.width, tc.height), func(t *testing.T) {
			video := &VideoSpecs{File: "widen.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, false); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			xPath, _, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}
			outX, _, sample := readMapSamples(t, xPath)
			at := func(x int) int { return sample(x, 0) }

			for x := 0; x < outX; x++ {
				if v := at(x); v > tc.width-1 {
					t.Fatalf("x=%d samples column %d, outside a %d-pixel row: remap fills it black", x, v, tc.width)
				}
				if x > 0 && at(x) < at(x-1) {
					t.Fatalf("map goes backwards at x=%d (%d after %d): the image would fold", x, at(x), at(x-1))
				}
			}
			if got := at(0); got != 0 {
				t.Errorf("left edge samples %d, want 0", got)
			}
			if got := at(outX - 1); got < tc.width-2 {
				t.Errorf("right edge samples %d, want %d (within 1)", got, tc.width-1)
			}
			if got, want := at(outX/2), tc.width/2; got < want-1 || got > want+1 {
				t.Errorf("centre samples %d, want %d (within 1): the middle of the frame must stay put", got, want)
			}
		})
	}
}

// TestGeneratePGM_YMapIsIdentity pins the other half of the remap: the
// distortion is horizontal only, so row y of the Y map holds y everywhere, in
// both modes.
func TestGeneratePGM_YMapIsIdentity(t *testing.T) {
	for _, squeeze := range []bool{false, true} {
		t.Run(fmt.Sprintf("squeeze=%v", squeeze), func(t *testing.T) {
			video := &VideoSpecs{File: "rows.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: 641, Height: 481,
				Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
			}}}
			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, squeeze); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			_, yPath, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}
			width, height, sample := readMapSamples(t, yPath)
			if height != 481 {
				t.Fatalf("Y map has %d rows, want 481", height)
			}
			for y := 0; y < height; y++ {
				for x := 0; x < width; x++ {
					if got := sample(x, y); got != y {
						t.Fatalf("Y map at (%d,%d) = %d, want %d", x, y, got, y)
					}
				}
			}
		})
	}
}
