//! ## Font loading and text shaping via Core Text
//!
//! Loads a monospace system font, measures glyph metrics, and rasterizes
//! individual glyphs to alpha bitmaps for the atlas texture.

use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::font::CGGlyph;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
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
    pub scale: f64,
}

impl Font {
    /// Load a monospace system font. `size` is physical pixels (pt * scale).
    /// `scale` is the Retina factor (1.0 or 2.0).
    pub fn new(size: f64, scale: f64) -> Self {
        let pt_size = size / scale;
        let ct_font = load_monospace_font(pt_size);
        let metrics = measure_metrics(&ct_font, scale);
        Font {
            ct_font,
            metrics,
            font_size: pt_size,
            scale,
        }
    }

    /// Get the CGGlyph for a character.
    pub fn glyph_for_char(&self, ch: char) -> CGGlyph {
        get_glyph(&self.ct_font, ch)
    }

    /// Rasterize a glyph to an 8-bit alpha bitmap.
    ///
    /// Returns (pixels, width, height) where pixels is row-major alpha values.
    /// The bitmap is sized to fit the glyph's bounding box plus 1px padding.
    pub fn rasterize_glyph(&self, ch: char) -> (Vec<u8>, usize, usize) {
        rasterize(&self.ct_font, ch, self.scale)
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

/// Measure cell metrics from a CTFont. `scale` multiplies to physical pixels.
fn measure_metrics(font: &CTFont, scale: f64) -> CellMetrics {
    let ascent = font.ascent() * scale;
    let descent = font.descent() * scale;
    let leading = font.leading() * scale;
    let height = ascent + descent + leading;
    let advance = measure_glyph_advance(font, 'M').unwrap_or(8.0) * scale;

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

/// Rasterize a single glyph to an 8-bit alpha bitmap.
fn rasterize(font: &CTFont, ch: char, scale: f64) -> (Vec<u8>, usize, usize) {
    let glyph = get_glyph(font, ch);
    if glyph == 0 {
        return (vec![0u8; 4], 2, 2);
    }

    let bbox: CGRect = font.get_bounding_rects_for_glyphs(
        core_text::font_descriptor::kCTFontOrientationDefault,
        &[glyph],
    );

    let pad: f64 = 2.0 * scale;
    let w = (bbox.size.width.ceil() * scale + pad * 2.0) as usize;
    let h = (bbox.size.height.ceil() * scale + pad * 2.0) as usize;
    let w = w.max(1);
    let h = h.max(1);
    let len = w * h;

    // Own the buffer so we can read after draw_glyphs consumes the context
    let mut pixels: Vec<u8> = vec![255u8; len];

    let color_space = CGColorSpace::create_device_gray();
    let ctx = CGContext::create_bitmap_context(
        Some(pixels.as_mut_ptr() as *mut _),
        w,
        h,
        8,
        w,
        &color_space,
        core_graphics::base::kCGImageAlphaNone,
    );

    let black = core_graphics::color::CGColor::rgb(0.0, 0.0, 0.0, 1.0);
    ctx.set_fill_color(&black);

    let x = -bbox.origin.x + pad / scale;
    let y = -bbox.origin.y + pad / scale;
    let pos = [CGPoint::new(x, y)];

    // Scale CTM for Retina: glyph rendered at 2x resolution
    ctx.scale(scale, scale);
    font.draw_glyphs(&[glyph], &pos, ctx);

    // Invert: white (255) bg → 0 alpha, black (0) glyph → 255 alpha
    for p in &mut pixels {
        *p = 255 - *p;
    }
    (pixels, w, h)
}

fn get_glyph(font: &CTFont, ch: char) -> CGGlyph {
    let mut glyph: CGGlyph = 0;
    let c = ch as u16;
    unsafe {
        font.get_glyphs_for_characters(&c as *const u16, &mut glyph as *mut _, 1);
    }
    glyph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_font() {
        let font = Font::new(14.0, 1.0);
        assert!(font.metrics.width > 0.0);
        assert!(font.metrics.height > 0.0);
    }

    #[test]
    fn test_rasterize_glyph() {
        let font = Font::new(14.0, 1.0);
        let (pixels, w, h) = font.rasterize_glyph('A');
        assert!(w > 0 && h > 0, "bitmap should have non-zero size");
        assert!(
            pixels.iter().any(|&p| p > 0),
            "glyph 'A' should have some non-zero alpha pixels"
        );
    }
}
