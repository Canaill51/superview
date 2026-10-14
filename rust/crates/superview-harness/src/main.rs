//! Banc d'essai : jamais livré, pas une CLI (DEC-44).
//!
//! Servira aux tests croisés Go/Rust et aux tests FFmpeg réels.

fn main() {
    println!("superview-harness (core {})", superview_core::version());
}
