//! ## Font loading and text shaping via Core Text
//!
//! Loads monospace fonts, measures glyph metrics (advance, ascent, descent),
//! and maps Unicode code points to glyph IDs and `CGSize` bounding boxes.
//!
//! ### Shaping
//!
//! Core Text handles basic shaping (kerning, ligatures like `fi`/`fl`).
//! For programming ligatures (Fira Code style, e.g., `!=` → `≠`),
//! HarfBuzz may be integrated later.

// Placeholder — to be implemented in Phase 2.
