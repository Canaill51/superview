use super::*;

/// A status that is an exit code, on either platform.
fn exited(code: i32) -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }
}

// Go: TestClassifyFfmpegFailure_KeepsStderrForOrdinaryFailures
#[test]
fn an_ordinary_failure_keeps_ffmpegs_stderr() {
    let err = classify_ffmpeg_failure(&exited(1), "Unknown encoder 'nope'", "5.2 GB");
    assert!(
        matches!(err, Error::Plain(_)),
        "an ordinary failure is not an encoder problem: {err:?}"
    );
    let text = err.to_string();
    assert!(text.starts_with("ffmpeg failed: exit"), "{text}");
    assert!(
        text.ends_with("\nffmpeg stderr:\nUnknown encoder 'nope'"),
        "{text}"
    );
}

// Go: TestClassifyFfmpegFailure_WithoutStderr
#[test]
fn no_stderr_is_not_announced() {
    let text = classify_ffmpeg_failure(&exited(8), "", "5.2 GB").to_string();
    assert!(text.starts_with("ffmpeg failed: exit"), "{text}");
    assert!(text.contains('8'), "the exit code must stay: {text}");
    assert!(!text.contains("ffmpeg stderr"), "{text}");
    assert!(!text.contains('\n'), "{text}");
}

// The memory reading is only for a kill: an ordinary failure must not carry it.
#[test]
fn an_ordinary_failure_does_not_mention_memory() {
    let text = classify_ffmpeg_failure(&exited(1), "boom", "5.2 GB").to_string();
    assert!(
        !text.contains("5.2 GB") && !text.contains("memory"),
        "{text}"
    );
}

#[cfg(unix)]
fn killed() -> ExitStatus {
    let _guard = crate::process::tests::spawn_lock();
    crate::process::background_command("sh", ["-c", "kill -9 $$"])
        .status()
        .expect("cannot start a helper process on this machine")
}

// Go: TestClassifyFfmpegFailure_DiagnosesAKillAsMemoryExhaustion
#[cfg(unix)]
#[test]
fn a_kill_is_diagnosed_as_memory_exhaustion() {
    let err = classify_ffmpeg_failure(&killed(), "", UNKNOWN_MEMORY);
    assert!(
        matches!(err, Error::Encoder(_)),
        "a kill must be an encoder error, got {err:?}"
    );
    let text = err.to_string();
    for want in [
        "killed by the system",
        "signal: killed",
        "ran out of memory",
    ] {
        assert!(text.contains(want), "{want:?} missing from {text:?}");
    }
    // The stderr of a killed run is not what explains it.
    let with_stderr = classify_ffmpeg_failure(&killed(), "noise", UNKNOWN_MEMORY);
    assert!(matches!(with_stderr, Error::Encoder(_)));
    assert!(!with_stderr.to_string().contains("noise"));
}

// The whole text, written out by hand from encode.go.
#[cfg(unix)]
#[test]
fn the_kill_message_is_the_go_text_word_for_word() {
    let head = "ffmpeg was killed by the system (signal: killed). Nothing in Superview sent that signal, \
which on this platform almost always means the machine ran out of memory.";
    let tail = " Close other applications, choose a hardware encoder if this machine offers one, \
or convert a video with fewer pixels.";
    assert_eq!(
        classify_ffmpeg_failure(&killed(), "", "5.2 GB").to_string(),
        format!("encoder error: {head} Memory available when it stopped: 5.2 GB.{tail}")
    );
    assert_eq!(
        classify_ffmpeg_failure(&killed(), "", UNKNOWN_MEMORY).to_string(),
        format!("encoder error: {head}{tail}")
    );
}

// Go: TestClassifyFfmpegFailure_ReportsTheMemoryReadingWhenThereIsOne, with the reading
// injected so that it runs on any machine.
#[cfg(unix)]
#[test]
fn a_known_memory_reading_is_reported() {
    let text = classify_ffmpeg_failure(&killed(), "", "5.2 GB").to_string();
    assert!(
        text.contains("Memory available when it stopped: 5.2 GB."),
        "{text}"
    );
    assert!(!text.contains(UNKNOWN_MEMORY), "{text}");
}

// An unknown reading is a word, not a figure: it must not reach the message.
#[cfg(unix)]
#[test]
fn an_unknown_memory_reading_is_left_out() {
    let text = classify_ffmpeg_failure(&killed(), "", UNKNOWN_MEMORY).to_string();
    assert!(!text.contains("Memory available"), "{text}");
    assert!(!text.contains(UNKNOWN_MEMORY), "{text}");
    assert!(
        text.contains("ran out of memory. Close other applications"),
        "{text}"
    );
}

// The convenience variant takes the reading of this machine, like the Go function.
#[cfg(unix)]
#[test]
fn the_convenient_variant_reads_this_machine() {
    let text = classify_ffmpeg_failure_now(&killed(), "").to_string();
    assert!(text.contains("killed by the system"), "{text}");
    let reading = format_available_memory();
    if reading == UNKNOWN_MEMORY {
        assert!(!text.contains("Memory available"), "{text}");
    } else {
        assert!(text.contains("Memory available when it stopped:"), "{text}");
    }
}
