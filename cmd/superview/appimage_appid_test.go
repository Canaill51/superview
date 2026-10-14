package main

import (
	"os"
	"regexp"
	"testing"
)

// On Wayland the window's app_id is the ID given to app.NewWithID, and the
// compositor finds the taskbar and title-bar icon through a desktop file of
// exactly that name. The AppImage script must therefore build the same ID, or
// KDE shows its generic icon (measured 2026-10-14, DEC-41).
func TestAppImageDesktopFileIsNamedAfterTheAppID(t *testing.T) {
	src, err := os.ReadFile("gui_main.go")
	if err != nil {
		t.Fatal(err)
	}
	m := regexp.MustCompile(`app\.NewWithID\("([^"]+)"\)`).FindSubmatch(src)
	if m == nil {
		t.Fatal("app.NewWithID(\"…\") not found in gui_main.go")
	}
	appID := string(m[1])

	script, err := os.ReadFile("../../packaging/linux/build_appimage.sh")
	if err != nil {
		t.Fatal(err)
	}
	want := `APP_ID="` + appID + `"`
	if !regexp.MustCompile(regexp.QuoteMeta(want)).Match(script) {
		t.Fatalf("build_appimage.sh must set %s so its desktop file is named after the GUI's application ID", want)
	}
}
