package main

import (
	"fmt"
	"log/slog"
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
type CLIHandler struct {
	logger *slog.Logger
}

func (h *CLIHandler) ShowError(err error) {
	h.logger.Error("Encoding error", slog.String("error", err.Error()))
}

func (h *CLIHandler) ShowInfo(msg string) {
	h.logger.Info(msg)
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
	// Initialize logger for CLI (text format for readability)
	opts_logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{
		Level: slog.LevelInfo,
	}))
	common.SetLogger(opts_logger)

	fmt.Println("===> Superview - dynamic video stretching <===\n")

	// Load configuration (from superview.yaml or env vars)
	cfg, err := common.LoadConfig("superview.yaml")
	if err != nil {
		opts_logger.Error("Failed to load configuration", slog.String("error", err.Error()))
		os.Exit(1)
	}
	common.SetConfig(cfg)
	opts_logger.Debug("Configuration loaded", slog.String("config", cfg.String()))

	// Check for ffmpeg
	ffmpeg, err := common.CheckFfmpeg()
	if err != nil {
		opts_logger.Error("Failed to check ffmpeg", slog.String("error", err.Error()))
		os.Exit(1)
	}

	fmt.Print(common.GetHeader(ffmpeg))

	// Parse flags
	_, err = flags.Parse(&opts)
	if err != nil {
		os.Exit(0)
	}

	// Create CLI handler and perform encoding
	handler := &CLIHandler{logger: opts_logger}
	if err := common.PerformEncoding(opts.Input, opts.Output, handler, ffmpeg); err != nil {
		handler.ShowError(err)
		os.Exit(1)
	}

	fmt.Printf("\nDone! You can open the output file %s to see the result\n", opts.Output)
}
