//! The OpenFX plugin of Superview for DaVinci Resolve (DEC-65, DEC-66).
//!
//! `warp` is the whole of the picture work and knows nothing about OpenFX, so it is tested
//! against the maps the application hands to ffmpeg; `cuda` and `opencl` run the same map on the
//! GPU, tested against `warp`, and `gpu` holds what they share; `journal` is the plugin's log
//! file; `plugin` is the glue with the host.
//!
//! The workspace denies `unsafe` code; this crate is the one exception. OpenFX, the CUDA driver
//! and the OpenCL loader are C interfaces, reached through the ofx fork and through libraries
//! loaded at run time (DEC-65, DEC-66, DEC-68).

#![allow(unsafe_code)]

extern crate ofx;

use ofx::*;

pub mod cuda;
pub mod gpu;
pub mod journal;
pub mod opencl;
mod plugin;
pub mod warp;

register_modules!(plugin);
