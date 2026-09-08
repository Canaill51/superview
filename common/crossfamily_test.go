package common

import (
	"strings"
	"testing"
)

// hevcSource is the shape of the file that produced this whole finding: a DJI
// clip in 4:3, HEVC, 10-bit.
func hevcSource(pixFmt string) *VideoSpecs {
	return &VideoSpecs{
		File: "clip.mp4",
		Streams: []VideoStream{{
			Codec:         "hevc",
			Width:         3840,
			Height:        2880,
			PixFmt:        pixFmt,
			DurationFloat: 43,
			BitrateInt:    120000000,
		}},
	}
}

// TestFindEncoder_UsesHardwareFromTheOtherFamilyWhenTheSourceFamilyHasNone is
// the measured case: an Intel HD 620 refuses every hevc_* encoder and accepts
// h264_vaapi. Before this, the H.264 list was never consulted and the clip went
// to libx265 -- a quarter of an hour, and 6 GB of memory on a machine with 8.
func TestFindEncoder_UsesHardwareFromTheOtherFamilyWhenTheSourceFamilyHasNone(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_vaapi,libx264,libx265",
		"accels":   "vaapi",
	}

	encoder, err := FindEncoder("", ffmpegInfo, hevcSource("yuv420p10le"))
	if err != nil {
		t.Fatalf("FindEncoder failed: %v", err)
	}
	if encoder != "h264_vaapi" {
		t.Fatalf("expected the usable hardware encoder h264_vaapi, got %s", encoder)
	}
}

// TestFindEncoder_KeepsTheSourceFamilyWhenItsHardwareWorks is the other half:
// the switch is a last resort, not a preference. A machine whose HEVC encoder
// works must keep producing HEVC.
func TestFindEncoder_KeepsTheSourceFamilyWhenItsHardwareWorks(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_vaapi,hevc_vaapi,libx264,libx265",
		"accels":   "vaapi",
	}

	encoder, err := FindEncoder("", ffmpegInfo, hevcSource("yuv420p10le"))
	if err != nil {
		t.Fatalf("FindEncoder failed: %v", err)
	}
	if encoder != "hevc_vaapi" {
		t.Fatalf("expected hevc_vaapi, got %s", encoder)
	}
}

// TestFindEncoder_DoesNotSwitchFamilyForAnotherCPUEncoder guards the reason for
// switching. Trading libx265 for libx264 buys nothing and costs the codec; only
// reaching actual hardware justifies the move.
func TestFindEncoder_DoesNotSwitchFamilyForAnotherCPUEncoder(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "libx264,libx265",
		"accels":   "",
	}

	encoder, err := FindEncoder("", ffmpegInfo, hevcSource("yuv420p"))
	if err != nil {
		t.Fatalf("FindEncoder failed: %v", err)
	}
	if encoder != "libx265" {
		t.Fatalf("expected the source family's CPU encoder libx265, got %s", encoder)
	}
}

// TestFindEncoder_ExplicitChoiceIsNotOverridden pins that the switch only ever
// applies to the automatic path. A user who picked an encoder in the dropdown
// gets that encoder.
func TestFindEncoder_ExplicitChoiceIsNotOverridden(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_vaapi,libx264,libx265",
		"accels":   "vaapi",
	}

	encoder, err := FindEncoder("libx265", ffmpegInfo, hevcSource("yuv420p10le"))
	if err != nil {
		t.Fatalf("FindEncoder failed: %v", err)
	}
	if encoder != "libx265" {
		t.Fatalf("an explicit choice must win, got %s", encoder)
	}
}

func TestDescribeCodecFamilySwitch_NamesWhatItCosts(t *testing.T) {
	note := describeCodecFamilySwitch(hevcSource("yuv420p10le"), "h264_vaapi")

	for _, want := range []string{"H.265", "H.264", "8-bit"} {
		if !strings.Contains(note, want) {
			t.Fatalf("expected %q in the note, got %q", want, note)
		}
	}
}

// TestDescribeCodecFamilySwitch_MentionsBitDepthOnlyWhenItIsLost keeps the
// message honest: an 8-bit source loses nothing by moving to H.264, and saying
// otherwise would train the user to ignore the line.
func TestDescribeCodecFamilySwitch_MentionsBitDepthOnlyWhenItIsLost(t *testing.T) {
	note := describeCodecFamilySwitch(hevcSource("yuv420p"), "h264_vaapi")

	if note == "" {
		t.Fatal("a family switch must still be announced for an 8-bit source")
	}
	if strings.Contains(note, "8-bit") {
		t.Fatalf("nothing is lost from an 8-bit source, yet the note says so: %q", note)
	}
}

func TestDescribeCodecFamilySwitch_SaysNothingInTheOrdinaryCase(t *testing.T) {
	cases := map[string]struct {
		video   *VideoSpecs
		encoder string
	}{
		"same family, CPU":      {hevcSource("yuv420p10le"), "libx265"},
		"same family, hardware": {hevcSource("yuv420p10le"), "hevc_vaapi"},
		"no video":              {nil, "h264_vaapi"},
		"no encoder":            {hevcSource("yuv420p"), ""},
	}

	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			if note := describeCodecFamilySwitch(tc.video, tc.encoder); note != "" {
				t.Fatalf("expected no note, got %q", note)
			}
		})
	}
}

// TestSoftwareEncoderForFallback_ReturnsToTheSourceFamily is what keeps the
// cross-family move from costing anything when the hardware then refuses the
// real frame. Without it, an HEVC source whose h264_vaapi fails would land on
// libx264 -- a codec nobody wanted once the GPU was out of the picture, and one
// that drops the source's extra bits.
func TestSoftwareEncoderForFallback_ReturnsToTheSourceFamily(t *testing.T) {
	profile := AnalyzeMachineProfile(map[string]string{
		"encoders": "h264_vaapi,libx264,libx265",
		"accels":   "vaapi",
	})

	fallback := softwareEncoderForFallback(hevcSource("yuv420p10le"), "h264_vaapi", profile)
	if fallback != "libx265" {
		t.Fatalf("expected libx265, the source family's CPU encoder, got %s", fallback)
	}
}

func TestSoftwareEncoderForFallback_UsesTheEncoderFamilyWhenTheSourceHasNoneKnown(t *testing.T) {
	profile := AnalyzeMachineProfile(map[string]string{
		"encoders": "h264_vaapi,libx264,libx265",
		"accels":   "vaapi",
	})
	video := &VideoSpecs{
		File:    "clip.mp4",
		Streams: []VideoStream{{Codec: "vp9", Width: 1920, Height: 1440, DurationFloat: 10, BitrateInt: 5000000}},
	}

	fallback := softwareEncoderForFallback(video, "h264_vaapi", profile)
	if fallback != "libx264" {
		t.Fatalf("expected libx264 for an unmodelled source codec, got %s", fallback)
	}
}

func TestSoftwareEncoderForFallback_SkipsWhatThisFFmpegDoesNotHave(t *testing.T) {
	// An ffmpeg built without libx265: the source family cannot be honoured, so
	// the failed encoder's family is the answer rather than nothing at all.
	profile := AnalyzeMachineProfile(map[string]string{
		"encoders": "h264_vaapi,libx264",
		"accels":   "vaapi",
	})

	fallback := softwareEncoderForFallback(hevcSource("yuv420p"), "h264_vaapi", profile)
	if fallback != "libx264" {
		t.Fatalf("expected libx264 when libx265 is absent, got %s", fallback)
	}
}

func TestDescribePlannedHardwarePath_ShowsWhatTheSwitchCosts(t *testing.T) {
	ffmpegInfo := map[string]string{
		"encoders": "h264_vaapi,libx264,libx265",
		"accels":   "vaapi",
	}

	line := DescribeHardwareAccelerationPlan(ffmpegInfo, hevcSource("yuv420p10le"), "")
	if !strings.Contains(line, "h264_vaapi") {
		t.Fatalf("expected the planned encoder in %q", line)
	}
	if !strings.Contains(line, "8-bit") {
		t.Fatalf("the window must show what the switch costs, got %q", line)
	}
}

func TestWithCodecSwitchNote_LeavesAnOrdinarySummaryAlone(t *testing.T) {
	summary := "Hardware: used hevc_vaapi encode + VAAPI decode"

	if got := withCodecSwitchNote(hevcSource("yuv420p10le"), "hevc_vaapi", summary); got != summary {
		t.Fatalf("expected the summary unchanged, got %q", got)
	}
}
