//! ## Glyph atlas — MTLTexture cache
//!
//! A shared `MTLTexture` that stores rasterized glyph bitmaps.
//! Uses a simple shelf-packing algorithm for glyph placement.
//! LRU eviction when the atlas is full (not yet implemented).
//!
//! ### Atlas entry
//!
//! Each entry stores `(x, y, width, height)` in texel coordinates.
//! The fragment shader uses these UV coordinates to sample the correct glyph.

use metal::Device;

/// A glyph atlas backed by an MTLTexture.
pub struct GlyphAtlas {
    pub texture: metal::Texture,
    pub atlas_size: u64,
}

impl GlyphAtlas {
    /// Create a new atlas texture with the given side length (e.g., 2048).
    ///
    /// Format: RGBA8Unorm, one byte per channel.
    pub fn new(device: &Device, size: u64) -> Self {
        let desc = metal::TextureDescriptor::new();
        desc.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
        desc.set_width(size);
        desc.set_height(size);
        desc.set_storage_mode(metal::MTLStorageMode::Private);

        let texture = device.new_texture(&desc);

        GlyphAtlas {
            texture,
            atlas_size: size,
        }
    }
}
