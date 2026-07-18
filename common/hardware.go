package common

import (
	"fmt"
	"runtime"
	"strings"
)

// MachineProfile describes runtime hardware/software capabilities detected from ffmpeg and host CPU.
type MachineProfile struct {
	CPUCores          int
	HardwareAccels    []string
	AvailableEncoders []string
}

// HardwarePlan describes the encoder and decode path Superview expects to use.
type HardwarePlan struct {
	Encoder        string
	DecodeAccel    string
	HardwareEncode bool
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
	return strings.Contains(encoder, "_nvenc") || strings.Contains(encoder, "_amf") || strings.Contains(encoder, "_qsv") || strings.Contains(encoder, "_vaapi") || strings.Contains(encoder, "_v4l2m2m")
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

func decodeAccelCandidatesForEncoder(encoder string) []string {
	switch {
	case strings.Contains(encoder, "_nvenc"):
		return []string{"cuda", "d3d11va", "dxva2"}
	case strings.Contains(encoder, "_amf"):
		return []string{"d3d11va", "dxva2"}
	case strings.Contains(encoder, "_qsv"):
		return []string{"qsv", "d3d11va", "dxva2"}
	default:
		requiredAccel := accelForEncoder(encoder)
		if requiredAccel == "" {
			return []string{}
		}
		return []string{requiredAccel}
	}
}

func canUseDedicatedHardwareEncoderWithoutMatchingAccel(encoder string) bool {
	return strings.Contains(encoder, "_nvenc") || strings.Contains(encoder, "_amf") || strings.Contains(encoder, "_qsv")
}

func selectHardwareDecodeAccel(encoder string, profile *MachineProfile) string {
	if profile == nil {
		return ""
	}

	accelSet := toSet(profile.HardwareAccels)
	for _, accel := range decodeAccelCandidatesForEncoder(encoder) {
		if accelSet[accel] {
			return accel
		}
	}

	return ""
}

func describePlannedHardwarePath(plan HardwarePlan) string {
	if plan.Encoder == "" {
		return "Hardware: no compatible encoder selected"
	}

	if !plan.HardwareEncode {
		return fmt.Sprintf("Hardware: planned CPU encode (%s) + CPU decode", plan.Encoder)
	}

	if plan.DecodeAccel != "" {
		return fmt.Sprintf("Hardware: planned %s encode + %s decode", plan.Encoder, strings.ToUpper(plan.DecodeAccel))
	}

	return fmt.Sprintf("Hardware: planned %s encode + CPU decode fallback", plan.Encoder)
}

// BuildHardwarePlan determines the most likely encoder and decode path for the current input.
func BuildHardwarePlan(ffmpeg map[string]string, video *VideoSpecs, requestedEncoder string) (HardwarePlan, error) {
	if video == nil || len(video.Streams) == 0 {
		return HardwarePlan{}, &InvalidVideoError{Reason: "no video streams"}
	}

	encoder, err := FindEncoder(requestedEncoder, ffmpeg, video)
	if err != nil {
		return HardwarePlan{}, err
	}

	profile := AnalyzeMachineProfile(ffmpeg)
	return HardwarePlan{
		Encoder:        encoder,
		DecodeAccel:    selectHardwareDecodeAccel(encoder, profile),
		HardwareEncode: isHardwareEncoder(encoder),
	}, nil
}

// DescribeHardwareAccelerationPlan returns a user-facing summary of the planned hardware path.
func DescribeHardwareAccelerationPlan(ffmpeg map[string]string, video *VideoSpecs, requestedEncoder string) string {
	if video == nil {
		return "Hardware: waiting for input video"
	}

	plan, err := BuildHardwarePlan(ffmpeg, video, requestedEncoder)
	if err != nil {
		return fmt.Sprintf("Hardware: %s", err.Error())
	}

	return describePlannedHardwarePath(plan)
}

func candidateEncodersForCodec(codec string) []string {
	switch strings.ToLower(codec) {
	case "h264", "avc":
		return []string{"h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi", "h264_v4l2m2m", "libx264", "libx264rgb"}
	case "h265", "hevc":
		return []string{"hevc_nvenc", "hevc_amf", "hevc_qsv", "hevc_vaapi", "hevc_v4l2m2m", "libx265"}
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

	if canUseDedicatedHardwareEncoderWithoutMatchingAccel(encoder) {
		return true
	}

	requiredAccel := accelForEncoder(encoder)
	if requiredAccel == "" {
		return true
	}
	accelSet := toSet(profile.HardwareAccels)
	return accelSet[requiredAccel]
}
