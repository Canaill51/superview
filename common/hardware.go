package common

import (
	"runtime"
	"strings"
)

// MachineProfile describes runtime hardware/software capabilities detected from ffmpeg and host CPU.
type MachineProfile struct {
	CPUCores         int
	HardwareAccels   []string
	AvailableEncoders []string
}

// AnalyzeMachineProfile analyzes host and ffmpeg capabilities for encoder selection.
func AnalyzeMachineProfile(ffmpeg map[string]string) *MachineProfile {
	profile := &MachineProfile{
		CPUCores: runtime.NumCPU(),
	}

	if ffmpeg == nil {
		return profile
	}

	profile.HardwareAccels = splitCSV(ffmpeg["accels"])
	profile.AvailableEncoders = splitCSV(ffmpeg["encoders"])
	return profile
}

func splitCSV(value string) []string {
	if strings.TrimSpace(value) == "" {
		return []string{}
	}
	parts := strings.Split(value, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func toSet(items []string) map[string]bool {
	set := make(map[string]bool, len(items))
	for _, item := range items {
		set[item] = true
	}
	return set
}

func isHardwareEncoder(encoder string) bool {
	return strings.Contains(encoder, "_nvenc") || strings.Contains(encoder, "_qsv") || strings.Contains(encoder, "_vaapi") || strings.Contains(encoder, "_v4l2m2m")
}

func accelForEncoder(encoder string) string {
	switch {
	case strings.Contains(encoder, "_nvenc"):
		return "cuda"
	case strings.Contains(encoder, "_qsv"):
		return "qsv"
	case strings.Contains(encoder, "_vaapi"):
		return "vaapi"
	case strings.Contains(encoder, "_v4l2m2m"):
		return "drm"
	default:
		return ""
	}
}

func candidateEncodersForCodec(codec string) []string {
	switch strings.ToLower(codec) {
	case "h264", "avc":
		return []string{"h264_nvenc", "h264_qsv", "h264_vaapi", "h264_v4l2m2m", "libx264", "libx264rgb"}
	case "h265", "hevc":
		return []string{"hevc_nvenc", "hevc_qsv", "hevc_vaapi", "hevc_v4l2m2m", "libx265"}
	default:
		return []string{"libx264", "libx265"}
	}
}

func canUseEncoderWithProfile(encoder string, profile *MachineProfile) bool {
	if profile == nil {
		return false
	}

	encSet := toSet(profile.AvailableEncoders)
	if !encSet[encoder] {
		return false
	}

	if !isHardwareEncoder(encoder) {
		return true
	}

	requiredAccel := accelForEncoder(encoder)
	if requiredAccel == "" {
		return true
	}
	accelSet := toSet(profile.HardwareAccels)
	return accelSet[requiredAccel]
}
