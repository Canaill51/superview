//! System stop requests. The core installs no handler (it only reads a `StopControl`); the
//! harness writes the flags. On Unix the real signal number is stored (2 or 15), which keeps
//! the "interrupt" / "terminated" messages of the Go code; on Windows only Ctrl+C exists.

use std::sync::atomic::{AtomicBool, AtomicI32};

#[cfg(any(unix, windows))]
use std::sync::atomic::Ordering;

static CANCEL: AtomicBool = AtomicBool::new(false);
static SIGNAL: AtomicI32 = AtomicI32::new(0);

/// The flags the handlers write and the conversion reads.
#[derive(Clone, Copy)]
pub struct StopRefs {
    pub cancel: &'static AtomicBool,
    pub signal: &'static AtomicI32,
}

/// Records a signal: the number first, so that a reader who sees `cancel` also sees it.
#[cfg(any(unix, windows))]
fn record(number: i32) {
    SIGNAL.store(number, Ordering::Release);
    CANCEL.store(true, Ordering::Release);
}

/// Installs the handlers (once per process) and returns the flags they write.
#[cfg(unix)]
pub fn install() -> StopRefs {
    use signal_hook::consts::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    match Signals::new([SIGINT, SIGTERM]) {
        Ok(mut signals) => {
            std::thread::spawn(move || {
                for number in signals.forever() {
                    record(number);
                }
            });
        }
        Err(e) => log::error!("Cannot install the signal handlers: {e}"),
    }
    refs()
}

/// Installs the handler (once per process) and returns the flag it writes.
#[cfg(windows)]
pub fn install() -> StopRefs {
    // Ctrl+C is the only signal there and the handler is not told which one it is.
    if let Err(e) = ctrlc::set_handler(|| record(2)) {
        log::error!("Cannot install the Ctrl+C handler: {e}");
    }
    refs()
}

/// Without handlers the flags simply stay down.
#[cfg(not(any(unix, windows)))]
pub fn install() -> StopRefs {
    refs()
}

fn refs() -> StopRefs {
    StopRefs {
        cancel: &CANCEL,
        signal: &SIGNAL,
    }
}
