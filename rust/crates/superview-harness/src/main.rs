//! Test bench: never shipped, not a product CLI (DEC-44).
//!
//! Two subcommands that print JSON on stdout, for the cross tests against Go and for the
//! real-ffmpeg tests. Progress and the log go to stderr so that stdout stays pure JSON.
//! Exit code: 0 success, 1 failure, 2 wrong arguments (a non-UTF-8 argument included), 101 a
//! panic of the bench itself (Rust's own code for it).

mod args;
mod signals;

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Value, json};
use superview_core::config::Config;
use superview_core::encode::run::StopControl;
use superview_core::error::{Error, signal_description};
use superview_core::pipeline::{PipelineServices, PipelineUi, perform_encoding};
use superview_core::probe::Prober;
use superview_core::tools::{ToolResolver, check_ffmpeg};

use args::{Command, ConvertArgs};

/// Log lines (warnings and errors from the core) on stderr.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

/// What the pipeline asks, answered from the command line; progress goes to stderr.
struct HarnessUi<'a> {
    args: &'a ConvertArgs,
}

impl PipelineUi for HarnessUi<'_> {
    fn get_encoder(&self) -> String {
        self.args.encoder.clone()
    }
    fn get_bitrate(&self) -> Result<i64, Error> {
        Ok(self.args.bitrate)
    }
    fn get_squeeze(&self) -> bool {
        self.args.squeeze
    }
    fn show_progress(&self, percent: f64) {
        eprintln!("progress {percent:.1}");
    }
}

fn main() -> ExitCode {
    let mut strings = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(arg) => strings.push(arg),
            Err(bad) => {
                eprintln!(
                    "superview-harness: argument is not valid UTF-8: {bad:?}\n{}",
                    args::USAGE
                );
                return ExitCode::from(2);
            }
        }
    }
    let command = match args::parse(strings) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("superview-harness: {message}\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let value = match command {
        Command::Probe => probe(),
        Command::Convert(args) => convert(&args),
    };
    println!("{value}");
    ExitCode::from(u8::from(value["ok"] != json!(true)))
}

fn list(map: &HashMap<String, String>, key: &str) -> Vec<String> {
    map.get(key)
        .map(|s| {
            s.split(',')
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn failure(kind: &str, error: &Error) -> Value {
    json!({"ok": false, "kind": kind, "error": error.to_string()})
}

fn kind_of(error: &Error) -> &'static str {
    match error {
        Error::Cancelled => "cancelled",
        Error::StoppedBySignal(_) => "signal",
        Error::InvalidVideo(_) => "invalid_video",
        Error::Encoder(_) => "encoder",
        Error::Session(_) => "session",
        Error::FailedAfterOutput(cause) if cause.is_stop_request() => kind_of(cause),
        Error::FailedAfterOutput(_) => "failed_after_output",
        Error::Plain(_) => "other",
    }
}

fn probe() -> Value {
    let stop = signals::install();
    let resolver = ToolResolver::from_process();
    let caps = match check_ffmpeg(&Config::default(), &resolver) {
        Ok(caps) => caps,
        Err(e) => return failure(kind_of(&e), &e),
    };
    let prober = Prober::new(&resolver);
    let report = prober.probe_hardware_support(&caps, Some(stop.cancel));
    // A report cut short by a signal is not a report: never say `ok` then.
    let signal = stop.signal.load(Ordering::Acquire);
    if signal != 0 {
        return failure(
            "signal",
            &Error::StoppedBySignal(signal_description(signal)),
        );
    }
    let results: Vec<Value> = report
        .results
        .iter()
        .map(|p| json!({"encoder": p.encoder, "usable": p.usable, "reason": p.reason}))
        .collect();
    json!({
        "ok": true,
        "ffmpeg": {
            "version": caps.get("version"),
            "accels": list(&caps, "accels"),
            "encoders": list(&caps, "encoders"),
        },
        "probe": results,
    })
}

fn seconds(d: Duration) -> f64 {
    d.as_secs_f64()
}

fn convert(args: &ConvertArgs) -> Value {
    let stop_refs = signals::install();
    let resolver = ToolResolver::from_process();
    let cfg = Config::default();
    let caps = match check_ffmpeg(&cfg, &resolver) {
        Ok(caps) => caps,
        Err(e) => return failure(kind_of(&e), &e),
    };
    let prober = Prober::new(&resolver);
    let services = PipelineServices::system(&resolver, &prober);
    let stop = StopControl {
        cancel: stop_refs.cancel,
        signal: stop_refs.signal,
    };
    let ui = HarnessUi { args };
    match perform_encoding(
        Some(&cfg),
        &args.input,
        &args.output,
        &ui,
        &caps,
        &services,
        &stop,
    ) {
        Ok(outcome) => {
            let m = &outcome.metrics;
            json!({
                "ok": true,
                "encoder": outcome.encoder,
                "bitrate": outcome.bitrate,
                "hardware_acceleration": outcome.summary.hardware_acceleration,
                "metrics": {
                    "input_file_size": m.input_file_size,
                    "output_file_size": m.output_file_size,
                    "output_bitrate": m.output_bitrate,
                    "output_codec": m.output_codec,
                    "progress_updates": m.progress_updates,
                    "last_progress": m.last_progress,
                    "video_check_seconds": seconds(m.video_check_duration),
                    "pgm_generation_seconds": seconds(m.pgm_generation_time),
                    "encode_seconds": seconds(m.encode_duration),
                    "cleanup_seconds": seconds(m.cleanup_duration),
                },
            })
        }
        Err(e) => failure(kind_of(&e), &e),
    }
}
