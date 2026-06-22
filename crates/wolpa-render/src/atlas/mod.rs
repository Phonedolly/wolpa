//! ## Glyph atlas — MTLTexture cache
//!
//! A shared `MTLTexture` (2048² RGBA8) that stores rasterized glyph bitmaps.
//! Uses simple shelf-packing: glyphs are placed left-to-right in rows.
//! When a row is full, a new row starts below.

use metal::Device;
use std::collections::HashMap;

/// UV coordinates for a glyph in the atlas.
#[derive(Debug, Clone, Copy)]
pub struct GlyphUV {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

/// A glyph atlas backed by an MTLTexture with shelf-packing.
pub struct GlyphAtlas {
    pub texture: metal::Texture,
    pub atlas_size: u64,
    /// Current cursor: (x, y) for the next glyph to be placed.
    cursor_x: u64,
    cursor_y: u64,
    /// Height of the current row.
    row_height: u64,
    /// Cache: (font_size, ch) → GlyphUV.
    cache: HashMap<(u64, char), GlyphUV>,
}

impl GlyphAtlas {
    /// Create a new atlas texture with the given side length.
    pub fn new(device: &Device, size: u64) -> Self {
        let desc = metal::TextureDescriptor::new();
        desc.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
        desc.set_width(size);
        desc.set_height(size);
        desc.set_storage_mode(metal::MTLStorageMode::Managed);

        let texture = device.new_texture(&desc);

        GlyphAtlas {
            texture,
            atlas_size: size,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            cache: HashMap::new(),
        }
    }

    /// Look up UV for a glyph; rasterize and upload if not cached.
    ///
    /// Returns None if the atlas is full.
    pub fn get_or_upload(
        &mut self,
        font_size: u64,
        ch: char,
        pixels: &[u8],
        w: usize,
        h: usize,
    ) -> Option<GlyphUV> {
        let key = (font_size, ch);
        if let Some(uv) = self.cache.get(&key) {
            return Some(*uv);
        }

        // Check if we need to advance to next row
        if self.cursor_x + w as u64 > self.atlas_size {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }

        // Check if we're out of space
        if self.cursor_y + h as u64 > self.atlas_size {
            return None; // Atlas full — caller should evict
        }

        let x = self.cursor_x;
        let y = self.cursor_y;

        // Prepare RGBA data: R=alpha, G=alpha, B=alpha, A=255
        // This allows the fragment shader to sample any channel
        let mut rgba: Vec<u8> = Vec::with_capacity(w * h * 4);
        for &p in pixels {
            rgba.push(p);
            rgba.push(p);
            rgba.push(p);
            rgba.push(255);
        }

        let region = metal::MTLRegion {
            origin: metal::MTLOrigin { x, y, z: 0 },
            size: metal::MTLSize {
                width: w as u64,
                height: h as u64,
                depth: 1,
            },
        };

        self.texture
            .replace_region(region, 0, rgba.as_ptr() as *const _, (w * 4) as u64);

        let size = self.atlas_size as f32;
        let uv = GlyphUV {
            u0: x as f32 / size,
            v0: y as f32 / size,
            u1: (x + w as u64) as f32 / size,
            v1: (y + h as u64) as f32 / size,
        };

        self.cache.insert(key, uv);

        // Advance cursor
        self.cursor_x += w as u64;
        self.row_height = self.row_height.max(h as u64);

        Some(uv)
    }
}
