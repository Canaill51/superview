//go:build !windows
// +build !windows

package common

import "os/exec"

func prepareBackgroundCommand(cmd *exec.Cmd) {
}
