//! The distortion preview: two grids, the picture before the conversion and after it.
//!
//! The grids are drawn from `pgm::source_x_for_output_x`, the formula the remap maps are made
//! with, so the lines of the output fall where the conversion puts them (DEC-62). They are plain
//! alpha masks (white pixels, the line's strength in the alpha channel): the window tints them
//! with its accent, so a change of theme redraws nothing.
//!
//! The geometry is nominal (a 4:3 source widened to 16:9, or a 16:9 source squeezed back), not
//! that of the chosen file: the preview shows what the mode does, and the formula is the same
//! for every size up to rounding.

use superview_core::pgm::source_x_for_output_x;

/// The sizes of the grids, in pixels, which the window shows 1:1 (widths rounded down to an even
/// number, as the real output width is). Widening shows a 4:3 grid beside a 16:9 one, the
/// mockup's proportions; un-squeezing shows two 16:9 grids, smaller so that both still fit
/// the 560 px of the column.
const WIDENING: Sizes = Sizes {
    source: 234,
    output: 312,
    height: 176,
};
const SQUEEZING: Sizes = Sizes {
    source: 270,
    output: 270,
    height: 152,
};
/// The grid has this many cells across, and `ROWS` down.
const COLUMNS: usize = 16;
const ROWS: usize = 8;
/// Every fourth line is stronger, the others fainter.
const STRONG: u8 = 230;
const FAINT: u8 = 90;

struct Sizes {
    source: usize,
    output: usize,
    height: usize,
}

/// A rectangle of line strengths, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    alpha: Vec<u8>,
}

impl Grid {
    fn new(width: usize, height: usize) -> Self {
        Grid {
            width,
            height,
            alpha: vec![0; width * height],
        }
    }

    /// The strength of the line at this pixel, 0 where there is none.
    #[cfg(test)]
    pub fn alpha_at(&self, x: usize, y: usize) -> u8 {
        self.alpha[y * self.width + x]
    }

    /// Where two lines cross, the stronger one shows.
    fn raise(&mut self, x: usize, y: usize, alpha: u8) {
        let pixel = &mut self.alpha[y * self.width + x];
        *pixel = (*pixel).max(alpha);
    }

    fn vertical_line(&mut self, x: usize, alpha: u8) {
        for y in 0..self.height {
            self.raise(x, y, alpha);
        }
    }

    fn horizontal_lines(&mut self) {
        for j in 0..=ROWS {
            let y = j * (self.height - 1) / ROWS;
            for x in 0..self.width {
                self.raise(x, y, strength(j));
            }
        }
    }

    /// The columns holding a vertical line, read on a row no horizontal line crosses.
    #[cfg(test)]
    pub fn line_columns(&self) -> Vec<usize> {
        (0..self.width)
            .filter(|&x| self.alpha_at(x, 1) > 0)
            .collect()
    }

    /// White pixels with the strength as alpha, in the byte order of `slint::Rgba8Pixel`.
    pub fn to_rgba(&self) -> Vec<u8> {
        self.alpha
            .iter()
            .flat_map(|&alpha| [255, 255, 255, alpha])
            .collect()
    }
}

fn strength(index: usize) -> u8 {
    if index.is_multiple_of(4) {
        STRONG
    } else {
        FAINT
    }
}

/// The two grids for a mode: `squeeze` is the "source already stretched to 16:9" switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub source: Grid,
    pub output: Grid,
}

pub fn preview(squeeze: bool) -> Preview {
    let sizes = if squeeze { SQUEEZING } else { WIDENING };
    let (source_width, output_width) = (sizes.source, sizes.output);
    let mut source = Grid::new(source_width, sizes.height);
    let mut output = Grid::new(output_width, sizes.height);
    for i in 0..=COLUMNS {
        // The source's line, and the output column that takes its pixels from there.
        let wanted = i * (source_width - 1) / COLUMNS;
        source.vertical_line(wanted, strength(i));
        output.vertical_line(
            output_column_showing(wanted, source_width, output_width, squeeze),
            strength(i),
        );
    }
    source.horizontal_lines();
    output.horizontal_lines();
    Preview { source, output }
}

/// The output column whose pixel comes from the source column nearest `wanted`.
fn output_column_showing(wanted: usize, width: usize, out_x: usize, squeeze: bool) -> usize {
    (0..out_x)
        .min_by_key(|&x| {
            source_x_for_output_x(x as i64, out_x as i64, width as i64, squeeze)
                .abs_diff(wanted as i64)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaps(columns: &[usize]) -> Vec<usize> {
        columns.windows(2).map(|pair| pair[1] - pair[0]).collect()
    }

    #[test]
    fn the_source_grid_is_regular_and_reaches_both_edges() {
        let grid = preview(false).source;
        assert_eq!(
            (grid.width, grid.height),
            (WIDENING.source, WIDENING.height)
        );
        let columns = grid.line_columns();
        assert_eq!(columns.len(), COLUMNS + 1, "one line per cell edge");
        assert_eq!((columns[0], columns[COLUMNS]), (0, WIDENING.source - 1));
        let spread = gaps(&columns);
        let (min, max) = (spread.iter().min().unwrap(), spread.iter().max().unwrap());
        assert!(max - min <= 1, "gaps of {min} to {max} px are not even");
    }

    #[test]
    fn widening_spreads_the_lines_at_the_edges_and_keeps_the_centre() {
        let Preview { source, output } = preview(false);
        assert_eq!(
            (output.width, output.height),
            (WIDENING.output, WIDENING.height)
        );
        let columns = output.line_columns();
        assert_eq!(columns.len(), COLUMNS + 1, "no line lost or merged");
        assert_eq!((columns[0], columns[COLUMNS]), (0, WIDENING.output - 1));
        let spread = gaps(&columns);
        let centre = spread[COLUMNS / 2 - 1] + spread[COLUMNS / 2];
        let edge = spread[0] + spread[COLUMNS - 1];
        assert!(
            edge > centre,
            "edge cells ({edge} px) must be wider than the centre's ({centre} px)"
        );
        // The centre keeps its ratio: its cells are those of the source.
        let source_gaps = gaps(&source.line_columns());
        let source_centre = source_gaps[COLUMNS / 2 - 1] + source_gaps[COLUMNS / 2];
        assert!(
            centre.abs_diff(source_centre) <= 2,
            "the centre cells ({centre} px) are not the source's ({source_centre} px)"
        );
    }

    #[test]
    fn both_modes_are_symmetric_and_squeezing_differs_from_widening() {
        for squeeze in [false, true] {
            let Preview { source, output } = preview(squeeze);
            assert_eq!(
                output.width,
                if squeeze {
                    SQUEEZING.output
                } else {
                    WIDENING.output
                }
            );
            for grid in [&source, &output] {
                let columns = grid.line_columns();
                assert_eq!(columns.len(), COLUMNS + 1, "squeeze={squeeze}");
                // The map truncates towards zero, so it is mirrored to within a few pixels.
                for (left, right) in columns.iter().zip(columns.iter().rev()) {
                    assert!(
                        (left + right).abs_diff(grid.width - 1) <= 3,
                        "squeeze={squeeze}: columns {left} and {right} are not mirrored"
                    );
                }
            }
        }
        assert_ne!(
            preview(false).output.line_columns(),
            preview(true).output.line_columns()
        );
    }

    #[test]
    fn every_fourth_line_is_stronger_and_a_crossing_shows_the_stronger() {
        let grid = preview(false).source;
        let columns = grid.line_columns();
        for (i, &x) in columns.iter().enumerate() {
            let want = if i.is_multiple_of(4) { STRONG } else { FAINT };
            assert_eq!(grid.alpha_at(x, 1), want, "vertical line {i}");
        }
        // Row 0 is a strong horizontal line: a faint vertical line does not weaken it.
        assert_eq!(grid.alpha_at(columns[1], 0), STRONG);
        assert_eq!(grid.alpha_at(columns[1] + 1, 0), STRONG);
        // Between lines, nothing.
        assert_eq!(grid.alpha_at(columns[1] + 2, 1), 0);
    }

    /// Un-squeezing narrows the centre: its cells are narrower than the source's, the edge
    /// ones wider (the formula's offset is positive across the frame).
    #[test]
    fn un_squeezing_narrows_the_centre_and_widens_the_edge_cells() {
        let Preview { source, output } = preview(true);
        let (before, after) = (gaps(&source.line_columns()), gaps(&output.line_columns()));
        let middle = |g: &[usize]| g[COLUMNS / 2 - 1] + g[COLUMNS / 2];
        let edges = |g: &[usize]| g[0] + g[COLUMNS - 1];
        assert!(
            middle(&after) < middle(&before),
            "the centre is not narrowed"
        );
        assert!(
            edges(&after) > edges(&before),
            "the edge cells are not wider"
        );
    }

    /// What the window lays out: each grid has the ratio of its label, to within the rounding of
    /// whole even pixels, and the two fit the column side by side.
    #[test]
    fn the_grids_have_the_ratios_they_are_labelled_with_and_fit_the_column() {
        let ratio = |grid: &Grid| grid.width as f64 / grid.height as f64;
        let near = |value: f64, expected: f64| (value / expected - 1.0).abs() < 0.01;
        let widening = preview(false);
        assert!(near(ratio(&widening.source), 4.0 / 3.0), "source 4:3");
        assert!(near(ratio(&widening.output), 16.0 / 9.0), "output 16:9");
        let squeezing = preview(true);
        assert!(near(ratio(&squeezing.source), 16.0 / 9.0), "source 16:9");
        assert!(near(ratio(&squeezing.output), 16.0 / 9.0), "output 16:9");
        for Preview { source, output } in [widening, squeezing] {
            // 12 px between them, in a column 560 px wide.
            assert!(source.width + output.width + 12 <= 560);
        }
    }

    #[test]
    fn the_image_is_white_and_carries_the_strength_in_alpha() {
        let grid = preview(false).source;
        let rgba = grid.to_rgba();
        assert_eq!(rgba.len(), grid.width * grid.height * 4);
        for (pixel, &alpha) in rgba.as_chunks::<4>().0.iter().zip(&grid.alpha) {
            assert_eq!(*pixel, [255, 255, 255, alpha]);
        }
    }
}
