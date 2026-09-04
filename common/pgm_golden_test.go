package common

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// TestGeneratePGM_Golden pins the exact bytes of the remap maps.
//
// The two squeeze hashes were re-recorded on 2026-09-04 when the integer
// divisions in the squeeze offset were corrected to floating point. The
// non-squeeze hashes did not move, which is the expected blast radius: only the
// squeeze branch had the defect. See TestGeneratePGM_SqueezeMapIsContinuous for
// what the change actually fixed -- these hashes record that the bytes changed,
// not why the new ones are right.
//
// The distortion math comes from Banelle's reference Python implementation
// (https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview);
// its integer divisions and float truncations are part of the algorithm, not
// accidents. Any change to GeneratePGM that alters a single output byte changes
// what users actually see, so it must be a deliberate, reviewed decision --
// this test makes that impossible to do by accident.
//
// Hashes were first recorded 2026-09-04 against the implementation that shipped
// in e3269e7, certifying the buffering/hoisting optimisation as
// behaviour-preserving. They were re-recorded when the maps moved from PGM P2
// (ASCII) to PGM P5 (binary): that change rewrites every byte of these files by
// construction, so this test cannot certify it. What certifies it is
// TestGeneratePGM_RemapOutputIsStable, which pins the decoded frames FFmpeg
// produces from these maps -- those fingerprints did not move.
func TestGeneratePGM_Golden(t *testing.T) {
	cases := []struct {
		name          string
		width, height int
		squeeze       bool
		xSize, ySize  int
		xHash, yHash  string
	}{
		{
			name: "1440x1080_noSqueeze", width: 1440, height: 1080, squeeze: false,
			// Both maps are now the same size: P5 stores a fixed two bytes per
			// sample, so the file size no longer depends on how many digits
			// each coordinate happens to need.
			xSize: 4147219, ySize: 4147219,
			xHash: "2227c17119b499db7ce11bc27376e423ecccbfe24bf4a9f6c49363312261202f",
			yHash: "87348bfb3fbb4a513dfc5f02293445c09e940ab663f1befb033c8dc383b9e6fa",
		},
		{
			name: "1440x1080_squeeze", width: 1440, height: 1080, squeeze: true,
			xSize: 3110419, ySize: 3110419,
			xHash: "f9c97781445026e2d306e05b521df416e77bca843e66b04ea3c60a44e31c6421",
			yHash: "38cb195cc83f764cab65cd5c3db87de8a4a66b16bdaf2e68b55b994ef807a33e",
		},
		{
			// Odd dimensions exercise the integer-division rounding.
			name: "641x481_noSqueeze", width: 641, height: 481, squeeze: false,
			xSize: 821565, ySize: 821565,
			xHash: "e500fe0ca537005f4d9a9c78b2a25492cc7e5bb0592dd0d7f4df856166897a8e",
			yHash: "aeddcf2ebad943c66d267d387f77eb63ab49491a7945d473a6e0e1f881656cfa",
		},
		{
			name: "641x481_squeeze", width: 641, height: 481, squeeze: true,
			xSize: 616659, ySize: 616659,
			xHash: "a10aa7c6ead73b685b651cb4e04d338ae3ebda4e2e6144b533705667de5f1e46",
			yHash: "aace303c7eb5336eba6bdec932e263f3610a377731e01cdbdffde248380abb72",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			video := &VideoSpecs{File: "golden.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "60", DurationFloat: 60,
				Bitrate: "5000000", BitrateInt: 5000000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() {
				if err := CleanUp(); err != nil {
					t.Errorf("CleanUp: %v", err)
				}
			}()

			if err := GeneratePGM(video, tc.squeeze); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}

			xPath, yPath, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}

			for _, f := range []struct {
				label, path, wantHash string
				wantSize              int
			}{
				{"x.pgm", xPath, tc.xHash, tc.xSize},
				{"y.pgm", yPath, tc.yHash, tc.ySize},
			} {
				data, err := os.ReadFile(f.path)
				if err != nil {
					t.Fatalf("read %s: %v", f.label, err)
				}
				if len(data) != f.wantSize {
					t.Errorf("%s size = %d bytes, want %d", f.label, len(data), f.wantSize)
				}
				sum := sha256.Sum256(data)
				if got := hex.EncodeToString(sum[:]); got != f.wantHash {
					t.Errorf("%s content changed:\n  got  %s\n  want %s", f.label, got, f.wantHash)
				}
			}
		})
	}
}

// TestGeneratePGM_XMapIsRowInvariant documents the property the generation
// loop relies on: every row of x.pgm is identical, which is why it is built
// once and written outY times.
func TestGeneratePGM_XMapIsRowInvariant(t *testing.T) {
	video := &VideoSpecs{File: "rows.mp4", Streams: []VideoStream{{
		Codec: "h264", Width: 160, Height: 120,
		Duration: "1", DurationFloat: 1,
		Bitrate: "100000", BitrateInt: 100000,
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
	data, err := os.ReadFile(xPath)
	if err != nil {
		t.Fatalf("read x.pgm: %v", err)
	}

	header, body, width, height := parsePGMP5(t, data)
	if width == 0 || height < 2 {
		t.Fatalf("unexpected map geometry %dx%d (header %q)", width, height, header)
	}

	rowBytes := width * 2
	first := body[:rowBytes]
	for y := 1; y < height; y++ {
		if !bytes.Equal(body[y*rowBytes:(y+1)*rowBytes], first) {
			t.Fatalf("row %d differs from row 0; the X map is not row-invariant", y)
		}
	}
}

// TestGeneratePGM_MapsAreBigEndianP5 pins the wire format itself. The PGM
// specification mandates big-endian samples for a maxval above 255; a
// little-endian writer would produce maps FFmpeg accepts and misreads, so this
// has to be asserted explicitly rather than left to the golden hashes.
func TestGeneratePGM_MapsAreBigEndianP5(t *testing.T) {
	video := &VideoSpecs{File: "endian.mp4", Streams: []VideoStream{{
		Codec: "h264", Width: 640, Height: 480,
		Duration: "1", DurationFloat: 1,
		Bitrate: "100000", BitrateInt: 100000,
	}}}

	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("InitEncodingSession: %v", err)
	}
	defer func() { _ = CleanUp() }()

	if err := GeneratePGM(video, false); err != nil {
		t.Fatalf("GeneratePGM: %v", err)
	}
	_, yPath, err := getSessionPaths()
	if err != nil {
		t.Fatalf("getSessionPaths: %v", err)
	}
	data, err := os.ReadFile(yPath)
	if err != nil {
		t.Fatalf("read y.pgm: %v", err)
	}

	header, body, width, height := parsePGMP5(t, data)
	if !strings.HasPrefix(header, "P5 ") {
		t.Errorf("header = %q, want a P5 (binary) magic number", header)
	}

	// The Y map holds the row index, so row 256 is the first one whose value
	// does not fit in a single byte: big-endian stores it as 0x01 0x00.
	if height <= 256 {
		t.Fatalf("need more than 256 rows to test the byte order, got %d", height)
	}
	sample := body[256*width*2 : 256*width*2+2]
	if sample[0] != 0x01 || sample[1] != 0x00 {
		t.Errorf("row 256 encoded as %#v, want [0x01 0x00] (big-endian 256)", sample)
	}
}

// TestGeneratePGM_RemapOutputIsStable is what actually certifies the move from
// PGM P2 (ASCII) to PGM P5 (binary).
//
// The golden hashes cannot: switching container format rewrites every byte of
// the map files by construction. What must not change is what FFmpeg *does*
// with them. So this transcodes the generated binary maps back to the ASCII
// form -- same coordinates, different encoding -- runs the real remap filter
// with each, and requires the decoded frames to be identical.
//
// Deliberately self-referential: it compares two runs of the FFmpeg that is
// present, instead of pinning a hash that would depend on the encoder build and
// break on every runner.
func TestGeneratePGM_RemapOutputIsStable(t *testing.T) {
	if _, err := exec.LookPath("ffmpeg"); err != nil {
		t.Skip("ffmpeg not installed; skipping remap equivalence test")
	}

	cases := []struct {
		name          string
		width, height int
		squeeze       bool
	}{
		{"320x240_noSqueeze", 320, 240, false},
		{"320x240_squeeze", 320, 240, true},
		{"642x482_noSqueeze", 642, 482, false},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			clip := makeTestClip(t, tc.width, tc.height, 1)

			video := &VideoSpecs{File: clip, Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "1", DurationFloat: 1,
				Bitrate: "2000000", BitrateInt: 2000000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, tc.squeeze); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			xPath, yPath, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}

			dir := t.TempDir()
			asciiX := transcodePGMP5ToP2(t, xPath, filepath.Join(dir, "x_ascii.pgm"))
			asciiY := transcodePGMP5ToP2(t, yPath, filepath.Join(dir, "y_ascii.pgm"))

			fromBinary := remapFingerprint(t, clip, xPath, yPath)
			fromASCII := remapFingerprint(t, clip, asciiX, asciiY)

			if fromBinary != fromASCII {
				t.Errorf("the binary maps do not remap like the ASCII ones:\n  P5 %s\n  P2 %s",
					fromBinary, fromASCII)
			}
		})
	}
}

// parsePGMP5 splits a binary PGM into its header line and pixel data, and
// returns the declared geometry.
func parsePGMP5(t *testing.T, data []byte) (header string, body []byte, width, height int) {
	t.Helper()

	nl := bytes.IndexByte(data, '\n')
	if nl < 0 {
		t.Fatalf("no header line found in map file")
	}
	header = string(data[:nl])
	body = data[nl+1:]

	var magic string
	var maxval int
	if _, err := fmt.Sscanf(header, "%s %d %d %d", &magic, &width, &height, &maxval); err != nil {
		t.Fatalf("cannot parse PGM header %q: %v", header, err)
	}
	if maxval != 65535 {
		t.Fatalf("maxval = %d, want 65535 (16-bit samples)", maxval)
	}
	if want := width * height * 2; len(body) != want {
		t.Fatalf("pixel data = %d bytes, want %d for a %dx%d 16-bit map", len(body), want, width, height)
	}
	return header, body, width, height
}

// transcodePGMP5ToP2 rewrites a binary PGM as the ASCII form, preserving every
// sample value. It is the reference encoding the maps used before, so a remap
// driven by the result must produce exactly what the binary map produces.
func transcodePGMP5ToP2(t *testing.T, src, dst string) string {
	t.Helper()

	data, err := os.ReadFile(src)
	if err != nil {
		t.Fatalf("read %s: %v", src, err)
	}
	_, body, width, height := parsePGMP5(t, data)

	var buf bytes.Buffer
	fmt.Fprintf(&buf, "P2 %d %d 65535\n", width, height)
	for y := 0; y < height; y++ {
		row := body[y*width*2 : (y+1)*width*2]
		for x := 0; x < width; x++ {
			buf.WriteString(strconv.Itoa(int(row[x*2])<<8 | int(row[x*2+1])))
			buf.WriteByte(' ')
		}
		buf.WriteByte('\n')
	}

	if err := os.WriteFile(dst, buf.Bytes(), 0o600); err != nil {
		t.Fatalf("write %s: %v", dst, err)
	}
	return dst
}

// remapFingerprint runs the production filter chain with the given maps and
// returns the SHA-256 of the decoded frames -- the only comparison that says
// anything about what the user sees.
func remapFingerprint(t *testing.T, clip, xPath, yPath string) string {
	t.Helper()

	out, err := exec.Command("ffmpeg", "-hide_banner", "-loglevel", "error",
		"-i", clip, "-i", xPath, "-i", yPath,
		"-filter_complex", "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p",
		"-f", "rawvideo", "-pix_fmt", "yuv420p", "-").Output()
	if err != nil {
		t.Fatalf("remap with %s failed: %v", filepath.Base(xPath), err)
	}
	if len(out) == 0 {
		t.Fatalf("remap with %s produced no frames", filepath.Base(xPath))
	}
	sum := sha256.Sum256(out)
	return hex.EncodeToString(sum[:])
}

// TestRemapMapBytes_MatchesWhatGeneratePGMWrites ties the space check to
// reality.
//
// checkTempSpaceForMaps refuses to start an encode when the temporary
// filesystem cannot hold the maps, which is only useful if the figure it
// reserves is the figure GeneratePGM actually writes. An estimate that drifts
// from the writer is worse than no check: it would either block encodes that
// would have worked or wave through the ones that cannot.
func TestRemapMapBytes_MatchesWhatGeneratePGMWrites(t *testing.T) {
	cases := []struct {
		name          string
		width, height int
		squeeze       bool
	}{
		{"1440x1080_noSqueeze", 1440, 1080, false},
		{"1440x1080_squeeze", 1440, 1080, true},
		{"641x481_noSqueeze", 641, 481, false},
		{"641x481_squeeze", 641, 481, true},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			video := &VideoSpecs{File: "sizing.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "60", DurationFloat: 60, Bitrate: "5000000", BitrateInt: 5000000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, tc.squeeze); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			xPath, yPath, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}

			var written int64
			for _, path := range []string{xPath, yPath} {
				info, statErr := os.Stat(path)
				if statErr != nil {
					t.Fatalf("stat %s: %v", path, statErr)
				}
				written += info.Size()
			}

			if predicted := remapMapBytes(video, tc.squeeze); predicted != written {
				t.Errorf("remapMapBytes = %d, but GeneratePGM wrote %d bytes", predicted, written)
			}
		})
	}
}

func TestRemapMapBytes_NoStreams(t *testing.T) {
	if got := remapMapBytes(nil, false); got != 0 {
		t.Errorf("remapMapBytes(nil) = %d, want 0", got)
	}
	if got := remapMapBytes(&VideoSpecs{}, false); got != 0 {
		t.Errorf("remapMapBytes(no streams) = %d, want 0", got)
	}
}

// TestCheckTempSpaceForMaps_PassesWhenThereIsRoom guards against the check
// becoming a blanket refusal: a modest map on a normal machine must go through.
func TestCheckTempSpaceForMaps_PassesWhenThereIsRoom(t *testing.T) {
	video := &VideoSpecs{File: "small.mp4", Streams: []VideoStream{{
		Codec: "h264", Width: 640, Height: 480,
		Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
	}}}

	if err := checkTempSpaceForMaps(video, false); err != nil {
		t.Errorf("a 640x480 job was refused for lack of space: %v", err)
	}
}

// TestGeneratePGM_SqueezeMapIsContinuous is the test that actually guards the
// fix; the golden hashes only record that bytes moved.
//
// The squeeze offset is built from two terms that are algebraically identical
// at the centre of the frame -- both reduce to 7/32 * outX * inv -- so the
// curve is meant to pass through zero there. It did not, because outX/16 and
// outX/7 were integer divisions: the truncation left a residue which, mirrored
// for the left half, became a jump of 0.9 to 2.6 px straight down the middle of
// the image. It varied erratically with the resolution and vanished entirely
// when the width happened to be divisible by 112 (16*7), which is what
// identified the cause.
//
// The widths below are deliberately not multiples of 112: on those, the bug
// could not reproduce.
func TestGeneratePGM_SqueezeMapIsContinuous(t *testing.T) {
	widths := []struct {
		width, height int
	}{
		{640, 480}, {1440, 1080}, {1920, 1440}, {2880, 2160}, {4064, 3048},
	}

	for _, tc := range widths {
		t.Run(fmt.Sprintf("%dx%d", tc.width, tc.height), func(t *testing.T) {
			if tc.width%112 == 0 {
				t.Fatalf("width %d is a multiple of 112, on which the defect cannot appear", tc.width)
			}

			video := &VideoSpecs{File: "seam.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, true); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			xPath, _, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}
			data, err := os.ReadFile(xPath)
			if err != nil {
				t.Fatalf("read x.pgm: %v", err)
			}

			_, body, width, _ := parsePGMP5(t, data)
			sample := func(x int) int {
				return int(body[x*2])<<8 | int(body[x*2+1])
			}

			// Across the centre line, the map must step like it does anywhere
			// else. A seam shows up as one step several times larger than its
			// neighbours.
			mid := width / 2
			seam := sample(mid) - sample(mid-1)
			before := sample(mid-1) - sample(mid-2)
			after := sample(mid+1) - sample(mid)

			neighbour := before
			if after > neighbour {
				neighbour = after
			}
			if seam > neighbour+1 {
				t.Errorf("discontinuity at the centre: the step across it is %d px against %d px on either side",
					seam, neighbour)
			}
		})
	}
}

// TestGeneratePGM_SqueezeMapStaysWellFormed pins the properties the golden
// hashes cannot express: the curve must never fold back on itself, must stay
// inside the source frame, and must leave the centre pixel where it is.
func TestGeneratePGM_SqueezeMapStaysWellFormed(t *testing.T) {
	for _, tc := range []struct{ width, height int }{
		{640, 480}, {1440, 1080}, {2880, 2160}, {4064, 3048},
	} {
		t.Run(fmt.Sprintf("%dx%d", tc.width, tc.height), func(t *testing.T) {
			video := &VideoSpecs{File: "shape.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: tc.width, Height: tc.height,
				Duration: "1", DurationFloat: 1, Bitrate: "100000", BitrateInt: 100000,
			}}}

			if err := InitEncodingSession(nil); err != nil {
				t.Fatalf("InitEncodingSession: %v", err)
			}
			defer func() { _ = CleanUp() }()

			if err := GeneratePGM(video, true); err != nil {
				t.Fatalf("GeneratePGM: %v", err)
			}
			xPath, _, err := getSessionPaths()
			if err != nil {
				t.Fatalf("getSessionPaths: %v", err)
			}
			data, err := os.ReadFile(xPath)
			if err != nil {
				t.Fatalf("read x.pgm: %v", err)
			}

			_, body, width, _ := parsePGMP5(t, data)
			sample := func(x int) int { return int(body[x*2])<<8 | int(body[x*2+1]) }

			// Monotonic: a step backwards would fold the image onto itself.
			for x := 1; x < width; x++ {
				if sample(x) < sample(x-1) {
					t.Fatalf("map goes backwards at x=%d (%d after %d): the image would fold",
						x, sample(x), sample(x-1))
				}
			}
			if got := sample(0); got != 0 {
				t.Errorf("left edge samples %d, want 0", got)
			}
			if got := sample(width - 1); got != tc.width-1 {
				t.Errorf("right edge samples %d, want %d", got, tc.width-1)
			}
			if got, want := sample(width/2), tc.width/2; got != want {
				t.Errorf("centre samples %d, want %d: the middle of the frame must stay put", got, want)
			}
		})
	}
}
