//! What does the work for the window: ffmpeg, ffprobe and the pipeline, behind the
//! [`Engine`](crate::app::Engine) trait so that the window's tests need none of them.
//!
//! One resolver and one prober per process. The prober is the point: it caches what each
//! encoder answered for each frame size, and a prober made per call would never be asked twice
//! (every verification would spend its 0.3 s again). It borrows the resolver, so both live in
//! statics: `'static` without leaking anything.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32};

use superview_core::config::Config;
use superview_core::disk::free_disk_gb;
use superview_core::encode::run::StopControl;
use superview_core::error::Error;
use superview_core::health::{check_health, get_health_report, log_health};
use superview_core::pipeline::{EncodeOutcome, PipelineServices, PipelineUi, perform_encoding};
use superview_core::probe::{EncoderProbeReport, Prober};
use superview_core::tools::{ToolResolver, check_ffmpeg};
use superview_core::video::{VideoSpecs, check_video};

use crate::app::{EncodeJob, Engine};
use crate::format::resolve_start_settings;
use crate::state::VerificationRequest;

static RESOLVER: OnceLock<ToolResolver> = OnceLock::new();
static PROBER: OnceLock<Prober<'static>> = OnceLock::new();

/// The process's resolver and prober, made on first use.
fn process_tools() -> (&'static ToolResolver, &'static Prober<'static>) {
    let resolver = RESOLVER.get_or_init(ToolResolver::from_process);
    let prober = PROBER.get_or_init(|| Prober::new(resolver));
    (resolver, prober)
}

/// The real ffmpeg, ffprobe and pipeline.
pub struct SystemEngine {
    cfg: Config,
    resolver: &'static ToolResolver,
    prober: &'static Prober<'static>,
    /// Written by the signal handlers (see `signals.rs`); the pipeline reads it.
    signal: &'static AtomicI32,
}

impl SystemEngine {
    /// The engine of this process: its environment, its executable's directory.
    pub fn for_process(cfg: Config, signal: &'static AtomicI32) -> Self {
        let (resolver, prober) = process_tools();
        Self {
            cfg,
            resolver,
            prober,
            signal,
        }
    }

    /// An engine over the given tools; they must outlive it, which a test gets by leaking them.
    /// Unix only, like the tests that use it: Windows finds the machine's own ffmpeg whatever
    /// the injected environment says.
    #[cfg(all(test, unix))]
    pub fn with_tools(
        cfg: Config,
        resolver: &'static ToolResolver,
        prober: &'static Prober<'static>,
        signal: &'static AtomicI32,
    ) -> Self {
        Self {
            cfg,
            resolver,
            prober,
            signal,
        }
    }
}

impl Engine for SystemEngine {
    fn check_ffmpeg(&self) -> Result<HashMap<String, String>, Error> {
        check_ffmpeg(&self.cfg, self.resolver)
    }

    fn forget_tools(&self) {
        // Failed lookups are not cached, so this picks up an ffmpeg installed while the
        // application was running; the verdicts of the probe belong to the old one.
        self.resolver.reset_cache();
        self.prober.cache().reset();
    }

    fn probe_hardware(
        &self,
        ffmpeg: &HashMap<String, String>,
        cancel: &AtomicBool,
    ) -> EncoderProbeReport {
        self.prober.probe_hardware_support(ffmpeg, Some(cancel))
    }

    fn check_video(&self, path: &str) -> Result<VideoSpecs, Error> {
        check_video(self.resolver, path)
    }

    fn verify_hardware(&self, request: &VerificationRequest, cancel: &AtomicBool) -> String {
        self.prober.describe_verified_hardware_plan(
            &request.ffmpeg,
            Some(&request.video),
            request.squeeze,
            &request.selection,
            // The preset Start will pass, so the preview and the conversion share one answer.
            &resolve_start_settings(&self.cfg, &request.quality, 0).1,
            Some(cancel),
        )
    }

    fn encode(
        &self,
        job: &EncodeJob,
        ui: &dyn PipelineUi,
        cancel: &AtomicBool,
    ) -> Result<EncodeOutcome, Error> {
        let services = PipelineServices::system(self.resolver, self.prober);
        let stop = StopControl {
            cancel,
            signal: self.signal,
        };
        perform_encoding(
            Some(&job.cfg),
            &job.input,
            &job.output,
            ui,
            &job.ffmpeg,
            &services,
            &stop,
        )
    }

    fn health_report(&self, temp_dir: &Path, cancel: &AtomicBool) -> String {
        let health = check_health(
            &self.cfg,
            self.resolver,
            temp_dir,
            free_disk_gb,
            Some(cancel),
        );
        // Mirrored into the log, so a user can attach it to a bug report without retyping
        // what the dialog showed.
        log_health(Some(&health));
        get_health_report(Some(&health))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Diagnostic's report on a directory of its own, which the test removes afterwards: the
    /// check must leave nothing in it (it reads the room, it writes nothing). Unix only: on
    /// Windows the system search finds the machine's own ffmpeg whatever the environment says.
    #[cfg(unix)]
    #[test]
    fn the_health_report_leaves_nothing_in_the_directory_it_checks() {
        let resolver: &'static ToolResolver =
            Box::leak(Box::new(ToolResolver::new(|_| None, None)));
        let prober: &'static Prober<'static> = Box::leak(Box::new(Prober::new(resolver)));
        static SIGNAL: AtomicI32 = AtomicI32::new(0);
        let engine = SystemEngine::with_tools(Config::default(), resolver, prober, &SIGNAL);

        let dir = tempfile::tempdir().expect("tempdir");
        let report = engine.health_report(dir.path(), &AtomicBool::new(false));
        assert!(report.contains("UNHEALTHY"), "{report}");
        assert!(report.contains("FFMPEG"), "{report}");
        assert!(
            report.contains(&dir.path().display().to_string()),
            "the report does not name the directory it checked: {report}"
        );
        let left: Vec<_> = std::fs::read_dir(dir.path()).expect("read").collect();
        assert!(left.is_empty(), "the check left files behind: {left:?}");
        let path = dir.path().to_path_buf();
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn the_process_has_one_resolver_and_one_prober() {
        let (resolver_a, prober_a) = process_tools();
        let (resolver_b, prober_b) = process_tools();
        assert!(std::ptr::eq(resolver_a, resolver_b));
        assert!(std::ptr::eq(prober_a, prober_b));
    }
}
