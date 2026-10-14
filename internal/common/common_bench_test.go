package common

import (
	"bytes"
	"math"
	"strconv"
	"testing"
)

// BenchmarkGeneratePGMMapCalculation measures the core mathematical calculations
// for pixel coordinate remapping, which is the critical path in GeneratePGM.
// This isolates algorithm performance from I/O overhead.
func BenchmarkGeneratePGMMapCalculation(b *testing.B) {
	width := 1920
	height := 1080
	outX := int(float64(height)*(16.0/9.0)) / 2 * 2

	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		for y := 0; y < height; y++ {
			for x := 0; x < outX; x++ {
				sx := float64(x) - float64(outX-width)/2.0
				tx := (float64(x)/float64(outX) - 0.5) * 2.0
				offset := math.Pow(tx, 2) * (float64(outX-width) / 2.0)
				if tx < 0 {
					offset *= -1
				}
				_ = sx - offset
			}
		}
	}
}

// BenchmarkStringFormatting measures the overhead of converting integers to strings
// using strconv vs bytes.Buffer approach during PGM file generation.
func BenchmarkStringFormatting(b *testing.B) {
	b.Run("strconv_itoa", func(b *testing.B) {
		var buf bytes.Buffer
		b.ResetTimer()

		for i := 0; i < b.N; i++ {
			buf.Reset()
			for x := 0; x < 1920; x++ {
				buf.WriteString(strconv.Itoa(x))
				buf.WriteString(" ")
			}
		}
	})

	b.Run("fmt_sprintf", func(b *testing.B) {
		var buf bytes.Buffer
		b.ResetTimer()

		for i := 0; i < b.N; i++ {
			buf.Reset()
			for x := 0; x < 1920; x++ {
				buf.WriteString(strconv.FormatInt(int64(x), 10))
				buf.WriteString(" ")
			}
		}
	})

	b.Run("preallocated_buffer", func(b *testing.B) {
		buf := make([]byte, 0, 20000)
		b.ResetTimer()

		for i := 0; i < b.N; i++ {
			buf = buf[:0]
			for x := 0; x < 1920; x++ {
				buf = strconv.AppendInt(buf, int64(x), 10)
				buf = append(buf, ' ')
			}
		}
	})
}

// BenchmarkLineGeneration measures the performance of generating a single line of PGM output
// comparing different buffer strategies.
func BenchmarkLineGeneration(b *testing.B) {
	width := 1920
	height := 1080
	outX := int(float64(height)*(16.0/9.0)) / 2 * 2

	b.Run("current_writestring", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			var outLine bytes.Buffer
			for x := 0; x < outX; x++ {
				sx := float64(x) - float64(outX-width)/2.0
				tx := (float64(x)/float64(outX) - 0.5) * 2.0
				offset := math.Pow(tx, 2) * (float64(outX-width) / 2.0)
				if tx < 0 {
					offset *= -1
				}
				outLine.WriteString(strconv.Itoa(int(sx - offset)))
				outLine.WriteString(" ")
			}
			_ = outLine.String()
		}
	})

	b.Run("optimized_appendint", func(b *testing.B) {
		buf := make([]byte, 0, outX*8) // Pre-allocate reasonable capacity
		for i := 0; i < b.N; i++ {
			buf = buf[:0]
			for x := 0; x < outX; x++ {
				sx := float64(x) - float64(outX-width)/2.0
				tx := (float64(x)/float64(outX) - 0.5) * 2.0
				offset := math.Pow(tx, 2) * (float64(outX-width) / 2.0)
				if tx < 0 {
					offset *= -1
				}
				buf = strconv.AppendInt(buf, int64(int(sx-offset)), 10)
				buf = append(buf, ' ')
			}
		}
	})
}

// BenchmarkBufferFlush measures the impact of buffered I/O with regular flushing
// vs accumulating more data before flushing.
func BenchmarkBufferFlush(b *testing.B) {
	b.Run("flush_per_line", func(b *testing.B) {
		lines := 1080
		width := 1920

		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			outLine := bytes.Buffer{}
			for y := 0; y < lines; y++ {
				outLine.Reset()
				for x := 0; x < width; x++ {
					outLine.WriteString(strconv.Itoa(x))
					outLine.WriteString(" ")
				}
				_ = outLine.Bytes() // Simulate flush
			}
		}
	})

	b.Run("batch_flush", func(b *testing.B) {
		lines := 1080
		width := 1920

		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			outBuf := bytes.Buffer{}
			outBuf.Grow(lines * width * 8) // Pre-allocate
			for y := 0; y < lines; y++ {
				for x := 0; x < width; x++ {
					outBuf.WriteString(strconv.Itoa(x))
					outBuf.WriteString(" ")
				}
				outBuf.WriteString("\n")
			}
			_ = outBuf.Bytes() // Single flush
		}
	})
}
