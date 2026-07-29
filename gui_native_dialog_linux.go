//go:build linux
// +build linux

package main

import (
	"errors"
	"os/exec"
	"strings"

	"superview/common"
)

var errNoNativeDialogTool = errors.New("no native dialog tool available (zenity/kdialog)")

func runCommandAndGetPath(name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	prepareNativeDialogCommand(cmd)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

// prepareNativeDialogCommand is a no-op on Linux: there is no equivalent to
// Windows' HideWindow SysProcAttr needed here, since zenity/kdialog do not
// spawn a console window in the first place.
func prepareNativeDialogCommand(cmd *exec.Cmd) {
}

func normalizeNativeDialogResult(path string, err error) (string, error) {
	return common.NormalizeNativeDialogResult(path, err)
}

// nativeFilePickerBinary returns the first available native dialog tool found
// on PATH, preferring zenity (GTK, most common) over kdialog (KDE).
// Returns "" if neither is installed.
func nativeFilePickerBinary() string {
	for _, bin := range []string{"zenity", "kdialog"} {
		if _, err := exec.LookPath(bin); err == nil {
			return bin
		}
	}
	return ""
}

func chooseInputFileNative() (string, error) {
	switch nativeFilePickerBinary() {
	case "zenity":
		path, err := runCommandAndGetPath("zenity", "--file-selection",
			"--title=Select input video",
			"--file-filter=Video Files | *.mp4 *.MP4 *.mov *.MOV *.mkv *.MKV *.avi *.AVI *.m4v *.M4V *.webm *.WEBM *.flv *.FLV *.wmv *.WMV *.mpeg *.MPEG *.mpg *.MPG",
		)
		return normalizeNativeDialogResult(path, err)
	case "kdialog":
		path, err := runCommandAndGetPath("kdialog", "--getopenfilename", ".",
			"*.mp4 *.mov *.mkv *.avi *.m4v *.webm *.flv *.wmv *.mpeg *.mpg|Video Files")
		return normalizeNativeDialogResult(path, err)
	default:
		return "", errNoNativeDialogTool
	}
}

func chooseOutputFileNative() (string, error) {
	switch nativeFilePickerBinary() {
	case "zenity":
		path, err := runCommandAndGetPath("zenity", "--file-selection", "--save",
			"--title=Save output video",
			"--filename=output.mp4",
			"--confirm-overwrite",
		)
		return normalizeNativeDialogResult(path, err)
	case "kdialog":
		path, err := runCommandAndGetPath("kdialog", "--getsavefilename", "output.mp4",
			"*.mp4|MP4 Video")
		return normalizeNativeDialogResult(path, err)
	default:
		return "", errNoNativeDialogTool
	}
}
