// The per-conversion temporary directory and the remap maps it holds.

package common

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

// EncodingSession manages temporary files for a single encoding job.
// It ensures all PGM filter maps are created in a secure, isolated directory.
type EncodingSession struct {
	tempDir  string // Path to temporary directory created with os.MkdirTemp
	pgmXPath string // Path to X-coordinate remap filter (PGM format)
	pgmYPath string // Path to Y-coordinate remap filter (PGM format)
}

var (
	currentSession *EncodingSession
	sessionMutex   sync.Mutex
)

// InitEncodingSession creates a new secure temporary directory for this encoding job.
// Call this before GeneratePGM and EncodeVideo.
// Always use defer common.CleanUp() to guarantee cleanup even on error.
// cfg supplies TempDirPrefix; nil means the built-in defaults.
func InitEncodingSession(cfg *Config) error {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession != nil {
		return &SessionError{Msg: "encoding session already active"}
	}

	tempDir, err := os.MkdirTemp("", configOrDefault(cfg).TempDirPrefix)
	if err != nil {
		return &SessionError{Msg: "failed to create temp directory", Err: err}
	}

	session := &EncodingSession{
		tempDir:  tempDir,
		pgmXPath: filepath.Join(tempDir, "x.pgm"),
		pgmYPath: filepath.Join(tempDir, "y.pgm"),
	}

	currentSession = session
	return nil
}

// CloseEncodingSession closes the current encoding session and removes its temporary directory.
// This function is idempotent and safe to call multiple times.
func CloseEncodingSession() error {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession == nil {
		return nil
	}

	defer func() {
		currentSession = nil
	}()

	return os.RemoveAll(currentSession.tempDir)
}

func getSessionPaths() (xPath, yPath string, err error) {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession == nil {
		return "", "", &SessionError{Msg: "no encoding session active"}
	}

	return currentSession.pgmXPath, currentSession.pgmYPath, nil
}

// sessionOutputSize reads the frame geometry out of the remap map this session
// generated.
//
// The maps are the conversion's own statement of what it is producing, so a
// caller that has them needs no squeeze flag and cannot disagree with what
// ffmpeg is about to be fed. GeneratePGM writes the header as
// "P5 <width> <height> 65535\n", which is the whole of what is read here.
//
// Not being able to read it is not an error worth propagating: the caller uses
// the size to price memory, and no price is better than a wrong one.
func sessionOutputSize() (width, height int, ok bool) {
	xPath, _, err := getSessionPaths()
	if err != nil {
		return 0, 0, false
	}

	file, err := os.Open(xPath)
	if err != nil {
		return 0, 0, false
	}
	defer func() { _ = file.Close() }()

	// The header is at most a few dozen bytes; the rest of the file is samples.
	header := make([]byte, 64)
	n, err := file.Read(header)
	if n == 0 && err != nil {
		return 0, 0, false
	}

	if _, err := fmt.Sscanf(string(header[:n]), "P5 %d %d", &width, &height); err != nil {
		return 0, 0, false
	}
	if width <= 0 || height <= 0 {
		return 0, 0, false
	}
	return width, height, true
}

func CleanUp() error {
	return CloseEncodingSession()
}
