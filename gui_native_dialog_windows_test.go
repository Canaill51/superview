//go:build windows

package main

import (
	"strings"
	"testing"
)

// TestNativeDialogScriptsWriteUTF8 pins E-12: both pickers must hand the path
// back through an explicit UTF-8 writer. [Console]::Out.Write follows the
// console's OEM code page, and a path such as C:\Users\Cédric then reaches Go
// with U+FFFD in place of the accent.
func TestNativeDialogScriptsWriteUTF8(t *testing.T) {
	for name, script := range map[string]string{
		"input":  inputDialogScript(),
		"output": outputDialogScript(),
	} {
		if !strings.Contains(script, "UTF8Encoding") {
			t.Errorf("%s dialog script does not write the path as UTF-8:\n%s", name, script)
		}
		if strings.Contains(script, "[Console]::Out.Write") {
			t.Errorf("%s dialog script still writes through the console encoding:\n%s", name, script)
		}
	}
}
