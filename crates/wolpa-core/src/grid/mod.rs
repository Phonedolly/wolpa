//! ## Grid state machine
//!
//! This module maintains the 2D cell grid that represents the current Neovim
//! screen state. It consumes `RedrawEvent`s and mutates internal state.
//!
//! ### Design
//!
//! - **Multi-grid**: Neovim can have multiple grids (grid 1 = main, grid 2+ = popupmenu,
//!   floating windows, etc.). `GridState` manages them by `HashMap<u64, Grid>`.
//! - **Flush-bounded**: Events between two `Flush` events form a batch. The caller
//!   should `apply_batch()` then render once.
//! - **Cell granularity**: The grid is cell-level (not pixel-level). Pixel layout
//!   is handled by `wolpa-render`.

use std::collections::HashMap;

use super::event::{GridLineCell, RedrawEvent};

/// A single cell in the text grid.
///
/// Each cell holds one grapheme cluster and a highlight ID that indexes into
/// the `HighlightResolver`. The default cell is a space (`' '`) with `hl_id = 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub text: String,
    pub hl_id: u64,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            text: " ".to_string(),
            hl_id: 0,
        }
    }
}

/// A 2D grid of cells representing one Neovim screen surface.
///
/// Cells are stored in row-major order. The grid supports line insertion,
/// clearing, scrolling, and resizing. Bounds are checked on all operations;
/// out-of-bounds cell access returns a reference to a space cell.
#[derive(Debug, Clone)]
pub struct Grid {
    pub id: u64,
    pub width: u64,
    pub height: u64,
    cells: Vec<Cell>,
    pub cursor_row: u64,
    pub cursor_col: u64,
}

impl Grid {
    /// Create a new grid filled with default cells.
    pub fn new(id: u64, width: u64, height: u64) -> Self {
        let size = (width * height) as usize;
        Grid {
            id,
            width,
            height,
            cells: vec![Cell::default(); size],
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    fn index(&self, row: u64, col: u64) -> usize {
        (row * self.width + col) as usize
    }

    /// Get a reference to the cell at `(row, col)`.
    ///
    /// Out-of-bounds access returns a reference to a default space cell.
    pub fn cell(&self, row: u64, col: u64) -> &Cell {
        let idx = self.index(row, col);
        self.cells.get(idx).unwrap_or(&self.cells[0])
    }

    /// Set the text and highlight of the cell at `(row, col)`.
    /// Out-of-bounds writes are silently ignored.
    fn set_cell(&mut self, row: u64, col: u64, text: &str, hl_id: u64) {
        if row < self.height && col < self.width {
            let idx = self.index(row, col);
            if let Some(cell) = self.cells.get_mut(idx) {
                cell.text = text.to_string();
                cell.hl_id = hl_id;
            }
        }
    }

    /// Insert a sequence of `GridLineCell`s starting at `(row, col_start)`.
    ///
    /// Handles run-length encoding: cells with `repeat > 1` are expanded
    /// to `repeat` consecutive cells.
    fn put_line(&mut self, row: u64, col_start: u64, cells: &[GridLineCell]) {
        let mut col = col_start;
        for cell in cells {
            let hl_id = cell.hl_id.unwrap_or(0);
            for _ in 0..cell.repeat {
                self.set_cell(row, col, &cell.text, hl_id);
                col += 1;
            }
        }
    }

    /// Reset all cells to the default (space, hl_id = 0).
    fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    /// Scroll a rectangular region of the grid.
    ///
    /// - `rows > 0`: scroll down — content moves down, top rows cleared.
    /// - `rows < 0`: scroll up — content moves up, bottom rows cleared.
    ///
    /// `cols` is accepted but not yet implemented (Neovim rarely sends column scrolls).
    fn scroll(&mut self, top: u64, bot: u64, left: u64, right: u64, rows: i64, _cols: i64) {
        let top = top.min(self.height);
        let bot = bot.min(self.height).max(top);
        let left = left.min(self.width);
        let right = right.min(self.width).max(left);

        if rows > 0 {
            // Scroll down: move lines from top to bot-rows down, clear top rows
            for r in (top..bot.saturating_sub(rows as u64)).rev() {
                let src_row = r;
                let dst_row = r + rows as u64;
                for c in left..right {
                    let src_idx = self.index(src_row, c);
                    let dst_idx = self.index(dst_row, c);
                    self.cells.swap(src_idx, dst_idx);
                }
            }
            for r in top..(top + rows as u64).min(bot) {
                for c in left..right {
                    let idx = self.index(r, c);
                    if let Some(cell) = self.cells.get_mut(idx) {
                        *cell = Cell::default();
                    }
                }
            }
        } else if rows < 0 {
            // Scroll up: move lines from top-rows to bot up, clear bottom rows
            let count = (-rows) as u64;
            for r in (top + count)..bot {
                let src_row = r;
                let dst_row = r - count;
                for c in left..right {
                    let src_idx = self.index(src_row, c);
                    let dst_idx = self.index(dst_row, c);
                    self.cells.swap(src_idx, dst_idx);
                }
            }
            for r in bot.saturating_sub(count)..bot {
                for c in left..right {
                    let idx = self.index(r, c);
                    if let Some(cell) = self.cells.get_mut(idx) {
                        *cell = Cell::default();
                    }
                }
            }
        }
    }

    /// Resize the grid to `width × height`.
    ///
    /// Existing cell content is preserved where it fits within the new dimensions.
    /// Cells outside the new bounds are discarded.
    fn resize(&mut self, width: u64, height: u64) {
        let new_size = (width * height) as usize;
        let mut new_cells = vec![Cell::default(); new_size];

        let copy_width = self.width.min(width);
        let copy_height = self.height.min(height);

        for r in 0..copy_height {
            for c in 0..copy_width {
                let src_idx = self.index(r, c);
                let dst_idx = ((r * width + c) as usize).min(new_size - 1);
                new_cells[dst_idx] = self.cells[src_idx].clone();
            }
        }

        self.width = width;
        self.height = height;
        self.cells = new_cells;
    }
}

/// The top-level grid state: a collection of grids plus current mode.
///
/// ### Invariants
///
/// - Grid 1 always exists (the main editing grid).
/// - `current_mode` and `current_mode_idx` are updated by `ModeChange` events.
/// - Events that reference a non-existent grid create it on demand (`GridResize`).
#[derive(Debug, Clone)]
pub struct GridState {
    pub grids: HashMap<u64, Grid>,
    pub current_mode: String,
    pub current_mode_idx: u64,
}

impl GridState {
    /// Create a new grid state with a main grid of `width × height`.
    pub fn new(width: u64, height: u64) -> Self {
        let mut grids = HashMap::new();
        grids.insert(1, Grid::new(1, width, height));
        GridState {
            grids,
            current_mode: "normal".to_string(),
            current_mode_idx: 1,
        }
    }

    /// Get a reference to grid by ID.
    pub fn grid(&self, id: u64) -> Option<&Grid> {
        self.grids.get(&id)
    }

    /// Get a mutable reference to grid by ID.
    pub fn grid_mut(&mut self, id: u64) -> Option<&mut Grid> {
        self.grids.get_mut(&id)
    }

    /// Apply a single redraw event, mutating the grid state.
    ///
    /// Events that affect highlight or rendering (e.g., `HlAttrDefine`,
    /// `MsgShow`) are ignored here — they're handled by `HighlightResolver`
    /// and the renderer respectively.
    pub fn apply(&mut self, event: &RedrawEvent) {
        match event {
            RedrawEvent::GridResize {
                grid,
                width,
                height,
            } => {
                if let Some(g) = self.grids.get_mut(grid) {
                    g.resize(*width, *height);
                } else {
                    self.grids.insert(*grid, Grid::new(*grid, *width, *height));
                }
            }
            RedrawEvent::GridLine {
                grid,
                row,
                col_start,
                cells,
            } => {
                if let Some(g) = self.grids.get_mut(grid) {
                    g.put_line(*row, *col_start, cells);
                }
            }
            RedrawEvent::GridClear { grid } => {
                if let Some(g) = self.grids.get_mut(grid) {
                    g.clear();
                }
            }
            RedrawEvent::GridCursorGoto { grid, row, col } => {
                if let Some(g) = self.grids.get_mut(grid) {
                    g.cursor_row = *row;
                    g.cursor_col = *col;
                }
            }
            RedrawEvent::GridScroll {
                grid,
                top,
                bot,
                left,
                right,
                rows,
                cols,
            } => {
                if let Some(g) = self.grids.get_mut(grid) {
                    g.scroll(*top, *bot, *left, *right, *rows, *cols);
                }
            }
            RedrawEvent::GridDestroy { grid } => {
                self.grids.remove(grid);
            }
            RedrawEvent::ModeChange { name, index } => {
                self.current_mode = name.clone();
                self.current_mode_idx = *index;
            }
            _ => {}
        }
    }

    /// Apply a batch of redraw events (between two `Flush` events).
    pub fn apply_batch(&mut self, events: &[RedrawEvent]) {
        for event in events {
            self.apply(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_grid() {
        let g = Grid::new(1, 80, 24);
        assert_eq!(g.width, 80);
        assert_eq!(g.height, 24);
        assert_eq!(g.cells.len(), 80 * 24);
        assert_eq!(g.cell(0, 0).text, " ");
    }

    #[test]
    fn test_put_line() {
        let mut g = Grid::new(1, 10, 5);
        let cells = vec![
            GridLineCell {
                text: "H".into(),
                hl_id: Some(0),
                repeat: 1,
            },
            GridLineCell {
                text: "i".into(),
                hl_id: None,
                repeat: 1,
            },
        ];
        g.put_line(0, 0, &cells);
        assert_eq!(g.cell(0, 0).text, "H");
        assert_eq!(g.cell(0, 1).text, "i");
    }

    #[test]
    fn test_line_with_repeat() {
        let mut g = Grid::new(1, 10, 5);
        let cells = vec![GridLineCell {
            text: " ".into(),
            hl_id: Some(0),
            repeat: 5,
        }];
        g.put_line(0, 3, &cells);
        assert_eq!(g.cell(0, 3).text, " ");
        assert_eq!(g.cell(0, 7).text, " ");
    }

    #[test]
    fn test_clear() {
        let mut g = Grid::new(1, 5, 1);
        g.put_line(
            0,
            0,
            &[GridLineCell {
                text: "X".into(),
                hl_id: None,
                repeat: 5,
            }],
        );
        g.clear();
        assert_eq!(g.cell(0, 0).text, " ");
    }

    #[test]
    fn test_resize_smaller() {
        let mut g = Grid::new(1, 10, 10);
        g.put_line(
            0,
            0,
            &[GridLineCell {
                text: "A".into(),
                hl_id: None,
                repeat: 1,
            }],
        );
        g.resize(5, 5);
        assert_eq!(g.width, 5);
        assert_eq!(g.height, 5);
        assert_eq!(g.cell(0, 0).text, "A");
    }

    #[test]
    fn test_scroll_down() {
        let mut g = Grid::new(1, 1, 5);
        for i in 0..5 {
            g.set_cell(i as u64, 0, &format!("{i}"), 0);
        }
        g.scroll(0, 5, 0, 1, 2, 0);
        assert_eq!(g.cell(2, 0).text, "0");
        assert_eq!(g.cell(3, 0).text, "1");
        assert_eq!(g.cell(4, 0).text, "2");
        assert_eq!(g.cell(0, 0).text, " ");
    }

    #[test]
    fn test_grid_state_apply() {
        let mut state = GridState::new(80, 24);
        state.apply(&RedrawEvent::GridLine {
            grid: 1,
            row: 0,
            col_start: 0,
            cells: vec![GridLineCell {
                text: "H".into(),
                hl_id: None,
                repeat: 1,
            }],
        });
        state.apply(&RedrawEvent::GridCursorGoto {
            grid: 1,
            row: 0,
            col: 1,
        });
        let g = state.grid(1).unwrap();
        assert_eq!(g.cell(0, 0).text, "H");
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 1);
    }
}
