//! The OpenFX plugin of Superview for DaVinci Resolve (DEC-65).
//!
//! `warp` is the whole of the picture work and knows nothing about OpenFX, so it is tested
//! against the maps the application hands to ffmpeg; `plugin` is the glue with the host.

extern crate ofx;

use ofx::*;

mod plugin;
pub mod warp;

register_modules!(plugin);
