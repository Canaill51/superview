//! One ffmpeg run, split in three: the argument list (`args`), the run itself
//! (`run`: one pass, its progress and its stop reasons) and the fallback cascade (`cascade`:
//! `encode_video`). `failure` classifies a failed run (`classify_ffmpeg_failure`), `progress`
//! reads one line of ffmpeg's `-progress` output.

pub mod args;
pub mod cascade;
pub mod failure;
pub mod progress;
pub mod run;
