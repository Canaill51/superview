package common

import (
	"errors"
	"runtime"
	"strings"
	"testing"
)

// reportedClip is the geometry the whole memory finding was measured on: the
// user's 3840x2880 source, which the conversion widens to 5120x2880.
func reportedClip(codec, pixFmt string) *VideoSpecs {
	return &VideoSpecs{
		File: "clip.mp4",
		Streams: []VideoStream{{
			Codec:         codec,
			Width:         3840,
			Height:        2880,
			PixFmt:        pixFmt,
			DurationFloat: 43,
			BitrateInt:    120000000,
		}},
	}
}

const gib = 1024 * 1024 * 1024

// TestMemoryNeededForEncode_MatchesTheMeasurement ties the constants to the
// bench they came from. libx265 at preset medium on that frame peaked at
// 3.97 GiB for an 8-bit source and 6.25 GiB for a 10-bit one; an estimate that
// drifted away from those would either refuse conversions that fit or let
// through the one that got a user's ffmpeg killed.
func TestMemoryNeededForEncode_MatchesTheMeasurement(t *testing.T) {
	cases := []struct {
		name     string
		pixFmt   string
		encoder  string
		measured float64 // GiB
	}{
		{"8-bit source", "yuv420p", "libx265", 3.97},
		{"10-bit source", "yuv420p10le", "libx265", 6.25},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			needed := float64(memoryNeededForEncode(reportedClip("hevc", tc.pixFmt), false, tc.encoder, memoryBaselineThreads)) / gib

			// A tenth of a gigabyte of tolerance: the estimate is a straight
			// line through two measured points, not a model of x265.
			if needed < tc.measured-0.15 || needed > tc.measured+0.15 {
				t.Fatalf("estimate %.2f GiB is not the measured %.2f GiB", needed, tc.measured)
			}
		})
	}
}

// TestMemoryNeededForEncode_FollowsTheFilterChain is the reason encodesInTenBits
// is one function. A 10-bit source handed to an H.264 encoder is converted to
// 8-bit by remapFilterChain, so costing it at the 10-bit rate would refuse
// conversions that fit comfortably.
func TestMemoryNeededForEncode_FollowsTheFilterChain(t *testing.T) {
	tenBitSource := reportedClip("hevc", "yuv420p10le")

	onHEVC := memoryNeededForEncode(tenBitSource, false, "libx265", memoryBaselineThreads)
	onH264 := memoryNeededForEncode(tenBitSource, false, "libx264", memoryBaselineThreads)

	if onH264 >= onHEVC {
		t.Fatalf("a 10-bit source encoded as H.264 is stored at 8-bit, so it must cost less: %d vs %d", onH264, onHEVC)
	}
	if onH264 != memoryNeededForEncode(reportedClip("hevc", "yuv420p"), false, "libx264", memoryBaselineThreads) {
		t.Fatal("an 8-bit path must cost the same whatever the source depth")
	}
}

// TestMemoryNeededForEncode_SaysNothingAboutHardwareEncoders keeps the check
// honest: nothing here has measured a GPU encode, and refusing a conversion on
// an invented number would be worse than not checking at all.
func TestMemoryNeededForEncode_SaysNothingAboutHardwareEncoders(t *testing.T) {
	for _, encoder := range []string{"h264_vaapi", "hevc_nvenc", "h264_qsv"} {
		if needed := memoryNeededForEncode(reportedClip("hevc", "yuv420p10le"), false, encoder, memoryBaselineThreads); needed != 0 {
			t.Fatalf("%s is a hardware encoder and was not measured, yet it was costed at %d", encoder, needed)
		}
	}
}

func TestMemoryNeededForEncode_HandlesNothingToEstimate(t *testing.T) {
	cases := map[string]struct {
		video   *VideoSpecs
		encoder string
	}{
		"no video":   {nil, "libx265"},
		"no streams": {&VideoSpecs{File: "clip.mp4"}, "libx265"},
		"no encoder": {reportedClip("hevc", "yuv420p"), ""},
	}

	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			if needed := memoryNeededForEncode(tc.video, false, tc.encoder, memoryBaselineThreads); needed != 0 {
				t.Fatalf("expected no estimate, got %d", needed)
			}
		})
	}
}

// TestMemoryNeededForEncode_CostsTheOutputFrameNotTheInput guards the direction
// of the whole finding: the conversion widens the frame, and it is the widened
// frame that has to be held in memory.
func TestMemoryNeededForEncode_CostsTheOutputFrameNotTheInput(t *testing.T) {
	video := reportedClip("hevc", "yuv420p")

	widened := memoryNeededForEncode(video, false, "libx265", memoryBaselineThreads)
	unwidened := memoryNeededForEncode(video, true, "libx265", memoryBaselineThreads)

	if widened <= unwidened {
		t.Fatalf("the widened output must cost more than the un-widened one: %d vs %d", widened, unwidened)
	}
}

// withAvailableMemory hands the check a machine with a chosen amount of memory.
func withAvailableMemory(t *testing.T, available uint64, ok bool) {
	t.Helper()
	previous := readAvailableMemory
	readAvailableMemory = func() (uint64, bool) { return available, ok }
	t.Cleanup(func() { readAvailableMemory = previous })
}

// TestCheckMemoryForEncode_RefusesWhatCannotFit is the user's machine: about
// 6.25 GB wanted, and less than that free.
func TestCheckMemoryForEncode_RefusesWhatCannotFit(t *testing.T) {
	video := reportedClip("hevc", "yuv420p10le")
	withAvailableMemory(t, 4*gib, true)

	err := checkMemoryForEncode(video, false, "libx265", memoryBaselineThreads)
	if err == nil {
		t.Fatal("expected a refusal when the encode cannot fit in memory")
	}

	var encoderErr *EncoderError
	if !errors.As(err, &encoderErr) {
		t.Fatalf("expected an EncoderError, got %T", err)
	}
	for _, want := range []string{"5120x2880", "10-bit", "libx265", "6.2 GB", "4.0 GB"} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("the refusal must say %q, got %q", want, err.Error())
		}
	}
}

func TestCheckMemoryForEncode_AllowsWhatFitsWithRoom(t *testing.T) {
	video := reportedClip("hevc", "yuv420p10le")
	withAvailableMemory(t, 16*gib, true)

	if err := checkMemoryForEncode(video, false, "libx265", memoryBaselineThreads); err != nil {
		t.Fatalf("16 GB is ample for a 6.25 GB encode, got %v", err)
	}
}

// TestCheckMemoryForEncode_AllowsATightFit draws the line where it was meant to
// be. Between the estimate and a quarter above it the encode is warned about,
// not refused: the estimate is a measured average, and refusing a machine that
// would have coped is its own kind of failure.
func TestCheckMemoryForEncode_AllowsATightFit(t *testing.T) {
	video := reportedClip("hevc", "yuv420p10le")
	needed := memoryNeededForEncode(video, false, "libx265", memoryBaselineThreads)
	withAvailableMemory(t, needed+needed/8, true)

	if err := checkMemoryForEncode(video, false, "libx265", memoryBaselineThreads); err != nil {
		t.Fatalf("a tight fit must be warned about, not refused: %v", err)
	}
}

// TestCheckMemoryForEncode_DoesNotRefuseWhatItCannotMeasure covers Windows,
// where there is no /proc/meminfo, and any machine whose reading fails. The
// disk check follows the same rule: a question that cannot be asked is not a
// refusal.
func TestCheckMemoryForEncode_DoesNotRefuseWhatItCannotMeasure(t *testing.T) {
	withAvailableMemory(t, 0, false)

	if err := checkMemoryForEncode(reportedClip("hevc", "yuv420p10le"), false, "libx265", memoryBaselineThreads); err != nil {
		t.Fatalf("with no reading available the encode must go ahead, got %v", err)
	}
}

func TestCheckMemoryForEncode_IgnoresHardwareEncoders(t *testing.T) {
	withAvailableMemory(t, 1*gib, true)

	if err := checkMemoryForEncode(reportedClip("hevc", "yuv420p10le"), false, "h264_vaapi", memoryBaselineThreads); err != nil {
		t.Fatalf("a hardware encoder has no estimate and must not be refused, got %v", err)
	}
}

// TestMemoryNeededForEncode_GrowsWithThreads pins the slope Q-02 asked about.
//
// The 290 and 455 bytes per pixel were measured at "-threads 4", and the
// application encodes with one thread per core, up to libx265's 16. x265
// allocates per frame thread, so the peak climbs with the count. Measured on a
// 16-core machine (libx265, preset medium, 5120x2880, peak RSS):
//
//	8-bit  ..  4 threads 3.54 GiB   8 threads 4.15 GiB   16 threads 5.19 GiB
//	10-bit ..  4 threads 5.30 GiB   8 threads 6.14 GiB   16 threads > 7 GiB (killed at the cap)
//
// and 2560x1440 gave the same ratios (x1.15 at 8 threads, x1.46 at 16). An
// estimate flat in the thread count would tell a 16-core machine that a
// conversion fits when the kernel is about to kill it.
func TestMemoryNeededForEncode_GrowsWithThreads(t *testing.T) {
	for _, pixFmt := range []string{"yuv420p", "yuv420p10le"} {
		t.Run(pixFmt, func(t *testing.T) {
			video := reportedClip("hevc", pixFmt)
			base := float64(memoryNeededForEncode(video, false, "libx265", memoryBaselineThreads))

			for _, tc := range []struct {
				threads int
				ratio   float64 // measured, relative to 4 threads
			}{
				{8, 1.16},
				{16, 1.47},
			} {
				got := float64(memoryNeededForEncode(video, false, "libx265", tc.threads)) / base
				if got < tc.ratio-0.03 || got > tc.ratio+0.03 {
					t.Errorf("%d threads cost x%.2f the 4-thread estimate, measured x%.2f", tc.threads, got, tc.ratio)
				}
			}
		})
	}
}

// TestMemoryNeededForEncode_NeverBelowTheBaseline: fewer threads than the
// bench used were not measured, so they are not credited with a saving, and an
// unknown count (0) is priced as the baseline rather than as nothing.
func TestMemoryNeededForEncode_NeverBelowTheBaseline(t *testing.T) {
	video := reportedClip("hevc", "yuv420p")
	base := memoryNeededForEncode(video, false, "libx265", memoryBaselineThreads)

	for _, threads := range []int{-1, 0, 1, 2, memoryBaselineThreads} {
		if got := memoryNeededForEncode(video, false, "libx265", threads); got != base {
			t.Errorf("%d threads = %d bytes, want the baseline %d", threads, got, base)
		}
	}
}

// TestCheckMemoryForEncode_RefusesWhatOnlyManyThreadsCannotHold is the case the
// flat estimate got wrong: 7 GiB free holds the 6.25 GiB a four-thread encode
// asks for, but not what the same conversion asks for on 16 cores.
func TestCheckMemoryForEncode_RefusesWhatOnlyManyThreadsCannotHold(t *testing.T) {
	video := reportedClip("hevc", "yuv420p10le")
	withAvailableMemory(t, 7*gib+gib/2, true)

	if err := checkMemoryForEncode(video, false, "libx265", memoryBaselineThreads); err != nil {
		t.Fatalf("four threads fit in 7.5 GiB, got a refusal: %v", err)
	}
	if err := checkMemoryForEncode(video, false, "libx265", 16); err == nil {
		t.Fatal("sixteen threads need about 9 GiB: expected a refusal with 7.5 GiB free")
	}
}

// TestEffectiveEncoderThreads: the guard must price the count ffmpeg is really
// given -- the configured one, else one per core, clamped like the command line.
func TestEffectiveEncoderThreads(t *testing.T) {
	if got := effectiveEncoderThreads(&Config{EncoderThreads: 24}, "libx265"); got != x265MaxFrameThreads {
		t.Errorf("configured 24 on libx265 = %d, want the clamp %d", got, x265MaxFrameThreads)
	}
	if got := effectiveEncoderThreads(&Config{EncoderThreads: 24}, "libx264"); got != 24 {
		t.Errorf("configured 24 on libx264 = %d, want 24", got)
	}
	want := clampEncoderThreads("libx265", runtime.NumCPU())
	if got := effectiveEncoderThreads(&Config{}, "libx265"); got != want {
		t.Errorf("unconfigured = %d, want one per core clamped = %d", got, want)
	}
	if got := effectiveEncoderThreads(nil, "libx265"); got != want {
		t.Errorf("nil config = %d, want %d", got, want)
	}
}
