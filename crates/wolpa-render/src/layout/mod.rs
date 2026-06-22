//! ## Layout — grid coordinates to pixel coordinates
//!
//! Converts (row, col) grid positions to (x, y) pixel positions
//! based on font cell metrics. Handles padding and offset.

use crate::font::CellMetrics;

/// Converts grid coordinates to pixel coordinates.
pub struct Layout {
    /// Pixel offset of the first cell (top-left corner padding).
    pub origin_x: f64,
    pub origin_y: f64,
    /// Cell dimensions from font metrics.
    pub cell_width: f64,
    pub cell_height: f64,
    /// Total pixel dimensions of the render area.
    pub pixel_width: f64,
    pub pixel_height: f64,
}

impl Layout {
    /// Create a layout for a grid of `cols × rows` with the given cell metrics.
    ///
    /// Adds 2px padding on all sides.
    pub fn new(cols: u64, rows: u64, metrics: &CellMetrics) -> Self {
        let padding: f64 = 2.0;
        Layout {
            origin_x: padding,
            origin_y: padding,
            cell_width: metrics.width,
            cell_height: metrics.height,
            pixel_width: cols as f64 * metrics.width + padding * 2.0,
            pixel_height: rows as f64 * metrics.height + padding * 2.0,
        }
    }

    /// Convert a (col, row) grid position to pixel (x, y) for the cell's top-left corner.
    pub fn cell_to_pixel(&self, col: u64, row: u64) -> (f64, f64) {
        (
            self.origin_x + col as f64 * self.cell_width,
            self.origin_y + row as f64 * self.cell_height,
        )
    }
}
