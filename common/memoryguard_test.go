package common

import (
	"errors"
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
			needed := float64(memoryNeededForEncode(reportedClip("hevc", tc.pixFmt), false, tc.encoder)) / gib

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

	onHEVC := memoryNeededForEncode(tenBitSource, false, "libx265")
	onH264 := memoryNeededForEncode(tenBitSource, false, "libx264")

	if onH264 >= onHEVC {
		t.Fatalf("a 10-bit source encoded as H.264 is stored at 8-bit, so it must cost less: %d vs %d", onH264, onHEVC)
	}
	if onH264 != memoryNeededForEncode(reportedClip("hevc", "yuv420p"), false, "libx264") {
		t.Fatal("an 8-bit path must cost the same whatever the source depth")
	}
}

// TestMemoryNeededForEncode_SaysNothingAboutHardwareEncoders keeps the check
// honest: nothing here has measured a GPU encode, and refusing a conversion on
// an invented number would be worse than not checking at all.
func TestMemoryNeededForEncode_SaysNothingAboutHardwareEncoders(t *testing.T) {
	for _, encoder := range []string{"h264_vaapi", "hevc_nvenc", "h264_qsv"} {
		if needed := memoryNeededForEncode(reportedClip("hevc", "yuv420p10le"), false, encoder); needed != 0 {
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
			if needed := memoryNeededForEncode(tc.video, false, tc.encoder); needed != 0 {
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

	widened := memoryNeededForEncode(video, false, "libx265")
	unwidened := memoryNeededForEncode(video, true, "libx265")

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

	err := checkMemoryForEncode(video, false, "libx265")
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

	if err := checkMemoryForEncode(video, false, "libx265"); err != nil {
		t.Fatalf("16 GB is ample for a 6.25 GB encode, got %v", err)
	}
}

// TestCheckMemoryForEncode_AllowsATightFit draws the line where it was meant to
// be. Between the estimate and a quarter above it the encode is warned about,
// not refused: the estimate is a measured average, and refusing a machine that
// would have coped is its own kind of failure.
func TestCheckMemoryForEncode_AllowsATightFit(t *testing.T) {
	video := reportedClip("hevc", "yuv420p10le")
	needed := memoryNeededForEncode(video, false, "libx265")
	withAvailableMemory(t, needed+needed/8, true)

	if err := checkMemoryForEncode(video, false, "libx265"); err != nil {
		t.Fatalf("a tight fit must be warned about, not refused: %v", err)
	}
}

// TestCheckMemoryForEncode_DoesNotRefuseWhatItCannotMeasure covers Windows,
// where there is no /proc/meminfo, and any machine whose reading fails. The
// disk check follows the same rule: a question that cannot be asked is not a
// refusal.
func TestCheckMemoryForEncode_DoesNotRefuseWhatItCannotMeasure(t *testing.T) {
	withAvailableMemory(t, 0, false)

	if err := checkMemoryForEncode(reportedClip("hevc", "yuv420p10le"), false, "libx265"); err != nil {
		t.Fatalf("with no reading available the encode must go ahead, got %v", err)
	}
}

func TestCheckMemoryForEncode_IgnoresHardwareEncoders(t *testing.T) {
	withAvailableMemory(t, 1*gib, true)

	if err := checkMemoryForEncode(reportedClip("hevc", "yuv420p10le"), false, "h264_vaapi"); err != nil {
		t.Fatalf("a hardware encoder has no estimate and must not be refused, got %v", err)
	}
}
