// The remap maps: output geometry, the distortion formula, and the PGM writer.

package common

import (
	"bufio"
	"fmt"
	"log/slog"
	"math"
	"os"
)

// remapOutputSize returns the dimensions of the remapped frame, which are also
// the dimensions of each remap map.
//
// Shared by GeneratePGM and by the space check that runs before it: computing
// the geometry twice is exactly how the two would drift apart, and the check
// would then be reserving space for a map of a different size than the one
// about to be written.
func remapOutputSize(video *VideoSpecs, squeeze bool) (outX, outY int) {
	if squeeze {
		outX = video.Streams[0].Width
	} else {
		outX = int(float64(video.Streams[0].Height)*(16.0/9.0)) / 2 * 2 // multiplier of 2
	}
	return outX, video.Streams[0].Height
}

// checkGeometry refuses a frame the widening map cannot handle.
//
// In widening (non-squeeze) mode the X map is x - d/2 -/+ tx^2*d/2, where d is
// the number of columns being added. Its slope at the edges is 1 - 2d/outX, so
// the map stays monotonic only while d <= outX/2 -- that is, while the source is
// at least half as wide as the 16:9 output, H/W <= 9/8. Past that the curve
// turns back on itself near both edges: a 1080x1920 portrait clip produced
// samples up to 1164 for a 1080-pixel row, which remap renders as black bands
// and a mirrored ribbon, without an error anywhere.
//
// The test is written on the output width remapOutputSize actually computes,
// not on the ratio, so it cannot disagree with the map by a rounding.
//
// Squeeze mode keeps the source width and is never refused here.
func checkGeometry(video *VideoSpecs, squeeze bool) error {
	if squeeze || video == nil || len(video.Streams) == 0 {
		return nil
	}
	width, height := video.Streams[0].Width, video.Streams[0].Height
	outX, _ := remapOutputSize(video, squeeze)
	if outX <= 2*width {
		return nil
	}
	return &InvalidVideoError{Reason: fmt.Sprintf(
		"the frame is %dx%d, taller than 9:8; widening it to 16:9 (%dx%d) would fold the image back on itself at both edges. Superview converts 4:3 footage; a portrait clip cannot be widened",
		width, height, outX, height)}
}

// remapMapBytes returns the number of bytes the two remap maps will take.
//
// Two files, one 16-bit sample per pixel, plus a short ASCII header each. Exact
// rather than approximate: TestRemapMapBytes_MatchesWhatGeneratePGMWrites
// compares it against the files actually produced.
func remapMapBytes(video *VideoSpecs, squeeze bool) int64 {
	if video == nil || len(video.Streams) == 0 {
		return 0
	}
	outX, outY := remapOutputSize(video, squeeze)
	header := int64(len(fmt.Sprintf("P5 %d %d 65535\n", outX, outY)))
	perMap := header + int64(outX)*int64(outY)*2
	return perMap * 2
}

// checkTempSpaceForMaps refuses to start when the temporary filesystem cannot
// hold the remap maps.
//
// A 12 Mpx GoPro frame needs 66 MB of maps, and on most current distributions
// the temporary directory is a tmpfs -- which is to say RAM. Without this the
// shortfall surfaced halfway through as an opaque write error from a stage the
// user has no reason to know exists.
//
// A failed probe is not a refusal: if the free space cannot be read, the encode
// goes ahead and fails later at worst, exactly as it did before.
func checkTempSpaceForMaps(video *VideoSpecs, squeeze bool) error {
	required := remapMapBytes(video, squeeze)
	if required <= 0 {
		return nil
	}

	tempDir := os.TempDir()
	freeGB, err := getFreeDiskGB(tempDir)
	if err != nil {
		logger.Warn("Could not check free space before encoding",
			slog.String("path", tempDir),
			slog.String("error", err.Error()),
		)
		return nil
	}

	free := int64(freeGB * 1024 * 1024 * 1024)
	// A fifth of headroom: the maps are not the only thing written there while
	// ffmpeg runs, and landing exactly at the limit is its own kind of failure.
	needed := required + required/5
	if free >= needed {
		return nil
	}

	return &SessionError{Msg: fmt.Sprintf(
		"not enough space in %s: the remap maps need %d MB (%d MB with headroom) but only %d MB is free. On many systems this directory lives in RAM; set TMPDIR to a location with more room",
		tempDir, required/(1024*1024), needed/(1024*1024), free/(1024*1024))}
}

// putMapSample writes one remap coordinate into dst as the big-endian 16-bit
// sample the PGM P5 format mandates. Little-endian would produce maps that are
// silently wrong rather than rejected, so the order is not an implementation
// detail.
//
// The clamp is a guard, not a behaviour: for every geometry checkGeometry
// accepts -- any frame in squeeze mode, a frame no taller than 9:8 when
// widening -- the distortion math lands in [0, inputWidth-1], well inside the
// range. A taller frame does not, which is why it is refused before a map is
// written. The clamp exists so a future change to the offset formula degrades
// into a fill pixel instead of a wrapped coordinate pointing somewhere
// arbitrary in the frame.
func putMapSample(dst []byte, value int) {
	if value < 0 {
		value = 0
	} else if value > 65535 {
		value = 65535
	}
	dst[0] = byte(value >> 8)
	dst[1] = byte(value)
}

// GeneratePGM creates the remap filter maps for ffmpeg that apply the superview distortion.
// The maps are saved as binary PGM (P5) files in the current encoding session's temp directory.
// If squeeze is true, applies asymmetric scaling for 4:3 video stretched to 16:9.
// If squeeze is false, applies symmetric barrel-distortion correction.
func GeneratePGM(video *VideoSpecs, squeeze bool) (err error) {
	// Validate video before processing
	if err := video.Validate(); err != nil {
		return err
	}
	// PerformEncoding refuses this first; repeated here so no caller can write
	// a folded map.
	if err := checkGeometry(video, squeeze); err != nil {
		return err
	}

	outX, outY := remapOutputSize(video, squeeze)

	logger.Info("Scaling video",
		slog.String("file", video.File),
		slog.String("codec", video.Streams[0].Codec),
		slog.Int("duration_secs", int(video.Streams[0].DurationFloat)),
		slog.Int("input_width", video.Streams[0].Width),
		slog.Int("input_height", video.Streams[0].Height),
		slog.Int("output_width", outX),
		slog.Int("output_height", outY),
		slog.Bool("squeeze", squeeze),
	)

	// Generate PGM P5 files for remap filter, see https://trac.ffmpeg.org/wiki/RemapFilter
	//
	// P5 is the binary flavour of PGM: two bytes per sample instead of a decimal
	// number and a separator. FFmpeg's remap filter reads it exactly like the P2
	// ASCII form these maps used to use -- verified byte for byte on the decoded
	// output -- but a 12 Mpx GoPro frame drops from 146 MB of maps to 66 MB, and
	// generation no longer has to format 33 million integers. That matters more
	// than it looks: /tmp is a tmpfs on most current distributions, so those
	// megabytes are RAM.
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	fX, err := os.Create(xPath)
	if err != nil {
		return fmt.Errorf("failed to create temp file x.pgm: %w", err)
	}
	fY, err := os.Create(yPath)
	if err != nil {
		_ = fX.Close()
		return fmt.Errorf("failed to create temp file y.pgm: %w", err)
	}
	// Close errors matter here: a remap map truncated at flush time makes ffmpeg
	// fail much later with an opaque message. Surface it at the source instead.
	defer func() {
		if closeErr := fX.Close(); closeErr != nil && err == nil {
			err = fmt.Errorf("failed to close x.pgm: %w", closeErr)
		}
		if closeErr := fY.Close(); closeErr != nil && err == nil {
			err = fmt.Errorf("failed to close y.pgm: %w", closeErr)
		}
	}()

	// Buffer both files: the maps run to tens of megabytes and were previously
	// written one line at a time straight to the file descriptor.
	wX := bufio.NewWriterSize(fX, 1<<20)
	wY := bufio.NewWriterSize(fY, 1<<20)

	// Write PGM headers. Everything goes through the buffered writers, headers
	// included, so there is no ordering to reason about between the two.
	header := fmt.Sprintf("P5 %d %d 65535\n", outX, outY)

	if _, err := wX.WriteString(header); err != nil {
		return fmt.Errorf("failed to write header to x.pgm: %w", err)
	}
	if _, err := wY.WriteString(header); err != nil {
		return fmt.Errorf("failed to write header to y.pgm: %w", err)
	}

	rowBytes := outX * 2

	// The X map is row-invariant: sx, tx and offset are functions of x alone,
	// so every row of x.pgm is byte-for-byte identical. Build it once instead
	// of recomputing the same math outY times.
	bufX := make([]byte, rowBytes)
	for x := 0; x < outX; x++ {
		sx := float64(x) - float64(outX-video.Streams[0].Width)/2.0 // x - width diff/2
		tx := (float64(x)/float64(outX) - 0.5) * 2.0                // (x/width - 0.5) * 2

		var offset float64

		if squeeze {
			inv := 1 - math.Abs(tx)

			// The two terms are algebraically identical at the centre --
			// both reduce to 7/32 * outX * inv -- so the curve is meant to
			// pass through zero there, exactly as the non-squeeze branch does.
			//
			// They only failed to cancel because outX/16 and outX/7 were
			// integer divisions: the truncation left a residue, mirrored into
			// a jump of 0.9 to 2.6 px straight down the middle of the frame,
			// varying erratically with the resolution. A width divisible by
			// 112 (16*7) produced no seam at all, which is what pointed at the
			// truncation. Dividing in floating point is the whole fix; the
			// shape of the expression is kept recognisable against the
			// reference implementation, which spells it this way.
			offset = inv*(float64(outX)/16.0*7.0/2.0) - math.Pow((inv/16)*7, 2)*(float64(outX)/7.0*16.0/2.0)

			if tx < 0 {
				offset *= -1
			}

			putMapSample(bufX[x*2:], int(sx+offset))
		} else {
			offset = math.Pow(tx, 2) * (float64(outX-video.Streams[0].Width) / 2.0) // tx^2 * width diff/2

			if tx < 0 {
				offset *= -1
			}

			putMapSample(bufX[x*2:], int(sx-offset))
		}
	}

	bufY := make([]byte, rowBytes)
	var sample [2]byte

	for y := 0; y < outY; y++ {
		if _, err := wX.Write(bufX); err != nil {
			return fmt.Errorf("failed to write x.pgm: %w", err)
		}

		// A row of the Y map is the row index repeated outX times; encode the
		// sample once rather than once per pixel. It goes through putMapSample
		// like the X map so the byte order is defined in exactly one place --
		// inlining it here would let the two maps drift apart silently.
		putMapSample(sample[:], y)
		for i := 0; i < rowBytes; i += 2 {
			bufY[i], bufY[i+1] = sample[0], sample[1]
		}

		if _, err := wY.Write(bufY); err != nil {
			return fmt.Errorf("failed to write y.pgm: %w", err)
		}
	}

	if err := wX.Flush(); err != nil {
		return fmt.Errorf("failed to flush x.pgm: %w", err)
	}
	if err := wY.Flush(); err != nil {
		return fmt.Errorf("failed to flush y.pgm: %w", err)
	}

	logger.Info("Filter files generated successfully")

	return nil
}
