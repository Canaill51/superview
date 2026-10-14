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

// writeChosenPathUTF8 hands the chosen path back to Go as UTF-8.
//
// [Console]::Out.Write encodes with Console.OutputEncoding, which Windows
// PowerShell 5.1 takes from the system locale -- an OEM code page (850 on a
// French Windows), not UTF-8. Go reads the bytes as UTF-8, so the "é" of
// C:\Users\Cédric came back as U+FFFD and the file the user had just clicked
// could not be found. An explicit UTF-8 writer on the raw standard output
// sidesteps the console encoding entirely; $false leaves out the byte-order
// mark, which would otherwise prefix the path.
const writeChosenPathUTF8 = "$w = New-Object System.IO.StreamWriter([Console]::OpenStandardOutput(), (New-Object System.Text.UTF8Encoding $false)); $w.Write($dialog.FileName); $w.Flush()"

// inputDialogScript is the PowerShell that shows the native open dialog.
func inputDialogScript() string {
	return strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.OpenFileDialog",
		"$dialog.Title = 'Select input video'",
		"$dialog.Filter = 'MP4 Video|*.mp4;*.MP4'",
		"$dialog.CheckFileExists = $true",
		"$dialog.Multiselect = $false",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { " + writeChosenPathUTF8 + " }",
	}, "; ")
}

// outputDialogScript is the PowerShell that shows the native save dialog.
func outputDialogScript() string {
	return strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.SaveFileDialog",
		"$dialog.Title = 'Save output video'",
		"$dialog.Filter = 'MP4 Video|*.mp4|All Files|*.*'",
		"$dialog.DefaultExt = 'mp4'",
		"$dialog.AddExtension = $true",
		"$dialog.OverwritePrompt = $true",
		"$dialog.FileName = 'output.mp4'",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { " + writeChosenPathUTF8 + " }",
	}, "; ")
}

func chooseInputFileNative() (string, error) {
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", inputDialogScript())
	return normalizeNativeDialogResult(path, runErr)
}

func chooseOutputFileNative() (string, error) {
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", outputDialogScript())
	return normalizeNativeDialogResult(path, runErr)
}
