package common

import (
	"errors"
	"os/exec"
	"strings"
)

// NormalizeNativeDialogResult normalizes the output of a native OS file dialog command.
// Returns the trimmed path, empty string on cancellation, or an error on unexpected failure.
//
// Only exit code 1 is a cancellation. zenity(1), EXIT CODES: 0 returned
// normally, 1 the user pressed Cancel or closed the dialog, -1 an unexpected
// error occurred (255 once the shell has made it unsigned), 5 the dialog timed
// out. 255 used to be read as a cancellation too, so a broken zenity gave a
// click with no effect and a line at debug level -- and the Fyne dialog that
// exists as the fallback for exactly that case never opened. Returning the
// error is what opens it.
func NormalizeNativeDialogResult(path string, err error) (string, error) {
	if err == nil {
		return strings.TrimSpace(path), nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) && exitErr.ExitCode() == 1 {
		return "", nil
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
