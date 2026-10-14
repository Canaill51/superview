//go:build windows

package common

// killedBySignal always reports false on Windows: a process there ends with an
// exit code and nothing else, so there is no signal to name.
//
// The consequence is deliberate. The out-of-memory diagnosis this feeds only
// fires on the platforms that can actually report a kill, and a Windows encode
// that runs out of memory keeps the failure it had before -- ffmpeg's own
// allocation error, which it does print. Inventing a memory diagnosis from a
// bare non-zero exit code would attach it to every ordinary failure as well.
func killedBySignal(error) (string, bool) {
	return "", false
}
