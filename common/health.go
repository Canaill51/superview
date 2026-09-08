package common

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// HealthCheckResult represents the result of a single health check.
type HealthCheckResult struct {
	Name      string // Check name (e.g., "ffmpeg", "disk_space")
	Healthy   bool   // True if check passed
	Message   string // Description or error details
	Value     string // Current value (e.g., "6.1.1", "150GB free")
	Timestamp int64  // Unix timestamp when check was performed
}

// SystemHealth represents overall system health for encoding.
type SystemHealth struct {
	Overall   bool
	FFmpeg    HealthCheckResult
	FFprobe   HealthCheckResult
	Disk      HealthCheckResult
	Memory    HealthCheckResult
	CPU       HealthCheckResult
	AllChecks []HealthCheckResult // All checks performed

	// Encoders holds what each advertised encoder answered when asked to
	// encode a frame. It is not one of AllChecks and does not affect Overall:
	// a machine with no working hardware encoder is not unhealthy, it is a
	// machine that will encode on the CPU. But it is the section that explains
	// why, and the report exists to be attached to bug reports.
	Encoders *EncoderProbeReport
}

// CheckHealth performs comprehensive system health checks.
// Returns detailed results for each system component required for encoding.
// cfg is forwarded to the ffmpeg probe; nil means the built-in defaults.
func CheckHealth(cfg *Config) *SystemHealth {
	health := &SystemHealth{
		AllChecks: make([]HealthCheckResult, 0),
	}

	now := time.Now().Unix()

	// Check FFmpeg
	health.FFmpeg = checkFFmpegHealth(cfg, now)
	health.AllChecks = append(health.AllChecks, health.FFmpeg)

	// Check FFprobe
	health.FFprobe = checkFFprobeHealth(now)
	health.AllChecks = append(health.AllChecks, health.FFprobe)

	// Check Disk space
	health.Disk = checkDiskSpaceHealth(now)
	health.AllChecks = append(health.AllChecks, health.Disk)

	// Check Memory
	health.Memory = checkMemoryHealth(now)
	health.AllChecks = append(health.AllChecks, health.Memory)

	// Check CPU
	health.CPU = checkCPUHealth(now)
	health.AllChecks = append(health.AllChecks, health.CPU)

	// Overall health: all critical checks must pass
	health.Overall = health.FFmpeg.Healthy && health.FFprobe.Healthy && health.Disk.Healthy

	// Probing needs an ffmpeg to run; without one there is nothing to ask.
	if health.FFmpeg.Healthy {
		if ffmpeg, err := CheckFfmpeg(cfg); err == nil {
			health.Encoders = ProbeHardwareSupport(context.Background(), ffmpeg)
		}
	}

	return health
}

// checkFFmpegHealth verifies ffmpeg availability and version.
func checkFFmpegHealth(cfg *Config, timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "ffmpeg",
		Timestamp: timestamp,
	}

	ffmpeg, err := CheckFfmpeg(cfg)
	if err != nil {
		result.Healthy = false
		result.Message = err.Error()
		return result
	}

	if version, ok := ffmpeg["version"]; ok && version != "" {
		result.Healthy = true
		result.Value = version
		result.Message = fmt.Sprintf("FFmpeg %s available", version)
	} else {
		result.Healthy = false
		result.Message = "Could not determine FFmpeg version"
	}

	return result
}

// checkFFprobeHealth verifies ffprobe availability.
func checkFFprobeHealth(timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "ffprobe",
		Timestamp: timestamp,
	}

	// Try to run ffprobe -version
	cmd := newFFprobeCommand("-version")
	prepareBackgroundCommand(cmd)
	output, err := cmd.CombinedOutput()

	if err != nil {
		result.Healthy = false
		result.Message = "FFprobe not found or failed"
	} else {
		// Extract version from first line
		lines := strings.Split(string(output), "\n")
		if len(lines) > 0 {
			result.Healthy = true
			result.Value = lines[0]
			result.Message = "FFprobe available"
		}
	}

	return result
}

// minimumTempFreeGB is the floor below which the temp directory is worth
// reporting. See checkDiskSpaceHealth for why it is not larger.
const minimumTempFreeGB = 1

// checkDiskSpaceHealth verifies sufficient disk space for encoding operations.
// Checks both temp directory and output directory.
func checkDiskSpaceHealth(timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "disk_space",
		Timestamp: timestamp,
	}

	// Check temp directory
	tempDir := os.TempDir()
	tempFreeGB, err := getFreeDiskGB(tempDir)
	if err != nil {
		result.Healthy = false
		result.Message = fmt.Sprintf("Could not check temp disk: %v", err)
		return result
	}

	// The temp directory holds the two remap maps and nothing else: the output
	// is written next to the destination the user picked. Even the largest frame
	// this pipeline handles -- a 5.3K 4:3 source widened to 7082x3984 -- makes
	// about 113 MB of maps, so a floor of 1 GB is already generous.
	//
	// It used to be 10 GB, a number no per-conversion need justified and that a
	// tmpfs cannot satisfy: current distributions size /tmp at half the RAM, so
	// an 8 GiB machine reported UNHEALTHY for its entire life while holding
	// thirty times what the conversion needed. The check that actually decides
	// is checkTempSpaceForMaps, which knows the geometry being produced.
	if tempFreeGB < minimumTempFreeGB {
		result.Healthy = false
		result.Value = fmt.Sprintf("%.1f GB free", tempFreeGB)
		result.Message = fmt.Sprintf("Very little space in %s: %.1f GB, and the remap maps are written there", tempDir, tempFreeGB)
		return result
	}

	result.Healthy = true
	result.Value = fmt.Sprintf("%.1f GB free", tempFreeGB)
	result.Message = fmt.Sprintf("Sufficient disk space available in %s", tempDir)
	return result
}

// unknownMemory is what the memory helpers report where the reading cannot be
// taken. It is a word rather than a zero on purpose: "0 MB available" and "we
// could not look" lead to opposite decisions.
const unknownMemory = "unknown"

// availableMemoryBytes reports how much memory the kernel says it can hand out
// without swapping.
//
// MemAvailable, not MemFree. The free figure leaves out the page cache, which
// the kernel reclaims on demand, so reading it makes a healthy desktop that has
// been up for a day look like it is out of memory. MemAvailable is the kernel's
// own estimate of what an allocation can actually get.
//
// The second return value is false where the reading cannot be taken -- no
// /proc/meminfo, which includes every Windows machine. Callers must have an
// answer for "unknown": reporting an encode as impossible because the question
// could not be asked would be worse than letting it try.
func availableMemoryBytes() (uint64, bool) {
	data, err := os.ReadFile("/proc/meminfo")
	if err != nil {
		return 0, false
	}
	return parseMemAvailable(data)
}

// parseMemAvailable pulls the MemAvailable line out of /proc/meminfo's contents.
//
// Separate from the file read so the rejections can be tested: a file without
// the line, a truncated line, a value that is not a number. Reading the real
// /proc/meminfo can only ever exercise the success path, and a parser whose
// failure paths are never run is a parser whose failure paths are guesses.
func parseMemAvailable(data []byte) (uint64, bool) {
	for _, line := range strings.Split(string(data), "\n") {
		if !strings.HasPrefix(line, "MemAvailable:") {
			continue
		}
		// "MemAvailable:   3894132 kB" -- the value is always in kB, and the
		// unit is part of the format rather than something that varies.
		fields := strings.Fields(line)
		if len(fields) < 2 {
			return 0, false
		}
		kb, err := strconv.ParseUint(fields[1], 10, 64)
		if err != nil {
			return 0, false
		}
		return kb * 1024, true
	}

	return 0, false
}

// formatAvailableMemory renders the reading for a log line or a message, or the
// word unknownMemory when there is none.
func formatAvailableMemory() string {
	available, ok := availableMemoryBytes()
	if !ok {
		return unknownMemory
	}
	return fmt.Sprintf("%.1f GB", float64(available)/(1024*1024*1024))
}

// checkMemoryHealth verifies sufficient system memory for encoding.
func checkMemoryHealth(timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "memory",
		Timestamp: timestamp,
	}

	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	allocMB := float64(m.Alloc) / (1024 * 1024)
	totalMB := float64(m.TotalAlloc) / (1024 * 1024)
	sysMB := float64(m.Sys) / (1024 * 1024)

	// Check system memory using the kernel's own estimate, where there is one.
	if available, ok := availableMemoryBytes(); ok {
		availGB := float64(available) / (1024 * 1024 * 1024)
		result.Value = fmt.Sprintf("%.1f GB available (Proc: %.0f MB alloc, %.0f MB total)", availGB, allocMB, totalMB)

		if availGB < 1.0 {
			result.Healthy = false
			result.Message = fmt.Sprintf("Low memory available: %.1f GB", availGB)
		} else {
			result.Healthy = true
			result.Message = fmt.Sprintf("Sufficient memory: %.1f GB available", availGB)
		}
		return result
	}

	// Fallback to runtime stats
	result.Healthy = true
	result.Value = fmt.Sprintf("Alloc: %.0f MB, Total: %.0f MB, Sys: %.0f MB", allocMB, totalMB, sysMB)
	result.Message = "Memory available for encoding"
	return result
}

// checkCPUHealth verifies CPU availability.
func checkCPUHealth(timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "cpu",
		Timestamp: timestamp,
	}

	numCPU := runtime.NumCPU()
	result.Value = fmt.Sprintf("%d CPU cores", numCPU)

	if numCPU >= 1 {
		result.Healthy = true
		result.Message = fmt.Sprintf("CPU cores available: %d", numCPU)
	} else {
		result.Healthy = false
		result.Message = "No CPU cores detected"
	}

	return result
}

// LogHealth logs system health status.
func LogHealth(logger *slog.Logger, health *SystemHealth) {
	if health == nil {
		return
	}

	status := "✅ HEALTHY"
	if !health.Overall {
		status = "❌ UNHEALTHY"
	}

	logger.Info(status,
		slog.Bool("overall", health.Overall),
		slog.Bool("ffmpeg", health.FFmpeg.Healthy),
		slog.Bool("ffprobe", health.FFprobe.Healthy),
		slog.Bool("disk", health.Disk.Healthy),
		slog.Bool("memory", health.Memory.Healthy),
		slog.Bool("cpu", health.CPU.Healthy),
	)

	for _, probe := range health.Encoders.Unusable() {
		logger.Warn("encoder advertised but unusable",
			slog.String("encoder", probe.Encoder),
			slog.String("reason", probe.Reason),
		)
	}

	// Log details for failed checks
	for _, check := range health.AllChecks {
		if !check.Healthy {
			logger.Warn(check.Name+" check failed",
				slog.String("message", check.Message),
				slog.String("value", check.Value),
			)
		}
	}
}

// GetHealthReport returns a formatted health report as string.
func GetHealthReport(health *SystemHealth) string {
	if health == nil {
		return "No health data available"
	}

	report := "=== System Health Check ===\n"
	if health.Overall {
		report += "Status: ✅ HEALTHY\n\n"
	} else {
		report += "Status: ❌ UNHEALTHY\n\n"
	}

	for _, check := range health.AllChecks {
		status := "✅"
		if !check.Healthy {
			status = "❌"
		}
		report += fmt.Sprintf("%s %s: %s\n", status, strings.ToUpper(check.Name), check.Message)
		if check.Value != "" {
			report += fmt.Sprintf("   Value: %s\n", check.Value)
		}
	}

	if health.Encoders != nil {
		report += "\n" + DescribeEncoderProbe(health.Encoders)
	}

	return report
}
