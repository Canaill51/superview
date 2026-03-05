package common

import (
	"errors"
	"os/exec"
	"strings"
)

// NormalizeNativeDialogResult normalizes the output of a native OS file dialog command.
// A non-zero exit code of 1 or 255 indicates cancellation by the user (not an error).
// Returns the trimmed path, empty string on cancellation, or an error on unexpected failure.
func NormalizeNativeDialogResult(path string, err error) (string, error) {
	if err == nil {
		return strings.TrimSpace(path), nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		if exitErr.ExitCode() == 1 || exitErr.ExitCode() == 255 {
			return "", nil
		}
	}

	return "", err
}

// ParseEncoderSelection parses the encoder name from a GUI dropdown selection string.
// An empty or "Use same video codec as input file" selection returns "".
// Otherwise, the first whitespace-delimited token is returned as the encoder name.
func ParseEncoderSelection(selected string) string {
	selected = strings.TrimSpace(selected)
	if selected == "" || selected == "Use same video codec as input file" {
		return ""
	}
	parts := strings.Fields(selected)
	if len(parts) == 0 {
		return ""
	}
	return parts[0]
}
