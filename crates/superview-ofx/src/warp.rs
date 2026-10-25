//! The horizontal warp, on one row of pixels.
//!
//! In Resolve the effect receives the whole timeline frame, the clip already scaled to fit: a 4:3
//! picture sits centred between two bars. The warp reads the picture's columns and writes every
//! column of the frame, bars included. The curve is `pgm::source_x_for_output_x`, the one the
//! application's maps use, and the pixel is the nearest one, as ffmpeg's `remap` takes it, so the
//! result is the application's.

use superview_core::pgm::source_x_for_output_x;

/// What the effect does to the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A 4:3 picture centred in a wider frame is stretched to fill it.
    Widen,
    /// The picture already fills the frame, stretched by the camera; its centre is narrowed back.
    Unsqueeze,
}

/// For each column of a `width` x `height` frame, the column of the same frame it takes its pixel
/// from. Always `width` entries, each below `width`.
///
/// The frame is left alone (identity) when there is nothing to widen: a frame no wider than the
/// picture, no height, or wider than twice the picture, where the curve folds back on itself at
/// both edges. The application refuses that last case (`pgm::check_geometry`); an effect inside
/// an editor cannot refuse, so it does nothing.
///
/// Assumption to check by hand in Resolve: when `width - picture` is odd, the picture sits
/// `(width - picture) / 2` columns (rounded down) from the left edge.
pub fn column_map(width: usize, height: usize, mode: Mode) -> Vec<usize> {
    let identity = || (0..width).collect();
    let (picture, pad, squeeze) = match mode {
        Mode::Unsqueeze => (width, 0, true),
        Mode::Widen => {
            // 4:3 of the frame's height, rounded to the nearest pixel.
            let picture = height.saturating_mul(4).saturating_add(1) / 3;
            if picture >= width || width / 2 > picture {
                return identity();
            }
            (picture, (width - picture) / 2, false)
        }
    };
    (0..width)
        .map(|x| {
            let from = source_x_for_output_x(x as i64, width as i64, picture as i64, squeeze);
            // A guard: the tests show the curve stays inside the picture for every accepted size.
            from.clamp(0, picture as i64 - 1) as usize + pad
        })
        .collect()
}

/// Writes `dst[i] = src[map[i]]`. Returns `false`, writing nothing, when the three do not have the
/// same length: a mistake in the caller must not become a panic inside the editor.
#[must_use]
pub fn warp_row<T: Copy>(src: &[T], dst: &mut [T], map: &[usize]) -> bool {
    if src.len() != map.len() || dst.len() != map.len() || map.iter().any(|&from| from >= src.len())
    {
        return false;
    }
    for (out, &from) in dst.iter_mut().zip(map) {
        *out = src[from];
    }
    true
}
