package common

import (
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
	Name       string // Check name (e.g., "ffmpeg", "disk_space")
	Healthy    bool   // True if check passed
	Message    string // Description or error details
	Value      string // Current value (e.g., "6.1.1", "150GB free")
	Timestamp  int64  // Unix timestamp when check was performed
}

// SystemHealth represents overall system health for encoding.
type SystemHealth struct {
	Overall    bool
	FFmpeg     HealthCheckResult
	FFprobe    HealthCheckResult
	Disk       HealthCheckResult
	Memory     HealthCheckResult
	CPU        HealthCheckResult
	AllChecks  []HealthCheckResult // All checks performed
}

// CheckHealth performs comprehensive system health checks.
// Returns detailed results for each system component required for encoding.
func CheckHealth() *SystemHealth {
	health := &SystemHealth{
		AllChecks: make([]HealthCheckResult, 0),
	}

	now := time.Now().Unix()

	// Check FFmpeg
	health.FFmpeg = checkFFmpegHealth(now)
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

	return health
}

// checkFFmpegHealth verifies ffmpeg availability and version.
func checkFFmpegHealth(timestamp int64) HealthCheckResult {
	result := HealthCheckResult{
		Name:      "ffmpeg",
		Timestamp: timestamp,
	}

	ffmpeg, err := CheckFfmpeg()
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

	// Warning threshold: less than 10GB free
	if tempFreeGB < 10 {
		result.Healthy = false
		result.Value = fmt.Sprintf("%.1f GB free", tempFreeGB)
		result.Message = fmt.Sprintf("Insufficient temp disk space: %.1f GB (minimum 10GB recommended)", tempFreeGB)
		return result
	}

	result.Healthy = true
	result.Value = fmt.Sprintf("%.1f GB free", tempFreeGB)
	result.Message = fmt.Sprintf("Sufficient disk space available in %s", tempDir)
	return result
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

	// Check system memory using /proc/meminfo if available
	if data, err := os.ReadFile("/proc/meminfo"); err == nil {
		// Parse meminfo for MemAvailable
		lines := strings.Split(string(data), "\n")
		for _, line := range lines {
			if strings.HasPrefix(line, "MemAvailable:") {
				parts := strings.Fields(line)
				if len(parts) >= 2 {
					if availKB, err := strconv.ParseFloat(parts[1], 64); err == nil {
						availGB := availKB / (1024 * 1024)
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
				}
			}
		}
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

	return report
}
