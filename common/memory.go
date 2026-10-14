// The memory guard: what a software encode is measured to need, and the refusal.

package common

import (
	"fmt"
	"log/slog"
)

// Memory a software encode asks for, per pixel of the output frame.
//
// Measured, not reasoned. The geometry is the one from the report that produced
// this check -- a 3840x2880 source widened to 5120x2880 -- encoded with libx265
// at preset medium, "-threads 4" and "pools=4", which is what a four-core
// laptop runs:
//
//	8-bit source .... 3.97 GiB peak RSS -> 290 bytes per output pixel
//	10-bit source ... 6.25 GiB peak RSS -> 455 bytes per output pixel
//
// That is the figure at four threads. x265 allocates per frame thread, and the
// application encodes with one per core (up to 16), so the estimate grows with
// the thread count: see memoryThreadPercent.
//
// Three properties of that figure matter here. It belongs to the frame rather
// than to the codec: libx264 on the same frame peaked within 1% of libx265. It
// does not grow with the clip's length -- these are frame buffers, not an
// accumulating stream. And it drifts slightly *up* on smaller frames (312 bytes
// per pixel at 2560x1440), where the absolute requirement is small enough for
// the difference not to decide anything.
const (
	softwareEncodeBytesPerPixel      = 290
	softwareEncodeBytesPerPixel10Bit = 455
)

// What each thread beyond the baseline adds, in percent of the baseline figure.
//
// Q-02, measured on a 16-core machine with libx265 at preset medium (peak RSS):
//
//	5120x2880, 8-bit ..  4 threads 3.54 GiB  8 threads 4.15  16 threads 5.19
//	5120x2880, 10-bit .  4 threads 5.30 GiB  8 threads 6.14  16 threads over 7 (killed at the cap)
//	2560x1440, 8-bit ..  4 threads 1.11 GiB  8 threads 1.28  16 threads 1.64
//	2560x1440, 10-bit .  4 threads 1.60 GiB  8 threads 1.84  16 threads 2.34
//
// A straight line: +16% at 8 threads, +47% at 16, so about 4% per thread, on
// both depths and both sizes. Below the baseline nothing was measured, so
// nothing is credited. The bench fed a synthetic source with no remap filter,
// which put its 4-thread level 11 to 15% under the constants above; the slope
// is what it contributes, not its absolute level.
const (
	memoryBaselineThreads = 4
	memoryThreadPercent   = 4
)

// readAvailableMemory is the memory reading, behind a variable so a test can
// hand the check a machine that is out of memory. Swapping the reading is the
// only way to exercise the refusal: the alternative is filling the test
// machine's RAM, which proves the same thing and takes it down with it.
var readAvailableMemory = availableMemoryBytes

// memoryHeadroomDivisor sets the margin below which the estimate is worth a
// warning: needed + needed/4.
const memoryHeadroomDivisor = 4

// memoryNeededForEncode estimates the peak memory a conversion will ask for, or
// 0 when there is no estimate to give.
//
// Hardware encoders return 0 deliberately. Nothing here has measured one, and
// the buffers that dominate the figure above belong to the CPU encoder -- a GPU
// encode keeps its reference frames in video memory instead. Refusing a
// conversion on a number nobody measured would be worse than not checking.
//
// threads is the -threads value the encode will really be given.
func memoryNeededForEncode(video *VideoSpecs, squeeze bool, encoder string, threads int) uint64 {
	// remapOutputSize reads Streams[0] without a guard of its own, so the empty
	// cases are settled before it is called rather than inside it.
	if video == nil || len(video.Streams) == 0 {
		return 0
	}
	outX, outY := remapOutputSize(video, squeeze)
	return memoryNeededForFrame(outX, outY, video, encoder, threads)
}

// memoryNeededForFrame is the estimate for a frame whose size is already known.
//
// Split out for the caller that has the geometry but not the squeeze flag: the
// fallback inside EncodeVideo, which reads the size from the remap maps rather
// than recomputing it.
func memoryNeededForFrame(outX, outY int, video *VideoSpecs, encoder string, threads int) uint64 {
	if video == nil || len(video.Streams) == 0 || encoder == "" || isHardwareEncoder(encoder) {
		return 0
	}
	if outX <= 0 || outY <= 0 {
		return 0
	}

	perPixel := uint64(softwareEncodeBytesPerPixel)
	if encodesInTenBits(sourcePixelFormat(video), encoder) {
		perPixel = softwareEncodeBytesPerPixel10Bit
	}

	needed := uint64(outX) * uint64(outY) * perPixel
	if threads > memoryBaselineThreads {
		extra := uint64(threads - memoryBaselineThreads)
		needed += needed * extra * memoryThreadPercent / 100
	}
	return needed
}

// formatGB renders a byte count the way the messages here quote memory.
func formatGB(bytes uint64) string {
	return fmt.Sprintf("%.1f GB", float64(bytes)/(1024*1024*1024))
}

// checkMemoryForEncode refuses to start a conversion this machine cannot hold,
// and warns when it will only just fit.
//
// The disk equivalent, checkTempSpaceForMaps, has guarded 57 MB of remap maps
// since the sixth pass. Nothing guarded the several gigabytes the encode itself
// asks for, and that failure is the worse of the two by far: the kernel kills
// ffmpeg, systemd stops the whole application unit with it, and the window
// disappears mid-conversion. It happened to a user on 8 GiB.
//
// A reading that cannot be taken is not a refusal -- same rule as the disk
// check. On Windows there is no /proc/meminfo, so this never fires there.
func checkMemoryForEncode(video *VideoSpecs, squeeze bool, encoder string, threads int) error {
	if video == nil || len(video.Streams) == 0 {
		return nil
	}
	outX, outY := remapOutputSize(video, squeeze)
	return checkMemoryForFrame(outX, outY, video, encoder, threads)
}

// checkMemoryForFrame is checkMemoryForEncode for a frame already measured.
func checkMemoryForFrame(outX, outY int, video *VideoSpecs, encoder string, threads int) error {
	needed := memoryNeededForFrame(outX, outY, video, encoder, threads)
	if needed == 0 {
		return nil
	}

	available, ok := readAvailableMemory()
	if !ok {
		return nil
	}

	if available >= needed+needed/memoryHeadroomDivisor {
		return nil
	}

	depth := "8-bit"
	if encodesInTenBits(sourcePixelFormat(video), encoder) {
		depth = "10-bit"
	}

	logger.Warn("Memory will be tight for this conversion",
		slog.Int("output_width", outX),
		slog.Int("output_height", outY),
		slog.String("encoder", encoder),
		slog.String("depth", depth),
		slog.Int("threads", threads),
		slog.String("needed", formatGB(needed)),
		slog.String("available", formatGB(available)),
	)

	if available >= needed {
		return nil
	}

	return &EncoderError{Msg: fmt.Sprintf(
		"not enough memory for this conversion: encoding a %dx%d %s frame with %s needs about %s, "+
			"and only %s is available. Close other applications, convert a video with fewer pixels, "+
			"or pick a hardware encoder in the codec list if Diagnostic says this machine accepts one.",
		outX, outY, depth, encoder, formatGB(needed), formatGB(available))}
}
