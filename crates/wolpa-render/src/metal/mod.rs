//! ## Metal renderer
//!
//! Converts grid state into Metal draw commands. The pipeline:
//!
//! 1. For each visible cell, look up the glyph in the atlas.
//! 2. Build a quad vertex (position + UV) in a growable vertex buffer.
//! 3. Submit a single instanced draw call to the GPU.
//!
//! ### Shaders
//!
//! - **Vertex shader**: Transforms cell quads from grid space to clip space.
//! - **Fragment shader**: Samples the atlas texture using the glyph's alpha mask
//!   and tints it with the cell's foreground/background colors.
//! - **Cursor pass**: Renders the cursor (block, underline, or I-beam) as a
//!   separate draw on top of the glyph pass.

// Placeholder — to be implemented in Phase 2.
