//! One ffmpeg run, split as Go's `encode.go` (removed in phase 8; last release with it: v0.2.9) was: the argument list (`args`), the run itself
//! (`run`: one pass, its progress and its stop reasons) and the fallback cascade (`cascade`:
//! `EncodeVideo`). `failure` classifies a failed run (`classifyFfmpegFailure`), `progress`
//! reads one line of ffmpeg's `-progress` output.

pub mod args;
pub mod cascade;
pub mod failure;
pub mod progress;
pub mod run;
