//! ## Font loading and text shaping via Core Text
//!
//! Loads a monospace system font, measures glyph metrics (cell width/height,
//! baseline offset), and provides per-glyph rasterization for the atlas.

use core_graphics::font::CGGlyph;
use core_graphics::geometry::CGSize;
use core_text::font;
use core_text::font::CTFont;

/// Metrics for a monospace font cell.
#[derive(Debug, Clone, Copy)]
pub struct CellMetrics {
    pub width: f64,
    pub height: f64,
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
}

/// A loaded monospace font with measured metrics.
pub struct Font {
    pub ct_font: CTFont,
    pub metrics: CellMetrics,
    pub font_size: f64,
}

impl Font {
    /// Load a monospace system font at the given point size.
    ///
    /// Tries SF Mono first, then Menlo, then Courier fallback.
    pub fn new(size: f64) -> Self {
        let ct_font = load_monospace_font(size);
        let metrics = measure_metrics(&ct_font);
        Font {
            ct_font,
            metrics,
            font_size: size,
        }
    }
}

fn load_monospace_font(size: f64) -> CTFont {
    for name in &["SF Mono", "Menlo", "Courier"] {
        if let Ok(font) = font::new_from_name(name, size) {
            return font;
        }
    }
    panic!("no monospace font found on the system");
}

/// Measure cell metrics from a CTFont.
///
/// Cell width is the advance of the 'M' glyph.
/// Cell height is ascent + descent + leading.
fn measure_metrics(font: &CTFont) -> CellMetrics {
    let ascent = font.ascent();
    let descent = font.descent();
    let leading = font.leading();
    let height = ascent + descent + leading;
    let advance = measure_glyph_advance(font, 'M').unwrap_or(8.0);

    CellMetrics {
        width: advance,
        height,
        ascent,
        descent,
        leading,
    }
}

/// Get the horizontal advance width for a single character.
fn measure_glyph_advance(font: &CTFont, ch: char) -> Option<f64> {
    let mut glyph: CGGlyph = 0;
    let c = ch as u16;
    // SAFETY: single stack-allocated CGGlyph, valid pointer
    unsafe {
        font.get_glyphs_for_characters(&c as *const u16, &mut glyph as *mut _, 1);
    }
    if glyph == 0 {
        return None;
    }
    let mut advances = [CGSize::new(0.0, 0.0)];
    // SAFETY: advances array has room for 1 element
    unsafe {
        font.get_advances_for_glyphs(
            core_text::font_descriptor::kCTFontOrientationDefault,
            &glyph as *const CGGlyph,
            advances.as_mut_ptr(),
            1,
        );
    }
    Some(advances[0].width)
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("font_size", &self.font_size)
            .field("cell_width", &self.metrics.width)
            .field("cell_height", &self.metrics.height)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_font() {
        let font = Font::new(14.0);
        assert!(font.metrics.width > 0.0, "cell width should be positive");
        assert!(font.metrics.height > 0.0, "cell height should be positive");
        assert!(font.metrics.ascent > 0.0, "ascent should be positive");
    }
}
