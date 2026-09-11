//! Terminal-cell sampling topology for the render/display layer.
//!
//! A terminal cell is not a square pixel: each visible terminal cell maps
//! to a small block of logical density samples. This module makes that
//! mapping an explicit, testable abstraction:
//!
//! - `CellSamplingShape` fixes the per-cell logical sampling shape
//!   (`columns × rows`) and provides the terminal-cell -> logical-density
//!   index mapping.
//! - `Quadrant` names the four subcells of the 2×2 shape in row-major
//!   order.
//!
//! Two shapes are defined:
//!
//! - `CellSamplingShape::HALF_BLOCK` (1×2): the current production
//!   topology. Each terminal cell maps to one horizontal and two vertical
//!   logical samples (top/bottom halves consumed by half-block glyphs).
//! - `CellSamplingShape::QUADRANT` (2×2): the future quadrant topology.
//!   Each terminal cell maps to four logical samples (TL, TR, BL, BR).
//!
//! Subcell ordering is row-major: top-left, top-right, bottom-left,
//! bottom-right, i.e. offsets (0,0), (1,0), (0,1), (1,1).
//!
//! Phase 5A makes Spiral generation shape-aware: the generator derives its
//! logical dimensions from `CellSamplingShape`, and the production path
//! always uses `HALF_BLOCK`, so existing outputs remain bit-for-bit
//! unchanged. Phase 5B.2 consumes `QUADRANT`, `Quadrant`, and
//! `logical_index` from the pure quadrant renderer and the shape-aware
//! target-occupancy preparation. A few 2×2-only helpers are still
//! referenced only by tests, so dead-code warnings are suppressed for
//! non-test builds on those items only.

/// Named subcell positions of a 2×2 terminal cell, in row-major order.
///
/// Carries only positional information; no glyph or visibility semantics
/// are attached here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quadrant {
    /// Top-left subcell, offset (0, 0).
    TopLeft,
    /// Top-right subcell, offset (1, 0).
    TopRight,
    /// Bottom-left subcell, offset (0, 1).
    BottomLeft,
    /// Bottom-right subcell, offset (1, 1).
    BottomRight,
}

impl Quadrant {
    /// All quadrants in row-major order: TL, TR, BL, BR.
    pub(crate) const ALL: [Quadrant; 4] = [
        Quadrant::TopLeft,
        Quadrant::TopRight,
        Quadrant::BottomLeft,
        Quadrant::BottomRight,
    ];

    /// Subcell offset `(sx, sy)` of this quadrant within its terminal cell.
    pub(crate) const fn offset(self) -> (usize, usize) {
        match self {
            Quadrant::TopLeft => (0, 0),
            Quadrant::TopRight => (1, 0),
            Quadrant::BottomLeft => (0, 1),
            Quadrant::BottomRight => (1, 1),
        }
    }

    /// Returns the quadrant for a valid 2×2 subcell offset, or `None` for
    /// offsets outside the 2×2 set.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn from_offset(offset: (usize, usize)) -> Option<Quadrant> {
        match offset {
            (0, 0) => Some(Quadrant::TopLeft),
            (1, 0) => Some(Quadrant::TopRight),
            (0, 1) => Some(Quadrant::BottomLeft),
            (1, 1) => Some(Quadrant::BottomRight),
            _ => None,
        }
    }
}

/// Fixed sampling shape of one terminal cell in logical density samples.
///
/// A terminal cell at `(cell_x, cell_y)` owns `columns × rows` logical
/// samples. The global logical index of subcell `(sub_x, sub_y)` is
///
/// ```text
/// logical_x = cell_x * columns + sub_x
/// logical_y = cell_y * rows    + sub_y
/// ```
///
/// `HALF_BLOCK` (1×2) reduces exactly to the current renderer convention
/// (top -> `(cx, 2*cy)`, bottom -> `(cx, 2*cy + 1)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellSamplingShape {
    /// Logical samples per terminal cell along x.
    columns: usize,
    /// Logical samples per terminal cell along y.
    rows: usize,
}

impl CellSamplingShape {
    /// Current production topology: 1×2 (half-block).
    pub(crate) const HALF_BLOCK: Self = Self {
        columns: 1,
        rows: 2,
    };

    /// Future quadrant topology: 2×2.
    pub(crate) const QUADRANT: Self = Self {
        columns: 2,
        rows: 2,
    };

    /// Logical samples per terminal cell along x.
    pub(crate) const fn columns(&self) -> usize {
        self.columns
    }

    /// Logical samples per terminal cell along y.
    pub(crate) const fn rows(&self) -> usize {
        self.rows
    }

    /// Total logical samples per terminal cell (`columns * rows`).
    pub(crate) const fn subcells(&self) -> usize {
        self.columns * self.rows
    }

    /// Subcell offsets `(sx, sy)` in row-major order: TL, TR, BL, BR.
    pub(crate) fn subcell_offsets(&self) -> Vec<(usize, usize)> {
        let mut offsets = Vec::with_capacity(self.subcells());
        for sy in 0..self.rows {
            for sx in 0..self.columns {
                offsets.push((sx, sy));
            }
        }
        offsets
    }

    /// Maps terminal cell `(cell_x, cell_y)` and subcell `(sub_x, sub_y)`
    /// to the global logical density index `(logical_x, logical_y)`.
    pub(crate) const fn logical_index(
        &self,
        cell_x: usize,
        cell_y: usize,
        sub_x: usize,
        sub_y: usize,
    ) -> (usize, usize) {
        (cell_x * self.columns + sub_x, cell_y * self.rows + sub_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::density::DensityMap;

    #[test]
    fn half_block_shape_dimensions() {
        let shape = CellSamplingShape::HALF_BLOCK;
        assert_eq!(shape.columns(), 1);
        assert_eq!(shape.rows(), 2);
        assert_eq!(shape.subcells(), 2);
    }

    #[test]
    fn quadrant_shape_dimensions() {
        let shape = CellSamplingShape::QUADRANT;
        assert_eq!(shape.columns(), 2);
        assert_eq!(shape.rows(), 2);
        assert_eq!(shape.subcells(), 4);
    }

    #[test]
    fn quadrant_offsets() {
        assert_eq!(Quadrant::TopLeft.offset(), (0, 0));
        assert_eq!(Quadrant::TopRight.offset(), (1, 0));
        assert_eq!(Quadrant::BottomLeft.offset(), (0, 1));
        assert_eq!(Quadrant::BottomRight.offset(), (1, 1));
    }

    #[test]
    fn quadrant_row_major_ordering() {
        assert_eq!(
            Quadrant::ALL,
            [
                Quadrant::TopLeft,
                Quadrant::TopRight,
                Quadrant::BottomLeft,
                Quadrant::BottomRight,
            ]
        );

        let expected: Vec<(usize, usize)> = Quadrant::ALL
            .iter()
            .map(|quadrant| quadrant.offset())
            .collect();
        assert_eq!(CellSamplingShape::QUADRANT.subcell_offsets(), expected);

        for quadrant in Quadrant::ALL {
            assert_eq!(
                Quadrant::from_offset(quadrant.offset()),
                Some(quadrant),
                "from_offset must round-trip the offset of {quadrant:?}"
            );
        }

        for offset in [(2, 0), (0, 2), (2, 2), (3, 1)] {
            assert_eq!(
                Quadrant::from_offset(offset),
                None,
                "from_offset must reject the invalid offset {offset:?}"
            );
        }
    }

    #[test]
    fn logical_index_mapping() {
        let shape = CellSamplingShape::QUADRANT;

        // (cell_x, cell_y, sub_x, sub_y) -> expected (logical_x, logical_y)
        let cases = [
            (((0, 0), (0, 0)), (0, 0)),
            (((0, 0), (1, 0)), (1, 0)),
            (((0, 0), (0, 1)), (0, 1)),
            (((0, 0), (1, 1)), (1, 1)),
            (((1, 0), (0, 0)), (2, 0)),
            (((1, 0), (1, 1)), (3, 1)),
            (((0, 1), (0, 0)), (0, 2)),
            (((0, 1), (1, 1)), (1, 3)),
            (((3, 2), (1, 1)), (7, 5)),
        ];

        for (((cell_x, cell_y), (sub_x, sub_y)), expected) in cases {
            assert_eq!(
                shape.logical_index(cell_x, cell_y, sub_x, sub_y),
                expected,
                "cell ({cell_x},{cell_y}) subcell ({sub_x},{sub_y})"
            );
        }
    }

    #[test]
    fn half_block_logical_index_matches_current_renderer() {
        // The 1×2 shape must reduce exactly to the current renderer
        // convention: top -> (cx, 2*cy), bottom -> (cx, 2*cy + 1).
        let shape = CellSamplingShape::HALF_BLOCK;

        for (cell_x, cell_y) in [(0usize, 0usize), (1, 0), (7, 3), (19, 9)] {
            assert_eq!(
                shape.logical_index(cell_x, cell_y, 0, 0),
                (cell_x, 2 * cell_y),
                "top half of cell ({cell_x},{cell_y})"
            );
            assert_eq!(
                shape.logical_index(cell_x, cell_y, 0, 1),
                (cell_x, 2 * cell_y + 1),
                "bottom half of cell ({cell_x},{cell_y})"
            );
        }
    }

    #[test]
    fn adjacent_cells_do_not_overlap() {
        let shape = CellSamplingShape::QUADRANT;

        let mapped = |cell_x: usize, cell_y: usize| {
            shape
                .subcell_offsets()
                .into_iter()
                .map(|(sub_x, sub_y)| shape.logical_index(cell_x, cell_y, sub_x, sub_y))
                .collect::<Vec<_>>()
        };

        // Horizontal neighbors: cell (0,0) vs cell (1,0).
        let left = mapped(0, 0);
        let right = mapped(1, 0);
        for logical in &left {
            assert!(
                !right.contains(logical),
                "horizontal neighbors must not share logical cell {logical:?}"
            );
        }

        // Vertical neighbors: cell (0,0) vs cell (0,1).
        let top = mapped(0, 0);
        let bottom = mapped(0, 1);
        for logical in &top {
            assert!(
                !bottom.contains(logical),
                "vertical neighbors must not share logical cell {logical:?}"
            );
        }
    }

    #[test]
    fn complete_coverage_small_grid() {
        // A 2×2 terminal grid under QUADRANT spans a 4×4 logical field;
        // the mapped logical coordinates must cover every logical cell
        // exactly once.
        let shape = CellSamplingShape::QUADRANT;
        let terminal_width = 2;
        let terminal_height = 2;
        let logical_width = terminal_width * shape.columns();
        let logical_height = terminal_height * shape.rows();

        let mut mapped: Vec<(usize, usize)> = Vec::new();
        for cell_y in 0..terminal_height {
            for cell_x in 0..terminal_width {
                for (sub_x, sub_y) in shape.subcell_offsets() {
                    mapped.push(shape.logical_index(cell_x, cell_y, sub_x, sub_y));
                }
            }
        }

        assert_eq!(
            mapped.len(),
            terminal_width * terminal_height * shape.subcells(),
            "every subcell of every terminal cell must be mapped"
        );

        let mut unique = mapped.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            mapped.len(),
            "mapped logical cells must be unique"
        );

        // `unique` is sorted lexicographically (x major, y minor), so the
        // expected grid must be generated in the same order.
        let expected: Vec<(usize, usize)> = (0..logical_width)
            .flat_map(|logical_x| (0..logical_height).map(move |logical_y| (logical_x, logical_y)))
            .collect();
        assert_eq!(
            unique, expected,
            "mapped set must equal the full 4×4 logical grid"
        );
    }

    #[test]
    fn edge_cells() {
        // First and last terminal cells (corners) must map strictly inside
        // the logical bounds.
        let shape = CellSamplingShape::QUADRANT;
        let terminal_width = 3;
        let terminal_height = 2;
        let logical_width = terminal_width * shape.columns();
        let logical_height = terminal_height * shape.rows();

        let corners = [
            (0usize, 0usize),
            (terminal_width - 1, 0),
            (0, terminal_height - 1),
            (terminal_width - 1, terminal_height - 1),
        ];

        for (cell_x, cell_y) in corners {
            for (sub_x, sub_y) in shape.subcell_offsets() {
                let (logical_x, logical_y) = shape.logical_index(cell_x, cell_y, sub_x, sub_y);
                assert!(
                    logical_x < logical_width,
                    "cell ({cell_x},{cell_y}) subcell ({sub_x},{sub_y}) maps x out of bounds"
                );
                assert!(
                    logical_y < logical_height,
                    "cell ({cell_x},{cell_y}) subcell ({sub_x},{sub_y}) maps y out of bounds"
                );
            }
        }
    }

    #[test]
    fn synthetic_densitymap_extraction() {
        // Test-only extraction: a synthetic DensityMap with a unique value
        // per logical coordinate. Reading each quadrant of each terminal
        // cell through logical_index must return exactly that coordinate's
        // value.
        let shape = CellSamplingShape::QUADRANT;
        let terminal_width = 2;
        let terminal_height = 2;
        let logical_width = terminal_width * shape.columns();
        let logical_height = terminal_height * shape.rows();

        let map = DensityMap::from_fn(logical_width, logical_height, |x, y| {
            x as f64 + y as f64 * 100.0
        });

        for cell_y in 0..terminal_height {
            for cell_x in 0..terminal_width {
                for quadrant in Quadrant::ALL {
                    let (sub_x, sub_y) = quadrant.offset();
                    let (logical_x, logical_y) = shape.logical_index(cell_x, cell_y, sub_x, sub_y);
                    let expected = logical_x as f64 + logical_y as f64 * 100.0;
                    assert_eq!(
                        map.get(logical_x, logical_y),
                        expected,
                        "cell ({cell_x},{cell_y}) quadrant {quadrant:?} must read logical ({logical_x},{logical_y})"
                    );
                }
            }
        }
    }
}
