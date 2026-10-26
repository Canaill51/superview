//! The OpenFX plugin of Superview for DaVinci Resolve (DEC-65, DEC-66).
//!
//! `warp` is the whole of the picture work and knows nothing about OpenFX, so it is tested
//! against the maps the application hands to ffmpeg; `cuda` and `opencl` run the same map on the
//! GPU, tested against `warp`, and `gpu` holds what they share; `journal` is the plugin's log
//! file; `plugin` is the glue with the host.

extern crate ofx;

use ofx::*;

pub mod cuda;
pub mod gpu;
pub mod journal;
pub mod opencl;
mod plugin;
pub mod warp;

register_modules!(plugin);
