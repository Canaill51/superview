package main

import (
	"fmt"
	"log"
	"os"
	"superview/common"

	"github.com/jessevdk/go-flags"
)

var opts struct {
	Input   string `short:"i" long:"input" description:"The input video filename" value-name:"FILE" required:"true"`
	Output  string `short:"o" long:"output" description:"The output video filename" value-name:"FILE" required:"false" default:"output.mp4"`
	Encoder string `short:"e" long:"encoder" description:"The encoder to use, use -h to see a list. If not specified, it takes the standard encoder of the input file codec" value-name:"ENCODER" required:"false"`
	Bitrate int    `short:"b" long:"bitrate" description:"The bitrate in bytes/second to encode in. If not specified, take the same bitrate as the input file" value-name:"BITRATE" required:"false"`
	Squeeze bool   `short:"s" long:"squeeze" description:"Squeeze 4:3 video stretched to 16:9 (e.g. Caddx Tarsier 2.7k60)" required:"false"`
}

// CLIHandler implements UIHandler for command-line interface
type CLIHandler struct{}

func (h *CLIHandler) ShowError(err error) {
	log.Printf("Error: %v\n", err)
}

func (h *CLIHandler) ShowInfo(msg string) {
	fmt.Println(msg)
}

func (h *CLIHandler) ShowProgress(percent float64) {
	fmt.Printf("\rEncoding progress: %.2f%%", percent)
}

func (h *CLIHandler) GetBitrate() (int, error) {
	return opts.Bitrate, nil
}

func (h *CLIHandler) GetEncoder() string {
	return opts.Encoder
}

func (h *CLIHandler) GetSqueeze() bool {
	return opts.Squeeze
}

func main() {
	fmt.Println("===> Superview - dynamic video stretching <===\n")

	// Check for ffmpeg
	ffmpeg, err := common.CheckFfmpeg()
	if err != nil {
		log.Fatal(err)
	}

	fmt.Print(common.GetHeader(ffmpeg))

	// Parse flags
	_, err = flags.Parse(&opts)
	if err != nil {
		os.Exit(0)
	}

	// Create CLI handler and perform encoding
	handler := &CLIHandler{}
	if err := common.PerformEncoding(opts.Input, opts.Output, handler, ffmpeg); err != nil {
		handler.ShowError(err)
		os.Exit(1)
	}

	fmt.Printf("\nDone! You can open the output file %s to see the result\n", opts.Output)
}
