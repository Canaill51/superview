//! One ffmpeg run, split as the Go `encode.go` is: the argument list (`args`) and, in a
//! later pull request, the run itself and its fallback cascade. `failure` classifies a failed
//! run (`classifyFfmpegFailure`), `progress` reads one line of ffmpeg's `-progress` output.

pub mod args;
pub mod failure;
pub mod progress;
