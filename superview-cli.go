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

	_, err = os.Stat(opts.Input)
	if err != nil {
		log.Fatalf("Error opening input file: %s\n", opts.Input)
	}

	video, err := common.CheckVideo(opts.Input)
	if err != nil {
		log.Fatal(err)
	}

	// If no bitrate set, use from input video
	if opts.Bitrate == 0 {
		opts.Bitrate = video.Streams[0].BitrateInt
	}

	// Validate bitrate (min 100k, max 50M bytes/sec)
	if err := common.ValidateBitrate(opts.Bitrate, 100000, 50000000); err != nil {
		log.Fatal(err)
	}

	encoder, err := common.FindEncoder(opts.Encoder, ffmpeg, video)
	if err != nil {
		log.Fatal(err)
	}

	// Initialize encoding session with secure temp directory
	err = common.InitEncodingSession()
	if err != nil {
		log.Fatal(err)
	}
	defer common.CleanUp()

	err = common.GeneratePGM(video, opts.Squeeze)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Re-encoding video with %s encoder at %d MB/s bitrate\n", encoder, opts.Bitrate/1024/1024)

	err = common.EncodeVideo(video, encoder, opts.Bitrate, opts.Output, func(v float64) {
		fmt.Printf("\rEncoding progress: %.2f%%", v)
	})
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Done! You can open the output file %s to see the result\n", opts.Output)
}
