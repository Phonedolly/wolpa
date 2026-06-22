//! ## wolpa-render — macOS Metal rendering
//!
//! This crate renders the grid state (from `wolpa-core`) onto the screen using
//! Apple's Metal GPU API for fast 2D text rendering.
//!
//! ### Pipeline
//!
//! ```text
//! Grid state → Cell shaper (Core Text) → Glyph atlas (MTLTexture) → Metal draw
//! ```
//!
//! 1. **Font loading** (`font/`) — Load monospace fonts via Core Text, measure
//!    glyph advances, and compute pixel-per-grid-cell layout.
//! 2. **Glyph atlas** (`atlas/`) — Rasterize glyphs once into a shared `MTLTexture`
//!    cache with LRU eviction.
//! 3. **Metal renderer** (`metal/`) — Build quad vertex buffers from grid cells,
//!    issue instanced draw calls with custom shaders for glyph tinting.
//! 4. **Layout** (`layout/`) — Convert grid (row, col) → pixel (x, y) coordinates
//!    based on font metrics and cell padding.
//!
//! ### Platform
//!
//! This crate compiles only on macOS (`#[cfg(target_os = "macos")]`).
//! It depends on Metal, Core Text, and Core Graphics frameworks.

pub mod atlas;
pub mod font;
pub mod layout;
pub mod metal;
