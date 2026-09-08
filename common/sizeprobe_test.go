package common

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// hd620Refusal is what an Intel HD 620 answers when asked to encode the frame a
// 4:3 4K clip widens to. It is the message this whole check exists for.
const hd620Refusal = "[h264_vaapi @ 0x55] Hardware does not support encoding at size 5120x2880 (constraints: width 32-4096 height 32-4096)"

// vaapiMachine is a machine whose only usable hardware encoder is H.264 VAAPI,
// with both CPU encoders present -- the laptop from the report.
var vaapiMachine = map[string]string{
	"encoders": "h264_vaapi,libx264,libx265",
	"accels":   "vaapi",
}

// freshSizeProbeCache empties the cache for one test and puts it back after.
func freshSizeProbeCache(t *testing.T) {
	t.Helper()
	resetSizeProbeCache()
	t.Cleanup(resetSizeProbeCache)
}

// seedSizeProbe records a verdict the driver would have given, so the decision
// this file is about can be tested without a GPU that refuses 15-megapixel
// frames -- which no CI runner has.
func seedSizeProbe(t *testing.T, encoder, size string, usable bool, reason string) {
	t.Helper()

	sizeProbeMu.Lock()
	defer sizeProbeMu.Unlock()
	sizeProbeCache[encoder+"@"+size] = EncoderProbe{Encoder: encoder, Usable: usable, Reason: reason}
}

func TestProbeArgs_UsesTheSizeItIsGiven(t *testing.T) {
	joined := strings.Join(probeArgs("h264_vaapi", "5120x2880"), " ")

	if !strings.Contains(joined, "nullsrc=s=5120x2880") {
		t.Fatalf("the probe must encode the size it was given, got: %s", joined)
	}
	if strings.Contains(joined, probeFrameSize) {
		t.Fatalf("the startup size leaked into a sized probe: %s", joined)
	}
}

func TestOutputFrameSize(t *testing.T) {
	video := hevcSource("yuv420p10le") // 3840x2880

	if got := outputFrameSize(video, false); got != "5120x2880" {
		t.Errorf("a widened 4:3 frame is 5120x2880, got %q", got)
	}
	// Already stretched: the frame keeps its width, the centre is un-squeezed.
	if got := outputFrameSize(video, true); got != "3840x2880" {
		t.Errorf("an un-squeezed source keeps its width, got %q", got)
	}
	if got := outputFrameSize(nil, false); got != "" {
		t.Errorf("no video means no size, got %q", got)
	}
	if got := outputFrameSize(&VideoSpecs{File: "x.mp4"}, false); got != "" {
		t.Errorf("no streams means no size, got %q", got)
	}
}

// TestVerifyEncoderForOutput_LeavesSoftwareEncodersAlone keeps the probe off the
// path where it can only cost time: a CPU encoder has no device to consult.
func TestVerifyEncoderForOutput_LeavesSoftwareEncodersAlone(t *testing.T) {
	freshSizeProbeCache(t)

	encoder, reason := verifyEncoderForOutput(context.Background(), "libx265", hevcSource("yuv420p10le"), false, vaapiMachine)

	if encoder != "libx265" || reason != "" {
		t.Fatalf("expected libx265 untouched, got %q / %q", encoder, reason)
	}

	sizeProbeMu.Lock()
	defer sizeProbeMu.Unlock()
	if len(sizeProbeCache) != 0 {
		t.Fatalf("a software encoder must not be probed, cache holds %v", sizeProbeCache)
	}
}

func TestVerifyEncoderForOutput_KeepsAHardwareEncoderThatAcceptsTheFrame(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "5120x2880", true, "")

	encoder, reason := verifyEncoderForOutput(context.Background(), "h264_vaapi", hevcSource("yuv420p10le"), false, vaapiMachine)

	if encoder != "h264_vaapi" {
		t.Fatalf("the hardware encoder accepted the frame and must be kept, got %q", encoder)
	}
	if reason != "" {
		t.Fatalf("nothing changed, so there is nothing to explain: %q", reason)
	}
}

// TestVerifyEncoderForOutput_StepsBackToTheSourceFamilyWhenRefused is the
// measured case. Before this, the machine selected h264_vaapi, told the user
// H.264, failed twice inside EncodeVideo and landed on libx265 -- whose memory
// the guard had declined to estimate because it had been shown a hardware
// encoder.
func TestVerifyEncoderForOutput_StepsBackToTheSourceFamilyWhenRefused(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "5120x2880", false, hd620Refusal)

	encoder, reason := verifyEncoderForOutput(context.Background(), "h264_vaapi", hevcSource("yuv420p10le"), false, vaapiMachine)

	if encoder != "libx265" {
		t.Fatalf("expected the source family's CPU encoder, got %q", encoder)
	}
	for _, want := range []string{"h264_vaapi", "5120x2880", "constraints: width 32-4096"} {
		if !strings.Contains(reason, want) {
			t.Fatalf("the explanation must carry %q, got %q", want, reason)
		}
	}
}

// TestVerifyEncoderForOutput_KeepsTheEncoderWithNothingToStepBackTo covers an
// ffmpeg built without the CPU encoders. Returning "" there would leave the
// caller with no encoder at all; the cascade in EncodeVideo is then the only
// thing left, exactly as before this existed.
func TestVerifyEncoderForOutput_KeepsTheEncoderWithNothingToStepBackTo(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "5120x2880", false, hd620Refusal)

	bare := map[string]string{"encoders": "h264_vaapi", "accels": "vaapi"}
	encoder, reason := verifyEncoderForOutput(context.Background(), "h264_vaapi", hevcSource("yuv420p10le"), false, bare)

	if encoder != "h264_vaapi" {
		t.Fatalf("with no CPU encoder to fall back to the choice stands, got %q", encoder)
	}
	if reason == "" {
		t.Fatal("the refusal must still be reported even when nothing changes")
	}
}

// TestVerifyEncoderForOutput_FollowsTheSqueezeGeometry: the checkbox changes the
// width of the output frame, and width is exactly what the hardware refuses on.
// A verdict cached for one geometry must not answer for the other.
func TestVerifyEncoderForOutput_FollowsTheSqueezeGeometry(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "3840x2880", true, "")
	seedSizeProbe(t, "h264_vaapi", "5120x2880", false, hd620Refusal)

	video := hevcSource("yuv420p10le")

	if encoder, _ := verifyEncoderForOutput(context.Background(), "h264_vaapi", video, true, vaapiMachine); encoder != "h264_vaapi" {
		t.Fatalf("3840x2880 is accepted, so the hardware encoder stands, got %q", encoder)
	}
	if encoder, _ := verifyEncoderForOutput(context.Background(), "h264_vaapi", video, false, vaapiMachine); encoder != "libx265" {
		t.Fatalf("5120x2880 is refused, so the CPU encoder takes over, got %q", encoder)
	}
}

func TestDescribeVerifiedHardwarePlan_NamesWhatWillReallyRun(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "5120x2880", false, hd620Refusal)

	line := DescribeVerifiedHardwarePlan(context.Background(), vaapiMachine, hevcSource("yuv420p10le"), false, "")

	if !strings.Contains(line, "libx265") {
		t.Fatalf("the window must name the encoder that will run, got %q", line)
	}
	if strings.Contains(line, "10-bit source is stored as 8-bit") {
		t.Fatalf("the conversion stays in H.265, so nothing is downgraded: %q", line)
	}
	if !strings.Contains(line, "5120x2880") {
		t.Fatalf("the reason the hardware was dropped must be visible: %q", line)
	}
}

func TestDescribeVerifiedHardwarePlan_UnchangedWhenTheHardwareAccepts(t *testing.T) {
	freshSizeProbeCache(t)
	seedSizeProbe(t, "h264_vaapi", "5120x2880", true, "")

	line := DescribeVerifiedHardwarePlan(context.Background(), vaapiMachine, hevcSource("yuv420p10le"), false, "")

	if !strings.Contains(line, "h264_vaapi") {
		t.Fatalf("an accepted hardware encoder must still be announced, got %q", line)
	}
	if !strings.Contains(line, "8-bit") {
		t.Fatalf("the codec-switch note must survive verification, got %q", line)
	}
}

// fakeRefusingFFmpeg puts an ffmpeg on PATH that records the argv of every call
// and refuses the work.
//
// The argv matters as much as the count: it is how a test can say which encoder
// the conversion was actually asked to use, which is the whole subject here.
func fakeRefusingFFmpeg(t *testing.T, dir, ledger string) {
	t.Helper()

	name, mode := "ffmpeg", os.FileMode(0o755)
	script := "#!/bin/sh\n" +
		"echo \"call $*\" >> '" + ledger + "'\n" +
		"echo 'Hardware does not support encoding at size' 1>&2\n" +
		"exit 1\n"
	if runtime.GOOS == "windows" {
		name, mode = "ffmpeg.bat", os.FileMode(0o644)
		script = "@echo off\r\n" +
			"echo call %* >> \"" + ledger + "\"\r\n" +
			"echo Hardware does not support encoding at size 1>&2\r\n" +
			"exit /b 1\r\n"
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), mode); err != nil {
		t.Fatalf("failed to create the stand-in ffmpeg: %v", err)
	}

	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Cleanup(func() { toolResolveCache.Delete("ffmpeg") })
	toolResolveCache.Delete("ffmpeg")
}

// TestSizeProbeCache_AsksTheDriverOnce is what makes the window able to ask this
// question on every checkbox and dropdown click. Without the cache each of those
// would spend a process start.
func TestSizeProbeCache_AsksTheDriverOnce(t *testing.T) {
	freshSizeProbeCache(t)

	dir := t.TempDir()
	ledger := filepath.Join(dir, "calls.txt")
	fakeRefusingFFmpeg(t, dir, ledger)

	video := hevcSource("yuv420p10le")
	for i := 0; i < 3; i++ {
		if encoder, _ := verifyEncoderForOutput(context.Background(), "h264_vaapi", video, false, vaapiMachine); encoder != "libx265" {
			t.Fatalf("call %d: expected the step-back, got %q", i, encoder)
		}
	}

	data, err := os.ReadFile(ledger)
	if err != nil {
		t.Fatalf("the stand-in ffmpeg was never run: %v", err)
	}
	if calls := strings.Count(string(data), "call"); calls != 1 {
		t.Fatalf("the driver was asked %d times for the same question, want 1", calls)
	}
}
