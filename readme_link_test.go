package main

import (
	"net/url"
	"os"
	"strings"
	"testing"
)

// readmeHeadingSlugs returns the GitHub anchor of every Markdown heading in
// path, skipping fenced code blocks so that a "#" opening a comment inside one
// is not mistaken for a heading.
func readmeHeadingSlugs(t *testing.T, path string) map[string]bool {
	t.Helper()

	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("cannot read %s: %v", path, err)
	}

	slugs := make(map[string]bool)
	inFence := false
	for _, line := range strings.Split(string(raw), "\n") {
		if strings.HasPrefix(strings.TrimSpace(line), "```") {
			inFence = !inFence
			continue
		}
		if inFence || !strings.HasPrefix(line, "#") {
			continue
		}
		title := strings.TrimSpace(strings.TrimLeft(line, "#"))
		if title == "" {
			continue
		}
		slugs[githubSlug(title)] = true
	}
	return slugs
}

// githubSlug mirrors how GitHub turns a heading into a fragment: lower-case,
// punctuation dropped, spaces to hyphens. Letters keep their accents, which is
// why this cannot simply strip everything above ASCII.
func githubSlug(title string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(title) {
		switch {
		case r == ' ':
			b.WriteRune('-')
		case r == '-' || r == '_':
			b.WriteRune(r)
		case r >= '0' && r <= '9':
			b.WriteRune(r)
		case r >= 'a' && r <= 'z':
			b.WriteRune(r)
		case r > 127:
			b.WriteRune(r)
		}
	}
	return b.String()
}

// The application hands this URL to a user who has no ffmpeg -- in a hyperlink
// widget (gui_main.go) and in the error text (common/common.go). A fragment
// that matches no heading does not fail: it silently drops the reader at the
// top of the README, which is the one reader who cannot afford to go hunting.
// Renaming a README section is exactly how that happens, and nothing else
// catches it: Markdown is not compiled.
func TestInstallURLFragmentPointsAtARealHeading(t *testing.T) {
	parsed, err := url.Parse(installURL)
	if err != nil {
		t.Fatalf("installURL does not parse: %v", err)
	}

	fragment := parsed.Fragment
	if fragment == "" {
		t.Fatal("installURL has no fragment; it must land on the install section, not the top of the README")
	}

	slugs := readmeHeadingSlugs(t, "README.md")
	if !slugs[fragment] {
		have := make([]string, 0, len(slugs))
		for s := range slugs {
			have = append(have, s)
		}
		t.Fatalf("installURL points at #%s, which is not a heading of README.md.\n"+
			"Either the section was renamed or the constant was not updated with it.\n"+
			"Headings present: %v", fragment, have)
	}
}
