//! Runtime probing of the encoders ffmpeg claims to have.
//!
//! `ffmpeg -encoders` says what the binary was built with, not whether the driver on this
//! machine will accept the work (an NVENC build can demand a driver the machine cannot
//! have, `docs/hardware-support.md`). The only honest question is the one asked here:
//! encode one frame and see. The argv comes from the functions the conversion uses
//! (`hw_device_args`, `pixel_format_chain`, `encoder_facing_args`), so the two cannot
//! drift (E-01); `tests/recorded_probe_args.rs` pins it against the recorded fixtures.
//!
//! Design notes (exhaustive):
//! - A VAAPI driver that answers "supported modes: CQP" is asked again in constant quality
//!   (DEC-48) and, if that works, reported usable with [`EncoderProbe::rate_control`] set
//!   (v0.2.9 reported it unusable). The second question goes through the same
//!   `encoder_facing_args` as every other (DEC-05).
//! - No global. The cache of verdicts per frame size is the [`ProbeCache`] a [`Prober`]
//!   owns, [`ProbeCache::reset`] empties it, and the probe's own timeout is a field of the
//!   [`Prober`] ([`PROBE_TIMEOUT`] by default), so tests shorten it without touching
//!   anything shared.
//! - The caller gives an `Option<&AtomicBool>` and only the probe's own timeout is a
//!   deadline. There is no caller deadline: what would be one is a cancellation. The reason of a cancelled probe is the process's end status (or
//!   "cancelled" when nothing was started), not "gave no answer in ...", which stays the
//!   probe's own timeout. A cancelled probe is never cached (E-23), whenever the flag is
//!   up once the probe is back.
//! - A [`Prober`] must be long-lived (one for the application): its cache is the point, and
//!   a fresh one per question probes every time. Two simultaneous calls on the same key
//!   both probe (the lock is not held across a probe).
//! - An absent report is `None` in [`apply_encoder_probe`] and [`describe_encoder_probe`].
//! - The run error in a reason is Rust's: "exit status: 218" (only the leading text of such
//!   a reason is comparable, like in `tools.rs`).
//! - The reason budget is 300 bytes cut back to a character boundary.
//! - The durations in the report are rounded to the millisecond and printed compactly
//!   ("302ms", "1.5s", "1m2.5s") by [`format_duration`].
//! - The machine profile of the fallback is built with 0 cores: nothing in the selection
//!   reads the core count (`hardware.rs`).
//! - The logs are `log` records with `key=value` text.

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
/// Unlike v0.2.9, which kept them (DEC-48).
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

/// A duration rounded to the millisecond, printed compactly ("302ms", "1.5s", "1m2.5s").
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
        self.probe_asked(spec, cancel).0
    }

    /// [`Self::probe_encoder_at_size`], and whether ffmpeg was started every time it was
    /// asked: `false` means the verdict is about the machine, not the driver, and must not
    /// be remembered.
    fn probe_asked(&self, spec: &ProbeSpec, cancel: Option<&AtomicBool>) -> (EncoderProbe, bool) {
        let (first, started) = self.probe_once(spec, cancel);
        if !started
            || first.usable
            || spec.rate_control != RateControl::Bitrate
            || is_cancelled(cancel)
            || !is_cqp_only_refusal(&spec.encoder, &first.reason)
        {
            return (first, started);
        }
        log::warn!(
            "The driver only knows constant quality; asking again in that mode: encoder={} reason={}",
            spec.encoder,
            first.reason
        );
        let (second, started) = self.probe_once(
            &ProbeSpec {
                rate_control: RateControl::ConstantQuality,
                ..spec.clone()
            },
            cancel,
        );
        if second.usable {
            let took = first.took + second.took;
            return (EncoderProbe { took, ..second }, true);
        }
        // A cancelled second question answered nothing: the first refusal stands alone.
        if is_cancelled(cancel) || !started {
            return (first, started);
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
        let verdict = EncoderProbe {
            reason: format!("{}; in constant quality: {}", first.reason, second.reason),
            took: first.took + second.took,
            ..first
        };
        (verdict, true)
    }

    /// One question, one process. The flag is `false` when the process could not be started.
    fn probe_once(&self, spec: &ProbeSpec, cancel: Option<&AtomicBool>) -> (EncoderProbe, bool) {
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
        let launched = outcome.is_ok();
        let (output, run_err, timed_out) = match outcome {
            Ok(done)
                if done.end == RunEnd::Finished && done.status.is_some_and(|s| s.success()) =>
            {
                probe.usable = true;
                return (probe, true);
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
        (probe, launched)
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
        let (probe, launched) = self.probe_asked(spec, cancel);
        // A probe cut short by its caller answered nothing about the driver, and
        // remembering it would demote a working encoder for the whole session (E-23).
        // Neither did an ffmpeg that could not be started. Only the probe's own timeout
        // is a verdict.
        if is_cancelled(cancel) || !launched {
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
    /// thread; the cache makes every repeat of the same question free. `preset` is the one the
    /// conversion will pass: asked with another, the preview and the conversion would put two
    /// questions to the driver where one answers both.
    pub fn describe_verified_hardware_plan(
        &self,
        ffmpeg: &HashMap<String, String>,
        video: Option<&VideoSpecs>,
        squeeze: bool,
        requested_encoder: &str,
        preset: &str,
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
            preset,
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
pub(crate) mod tests;
