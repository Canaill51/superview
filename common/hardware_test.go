package common

import "testing"

func TestAnalyzeMachineProfile(t *testing.T) {
	ffmpeg := map[string]string{
		"accels":   "cuda,vaapi",
		"encoders": "h264_nvenc,libx264",
	}
	profile := AnalyzeMachineProfile(ffmpeg)
	if profile.CPUCores <= 0 {
		t.Fatalf("expected CPU cores > 0, got %d", profile.CPUCores)
	}
	if len(profile.HardwareAccels) != 2 {
		t.Fatalf("unexpected hwaccels: %+v", profile.HardwareAccels)
	}
	if len(profile.AvailableEncoders) != 2 {
		t.Fatalf("unexpected encoders: %+v", profile.AvailableEncoders)
	}
}

func TestCanUseEncoderWithProfile(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels:    []string{"cuda"},
		AvailableEncoders: []string{"h264_nvenc", "libx264"},
	}
	if !canUseEncoderWithProfile("h264_nvenc", profile) {
		t.Fatal("expected h264_nvenc to be usable")
	}
	if !canUseEncoderWithProfile("libx264", profile) {
		t.Fatal("expected libx264 to be usable")
	}
	if canUseEncoderWithProfile("hevc_qsv", profile) {
		t.Fatal("expected hevc_qsv to be unusable")
	}
}
