// What ffprobe says about the input, and its validation.

package common

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

// VideoSpecs contains metadata about a video file extracted by ffprobe.
type VideoSpecs struct {
	File    string        // Absolute path to the video file
	Streams []VideoStream // Video stream information (typically just the first stream)
}

// VideoStream contains metadata about a single video stream.
// The JSON tags correspond to ffprobe's output format.
type VideoStream struct {
	Codec         string  `json:"codec_name"`
	Width         int     `json:"width"`
	Height        int     `json:"height"`
	Duration      string  `json:"duration"`
	DurationFloat float64 `json:"-"`
	Bitrate       string  `json:"bit_rate"`
	BitrateInt    int     `json:"-"`
	// PixFmt is ffprobe's pixel format name, e.g. "yuv420p" or "yuv420p10le".
	// It is what tells the filter chain whether the source carries more than 8
	// bits per sample. Absent on very old ffprobe builds, which is handled as
	// 8-bit rather than guessed at.
	PixFmt string `json:"pix_fmt"`
	// RFrameRate is ffprobe's frame rate as a rational string, e.g. "30/1" or
	// "30000/1001". FrameRate holds it parsed, or 0 when it is missing or
	// unusable -- the encoding speed metric is then simply not published rather
	// than computed from a guess.
	RFrameRate string  `json:"r_frame_rate"`
	FrameRate  float64 `json:"-"`
}

// parseFrameRate turns ffprobe's rational frame rate into frames per second.
//
// Returns 0 for anything it cannot make sense of, including the "0/0" ffprobe
// reports for streams whose rate it could not determine. 0 means "unknown" to
// every caller; none of them substitutes a default.
func parseFrameRate(rational string) float64 {
	numText, denText, found := strings.Cut(strings.TrimSpace(rational), "/")
	if !found {
		value, err := strconv.ParseFloat(strings.TrimSpace(rational), 64)
		if err != nil || value <= 0 {
			return 0
		}
		return value
	}

	num, numErr := strconv.ParseFloat(strings.TrimSpace(numText), 64)
	den, denErr := strconv.ParseFloat(strings.TrimSpace(denText), 64)
	if numErr != nil || denErr != nil || den == 0 || num <= 0 {
		return 0
	}
	return num / den
}

// Validate checks if video specs contain all required and valid information.
// Returns InvalidVideoError if metadata is incomplete or invalid.
func (v *VideoSpecs) Validate() error {
	if len(v.Streams) == 0 {
		return &InvalidVideoError{Reason: "no video streams found"}
	}

	stream := &v.Streams[0]

	if stream.Width <= 0 || stream.Height <= 0 {
		return &InvalidVideoError{Reason: fmt.Sprintf("invalid dimensions: %dx%d", stream.Width, stream.Height)}
	}

	if stream.DurationFloat <= 0 {
		return &InvalidVideoError{Reason: "invalid or missing duration"}
	}

	if stream.BitrateInt <= 0 {
		return &InvalidVideoError{Reason: "invalid or missing bitrate"}
	}

	if stream.Codec == "" {
		return &InvalidVideoError{Reason: "no codec information"}
	}

	return nil
}

// CheckVideo loads and validates video metadata using ffprobe.
// It extracts codec, dimensions, duration, and bitrate from the first video stream.
// Returns InvalidVideoError if required metadata is missing or invalid.
func CheckVideo(file string) (*VideoSpecs, error) {
	// Check specs of the input video (codec, dimensions, duration, bitrate)
	cmd := newFFprobeCommand("-i", file, "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height,duration,bit_rate,pix_fmt,r_frame_rate", "-print_format", "json")
	prepareBackgroundCommand(cmd)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("ffprobe failed, output:\n%s", out)
	}

	return parseVideoSpecs(file, out)
}

// parseVideoSpecs turns ffprobe's JSON into a validated VideoSpecs.
//
// Split out from CheckVideo so it can be tested against recorded ffprobe
// output. This is the only place the program reads something it did not
// produce itself, and every one of its rejection paths -- malformed JSON, no
// stream, an unparsable duration, a missing or non-numeric bitrate -- used to
// be reachable only by finding a video file that provoked it, which is to say
// not at all. See common/testdata/ffprobe.
func parseVideoSpecs(file string, out []byte) (*VideoSpecs, error) {
	// Parse ffprobe output
	var response struct {
		Streams []VideoStream `json:"streams"`
	}
	if err := json.Unmarshal(out, &response); err != nil {
		return nil, fmt.Errorf("failed to parse video metadata: %w", err)
	}

	if len(response.Streams) == 0 {
		return nil, &InvalidVideoError{Reason: "no video streams in file"}
	}

	specs := &VideoSpecs{
		File:    file,
		Streams: response.Streams,
	}

	// Parse duration from first stream
	durationFloat, err := strconv.ParseFloat(specs.Streams[0].Duration, 64)
	if err != nil {
		return nil, fmt.Errorf("invalid duration value '%s': %w", specs.Streams[0].Duration, err)
	}
	specs.Streams[0].DurationFloat = durationFloat

	// Parse bitrate from first stream
	if specs.Streams[0].Bitrate == "" {
		return nil, &InvalidVideoError{Reason: "bitrate information not available"}
	}
	bitrateInt, err := strconv.Atoi(specs.Streams[0].Bitrate)
	if err != nil {
		return nil, fmt.Errorf("invalid bitrate value '%s': %w", specs.Streams[0].Bitrate, err)
	}
	specs.Streams[0].BitrateInt = bitrateInt

	// Optional: a missing or unusable frame rate is not a reason to reject a
	// file, it only means the encoding-speed metric stays unpublished.
	specs.Streams[0].FrameRate = parseFrameRate(specs.Streams[0].RFrameRate)

	// Validate all required data is present
	if err := specs.Validate(); err != nil {
		return nil, err
	}

	return specs, nil
}
