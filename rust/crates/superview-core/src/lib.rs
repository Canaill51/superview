//! Superview's logic, with no graphical interface (DEC-43).
//!
//! Modules arrive one pull request at a time, each with its tests (see the port plan).

pub mod config;
pub mod disk;
pub mod encode;
pub mod error;
pub mod hardware;
pub mod health;
pub mod memory;
pub mod pgm;
pub mod probe;
pub mod process;
pub mod security;
pub mod session;
pub mod tools;
pub mod video;

/// The core's version, which is the workspace's.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_follows_cargo_manifest() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }
}
