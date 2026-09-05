package common

import (
	"os"
	"testing"
)

// requireFFmpegEnv is the environment variable CI sets to say "ffmpeg is
// installed here, so a test that wants it must run".
const requireFFmpegEnv = "SUPERVIEW_REQUIRE_FFMPEG"

// skipWithoutFFmpeg ends a test that cannot run because ffmpeg, one of its
// encoders, or a fixture it renders is unavailable.
//
// It skips by default, so a checkout without ffmpeg still gets a usable suite.
// Under SUPERVIEW_REQUIRE_FFMPEG=1 it fails instead.
//
// The distinction matters because a skip is invisible. Every test that shells
// out to ffmpeg guards itself this way, so on a machine where ffmpeg is missing
// -- or present but built without libx264, or unable to render the 10-bit
// multi-track fixture -- the four integration tests and the remap equivalence
// test all report success having encoded nothing. That is the whole of what
// checks a real conversion end to end.
//
// CI installs ffmpeg on both runners, but installing it and using it are not
// the same claim: choco has reported success while leaving nothing on PATH,
// which is why test.yml runs "ffmpeg -version" after the install. The only
// signal that the tests then actually ran was the coverage number, and only
// indirectly -- 59.3% with ffmpeg against 51.4% without, either side of a 50%
// gate. A release could ship on the strength of a suite that skipped every
// test capable of catching an encoding defect.
//
// So CI sets the variable and the silence becomes a failure.
func skipWithoutFFmpeg(t *testing.T, format string, args ...any) {
	t.Helper()

	if os.Getenv(requireFFmpegEnv) == "1" {
		t.Fatalf("%s=1 but this test cannot run: "+format,
			append([]any{requireFFmpegEnv}, args...)...)
	}
	t.Skipf(format, args...)
}
