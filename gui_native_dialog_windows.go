//go:build windows
// +build windows

package main

import (
	"os/exec"
	"strings"
	"syscall"

	"superview/common"
)

func runCommandAndGetPath(name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	prepareNativeDialogCommand(cmd)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func prepareNativeDialogCommand(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
}

func normalizeNativeDialogResult(path string, err error) (string, error) {
	return common.NormalizeNativeDialogResult(path, err)
}

func chooseInputFileNative() (string, error) {
	script := strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.OpenFileDialog",
		"$dialog.Title = 'Select input video'",
		"$dialog.Filter = 'Video Files|*.mp4;*.MP4;*.mov;*.MOV;*.mkv;*.MKV;*.avi;*.AVI;*.m4v;*.M4V;*.webm;*.WEBM;*.flv;*.FLV;*.wmv;*.WMV;*.mpeg;*.MPEG;*.mpg;*.MPG|All Files|*.*'",
		"$dialog.CheckFileExists = $true",
		"$dialog.Multiselect = $false",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($dialog.FileName) }",
	}, "; ")
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script)
	return normalizeNativeDialogResult(path, runErr)
}

func chooseOutputFileNative() (string, error) {
	script := strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.SaveFileDialog",
		"$dialog.Title = 'Save output video'",
		"$dialog.Filter = 'MP4 Video|*.mp4|All Files|*.*'",
		"$dialog.DefaultExt = 'mp4'",
		"$dialog.AddExtension = $true",
		"$dialog.OverwritePrompt = $true",
		"$dialog.FileName = 'output.mp4'",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($dialog.FileName) }",
	}, "; ")
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script)
	return normalizeNativeDialogResult(path, runErr)
}
