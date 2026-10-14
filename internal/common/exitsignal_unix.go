//go:build !windows

package common

import (
	"errors"
	"os/exec"
	"syscall"
)

// killedBySignal reports the signal that ended a process, when its exit status
// says a signal ended it.
//
// Split by platform because the answer lives in a Unix-only field of the wait
// status. Reading it through os/exec's own String() would work too, but that
// method formats a whole sentence ("signal: killed") whose shape is not part of
// any contract; the wait status names the signal and nothing else.
func killedBySignal(err error) (string, bool) {
	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) {
		return "", false
	}

	status, ok := exitErr.Sys().(syscall.WaitStatus)
	if !ok || !status.Signaled() {
		return "", false
	}

	return status.Signal().String(), true
}
