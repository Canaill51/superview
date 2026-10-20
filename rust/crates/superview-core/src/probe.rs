//! Runtime probing of the encoders ffmpeg claims to have. Port of
//! Go's `internal/common/probe.go` (removed in phase 8; last release with it: v0.2.9) and, from `hardware.go`, `outputFrameSize`,
//! `probeEncoderForSizeCached`, `verifyEncoderForOutput` and
//! `DescribeVerifiedHardwarePlan`.
//!
//! `ffmpeg -encoders` says what the binary was built with, not whether the driver on this
//! machine will accept the work (an NVENC build can demand a driver the machine cannot
//! have, `docs/hardware-support.md`). The only honest question is the one asked here:
//! encode one frame and see. The argv comes from the functions the conversion uses
//! (`hw_device_args`, `pixel_format_chain`, `encoder_facing_args`), so the two cannot
//! drift (E-01); `tests/parity_probe_args.rs` pins it against Go.
//!
//! Deliberate differences from the Go code (exhaustive):
//! - A VAAPI driver that answers "supported modes: CQP" is asked again in constant quality
//!   (DEC-48) and, if that works, reported usable with [`EncoderProbe::rate_control`] set:
//!   Go reports it unusable. The second question goes through the same
//!   `encoder_facing_args` as every other (DEC-05).
//! - No global. The cache of verdicts per frame size (`sizeProbeCache`) is the
//!   [`ProbeCache`] a [`Prober`] owns, `resetSizeProbeCache` is [`ProbeCache::reset`], and
//!   the probe's own timeout (Go's `var probeTimeout`) is a field of the [`Prober`]
//!   ([`PROBE_TIMEOUT`] by default), so tests shorten it without touching anything shared.
//! - A Go `context.Context` carries a caller's deadline and cancellation; here the caller
//!   gives an `Option<&AtomicBool>` and only the probe's own timeout is a deadline. "The
//!   caller's deadline" (`TestProbeEncoder_HonoursTheCallerDeadline`) is therefore a
//!   cancellation: the reason of a cancelled probe is the process's end status (or
//!   "cancelled" when nothing was started), not "gave no answer in ...", which stays the
//!   probe's own timeout. A cancelled probe is never cached (E-23), whenever the flag is
//!   up once the probe is back, as in Go.
//! - A [`Prober`] must be long-lived (one for the application): its cache is the point, and
//!   a fresh one per question probes every time. Two simultaneous calls on the same key
//!   both probe, as in Go (the lock is not held across a probe).
//! - A Go `nil` report is `None` in [`apply_encoder_probe`] and [`describe_encoder_probe`].
//! - The run error in a reason is Rust's: "exit status: 218" where Go says "exit status
//!   218" (only the leading text of such a reason is comparable, like in `tools.rs`).
//! - The reason budget is 300 bytes cut back to a character boundary; Go cuts at 300
//!   bytes, possibly in the middle of a character.
//! - The durations in the report are printed as Go's `Duration.String` does after
//!   `Round(time.Millisecond)` ("302ms", "1.5s", "1m2.5s"), by [`format_duration`].
//! - The machine profile of the fallback is built with 0 cores: nothing in the selection
//!   reads the core count (`hardware.rs`).
//! - The logs are `log` records with `key=value` text, where Go uses `slog` attributes.
//! - Left out: the Go `ProbeEncoder(ctx, ...)` wrappers around package globals; the
//!   methods of [`Prober`] are their equivalents.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::encode::args::{RateControl, encoder_facing_args, pixel_format_chain};
use crate::hardware::{
    HardwarePlan, analyze_machine_profile, build_hardware_plan, describe_codec_family_switch,
    describe_planned_hardware_path, hw_device_args, is_hardware_encoder,
    select_hardware_decode_accel, software_encoder_for_fallback, source_pixel_format, split_csv,
};
use crate::pgm::remap_output_size;
use crate::process::{RunEnd, run_with_deadline};
use crate::tools::ToolResolver;
use crate::video::VideoSpecs;

/// Caps a single probe. A driver that is installed but wedged can leave ffmpeg waiting
/// with no output at all, and the probe runs while the user is looking at the window.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// The frame the startup sweep encodes: small enough to cost nothing, above the minimum
/// dimensions the hardware encoders enforce. It cannot prove a large frame will encode;
/// [`Prober::verify_encoder_for_output`] asks that once the geometry is known.
pub const PROBE_FRAME_SIZE: &str = "256x256";

/// How many of ffmpeg's lines are kept. The first ones, not the last: ffmpeg reports the
/// specific cause first and its consequences after.
pub const PROBE_REASON_LINES: usize = 3;

/// Caps the reason in bytes, so a talkative failure cannot turn the one-line hardware
/// status into a paragraph.
pub const PROBE_REASON_BUDGET: usize = 300;

/// The rate-control target a probe asks for. The value does not matter -- one frame at any
/// rate -- but asking for one does: a driver that cannot honour a bitrate at all must say
/// so here, not at conversion time.
pub const PROBE_BITRATE: i64 = 2_000_000;

/// What one encoder answered when it was actually asked to encode something.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncoderProbe {
    pub encoder: String,
    pub usable: bool,
    /// ffmpeg's own words; empty when the probe succeeded.
    pub reason: String,
    pub took: Duration,
    /// The rate control the encoder was found usable with. Only ever
    /// [`RateControl::ConstantQuality`] for a VAAPI driver that refused a bit rate and
    /// accepted CQP (DEC-48); meaningless while `usable` is false.
    pub rate_control: RateControl,
}

/// The verdict for every encoder that was probed. An encoder absent from `results` was
/// never asked, which is not the same as unusable: [`apply_encoder_probe`] leaves those
/// alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncoderProbeReport {
    pub results: Vec<EncoderProbe>,
    pub took: Duration,
}

impl EncoderProbeReport {
    /// The verdict recorded for one encoder.
    pub fn find(&self, encoder: &str) -> Option<&EncoderProbe> {
        self.results.iter().find(|probe| probe.encoder == encoder)
    }

    /// The encoders that were probed and refused the work.
    pub fn unusable(&self) -> Vec<&EncoderProbe> {
        self.results.iter().filter(|probe| !probe.usable).collect()
    }
}

/// The question a probe asks: this encoder, for this frame, from this source depth, with
/// this rate control and preset. Every field the conversion passes to the encoder is here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeSpec {
    pub encoder: String,
    /// "WxH".
    pub size: String,
    /// ffprobe's name for the source depth; "" is 8-bit.
    pub pix_fmt: String,
    pub bitrate: i64,
    /// x264-style, mapped per encoder exactly as the conversion does.
    pub preset: String,
    /// How the encoder is asked to spend bits; part of the question, so of the key.
    pub rate_control: RateControl,
}

impl ProbeSpec {
    pub fn cache_key(&self) -> String {
        format!(
            "{}@{}/{}/{}/{}/{}",
            self.encoder,
            self.size,
            self.pix_fmt,
            self.bitrate,
            self.preset,
            self.rate_control.label()
        )
    }
}

/// What a VAAPI driver says when it knows no rate control but CQP. ffmpeg's
/// `vaapi_encode.c` prints each mode after a space and closes the list: measured on
/// 2026-10-19, "... (supported modes: CQP)." A driver with several modes says
/// "supported modes: CQP CBR VBR)", which must not match.
const CQP_ONLY_MARKER: &str = "supported modes: CQP)";

/// Whether this refusal is the "CQP only" one of a VAAPI driver, the only case where a
/// second question in constant quality is worth asking (DEC-48). Not for QSV, whose
/// failure on such machines is a missing firmware, nor for any other reason.
fn is_cqp_only_refusal(encoder: &str, reason: &str) -> bool {
    matches!(encoder, "h264_vaapi" | "hevc_vaapi") && reason.contains(CQP_ONLY_MARKER)
}

/// The question the startup sweep asks: a small 8-bit frame, the nominal bitrate, no
/// preset.
pub fn sweep_spec(encoder: &str) -> ProbeSpec {
    ProbeSpec {
        encoder: encoder.to_string(),
        size: PROBE_FRAME_SIZE.to_string(),
        bitrate: PROBE_BITRATE,
        ..ProbeSpec::default()
    }
}

/// The argv for one probe.
///
/// VAAPI, Vulkan and D3D12 only accept frames that already live on the device: an argv
/// without the upload fails in the filter graph, before the driver is consulted, and
/// would condemn a working encoder on a malformed question. An argv without the rate
/// control did the opposite: it passed a driver that then refused every real conversion.
pub fn probe_args(spec: &ProbeSpec) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    args.extend(hw_device_args(&spec.encoder));
    args.extend(
        ["-f", "lavfi", "-i"]
            .map(String::from)
            .into_iter()
            .chain([format!("nullsrc=s={}:r=25:d=0.04", spec.size)]),
    );
    args.push("-vf".to_string());
    args.push(pixel_format_chain(&spec.pix_fmt, &spec.encoder));
    args.extend(encoder_facing_args(
        &spec.encoder,
        spec.rate_control,
        spec.bitrate,
        &spec.preset,
    ));
    args.extend(["-f", "null", "-"].map(String::from));
    args
}

/// libva announces its driver search with three `libva info:` lines before any VAAPI or QSV
/// session. They say nothing went wrong, and left in they filled the three lines kept and
/// pushed the real error out of the reason. A `libva error:` line is a cause and stays.
/// Rust only: Go keeps them (DEC-48).
const LIBVA_INFO_PREFIX: &str = "libva info:";

/// Turns ffmpeg's output into one line a user can act on.
///
/// `timed_out` is the probe's own deadline; the message reports what was actually waited
/// rather than the constant. `run_err` is what the run itself said, used when ffmpeg
/// printed nothing: it at least tells "refused" from "never ran".
pub fn summarize_probe_failure(
    output: &str,
    run_err: &str,
    timed_out: bool,
    took: Duration,
) -> String {
    if timed_out {
        return format!("gave no answer in {}", format_duration(took));
    }

    let lines: Vec<String> = output
        .replace("\r\n", "\n")
        .split('\n')
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !line.starts_with(LIBVA_INFO_PREFIX))
        .take(PROBE_REASON_LINES)
        .collect();
    if lines.is_empty() {
        return run_err.to_string();
    }

    let mut reason = lines.join("; ");
    if reason.len() > PROBE_REASON_BUDGET {
        let mut cut = PROBE_REASON_BUDGET;
        while !reason.is_char_boundary(cut) {
            cut -= 1;
        }
        reason.truncate(cut);
        reason.push_str("...");
    }
    reason
}

/// Go's `Duration.Round(time.Millisecond).String()`.
pub fn format_duration(duration: Duration) -> String {
    let ms = (duration.as_nanos() + 500_000) / 1_000_000;
    if ms == 0 {
        return "0s".to_string();
    }
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let (seconds, fraction) = (ms / 1000, ms % 1000);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let mut text = seconds.to_string();
    if fraction != 0 {
        text.push_str(format!(".{fraction:03}").trim_end_matches('0'));
    }
    text.push('s');
    if hours > 0 {
        format!("{hours}h{minutes}m{text}")
    } else if minutes > 0 {
        format!("{minutes}m{text}")
    } else {
        text
    }
}

/// The encoders worth asking about: every hardware one, plus the CPU encoders the
/// fallback depends on. An ffmpeg built without libx264 leaves Superview with no fallback
/// at all, and without this that only surfaces when a conversion has already failed twice.
pub fn encoders_to_probe(ffmpeg: &HashMap<String, String>) -> Vec<String> {
    split_csv(ffmpeg.get("encoders").map_or("", String::as_str))
        .into_iter()
        .filter(|encoder| {
            is_hardware_encoder(encoder) || encoder == "libx264" || encoder == "libx265"
        })
        .collect()
}

/// A copy of the capability map without the encoders that failed their probe. Only those:
/// an encoder never probed stays, silence is not a refusal. The map is copied so a caller
/// holding the pre-probe capabilities keeps them.
pub fn apply_encoder_probe(
    ffmpeg: &HashMap<String, String>,
    report: Option<&EncoderProbeReport>,
) -> HashMap<String, String> {
    let mut updated = ffmpeg.clone();
    let Some(report) = report else {
        return updated;
    };
    let refused: HashSet<&str> = report
        .results
        .iter()
        .filter(|probe| !probe.usable)
        .map(|probe| probe.encoder.as_str())
        .collect();
    if refused.is_empty() {
        return updated;
    }
    let kept: Vec<String> = split_csv(ffmpeg.get("encoders").map_or("", String::as_str))
        .into_iter()
        .filter(|encoder| !refused.contains(encoder.as_str()))
        .collect();
    updated.insert("encoders".to_string(), kept.join(","));
    updated
}

/// The probe verdicts, for the diagnostic report.
pub fn describe_encoder_probe(report: Option<&EncoderProbeReport>) -> String {
    let Some(report) = report else {
        return "Encoders: not probed\n".to_string();
    };
    if report.results.is_empty() {
        return "Encoders: none to probe\n".to_string();
    }
    let mut text = format!(
        "=== Encoders (probed in {}) ===\n",
        format_duration(report.took)
    );
    for probe in &report.results {
        if probe.usable {
            // The Diagnostic's reader (`report.rs`) takes this line for a pass: the mark
            // and the colon are what it looks for, not the words after them.
            let note = if probe.rate_control == RateControl::ConstantQuality {
                "works, constant quality only"
            } else {
                "works"
            };
            text.push_str(&format!(
                "✅ {}: {note} ({})\n",
                probe.encoder,
                format_duration(probe.took)
            ));
        } else {
            text.push_str(&format!("❌ {}: {}\n", probe.encoder, probe.reason));
        }
    }
    text
}

/// The frame the conversion will produce, in the "WxH" form lavfi sources take; "" when
/// there is no video, no stream or no usable size.
pub fn output_frame_size(video: Option<&VideoSpecs>, squeeze: bool) -> String {
    // remap_output_size indexes the first stream: the empty cases stop here.
    let Some(video) = video.filter(|video| !video.streams.is_empty()) else {
        return String::new();
    };
    let (out_x, out_y) = remap_output_size(video, squeeze);
    if out_x <= 0 || out_y <= 0 {
        return String::new();
    }
    format!("{out_x}x{out_y}")
}

/// Remembers what each encoder answered for a given frame size.
///
/// Session-lifetime, on the same assumption as the startup sweep: the driver's answer for
/// one encoder at one size does not change while the application runs. It matters because
/// the window asks again on every change of file, of the squeeze checkbox and of the
/// codec, and a probe costs about 0.30 s.
#[derive(Debug, Default)]
pub struct ProbeCache {
    verdicts: Mutex<HashMap<String, EncoderProbe>>,
}

impl ProbeCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, EncoderProbe>> {
        self.verdicts.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, key: &str) -> Option<EncoderProbe> {
        self.lock().get(key).cloned()
    }

    pub fn insert(&self, key: String, probe: EncoderProbe) {
        self.lock().insert(key, probe);
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Empties the cache (`resetSizeProbeCache`).
    pub fn reset(&self) {
        self.lock().clear();
    }
}

pub(crate) fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
}

/// Asks encoders to encode a frame, through the injected tool resolver.
pub struct Prober<'a> {
    resolver: &'a ToolResolver,
    timeout: Duration,
    cache: ProbeCache,
}

impl<'a> Prober<'a> {
    pub fn new(resolver: &'a ToolResolver) -> Self {
        Self {
            resolver,
            timeout: PROBE_TIMEOUT,
            cache: ProbeCache::default(),
        }
    }

    /// Replaces the probe's own timeout (tests only, in practice).
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn cache(&self) -> &ProbeCache {
        &self.cache
    }

    /// Encodes one frame with the given encoder and reports what happened. A refusal is
    /// the answer, not a failure: there is no error to return.
    pub fn probe_encoder(&self, encoder: &str, cancel: Option<&AtomicBool>) -> EncoderProbe {
        self.probe_encoder_at_size(&sweep_spec(encoder), cancel)
    }

    /// [`Self::probe_encoder`] with the whole question spelled out, for the caller that
    /// knows what the conversion is about to produce.
    ///
    /// Measured: a probe that succeeds costs about 0.30 s and the size does not move it
    /// (0.29 s at 256x256, 0.33 s at 7082x3984 on h264_nvenc): it is the process start and
    /// the device handshake being paid for. A refusal comes back in 0.02 s.
    ///
    /// A VAAPI driver that refuses the bit rate with "supported modes: CQP" is asked a
    /// second time, the same frame in constant quality (DEC-48); the refusal stays in the
    /// log and the returned verdict carries the mode that worked.
    pub fn probe_encoder_at_size(
        &self,
        spec: &ProbeSpec,
        cancel: Option<&AtomicBool>,
    ) -> EncoderProbe {
        let first = self.probe_once(spec, cancel);
        if first.usable
            || spec.rate_control != RateControl::Bitrate
            || is_cancelled(cancel)
            || !is_cqp_only_refusal(&spec.encoder, &first.reason)
        {
            return first;
        }
        log::warn!(
            "The driver only knows constant quality; asking again in that mode: encoder={} reason={}",
            spec.encoder,
            first.reason
        );
        let second = self.probe_once(
            &ProbeSpec {
                rate_control: RateControl::ConstantQuality,
                ..spec.clone()
            },
            cancel,
        );
        if second.usable {
            return EncoderProbe {
                took: first.took + second.took,
                ..second
            };
        }
        // A cancelled second question answered nothing: the first refusal stands alone.
        if is_cancelled(cancel) {
            return first;
        }
        // Neither mode works. Both reasons are kept: "supported modes: CQP" alone would
        // send the user after a firmware when the constant-quality attempt failed for
        // another reason (a frame size, a timeout).
        log::warn!(
            "The encoder refused constant quality too: encoder={} bitrate_reason={} constant_quality_reason={}",
            spec.encoder,
            first.reason,
            second.reason
        );
        EncoderProbe {
            reason: format!("{}; in constant quality: {}", first.reason, second.reason),
            took: first.took + second.took,
            ..first
        }
    }

    /// One question, one process.
    fn probe_once(&self, spec: &ProbeSpec, cancel: Option<&AtomicBool>) -> EncoderProbe {
        let started = Instant::now();
        let cmd = self.resolver.ffmpeg_command(probe_args(spec));
        let outcome = run_with_deadline(cmd, self.timeout, cancel);
        let took = started.elapsed();

        let mut probe = EncoderProbe {
            encoder: spec.encoder.clone(),
            usable: false,
            reason: String::new(),
            took,
            rate_control: spec.rate_control,
        };
        let (output, run_err, timed_out) = match outcome {
            Ok(done)
                if done.end == RunEnd::Finished && done.status.is_some_and(|s| s.success()) =>
            {
                probe.usable = true;
                return probe;
            }
            Ok(done) => {
                let run_err = done
                    .status
                    .map_or_else(|| "cancelled".to_string(), |status| status.to_string());
                (done.output, run_err, done.end == RunEnd::TimedOut)
            }
            Err(e) => (String::new(), e.to_string(), false),
        };
        probe.reason = summarize_probe_failure(&output, &run_err, timed_out, took);
        probe
    }

    /// Asks each encoder in turn, and stops when `cancel` is raised.
    ///
    /// Sequentially, deliberately: hardware encoders have a limited number of concurrent
    /// sessions, and several probes at once on the same GPU can make a good encoder
    /// report failure because the previous probe still holds a session.
    pub fn probe_encoders(
        &self,
        encoders: &[String],
        cancel: Option<&AtomicBool>,
    ) -> EncoderProbeReport {
        let started = Instant::now();
        let mut report = EncoderProbeReport::default();
        for encoder in encoders {
            if is_cancelled(cancel) {
                break;
            }
            report.results.push(self.probe_encoder(encoder, cancel));
        }
        report.took = started.elapsed();
        report
    }

    /// Probes what the ffmpeg capability map advertises, and logs the verdict.
    pub fn probe_hardware_support(
        &self,
        ffmpeg: &HashMap<String, String>,
        cancel: Option<&AtomicBool>,
    ) -> EncoderProbeReport {
        let report = self.probe_encoders(&encoders_to_probe(ffmpeg), cancel);
        for probe in &report.results {
            if probe.usable {
                log::debug!(
                    "Encoder probe succeeded: encoder={} took={}",
                    probe.encoder,
                    format_duration(probe.took)
                );
            } else {
                // Warn, not debug: this is the line that explains an encode running
                // three times slower than the user expects.
                log::warn!(
                    "Encoder is advertised but refused the work: encoder={} reason={}",
                    probe.encoder,
                    probe.reason
                );
            }
        }
        report
    }

    /// [`Self::probe_encoder_at_size`] behind the cache.
    pub fn probe_encoder_for_size_cached(
        &self,
        spec: &ProbeSpec,
        cancel: Option<&AtomicBool>,
    ) -> EncoderProbe {
        let key = spec.cache_key();
        if let Some(cached) = self.cache.get(&key) {
            return cached;
        }
        let probe = self.probe_encoder_at_size(spec, cancel);
        // A probe cut short by its caller answered nothing about the driver, and
        // remembering it would demote a working encoder for the whole session (E-23).
        // Only the probe's own timeout is a verdict.
        if is_cancelled(cancel) {
            return probe;
        }
        self.cache.insert(key, probe.clone());
        probe
    }

    /// Asks a hardware encoder whether it will take the frame this conversion is about to
    /// produce, and steps back to the CPU when it will not. Returns the encoder to use
    /// and, when that is not the one it was given, ffmpeg's own words for why.
    ///
    /// The startup sweep encodes 256x256, the only size available before a file is
    /// chosen. It proves the driver answers, not that it takes a 15-megapixel frame: an
    /// Intel HD 620 answers, at 5120x2880, "Hardware does not support encoding at size
    /// 5120x2880 (constraints: width 32-4096 height 32-4096)". Asking here makes the
    /// announcement, the memory estimate and the encoder that runs the same three things.
    ///
    /// Software encoders are returned untouched: they have no device to consult, and a
    /// probe would only cost the conversion a process start. `video_preset` is the preset
    /// the conversion will pass, so the probe passes it too; "" asks without one.
    pub fn verify_encoder_for_output(
        &self,
        encoder: &str,
        video: Option<&VideoSpecs>,
        squeeze: bool,
        ffmpeg: &HashMap<String, String>,
        video_preset: &str,
        cancel: Option<&AtomicBool>,
    ) -> (String, String) {
        let (encoder, refusal, _) = self.verify_encoder_with_rate_control(
            encoder,
            video,
            squeeze,
            ffmpeg,
            video_preset,
            cancel,
        );
        (encoder, refusal)
    }

    /// [`Self::verify_encoder_for_output`], plus the rate control the returned encoder must
    /// be given: [`RateControl::ConstantQuality`] only when a VAAPI driver refused a bit
    /// rate and took a fixed quantiser (DEC-48), otherwise [`RateControl::Bitrate`] -- which
    /// is also what a CPU encoder stepped back to gets, since x264 and x265 honour a rate.
    pub fn verify_encoder_with_rate_control(
        &self,
        encoder: &str,
        video: Option<&VideoSpecs>,
        squeeze: bool,
        ffmpeg: &HashMap<String, String>,
        video_preset: &str,
        cancel: Option<&AtomicBool>,
    ) -> (String, String, RateControl) {
        if encoder.is_empty() || !is_hardware_encoder(encoder) {
            return (encoder.to_string(), String::new(), RateControl::Bitrate);
        }
        let size = output_frame_size(video, squeeze);
        if size.is_empty() {
            return (encoder.to_string(), String::new(), RateControl::Bitrate);
        }

        let probe = self.probe_encoder_for_size_cached(
            &ProbeSpec {
                encoder: encoder.to_string(),
                size: size.clone(),
                pix_fmt: source_pixel_format(video).to_string(),
                bitrate: PROBE_BITRATE,
                preset: video_preset.to_string(),
                rate_control: RateControl::Bitrate,
            },
            cancel,
        );
        if probe.usable {
            return (encoder.to_string(), String::new(), probe.rate_control);
        }

        let reason = format!(
            "{encoder} cannot encode {size} on this machine: {}",
            probe.reason
        );
        let profile = analyze_machine_profile(ffmpeg, 0);
        let fallback = software_encoder_for_fallback(video, encoder, Some(&profile));
        if fallback.is_empty() {
            // Nothing to step back to. Say so and leave the encoder alone: the cascade
            // in the encode is then the only thing left, exactly as before.
            log::warn!(
                "The hardware encoder refuses this frame and there is no CPU encoder to fall back to: encoder={encoder} output_size={size} reason={}",
                probe.reason
            );
            return (encoder.to_string(), reason, RateControl::Bitrate);
        }
        log::warn!(
            "The hardware encoder refuses this frame; using the CPU encoder instead: encoder={encoder} output_size={size} reason={} using={fallback}",
            probe.reason
        );
        (fallback, reason, RateControl::Bitrate)
    }

    /// `describe_hardware_acceleration_plan` with the hardware encoder actually asked
    /// about the frame it would be given. It can spend a probe, so it belongs off the UI
    /// thread; the cache makes every repeat of the same question free.
    pub fn describe_verified_hardware_plan(
        &self,
        ffmpeg: &HashMap<String, String>,
        video: Option<&VideoSpecs>,
        squeeze: bool,
        requested_encoder: &str,
        cancel: Option<&AtomicBool>,
    ) -> String {
        if video.is_none() {
            return "Hardware: waiting for input video".to_string();
        }
        let mut plan = match build_hardware_plan(ffmpeg, video, requested_encoder) {
            Ok(plan) => plan,
            Err(err) => return format!("Hardware: {err}"),
        };

        let (verified, refusal, rate_control) = self.verify_encoder_with_rate_control(
            &plan.encoder,
            video,
            squeeze,
            ffmpeg,
            "",
            cancel,
        );
        if refusal.is_empty() {
            let line = describe_planned_hardware_path(&plan);
            return if rate_control == RateControl::ConstantQuality {
                format!("{line} (constant quality)")
            } else {
                line
            };
        }

        // Rebuild the plan around the encoder that will really run, so the decode path
        // and the codec-switch note describe it and not the one that refused.
        let profile = analyze_machine_profile(ffmpeg, 0);
        plan = HardwarePlan {
            decode_accel: select_hardware_decode_accel(&verified, Some(&profile)),
            hardware_encode: is_hardware_encoder(&verified),
            codec_switch: describe_codec_family_switch(video, &verified),
            encoder: verified,
        };
        format!("{} -- {refusal}", describe_planned_hardware_path(&plan))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::Config;
    use crate::hardware::HW_DEVICE_ALIAS;
    use crate::process::tests::spawn_lock;
    use crate::tools::check_ffmpeg;
    use crate::video::VideoStream;

    // ---- fixtures ----

    /// Go: hevcSource (crossfamily_test.go).
    fn hevc_source(pix_fmt: &str) -> VideoSpecs {
        VideoSpecs {
            file: "clip.mp4".into(),
            streams: vec![VideoStream {
                codec: "hevc".into(),
                width: 3840,
                height: 2880,
                pix_fmt: pix_fmt.into(),
                duration_float: 43.0,
                bitrate_int: 120_000_000,
                ..VideoStream::default()
            }],
        }
    }

    fn caps(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Go: vaapiMachine, the laptop from the report.
    fn vaapi_machine() -> HashMap<String, String> {
        caps(&[
            ("encoders", "h264_vaapi,libx264,libx265"),
            ("accels", "vaapi"),
        ])
    }

    /// Go: hd620Refusal.
    const HD620_REFUSAL: &str = "[h264_vaapi @ 0x55] Hardware does not support encoding at size 5120x2880 (constraints: width 32-4096 height 32-4096)";

    /// A resolver that never finds anything: for tests that must not start a process.
    fn no_tools() -> ToolResolver {
        ToolResolver::new(|_| None, None)
    }

    /// Go: seedSizeProbe. The key carries the source depth too (E-01), so the verdict is
    /// seeded for every depth the fixtures use.
    fn seed(prober: &Prober, encoder: &str, size: &str, usable: bool, reason: &str) {
        for pix_fmt in ["", "yuv420p", "yuv420p10le"] {
            let spec = ProbeSpec {
                encoder: encoder.into(),
                size: size.into(),
                pix_fmt: pix_fmt.into(),
                bitrate: PROBE_BITRATE,
                preset: String::new(),
                rate_control: RateControl::Bitrate,
            };
            prober.cache().insert(
                spec.cache_key(),
                EncoderProbe {
                    encoder: encoder.into(),
                    usable,
                    reason: reason.into(),
                    took: Duration::ZERO,
                    rate_control: RateControl::Bitrate,
                },
            );
        }
    }

    pub(crate) struct FakeTools {
        _dir: tempfile::TempDir,
        _empty: tempfile::TempDir,
        pub resolver: ToolResolver,
    }

    /// Writes the stand-ins (`name`, file content, executable bit) into a directory that
    /// goes first on a PATH that still holds the real one (the scripts call `sleep`,
    /// `ping`, `echo`...). The bundled lookup points at an empty directory; the override
    /// is unset.
    fn install_tools(files: &[(String, String)]) -> FakeTools {
        let (dir, empty) = (
            tempfile::TempDir::new().unwrap(),
            tempfile::TempDir::new().unwrap(),
        );
        for (name, content) in files {
            let file = dir.path().join(name);
            std::fs::write(&file, content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let real = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&real)),
        )
        .unwrap();
        let resolver = ToolResolver::new(
            move |key| (key == "PATH").then(|| path.clone()),
            Some(empty.path().to_path_buf()),
        )
        .with_batch_stand_ins();
        FakeTools {
            _dir: dir,
            _empty: empty,
            resolver,
        }
    }

    /// Shell stand-ins: each `(name, script body)` becomes an executable script.
    #[cfg(unix)]
    pub(crate) fn fake_tools(scripts: &[(&str, &str)]) -> FakeTools {
        let files: Vec<(String, String)> = scripts
            .iter()
            .map(|(name, body)| (name.to_string(), format!("#!/bin/sh\n{body}\n")))
            .collect();
        install_tools(&files)
    }

    /// Batch stand-ins, found on `PATH` through `with_batch_stand_ins` (`ffmpeg.bat`).
    #[cfg(windows)]
    pub(crate) fn fake_tools(scripts: &[(&str, &str)]) -> FakeTools {
        let files: Vec<(String, String)> = scripts
            .iter()
            .map(|(name, body)| (format!("{name}.bat"), format!("@echo off\r\n{body}\r\n")))
            .collect();
        install_tools(&files)
    }

    /// Go: fakeHangingFFmpeg. On Unix `exec`, so the process the deadline kills is the
    /// one that sleeps and holds no pipe behind it. On Windows the `.bat` runs under
    /// cmd.exe, which the kill reaches alone: `ping` keeps the pipe open after it, which
    /// is what the bounded wait for the output is for.
    fn hanging_ffmpeg() -> FakeTools {
        #[cfg(unix)]
        let body = "exec sleep 30";
        #[cfg(windows)]
        let body = "ping -n 30 127.0.0.1 >nul";
        fake_tools(&[("ffmpeg", body)])
    }

    /// Go: fakeRefusingFFmpeg: records the argv of every call in `ledger` and refuses.
    fn refusing_ffmpeg(ledger: &std::path::Path) -> FakeTools {
        #[cfg(unix)]
        let body = format!(
            "echo \"call $*\" >> '{}'\necho 'Hardware does not support encoding at size' 1>&2\nexit 1",
            ledger.display()
        );
        #[cfg(windows)]
        let body = format!(
            "echo call %* >> \"{}\"\r\necho Hardware does not support encoding at size 1>&2\r\nexit /b 1",
            ledger.display()
        );
        fake_tools(&[("ffmpeg", &body)])
    }

    fn joined(args: &[String]) -> String {
        args.join(" ")
    }

    // ---- the argv ----

    // Go: TestProbeArgs_DeviceFrameEncodersGetADeviceAndAnUpload
    #[test]
    fn device_frame_encoders_get_a_device_and_an_upload() {
        for (encoder, device_type) in [
            ("h264_vaapi", "vaapi"),
            ("hevc_vulkan", "vulkan"),
            ("h264_d3d12va", "d3d12va"),
        ] {
            let args = joined(&probe_args(&sweep_spec(encoder)));
            for needed in [
                format!("-init_hw_device {device_type}={HW_DEVICE_ALIAS}"),
                format!("-filter_hw_device {HW_DEVICE_ALIAS}"),
                "format=nv12,hwupload".to_string(),
            ] {
                assert!(
                    args.contains(&needed),
                    "the {encoder} probe is missing {needed:?}: {args}"
                );
            }
        }
        // NVENC, AMF and QSV upload for themselves; a device they do not need would fail
        // on machines that cannot create one.
        for encoder in ["h264_nvenc", "hevc_amf", "h264_qsv", "libx264"] {
            let args = joined(&probe_args(&sweep_spec(encoder)));
            for unwanted in ["-init_hw_device", "hwupload"] {
                assert!(
                    !args.contains(unwanted),
                    "the {encoder} probe should not carry {unwanted:?}: {args}"
                );
            }
        }
    }

    /// Go: valueOf.
    fn value_of<'a>(args: &'a [String], option: &str) -> &'a str {
        args.windows(2)
            .find(|pair| pair[0] == option)
            .map_or("", |pair| pair[1].as_str())
    }

    // Go: TestProbeAndConversionAskTheSameQuestion. The whole argv of both builders is
    // compared, minus the differences listed here.
    #[test]
    fn probe_and_conversion_ask_the_same_question() {
        use crate::encode::args::{build_encode_base_args, remap_filter_chain};
        use std::collections::BTreeMap;

        // Options the probe is allowed to lack or to differ on, with their value count.
        let conversion_only: BTreeMap<&str, usize> = [
            ("-progress", 1),
            ("-y", 0),
            ("-i", 1),
            ("-filter_complex", 1),
            ("-map", 1),
            ("-map_metadata", 1),
            ("-c:a", 1),
            ("-threads", 1),
            ("-filter_threads", 1),
            ("-x265-params", 1),
        ]
        .into();
        let probe_only: BTreeMap<&str, usize> = [("-f", 1), ("-i", 1), ("-vf", 1)].into();

        // The argv without the listed options and without the trailing output (a path
        // for the conversion, "-" for the probe).
        let strip = |args: &[String], drop: &BTreeMap<&str, usize>| -> Vec<String> {
            let mut kept = Vec::new();
            let mut i = 0;
            while i + 1 < args.len() {
                if let Some(n) = drop.get(args[i].as_str()) {
                    i += 1 + n;
                    continue;
                }
                kept.push(args[i].clone());
                i += 1;
            }
            kept
        };

        let video = hevc_source("yuv420p10le");
        let video = VideoSpecs {
            file: "in.mp4".into(),
            streams: vec![VideoStream {
                width: 1440,
                height: 1080,
                duration_float: 10.0,
                bitrate_int: 8_000_000,
                ..video.streams[0].clone()
            }],
        };
        for encoder in [
            "h264_nvenc",
            "hevc_nvenc",
            "hevc_amf",
            "h264_qsv",
            "h264_vaapi",
            "hevc_vaapi",
            "hevc_vulkan",
            "h264_d3d12va",
            "h264_v4l2m2m",
            "libx264",
            "libx265",
        ] {
            // Both modes are one question asked twice (DEC-05, DEC-48): constant quality
            // goes through `encoder_facing_args` like the bit rate does.
            for (rate_control, preset) in [RateControl::Bitrate, RateControl::ConstantQuality]
                .into_iter()
                .flat_map(|mode| ["", "fast", "veryslow"].map(move |preset| (mode, preset)))
            {
                let conversion = build_encode_base_args(
                    &video,
                    "x.pgm",
                    "y.pgm",
                    encoder,
                    rate_control,
                    8_000_000,
                    "aac",
                    4,
                    0,
                    preset,
                );
                let probe = probe_args(&ProbeSpec {
                    encoder: encoder.into(),
                    size: "1920x1080".into(),
                    pix_fmt: source_pixel_format(Some(&video)).into(),
                    bitrate: 8_000_000,
                    preset: preset.into(),
                    rate_control,
                });
                let (got, want) = (
                    joined(&strip(&probe, &probe_only)),
                    joined(&strip(&conversion, &conversion_only)),
                );
                assert_eq!(
                    got, want,
                    "{encoder} {rate_control:?} preset={preset:?}: the probe and the conversion ask the encoder different questions"
                );

                // And the pixel-format steps: everything after remap in the conversion
                // must be the probe's whole -vf chain.
                let chain = remap_filter_chain(source_pixel_format(Some(&video)), encoder);
                let after_remap = chain
                    .split_once("remap,")
                    .unwrap()
                    .1
                    .strip_suffix("[v]")
                    .unwrap();
                assert_eq!(value_of(&probe, "-vf"), after_remap, "{encoder}");
            }
        }
    }

    // Go: TestProbeArgs_NamesTheEncoderAndDiscardsOutput
    #[test]
    fn names_the_encoder_and_discards_output() {
        let args = joined(&probe_args(&sweep_spec("hevc_nvenc")));
        assert!(args.contains("-c:v hevc_nvenc"), "{args}");
        assert!(args.ends_with("-f null -"), "{args}");
        assert!(args.contains("-loglevel error"), "{args}");
    }

    // Go: TestProbeArgs_UsesTheSizeItIsGiven
    #[test]
    fn uses_the_size_it_is_given() {
        let args = joined(&probe_args(&ProbeSpec {
            encoder: "h264_vaapi".into(),
            size: "5120x2880".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        }));
        assert!(args.contains("nullsrc=s=5120x2880"), "{args}");
        assert!(!args.contains(PROBE_FRAME_SIZE), "{args}");
    }

    // Go: TestProbeArgs_AskForRateControl
    #[test]
    fn asks_for_rate_control() {
        let args = joined(&probe_args(&sweep_spec("h264_vaapi")));
        for needed in ["-b:v ", "-maxrate ", "-bufsize "] {
            assert!(args.contains(needed), "no {needed:?}: {args}");
        }
    }

    // Go: TestProbeArgs_AskAtTheSourceDepth
    #[test]
    fn asks_at_the_source_depth() {
        let spec = |pix_fmt: &str| ProbeSpec {
            encoder: "hevc_vaapi".into(),
            size: "1920x1080".into(),
            pix_fmt: pix_fmt.into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        };
        let ten = joined(&probe_args(&spec("yuv420p10le")));
        assert!(
            ten.contains("format=yuv444p10le,format=yuv420p10le,format=p010,hwupload"),
            "a 10-bit source must be probed in 10 bits: {ten}"
        );
        let eight = joined(&probe_args(&spec("yuv420p")));
        assert!(!eight.contains("10le"), "{eight}");
    }

    #[test]
    fn cache_key_tells_every_field_apart() {
        let base = sweep_spec("libx264");
        assert_eq!(base.cache_key(), "libx264@256x256//2000000//bitrate");
        let mut keys = vec![base.cache_key()];
        for changed in [
            ProbeSpec {
                encoder: "libx265".into(),
                ..base.clone()
            },
            ProbeSpec {
                size: "640x480".into(),
                ..base.clone()
            },
            ProbeSpec {
                pix_fmt: "yuv420p10le".into(),
                ..base.clone()
            },
            ProbeSpec {
                bitrate: 1,
                ..base.clone()
            },
            ProbeSpec {
                preset: "slow".into(),
                ..base.clone()
            },
            ProbeSpec {
                rate_control: RateControl::ConstantQuality,
                ..base.clone()
            },
        ] {
            keys.push(changed.cache_key());
        }
        let distinct: HashSet<&String> = keys.iter().collect();
        assert_eq!(distinct.len(), keys.len(), "{keys:?}");
    }

    // ---- the reason ----

    // Rust only (DEC-48). libva prints three informational lines before any VAAPI or QSV
    // session, and they filled the whole three-line reason: a QSV refusal read
    // "libva info: VA-API version 1.23.0; libva info: Trying to open ...; libva info: Found init
    // function" and the actual error was cut off (RC-0.3.0, Tiger Lake). They are not a cause.
    #[test]
    fn libva_information_lines_do_not_crowd_out_the_error() {
        let ms = Duration::from_millis;
        let qsv = "libva info: VA-API version 1.23.0\n\
                   libva info: Trying to open /usr/lib/x86_64-linux-gnu/dri/iHD_drv_video.so\n\
                   libva info: Found init function __vaDriverInit_1_22\n\
                   [h264_qsv @ 0x59] Error creating a MFX session: -9.\n\
                   [vost#0:0/h264_qsv @ 0x59] Error while opening encoder\n";
        let got = summarize_probe_failure(qsv, "exit status: 218", false, ms(0));
        assert!(!got.contains("libva info"), "{got}");
        assert!(got.contains("Error creating a MFX session: -9."), "{got}");

        // The CQP-only marker must survive the same noise in front of it.
        let vaapi = "libva info: VA-API version 1.23.0\nlibva info: Trying to open x\nlibva info: Found init function y\n\
             [h264_vaapi @ 0x59] Driver does not support any RC mode compatible with selected options (supported modes: CQP).\n";
        let got = summarize_probe_failure(vaapi, "exit status: 234", false, ms(0));
        assert!(is_cqp_only_refusal("h264_vaapi", &got), "{got}");

        // Only the information lines go: a libva *error* is a cause and stays; and a probe
        // whose whole output is noise still says something.
        let err = "libva error: vaGetDriverNames() failed with unknown libva error\n";
        assert!(summarize_probe_failure(err, "x", false, ms(0)).contains("libva error"));
        assert_eq!(
            summarize_probe_failure("libva info: only noise\n", "exit status: 1", false, ms(0)),
            "exit status: 1"
        );
    }

    // Go: TestSummarizeProbeFailure
    #[test]
    fn summarizes_a_failure_in_one_line() {
        let ms = Duration::from_millis;
        let nvenc = "[h264_nvenc @ 0x55] Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0\n\
                     [h264_nvenc @ 0x55] The minimum required Nvidia driver for nvenc is 610.00 or newer\n";
        // NVENC prints the mismatch and then the driver it wants; both lines have to
        // survive, because together they tell the user what to do.
        let got = summarize_probe_failure(nvenc, "exit status: 1", false, ms(0));
        assert!(got.contains("Required: 13.1 Found: 13.0"), "{got}");
        assert!(got.contains("610.00 or newer"), "{got}");

        assert_eq!(
            summarize_probe_failure("   \n\n", "exit status: 218", false, ms(0)),
            "exit status: 218",
            "a silent failure still says something"
        );
        assert_eq!(
            summarize_probe_failure("", "killed", true, ms(302)),
            "gave no answer in 302ms",
            "a deadline reports what was actually waited"
        );
        // The cause first, the consequences after: keeping the tail would keep the
        // restatements and throw away the only line that identifies the problem.
        assert_eq!(
            summarize_probe_failure(
                "Unknown encoder 'nope'\nError selecting an encoder\nError opening output file -.\nError opening output files: Encoder not found\n",
                "exit status: 1",
                false,
                ms(0)
            ),
            "Unknown encoder 'nope'; Error selecting an encoder; Error opening output file -."
        );
        let long = "x".repeat(PROBE_REASON_BUDGET + 50);
        let got = summarize_probe_failure(&long, "exit status: 1", false, ms(0));
        assert!(got.ends_with("..."), "a talkative failure is capped: {got}");
        assert_eq!(got.len(), PROBE_REASON_BUDGET + 3);
    }

    // No Go test: the constants are the measured Go ones, spelled out rather than read
    // back from the code they govern.
    #[test]
    fn the_constants_are_gos() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(15));
        assert_eq!(PROBE_FRAME_SIZE, "256x256");
        assert_eq!(PROBE_REASON_LINES, 3);
        assert_eq!(PROBE_REASON_BUDGET, 300);
        assert_eq!(PROBE_BITRATE, 2_000_000);
        let long = "x".repeat(1000);
        let got = summarize_probe_failure(&long, "x", false, Duration::ZERO);
        assert_eq!(got, format!("{}...", "x".repeat(300)));
    }

    // No Go test: Go cuts a reason at 300 bytes, mid-character if need be; Rust cannot.
    #[test]
    fn a_reason_is_cut_on_a_character_boundary() {
        // One ASCII byte, then two-byte characters: byte 300 falls inside a character
        // (299 and 300 are the two halves of one), so the cut must back off to 299.
        let long = format!("x{}", "é".repeat(PROBE_REASON_BUDGET));
        let got = summarize_probe_failure(&long, "x", false, Duration::ZERO);
        assert_eq!(got.len(), PROBE_REASON_BUDGET - 1 + 3);
        assert!(got.ends_with("é..."), "{got}");
    }

    // No Go test: Windows ffmpeg ends its lines with CRLF.
    #[test]
    fn crlf_lines_leave_no_carriage_return() {
        let got = summarize_probe_failure("one\r\ntwo\r\n", "x", false, Duration::ZERO);
        assert_eq!(got, "one; two");
    }

    // No Go test: Duration.String after Round(time.Millisecond).
    #[test]
    fn durations_read_like_go_after_rounding_to_a_millisecond() {
        let ms = Duration::from_millis;
        assert_eq!(format_duration(Duration::from_micros(499)), "0s");
        assert_eq!(format_duration(Duration::from_micros(500)), "1ms");
        assert_eq!(format_duration(ms(302)), "302ms");
        assert_eq!(format_duration(ms(999)), "999ms");
        assert_eq!(format_duration(ms(1000)), "1s");
        assert_eq!(format_duration(ms(1500)), "1.5s");
        assert_eq!(format_duration(ms(15_005)), "15.005s");
        assert_eq!(format_duration(ms(62_500)), "1m2.5s");
        assert_eq!(format_duration(ms(60_000)), "1m0s");
        assert_eq!(format_duration(ms(3_600_000)), "1h0m0s");
        assert_eq!(format_duration(ms(3_723_000)), "1h2m3s");
    }

    // ---- which encoders, what the verdict does ----

    // Go: TestEncodersToProbe
    #[test]
    fn encoders_to_probe_keeps_hardware_and_the_cpu_pair() {
        let ffmpeg = caps(&[(
            "encoders",
            "libx264,libx264rgb,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc",
        )]);
        // h264_vulkan is in there: a hardware encoder like the rest, and it survives an
        // NVENC the driver refuses. Probing it is how that survival gets established.
        assert_eq!(
            encoders_to_probe(&ffmpeg).join(","),
            "libx264,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc"
        );
    }

    // Go: TestEncodersToProbe_EmptyCapabilities
    #[test]
    fn nothing_advertised_means_nothing_to_probe() {
        assert!(encoders_to_probe(&HashMap::new()).is_empty());
    }

    fn refused(encoder: &str) -> EncoderProbe {
        EncoderProbe {
            encoder: encoder.into(),
            reason: "driver too old".into(),
            ..EncoderProbe::default()
        }
    }

    fn worked(encoder: &str) -> EncoderProbe {
        EncoderProbe {
            encoder: encoder.into(),
            usable: true,
            ..EncoderProbe::default()
        }
    }

    // Go: TestApplyEncoderProbe_RemovesOnlyWhatWasRefused
    #[test]
    fn apply_removes_only_what_was_refused() {
        let ffmpeg = caps(&[
            ("version", "8.1.2"),
            ("encoders", "libx264,h264_nvenc,h264_vulkan,hevc_nvenc"),
        ]);
        let report = EncoderProbeReport {
            results: vec![
                worked("libx264"),
                refused("h264_nvenc"),
                refused("hevc_nvenc"),
            ],
            took: Duration::ZERO,
        };
        let updated = apply_encoder_probe(&ffmpeg, Some(&report));
        assert_eq!(
            updated["encoders"], "libx264,h264_vulkan",
            "an unprobed encoder must survive"
        );
        assert_eq!(updated["version"], "8.1.2");
    }

    // Go: TestApplyEncoderProbe_DoesNotMutateItsInput
    #[test]
    fn apply_does_not_edit_the_map_it_was_given() {
        let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc")]);
        let report = EncoderProbeReport {
            results: vec![refused("h264_nvenc")],
            took: Duration::ZERO,
        };
        let updated = apply_encoder_probe(&ffmpeg, Some(&report));
        assert_eq!(updated["encoders"], "libx264");
        assert_eq!(ffmpeg["encoders"], "libx264,h264_nvenc");
    }

    // Go: TestApplyEncoderProbe_NilReportChangesNothing
    #[test]
    fn apply_without_a_report_changes_nothing() {
        let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc")]);
        assert_eq!(
            apply_encoder_probe(&ffmpeg, None)["encoders"],
            "libx264,h264_nvenc"
        );
    }

    // Go: TestEncoderProbeReport_FindAndUnusable (a nil report is an Option here, see the
    // header).
    #[test]
    fn find_and_unusable() {
        let report = EncoderProbeReport {
            results: vec![worked("libx264"), refused("h264_nvenc")],
            took: Duration::ZERO,
        };
        assert_eq!(report.find("h264_nvenc").unwrap().reason, "driver too old");
        assert!(report.find("hevc_qsv").is_none(), "never probed");
        let unusable = report.unusable();
        assert_eq!(unusable.len(), 1);
        assert_eq!(unusable[0].encoder, "h264_nvenc");
    }

    // Go: TestDescribeEncoderProbe_CarriesTheReason
    #[test]
    fn describe_carries_the_reason() {
        let report = EncoderProbeReport {
            took: Duration::from_millis(812),
            results: vec![
                EncoderProbe {
                    took: Duration::from_millis(40),
                    ..worked("libx264")
                },
                EncoderProbe {
                    encoder: "h264_nvenc".into(),
                    reason: "The minimum required Nvidia driver for nvenc is 610.00 or newer"
                        .into(),
                    ..EncoderProbe::default()
                },
            ],
        };
        assert_eq!(
            describe_encoder_probe(Some(&report)),
            "=== Encoders (probed in 812ms) ===\n\
             ✅ libx264: works (40ms)\n\
             ❌ h264_nvenc: The minimum required Nvidia driver for nvenc is 610.00 or newer\n"
        );
    }

    // Go: TestDescribeEncoderProbe_DegradedCases
    #[test]
    fn describe_degraded_cases() {
        assert_eq!(describe_encoder_probe(None), "Encoders: not probed\n");
        assert_eq!(
            describe_encoder_probe(Some(&EncoderProbeReport::default())),
            "Encoders: none to probe\n"
        );
    }

    // ---- the sweep, with a cancellation ----

    // Go: TestProbeEncoders_StopsOnCancellation
    #[test]
    fn a_cancelled_sweep_probes_nothing() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        let cancel = AtomicBool::new(true);
        let encoders = ["libx264", "libx265", "h264_nvenc"].map(String::from);
        let report = prober.probe_encoders(&encoders, Some(&cancel));
        assert!(report.results.is_empty(), "{:?}", report.results);
    }

    // Go: TestSizeProbeCache_DoesNotRememberACancelledProbe (E-23). Nothing is started
    // here (the flag is up at the start), so it runs on every platform.
    #[test]
    fn a_probe_cancelled_before_it_starts_is_not_cached() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        let cancel = AtomicBool::new(true);
        let spec = ProbeSpec {
            encoder: "h264_vaapi".into(),
            size: "640x480".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        };
        let probe = prober.probe_encoder_for_size_cached(&spec, Some(&cancel));
        assert!(
            !probe.usable,
            "a cancelled probe cannot report the encoder usable"
        );
        assert_eq!(probe.reason, "cancelled");
        assert!(
            prober.cache().get(&spec.cache_key()).is_none(),
            "a cancelled probe was cached as the driver's answer"
        );
    }

    // Go: TestSizeProbeCache_DoesNotRememberACancelledProbe, with a probe actually cut
    // short while it runs.
    #[test]
    fn a_probe_cut_short_while_running_is_not_cached() {
        let _guard = spawn_lock();
        let fake = hanging_ffmpeg();
        let prober = Prober::new(&fake.resolver);
        let cancel = AtomicBool::new(false);
        let spec = ProbeSpec {
            encoder: "h264_vaapi".into(),
            size: "640x480".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        };
        let started = Instant::now();
        let probe = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                cancel.store(true, Ordering::Release);
            });
            prober.probe_encoder_for_size_cached(&spec, Some(&cancel))
        });
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!probe.usable);
        assert!(
            !probe.reason.starts_with("gave no answer"),
            "a cancellation is not the probe's own timeout: {}",
            probe.reason
        );
        assert!(prober.cache().is_empty(), "{:?}", prober.cache());
    }

    // ---- the frame, and verifying an encoder against it ----

    // Go: TestOutputFrameSize
    #[test]
    fn output_frame_size_follows_the_geometry() {
        let video = hevc_source("yuv420p10le"); // 3840x2880
        assert_eq!(output_frame_size(Some(&video), false), "5120x2880");
        // Already stretched: the frame keeps its width.
        assert_eq!(output_frame_size(Some(&video), true), "3840x2880");
        assert_eq!(output_frame_size(None, false), "");
        let no_streams = VideoSpecs {
            file: "x.mp4".into(),
            streams: vec![],
        };
        assert_eq!(output_frame_size(Some(&no_streams), false), "");
    }

    // Go: TestVerifyEncoderForOutput_LeavesSoftwareEncodersAlone
    #[test]
    fn software_encoders_are_left_alone() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        let (encoder, reason) = prober.verify_encoder_for_output(
            "libx265",
            Some(&hevc_source("yuv420p10le")),
            false,
            &vaapi_machine(),
            "",
            None,
        );
        assert_eq!((encoder.as_str(), reason.as_str()), ("libx265", ""));
        assert!(
            prober.cache().is_empty(),
            "a software encoder must not be probed"
        );
    }

    fn verify(
        prober: &Prober,
        video: &VideoSpecs,
        squeeze: bool,
        ffmpeg: &HashMap<String, String>,
    ) -> (String, String) {
        prober.verify_encoder_for_output("h264_vaapi", Some(video), squeeze, ffmpeg, "", None)
    }

    // Go: TestVerifyEncoderForOutput_KeepsAHardwareEncoderThatAcceptsTheFrame
    #[test]
    fn keeps_a_hardware_encoder_that_accepts_the_frame() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", true, "");
        let (encoder, reason) = verify(
            &prober,
            &hevc_source("yuv420p10le"),
            false,
            &vaapi_machine(),
        );
        assert_eq!((encoder.as_str(), reason.as_str()), ("h264_vaapi", ""));
    }

    // Go: TestVerifyEncoderForOutput_StepsBackToTheSourceFamilyWhenRefused
    #[test]
    fn steps_back_to_the_source_family_when_refused() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
        let (encoder, reason) = verify(
            &prober,
            &hevc_source("yuv420p10le"),
            false,
            &vaapi_machine(),
        );
        assert_eq!(encoder, "libx265");
        assert_eq!(
            reason,
            format!("h264_vaapi cannot encode 5120x2880 on this machine: {HD620_REFUSAL}")
        );
    }

    // Go: TestVerifyEncoderForOutput_KeepsTheEncoderWithNothingToStepBackTo
    #[test]
    fn keeps_the_encoder_with_nothing_to_step_back_to() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
        let bare = caps(&[("encoders", "h264_vaapi"), ("accels", "vaapi")]);
        let (encoder, reason) = verify(&prober, &hevc_source("yuv420p10le"), false, &bare);
        assert_eq!(
            encoder, "h264_vaapi",
            "with no CPU encoder the choice stands"
        );
        assert!(!reason.is_empty(), "the refusal must still be reported");
    }

    // Go: TestVerifyEncoderForOutput_FollowsTheSqueezeGeometry
    #[test]
    fn follows_the_squeeze_geometry() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "3840x2880", true, "");
        seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
        let video = hevc_source("yuv420p10le");
        assert_eq!(
            verify(&prober, &video, true, &vaapi_machine()).0,
            "h264_vaapi"
        );
        assert_eq!(
            verify(&prober, &video, false, &vaapi_machine()).0,
            "libx265"
        );
    }

    // No Go test: a missing video or an empty encoder name is answered without a probe.
    #[test]
    fn nothing_to_verify_without_an_encoder_or_a_frame() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        let ffmpeg = vaapi_machine();
        assert_eq!(
            prober.verify_encoder_for_output("", None, false, &ffmpeg, "", None),
            (String::new(), String::new())
        );
        assert_eq!(
            prober.verify_encoder_for_output("h264_vaapi", None, false, &ffmpeg, "", None),
            ("h264_vaapi".to_string(), String::new())
        );
        assert!(prober.cache().is_empty());
    }

    // Go: TestDescribeVerifiedHardwarePlan_NamesWhatWillReallyRun
    #[test]
    fn the_plan_names_what_will_really_run() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
        let line = prober.describe_verified_hardware_plan(
            &vaapi_machine(),
            Some(&hevc_source("yuv420p10le")),
            false,
            "",
            None,
        );
        assert!(line.contains("libx265"), "{line}");
        assert!(
            !line.contains("10-bit source is stored as 8-bit"),
            "the conversion stays in H.265, so nothing is downgraded: {line}"
        );
        assert!(line.contains("5120x2880"), "{line}");
        assert!(
            line.contains(" -- h264_vaapi cannot encode 5120x2880"),
            "{line}"
        );
    }

    // Go: TestDescribeVerifiedHardwarePlan_UnchangedWhenTheHardwareAccepts
    #[test]
    fn the_plan_is_unchanged_when_the_hardware_accepts() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", true, "");
        let line = prober.describe_verified_hardware_plan(
            &vaapi_machine(),
            Some(&hevc_source("yuv420p10le")),
            false,
            "",
            None,
        );
        assert!(line.contains("h264_vaapi"), "{line}");
        assert!(
            line.contains("8-bit"),
            "the codec-switch note must survive: {line}"
        );
        assert!(!line.contains("cannot encode"), "{line}");
    }

    // No Go test: the two early returns of DescribeVerifiedHardwarePlan.
    #[test]
    fn the_plan_without_a_video_or_an_encoder_says_so() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        assert_eq!(
            prober.describe_verified_hardware_plan(&vaapi_machine(), None, false, "", None),
            "Hardware: waiting for input video"
        );
        let no_streams = VideoSpecs {
            file: "x.mp4".into(),
            streams: vec![],
        };
        assert_eq!(
            prober.describe_verified_hardware_plan(
                &vaapi_machine(),
                Some(&no_streams),
                false,
                "",
                None
            ),
            "Hardware: invalid video: no video streams"
        );
    }

    // ---- with a stand-in ffmpeg ----

    // Go: TestSizeProbeCache_AsksTheDriverOnce. The ledger also shows the argv really
    // reached the process.
    #[test]
    fn the_driver_is_asked_once_for_the_same_question() {
        let _guard = spawn_lock();
        let ledger_dir = tempfile::TempDir::new().unwrap();
        let ledger = ledger_dir.path().join("calls.txt");
        let fake = refusing_ffmpeg(&ledger);
        let prober = Prober::new(&fake.resolver);

        let video = hevc_source("yuv420p10le");
        for call in 0..3 {
            let (encoder, _) = verify(&prober, &video, false, &vaapi_machine());
            assert_eq!(encoder, "libx265", "call {call}: expected the step-back");
        }
        // std quotes the arguments it hands a `.bat` that hold `=` or `,`
        // (`"vaapi=sv"`); the quotes are the batch file's view, not a defect.
        let data = std::fs::read_to_string(&ledger)
            .expect("the stand-in was never run")
            .replace('"', "");
        assert_eq!(
            data.matches("call").count(),
            1,
            "the driver was asked more than once: {data}"
        );
        assert!(
            data.contains("-init_hw_device vaapi=") && data.contains("nullsrc=s=5120x2880"),
            "the argv that reached ffmpeg: {data}"
        );
    }

    // No Go test: a different question (squeeze) is a different cache entry.
    #[test]
    fn a_different_frame_size_is_asked_again() {
        let _guard = spawn_lock();
        let ledger_dir = tempfile::TempDir::new().unwrap();
        let ledger = ledger_dir.path().join("calls.txt");
        let fake = refusing_ffmpeg(&ledger);
        let prober = Prober::new(&fake.resolver);
        let video = hevc_source("yuv420p10le");
        verify(&prober, &video, false, &vaapi_machine());
        verify(&prober, &video, true, &vaapi_machine());
        verify(&prober, &video, true, &vaapi_machine());
        let data = std::fs::read_to_string(&ledger).unwrap();
        assert_eq!(data.matches("call").count(), 2, "{data}");
        prober.cache().reset();
        assert!(prober.cache().is_empty());
        verify(&prober, &video, true, &vaapi_machine());
        let data = std::fs::read_to_string(&ledger).unwrap();
        assert_eq!(data.matches("call").count(), 3, "reset must forget: {data}");
    }

    // Go: TestProbeEncoder_BoundsItselfWithoutACallerDeadline. The probe's own timeout is
    // a field here, so the test shortens it without a global.
    #[test]
    fn the_probe_bounds_itself_without_a_caller_deadline() {
        let _guard = spawn_lock();
        let fake = hanging_ffmpeg();
        let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(300));
        let started = Instant::now();
        let probe = prober.probe_encoder("libx264", None);
        assert!(
            !probe.usable,
            "an ffmpeg that never exits cannot have succeeded"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the probe has no timeout of its own"
        );
        assert!(
            probe.reason.starts_with("gave no answer in "),
            "a timeout must be reported as one: {}",
            probe.reason
        );
        assert!(probe.took >= Duration::from_millis(300));
    }

    // Go: probeEncoderForSizeCached, "only the probe's own deadline is a verdict" (E-23,
    // the other half): a probe that timed out by itself IS cached, so a wedged driver is
    // not asked again at every click.
    #[test]
    fn a_probe_that_timed_out_by_itself_is_cached() {
        let _guard = spawn_lock();
        let fake = hanging_ffmpeg();
        let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(300));
        let spec = ProbeSpec {
            encoder: "h264_vaapi".into(),
            size: "640x480".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        };
        let probe = prober.probe_encoder_for_size_cached(&spec, None);
        assert!(!probe.usable);
        assert!(
            probe.reason.starts_with("gave no answer in "),
            "{}",
            probe.reason
        );
        assert_eq!(prober.cache().get(&spec.cache_key()), Some(probe.clone()));

        let again = Instant::now();
        let second = prober.probe_encoder_for_size_cached(&spec, None);
        assert_eq!(second, probe);
        assert!(
            again.elapsed() < Duration::from_millis(100),
            "asked the driver again"
        );
    }

    // Go: TestProbeEncoder_HonoursTheCallerDeadline, as a cancellation.
    #[test]
    fn the_probe_honours_a_cancellation() {
        let _guard = spawn_lock();
        let fake = hanging_ffmpeg();
        let prober = Prober::new(&fake.resolver);
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let probe = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                cancel.store(true, Ordering::Release);
            });
            prober.probe_encoder("libx264", Some(&cancel))
        });
        assert!(!probe.usable);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    // Go: TestProbeEncoder_UnknownEncoderIsRefusedWithFFmpegsWords, against a stand-in
    // that prints ffmpeg's four lines.
    #[cfg(unix)]
    #[test]
    fn a_refusal_carries_ffmpegs_first_words() {
        let _guard = spawn_lock();
        let fake = fake_tools(&[(
            "ffmpeg",
            "echo \"Unknown encoder 'superview_no_such_encoder'\" 1>&2\n\
             echo 'Error selecting an encoder' 1>&2\n\
             echo 'Error opening output file -.' 1>&2\n\
             echo 'Error opening output files: Encoder not found' 1>&2\n\
             exit 1",
        )]);
        let probe = Prober::new(&fake.resolver).probe_encoder("superview_no_such_encoder", None);
        assert!(!probe.usable);
        assert_eq!(
            probe.reason,
            "Unknown encoder 'superview_no_such_encoder'; Error selecting an encoder; Error opening output file -."
        );
    }

    // Go: "a silent failure still says something", end to end.
    #[cfg(unix)]
    #[test]
    fn a_silent_refusal_reports_the_exit_status() {
        let _guard = spawn_lock();
        let fake = fake_tools(&[("ffmpeg", "exit 218")]);
        let probe = Prober::new(&fake.resolver).probe_encoder("libx264", None);
        assert!(!probe.usable);
        assert!(probe.reason.contains("218"), "{}", probe.reason);
    }

    // No Go test: the stand-in's success, so the refusals above cannot come from a
    // broken stand-in.
    #[cfg(unix)]
    #[test]
    fn an_ffmpeg_that_encodes_makes_the_probe_usable() {
        let _guard = spawn_lock();
        let fake = fake_tools(&[("ffmpeg", "exit 0")]);
        let probe = Prober::new(&fake.resolver).probe_encoder("libx264", None);
        assert!(probe.usable, "{}", probe.reason);
        assert!(probe.reason.is_empty());
        assert!(probe.took > Duration::ZERO);
    }

    // No Go test: no ffmpeg to start is a refusal with the cause, not a panic.
    #[cfg(not(windows))]
    #[test]
    fn a_missing_ffmpeg_is_a_refusal_with_a_reason() {
        let _guard = spawn_lock();
        let empty = tempfile::TempDir::new().unwrap();
        let path = empty.path().as_os_str().to_owned();
        let resolver = ToolResolver::new(
            move |key| (key == "PATH").then(|| path.clone()),
            Some(empty.path().to_path_buf()),
        );
        let probe = Prober::new(&resolver).probe_encoder("libx264", None);
        assert!(!probe.usable);
        assert!(!probe.reason.is_empty());
    }

    // Go: TestProbeHardwareSupport_AgreesWithWhatWasAdvertised, with a stand-in that
    // refuses one encoder by name.
    #[cfg(unix)]
    #[test]
    fn the_sweep_probes_what_was_advertised_and_apply_drops_what_failed() {
        let _guard = spawn_lock();
        let fake = fake_tools(&[(
            "ffmpeg",
            "case \"$*\" in *h264_nvenc*) echo 'Driver too old' 1>&2; exit 1;; esac\nexit 0",
        )]);
        let prober = Prober::new(&fake.resolver);
        let ffmpeg = caps(&[("encoders", "libx264,h264_nvenc,libx264rgb,h264_vulkan")]);
        let report = prober.probe_hardware_support(&ffmpeg, None);

        let probed: Vec<&str> = report.results.iter().map(|p| p.encoder.as_str()).collect();
        assert_eq!(probed, ["libx264", "h264_nvenc", "h264_vulkan"]);
        assert_eq!(report.unusable().len(), 1);
        assert_eq!(report.find("h264_nvenc").unwrap().reason, "Driver too old");
        assert!(report.took > Duration::ZERO);
        assert_eq!(
            apply_encoder_probe(&ffmpeg, Some(&report))["encoders"],
            "libx264,libx264rgb,h264_vulkan"
        );
    }

    // ---- the real ffmpeg ----

    /// The machine's ffmpeg, or `None` where there is none -- unless
    /// `SUPERVIEW_REQUIRE_FFMPEG=1`, which CI sets, where its absence fails the test.
    fn real_ffmpeg_with_libx264() -> Option<ToolResolver> {
        let resolver = ToolResolver::from_process();
        let require = std::env::var("SUPERVIEW_REQUIRE_FFMPEG").as_deref() == Ok("1");
        match check_ffmpeg(&Config::default(), &resolver) {
            Ok(caps) if caps["encoders"].contains("libx264") => Some(resolver),
            Ok(_) => {
                assert!(
                    !require,
                    "this ffmpeg has no libx264 although SUPERVIEW_REQUIRE_FFMPEG=1"
                );
                None
            }
            Err(err) => {
                assert!(
                    !require,
                    "no usable ffmpeg although SUPERVIEW_REQUIRE_FFMPEG=1: {err}"
                );
                None
            }
        }
    }

    // Go: TestProbeEncoder_SoftwareEncoderWorks. libx264 is the right subject: if this
    // reddens on a machine that has ffmpeg, the probe itself is malformed.
    #[test]
    fn libx264_accepts_a_256x256_frame() {
        let _guard = spawn_lock();
        let Some(resolver) = real_ffmpeg_with_libx264() else {
            return;
        };
        let probe = Prober::new(&resolver).probe_encoder("libx264", None);
        assert!(
            probe.usable,
            "libx264 refused a 256x256 frame, so the probe asks a malformed question: {}",
            probe.reason
        );
        assert!(probe.took > Duration::ZERO);
    }

    // Go: TestProbeEncoder_UnknownEncoderIsRefusedWithFFmpegsWords
    #[test]
    fn an_unknown_encoder_is_refused_with_ffmpegs_words() {
        let _guard = spawn_lock();
        let Some(resolver) = real_ffmpeg_with_libx264() else {
            return;
        };
        let probe = Prober::new(&resolver).probe_encoder("superview_no_such_encoder", None);
        assert!(
            !probe.usable,
            "ffmpeg accepted an encoder that does not exist"
        );
        assert!(
            probe.reason.contains("superview_no_such_encoder"),
            "the reason does not name the encoder ffmpeg rejected: {:?}",
            probe.reason
        );
    }

    // Go: TestProbeHardwareSupport_AgreesWithWhatWasAdvertised
    #[test]
    fn the_real_sweep_agrees_with_what_was_advertised() {
        let _guard = spawn_lock();
        let Some(resolver) = real_ffmpeg_with_libx264() else {
            return;
        };
        let ffmpeg = check_ffmpeg(&Config::default(), &resolver).unwrap();
        let report = Prober::new(&resolver).probe_hardware_support(&ffmpeg, None);
        let wanted = encoders_to_probe(&ffmpeg);
        assert_eq!(report.results.len(), wanted.len(), "{wanted:?}");
        // A refusal is a finding on this machine, not a failure (a CI runner without a
        // GPU refuses most): what must hold is that the verdict carries a reason.
        for probe in &report.results {
            assert!(
                probe.usable || !probe.reason.is_empty(),
                "{} was refused with no reason recorded",
                probe.encoder
            );
        }
        let filtered = split_csv(&apply_encoder_probe(&ffmpeg, Some(&report))["encoders"]);
        for probe in report.unusable() {
            assert!(
                !filtered.contains(&probe.encoder),
                "{} refused the work but is still advertised",
                probe.encoder
            );
        }
    }

    // ---- constant quality (DEC-48) ----

    #[cfg(unix)]
    const CQP_ONLY: &str = "[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes: CQP).";

    /// An ffmpeg that records each call, takes `-rc_mode` and refuses everything else with
    /// `refusal`, the way an Intel VAAPI driver that only knows CQP does.
    #[cfg(unix)]
    fn cqp_only_ffmpeg(ledger: &std::path::Path, refusal: &str) -> FakeTools {
        fake_tools(&[(
            "ffmpeg",
            &format!(
                "echo \"call $*\" >> '{}'\ncase \"$*\" in *-rc_mode\\ CQP*) exit 0;; esac\necho '{refusal}' 1>&2\nexit 1",
                ledger.display()
            ),
        )])
    }

    #[cfg(unix)]
    fn calls(ledger: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(ledger)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn seed_constant_quality(prober: &Prober, encoder: &str, size: &str) {
        let spec = ProbeSpec {
            encoder: encoder.into(),
            size: size.into(),
            pix_fmt: "yuv420p10le".into(),
            bitrate: PROBE_BITRATE,
            ..ProbeSpec::default()
        };
        prober.cache().insert(
            spec.cache_key(),
            EncoderProbe {
                encoder: encoder.into(),
                usable: true,
                rate_control: RateControl::ConstantQuality,
                ..EncoderProbe::default()
            },
        );
    }

    // DEC-05 and DEC-48: the second question is the first with only the rate control
    // changed, built by the same function.
    #[cfg(unix)]
    #[test]
    fn a_cqp_only_driver_is_asked_again_in_constant_quality_and_used() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("calls");
        let fake = cqp_only_ffmpeg(&ledger, CQP_ONLY);
        let prober = Prober::new(&fake.resolver);
        for (encoder, qp) in [("h264_vaapi", 23), ("hevc_vaapi", 25)] {
            let _ = std::fs::remove_file(&ledger);
            let probe = prober.probe_encoder(encoder, None);
            assert!(probe.usable, "{encoder}: {}", probe.reason);
            assert_eq!(probe.rate_control, RateControl::ConstantQuality);
            let calls = calls(&ledger);
            assert_eq!(calls.len(), 2, "{calls:?}");
            assert!(calls[0].contains("-b:v"), "{calls:?}");
            assert!(
                calls[1].contains(&format!("-rc_mode CQP -qp {qp}")) && !calls[1].contains("-b:v"),
                "{calls:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn any_other_refusal_is_not_asked_again() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("calls");
        let fake = cqp_only_ffmpeg(&ledger, HD620_REFUSAL);
        let probe = Prober::new(&fake.resolver).probe_encoder("h264_vaapi", None);
        assert!(!probe.usable);
        assert_eq!(probe.rate_control, RateControl::Bitrate);
        assert_eq!(calls(&ledger).len(), 1, "{:?}", calls(&ledger));
    }

    #[cfg(unix)]
    #[test]
    fn only_the_vaapi_family_is_asked_again() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("calls");
        let fake = cqp_only_ffmpeg(&ledger, CQP_ONLY);
        let prober = Prober::new(&fake.resolver);
        for encoder in ["h264_qsv", "hevc_qsv", "h264_nvenc", "libx264"] {
            let _ = std::fs::remove_file(&ledger);
            let probe = prober.probe_encoder(encoder, None);
            assert!(!probe.usable, "{encoder} must stay refused");
            assert_eq!(calls(&ledger).len(), 1, "{encoder}: {:?}", calls(&ledger));
        }
    }

    // The verdict of the conversion's own probe, at the output size, carries the mode.
    #[cfg(unix)]
    #[test]
    fn the_conversion_probe_returns_the_mode_the_encoder_works_in() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let fake = cqp_only_ffmpeg(&dir.path().join("calls"), CQP_ONLY);
        let prober = Prober::new(&fake.resolver);
        let (encoder, refusal, mode) = prober.verify_encoder_with_rate_control(
            "h264_vaapi",
            Some(&hevc_source("yuv420p10le")),
            false,
            &vaapi_machine(),
            "",
            None,
        );
        assert_eq!(
            (encoder.as_str(), refusal.as_str(), mode),
            ("h264_vaapi", "", RateControl::ConstantQuality)
        );
    }

    // Stepping back to the CPU leaves constant quality behind: x264 and x265 keep a rate.
    #[test]
    fn a_cpu_encoder_stepped_back_to_is_asked_for_a_bitrate() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed(&prober, "h264_vaapi", "5120x2880", false, HD620_REFUSAL);
        let (encoder, _, mode) = prober.verify_encoder_with_rate_control(
            "h264_vaapi",
            Some(&hevc_source("yuv420p10le")),
            false,
            &vaapi_machine(),
            "",
            None,
        );
        assert_eq!(encoder, "libx265");
        assert_eq!(mode, RateControl::Bitrate);
    }

    #[test]
    fn the_plan_line_says_when_the_hardware_works_in_constant_quality_only() {
        let resolver = no_tools();
        let prober = Prober::new(&resolver);
        seed_constant_quality(&prober, "h264_vaapi", "5120x2880");
        let line = prober.describe_verified_hardware_plan(
            &vaapi_machine(),
            Some(&hevc_source("yuv420p10le")),
            false,
            "",
            None,
        );
        assert!(line.contains("h264_vaapi"), "{line}");
        assert!(line.ends_with(" (constant quality)"), "{line}");
        // And the ordinary case says nothing of it.
        let ordinary = Prober::new(&resolver);
        seed(&ordinary, "h264_vaapi", "5120x2880", true, "");
        let line = ordinary.describe_verified_hardware_plan(
            &vaapi_machine(),
            Some(&hevc_source("yuv420p10le")),
            false,
            "",
            None,
        );
        assert!(!line.contains("constant quality"), "{line}");
    }

    #[test]
    fn the_diagnostic_says_constant_quality_only() {
        let report = EncoderProbeReport {
            results: vec![
                EncoderProbe {
                    encoder: "h264_vaapi".into(),
                    usable: true,
                    rate_control: RateControl::ConstantQuality,
                    took: Duration::from_millis(300),
                    ..EncoderProbe::default()
                },
                EncoderProbe {
                    encoder: "libx264".into(),
                    usable: true,
                    took: Duration::from_millis(300),
                    ..EncoderProbe::default()
                },
            ],
            took: Duration::from_millis(600),
        };
        let text = describe_encoder_probe(Some(&report));
        assert!(
            text.contains("✅ h264_vaapi: works, constant quality only (300ms)\n"),
            "{text}"
        );
        assert!(text.contains("✅ libx264: works (300ms)\n"), "{text}");
    }

    // The exact text measured on 2026-10-19; vaapi_encode.c prints each mode after a space,
    // so a driver with several modes closes the list later.
    #[test]
    fn only_a_list_of_cqp_alone_is_the_cqp_only_refusal() {
        let text = |modes: &str| {
            format!(
                "[h264_vaapi @ 0x55] Driver does not support any RC mode compatible with selected options (supported modes:{modes})."
            )
        };
        assert!(is_cqp_only_refusal("h264_vaapi", &text(" CQP")));
        assert!(is_cqp_only_refusal("hevc_vaapi", &text(" CQP")));
        assert!(!is_cqp_only_refusal("h264_vaapi", &text(" CQP CBR VBR")));
        assert!(!is_cqp_only_refusal("h264_vaapi", &text(" CBR CQP")));
        assert!(!is_cqp_only_refusal("h264_qsv", &text(" CQP")));
    }

    /// Bit rate: the CQP-only message. Constant quality: `cqp_answer` (a shell snippet).
    #[cfg(unix)]
    fn cqp_then(ledger: &std::path::Path, cqp_answer: &str) -> FakeTools {
        fake_tools(&[(
            "ffmpeg",
            &format!(
                "echo \"call $*\" >> '{}'\ncase \"$*\" in *-rc_mode\\ CQP*) {cqp_answer};; esac\necho '{CQP_ONLY}' 1>&2\nexit 1",
                ledger.display()
            ),
        )])
    }

    #[cfg(unix)]
    const REFUSE_BIG_FRAME: &str = "case \"$*\" in *s=256x256*) exit 0;; esac; echo 'Hardware does not support encoding at size 5120x2880' 1>&2; exit 1";

    #[cfg(unix)]
    #[test]
    fn a_driver_that_refuses_both_modes_is_asked_twice_and_says_both() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("calls");
        let fake = cqp_then(
            &ledger,
            "echo 'Hardware does not support encoding at size' 1>&2; exit 1",
        );
        let probe = Prober::new(&fake.resolver).probe_encoder("h264_vaapi", None);
        assert!(!probe.usable);
        assert_eq!(calls(&ledger).len(), 2, "{:?}", calls(&ledger));
        assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
        assert!(
            probe
                .reason
                .contains("in constant quality: Hardware does not support encoding"),
            "{}",
            probe.reason
        );
    }

    // CQP is accepted by the sweep's small frame and refused at the real size: the window
    // must not read "supported modes: CQP" alone, which would send the user to the firmware.
    #[cfg(unix)]
    #[test]
    fn constant_quality_accepted_small_and_refused_at_the_real_size_says_so() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let fake = cqp_then(&dir.path().join("calls"), REFUSE_BIG_FRAME);
        let prober = Prober::new(&fake.resolver);
        assert!(prober.probe_encoder("h264_vaapi", None).usable);
        let (encoder, refusal, mode) = prober.verify_encoder_with_rate_control(
            "h264_vaapi",
            Some(&hevc_source("yuv420p10le")),
            false,
            &vaapi_machine(),
            "",
            None,
        );
        assert_eq!((encoder.as_str(), mode), ("libx265", RateControl::Bitrate));
        assert!(
            refusal.contains("supported modes: CQP).")
                && refusal.contains(
                    "in constant quality: Hardware does not support encoding at size 5120x2880"
                ),
            "{refusal}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_second_probe_that_times_out_is_a_verdict_with_both_reasons() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let fake = cqp_then(&dir.path().join("calls"), "exec sleep 30");
        let prober = Prober::new(&fake.resolver).with_timeout(Duration::from_millis(400));
        let spec = sweep_spec("h264_vaapi");
        let probe = prober.probe_encoder_for_size_cached(&spec, None);
        assert!(!probe.usable);
        assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
        assert!(
            probe
                .reason
                .contains("in constant quality: gave no answer in "),
            "{}",
            probe.reason
        );
        assert_eq!(prober.cache().get(&spec.cache_key()), Some(probe));
    }

    #[cfg(unix)]
    #[test]
    fn a_second_probe_that_is_cancelled_is_not_cached() {
        let _guard = spawn_lock();
        let dir = tempfile::tempdir().unwrap();
        let fake = cqp_then(&dir.path().join("calls"), "exec sleep 30");
        let prober = Prober::new(&fake.resolver);
        let cancel = AtomicBool::new(false);
        let spec = sweep_spec("h264_vaapi");
        let probe = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(500));
                cancel.store(true, Ordering::Release);
            });
            prober.probe_encoder_for_size_cached(&spec, Some(&cancel))
        });
        assert!(!probe.usable);
        assert!(probe.reason.starts_with(CQP_ONLY), "{}", probe.reason);
        assert!(
            !probe.reason.contains("in constant quality"),
            "a cancelled question answered nothing: {}",
            probe.reason
        );
        assert!(prober.cache().is_empty());
    }
}
