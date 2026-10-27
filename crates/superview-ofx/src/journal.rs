//! The plugin's log: a capped file under the user cache directory, so that a user can tell which
//! path renders (CUDA, OpenCL or the CPU) and why a GPU path was not used. Before it, no logger was
//! installed and every record went nowhere (DEC-65).
//!
//! - Installed once per process, at `Load`, by [`install`]; a later call returns the first result
//!   and opens nothing. A directory that cannot be written means no log, never a failed action.
//! - Its own file, `superview-ofx/superview-ofx.log`, not the application's: two processes writing
//!   one file would each keep their own count of its size against the cap.
//! - The `ofx` fork logs every message the host sends at info level, so every frame: only its
//!   errors are kept ([`admits`]). Ours are kept from info up, and nothing per frame is logged
//!   above debug ([`PathLog`]), the rule the application follows for progress.
//! - Each Rust plugin carries its own copy of `log`: this logger does not reach another plugin's
//!   records, nor theirs this one.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use log::{Level, LevelFilter, Log, Metadata, Record};
use superview_core::logfile::{FileLogger, open_log_file};

/// The log's cap; past this size it restarts empty. A few lines per instance fit many times over.
pub const MAX_LOG_FILE_BYTES: i64 = 1 << 20;

const LOG_NAME: &str = "superview-ofx";

/// Whether a record from `target` at `level` goes to the file: ours from info up, the rest
/// (the `ofx` fork) only when it is an error.
pub fn admits(target: &str, level: Level) -> bool {
    let ours = target.starts_with("superview_ofx") || target.starts_with("superview_core");
    level <= if ours { Level::Info } else { Level::Error }
}

struct PluginLogger(FileLogger);

impl Log for PluginLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        admits(metadata.target(), metadata.level())
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            self.0.log(record);
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

/// Opens `<cache_dir>/superview-ofx/superview-ofx.log` and routes the `log` records to it, once
/// per process. Returns the file's path, or `None` when there is no log (no cache directory, one
/// that cannot be written, or another logger already set); the same answer on every later call.
pub fn install(cache_dir: Option<&Path>) -> Option<PathBuf> {
    static INSTALLED: OnceLock<Option<PathBuf>> = OnceLock::new();
    static LOGGER: OnceLock<PluginLogger> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            let (writer, path) = open_log_file(LOG_NAME, MAX_LOG_FILE_BYTES, cache_dir?).ok()?;
            let logger = LOGGER.get_or_init(|| PluginLogger(FileLogger::new(writer)));
            log::set_logger(logger).ok()?;
            log::set_max_level(LevelFilter::Info);
            Some(path)
        })
        .clone()
}

/// The three ways a frame is rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderPath {
    Cuda,
    OpenCl,
    Cpu,
}

/// What an instance has already said in the log, so that it says each thing once: the first frame
/// on each path, and the first failure of each GPU path. The host renders many frames a second,
/// and a card whose kernel never loads fails every one of them.
#[derive(Debug, Default)]
pub struct PathLog {
    cuda: bool,
    opencl: bool,
    cpu: bool,
    cuda_failed: bool,
    opencl_failed: bool,
}

impl PathLog {
    /// True the first time this instance renders a frame on `path`.
    pub fn first_on(&mut self, path: RenderPath) -> bool {
        let seen = match path {
            RenderPath::Cuda => &mut self.cuda,
            RenderPath::OpenCl => &mut self.opencl,
            RenderPath::Cpu => &mut self.cpu,
        };
        !std::mem::replace(seen, true)
    }

    /// True the first time a frame of this instance fails on the GPU path `path`. The CPU path
    /// does not fail this way: the host has nowhere to fall back to.
    pub fn first_failure(&mut self, path: RenderPath) -> bool {
        let failed = match path {
            RenderPath::Cuda => &mut self.cuda_failed,
            RenderPath::OpenCl => &mut self.opencl_failed,
            RenderPath::Cpu => return false,
        };
        !std::mem::replace(failed, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_path_is_reported_once_per_instance() {
        let mut log = PathLog::default();
        assert!(log.first_on(RenderPath::Cuda));
        assert!(!log.first_on(RenderPath::Cuda));
        assert!(log.first_on(RenderPath::Cpu));
        assert!(log.first_on(RenderPath::OpenCl));
        // Back and forth, as when a GPU path fails on some frames only: nothing more.
        assert!(!log.first_on(RenderPath::Cuda));
        assert!(!log.first_on(RenderPath::Cpu));
        assert!(!log.first_on(RenderPath::OpenCl));
        // Another instance starts over.
        assert!(PathLog::default().first_on(RenderPath::Cpu));
    }

    #[test]
    fn only_the_first_failure_of_each_gpu_path_is_reported() {
        let mut log = PathLog::default();
        assert!(log.first_failure(RenderPath::Cuda));
        assert!(!log.first_failure(RenderPath::Cuda));
        // CUDA having failed says nothing of OpenCL, as when Resolve changes mode.
        assert!(log.first_failure(RenderPath::OpenCl));
        assert!(!log.first_failure(RenderPath::OpenCl));
        assert!(!log.first_failure(RenderPath::Cpu));
    }

    #[test]
    fn the_fork_reaches_the_file_only_with_its_errors() {
        // `ofx::registry` logs every host message at info: every frame.
        assert!(!admits("ofx::registry", Level::Info));
        assert!(!admits("ofx::plugin", Level::Warn));
        assert!(admits("ofx::plugin", Level::Error));
        assert!(admits("superview_ofx::plugin", Level::Info));
        assert!(admits("superview_core::logfile", Level::Warn));
        assert!(!admits("superview_ofx::plugin", Level::Debug));
    }
}
