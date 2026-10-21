//! System stop requests (SIGINT and SIGTERM on Unix, Ctrl+C on Windows). The core installs no
//! handler: it reads the [`AtomicI32`] given to it as `StopControl::signal`, and this module
//! writes it. Same handlers as `superview-harness`, which the window cannot depend on.
//!
//! Unlike the harness, the window has something to do besides stopping a conversion: when no
//! conversion runs, nothing would otherwise notice the signal, so `on_stop` ends the event loop
//! (no toolkit handler does it for us). The conversion, if there is one,
//! sees the number through the flag and unwinds on its own.
//!
//! A signal that arrives before the event loop runs is not lost, as far as this module can
//! make it: the flag is set, and `main` looks at it before `run()`. The narrow window between
//! that look and the loop starting depends on the backend queueing `quit_event_loop`; this was
//! not measured.

use std::sync::atomic::AtomicI32;

#[cfg(any(unix, windows))]
use std::sync::atomic::Ordering;

/// 0 while nothing happened, else the signal's number (2 = interrupt, 15 = terminated).
static SIGNAL: AtomicI32 = AtomicI32::new(0);

/// Records a signal: the number first, so that whoever is woken by `on_stop` sees it.
#[cfg(any(unix, windows))]
fn record(number: i32, on_stop: &(impl Fn() + ?Sized)) {
    SIGNAL.store(number, Ordering::Release);
    on_stop();
}

/// Installs the handlers (once per process) and returns the flag they write.
#[cfg(unix)]
pub fn install(on_stop: impl Fn() + Send + 'static) -> &'static AtomicI32 {
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    match Signals::new([SIGINT, SIGTERM]) {
        Ok(mut signals) => {
            let spawned = std::thread::Builder::new()
                .name("signals".into())
                .spawn(move || {
                    for number in signals.forever() {
                        record(number, &on_stop);
                    }
                });
            if let Err(e) = spawned {
                log::error!("Cannot start the thread that waits for signals: {e}");
            }
        }
        Err(e) => log::error!("Cannot install the signal handlers: {e}"),
    }
    &SIGNAL
}

/// Installs the handler (once per process) and returns the flag it writes.
#[cfg(windows)]
pub fn install(on_stop: impl Fn() + Send + 'static) -> &'static AtomicI32 {
    // Ctrl+C is the only signal there and the handler is not told which one it is.
    if let Err(e) = ctrlc::set_handler(move || record(2, &on_stop)) {
        log::error!("Cannot install the Ctrl+C handler: {e}");
    }
    &SIGNAL
}

/// Without handlers the flag simply stays down.
#[cfg(not(any(unix, windows)))]
pub fn install(_on_stop: impl Fn() + Send + 'static) -> &'static AtomicI32 {
    &SIGNAL
}
