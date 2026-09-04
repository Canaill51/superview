package common

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"testing"
)

// TestGeneratePGM_Golden pins the exact bytes of the remap maps.
//
// The distortion math comes from Banelle's reference Python implementation
// (https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview);
// its integer divisions and float truncations are part of the algorithm, not
// accidents. Any change to GeneratePGM that alters a single output byte changes
// what users actually see, so it must be a deliberate, reviewed decision --
// this test makes that impossible to do by accident.
//
// Hashes recorded 2026-09-04 against the implementation that shipped in
// e3269e7, so they also certify the later buffering/hoisting optimisation as
// behaviour-preserving.
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
			xSize: 8783659, ySize: 8237899,
			xHash: "0afbc681db96da5bdb41449c677e2e9b7cbea188a53a0de055296268356deb11",
			yHash: "1dbe4fb65cc6f9cfa592c8773030ed0d9f9b6e09268e1dbd942eb37cebaac152",
		},
		{
			name: "1440x1080_squeeze", width: 1440, height: 1080, squeeze: true,
			xSize: 6583699, ySize: 6178699,
			xHash: "d61496a1d38a6dde1dbf40594a22fd14236ebe15eab4fe3f28dd1cca11b5ebb8",
			yHash: "9e39466801c16e58a98b20393d34b3877c5f4f6c95dffeb5fdb89a6b57874416",
		},
		{
			// Odd dimensions exercise the integer-division rounding.
			name: "641x481_noSqueeze", width: 641, height: 481, squeeze: false,
			xSize: 1553166, ySize: 1549654,
			xHash: "7e1799046e102ffce445ffd1348464d6cffcf27c674c20bd9fccc0f6cb5161cd",
			yHash: "bdeb6d934f39806fe42ce00d71e556bca784ee07c9b10b8c41325412afbe7420",
		},
		{
			name: "641x481_squeeze", width: 641, height: 481, squeeze: true,
			xSize: 1160189, ySize: 1163272,
			xHash: "1cde77e906fb2b6da314bc223c8d875c0e090d0d3ab3f9d396415cf45dd8096c",
			yHash: "509d15152add2b5658761dd77c3d2f7a279ad58bd8de9df4d75fe05acde844a7",
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

	lines := splitLines(string(data))
	if len(lines) < 3 {
		t.Fatalf("expected a header plus rows, got %d lines", len(lines))
	}
	first := lines[1] // lines[0] is the "P2 w h 65535" header
	for i, line := range lines[2:] {
		if line != first {
			t.Fatalf("row %d differs from row 0; the X map is not row-invariant", i+1)
		}
	}
}

func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	return out
}
