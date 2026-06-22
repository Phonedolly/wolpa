//! ## Glyph atlas — MTLTexture cache
//!
//! Rasterizes glyphs into a shared `MTLTexture` (e.g., 2048×2048 RGBA8).
//! Uses an LRU eviction policy when the atlas is full. Each glyph is
//! stored with its UV coordinates for shader sampling.
//!
//! ### Atlas layout
//!
//! Glyphs are packed into rows using a simple shelf-packing algorithm.
//! Each glyph entry stores `(x, y, width, height)` in texel coordinates.

// Placeholder — to be implemented in Phase 2.
