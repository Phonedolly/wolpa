//! ## Highlight resolver
//!
//! Maps Neovim's `hl_id` (highlight ID) to `CellAttr` (render-ready attribute).
//!
//! ### Deferred resolution
//!
//! Neovim may send `grid_line` events with `hl_id` values **before** sending the
//! corresponding `hl_attr_define`. This is by design in the UI protocol to optimize
//! the redraw flow. The resolver handles this by:
//!
//! 1. When asked to resolve an unknown `hl_id`, return a default-colored placeholder.
//! 2. When `hl_attr_define` arrives, update the stored attribute.
//! 3. The renderer should re-resolve all visible cells after each redraw batch.
//!
//! ### Default colors
//!
//! `default_colors_set` sets the fallback foreground/background for `hl_id = 0`
//! (the `Normal` group) and for any undefined `hl_id`. The renderer applies these
//! when a cell's attribute has `foreground: None` or `background: None`.

use std::collections::HashMap;

use super::event::{CellAttr, RedrawEvent, RgbAttr};

/// Resolves `hl_id → CellAttr`, with support for deferred resolution.
///
/// Stores known highlight attributes in a `HashMap<u64, CellAttr>`.
/// Default colors are initialized to white-on-black and updated by
/// `default_colors_set` events.
#[derive(Debug, Clone)]
pub struct HighlightResolver {
    attrs: HashMap<u64, CellAttr>,
    default_fg: [f32; 4],
    default_bg: [f32; 4],
}

impl HighlightResolver {
    /// Create a new resolver with white-on-black defaults.
    pub fn new() -> Self {
        HighlightResolver {
            attrs: HashMap::new(),
            default_fg: [1.0, 1.0, 1.0, 1.0],
            default_bg: [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Resolve `hl_id` to a `CellAttr`.
    ///
    /// - `hl_id = 0`: Returns the default colors (from `default_colors_set`).
    /// - Known `hl_id`: Returns the stored attribute.
    /// - Unknown `hl_id`: Returns a placeholder with default colors.
    ///   The caller should re-resolve after the next `hl_attr_define` batch.
    pub fn resolve(&self, hl_id: u64) -> CellAttr {
        self.attrs
            .get(&hl_id)
            .cloned()
            .unwrap_or_else(|| CellAttr::default_colors(self.default_fg, self.default_bg))
    }

    /// Process a redraw event that affects highlight state.
    ///
    /// - `HlAttrDefine`: Converts `RgbAttr` → `CellAttr` and stores it.
    /// - `DefaultColorsSet`: Updates defaults and `hl_id = 0`.
    pub fn apply(&mut self, event: &RedrawEvent) {
        match event {
            RedrawEvent::HlAttrDefine { id, rgb_attr, .. } => {
                let attr = rgb_to_cell_attr(rgb_attr, self.default_fg, self.default_bg);
                self.attrs.insert(*id, attr);
            }
            RedrawEvent::DefaultColorsSet { rgb_fg, rgb_bg, .. } => {
                self.default_fg = CellAttr::pack_color(*rgb_fg);
                self.default_bg = CellAttr::pack_color(*rgb_bg);
                let default_attr = CellAttr::default_colors(self.default_fg, self.default_bg);
                self.attrs.insert(0, default_attr);
            }
            _ => {}
        }
    }
}

impl Default for HighlightResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a raw `RgbAttr` into a `CellAttr` by packing colors and applying
/// default fallbacks.
fn rgb_to_cell_attr(rgb: &RgbAttr, default_fg: [f32; 4], default_bg: [f32; 4]) -> CellAttr {
    CellAttr {
        foreground: rgb
            .foreground
            .map(CellAttr::pack_color)
            .or(Some(default_fg)),
        background: rgb
            .background
            .map(CellAttr::pack_color)
            .or(Some(default_bg)),
        special: rgb.special.map(CellAttr::pack_color),
        reverse: rgb.reverse,
        italic: rgb.italic,
        bold: rgb.bold,
        strikethrough: rgb.strikethrough,
        underline: rgb.underline,
        undercurl: rgb.undercurl,
        underdouble: rgb.underdouble,
        underdotted: rgb.underdotted,
        underdashed: rgb.underdashed,
        blend: rgb.blend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_hl() {
        let resolver = HighlightResolver::new();
        let attr = resolver.resolve(0);
        assert_eq!(attr.foreground, Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(attr.background, Some([0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn test_hl_define() {
        let mut resolver = HighlightResolver::new();
        resolver.apply(&RedrawEvent::HlAttrDefine {
            id: 1,
            rgb_attr: RgbAttr {
                foreground: Some(0xff0000),
                background: Some(0x00ff00),
                ..Default::default()
            },
            cterm_attr: Default::default(),
            info: vec![],
        });
        let attr = resolver.resolve(1);
        assert_eq!(attr.foreground, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(attr.background, Some([0.0, 1.0, 0.0, 1.0]));
    }

    #[test]
    fn test_default_colors_set() {
        let mut resolver = HighlightResolver::new();
        resolver.apply(&RedrawEvent::DefaultColorsSet {
            rgb_fg: 0xabcdef,
            rgb_bg: 0x123456,
            rgb_sp: 0,
            cterm_fg: 0,
            cterm_bg: 0,
        });
        let attr = resolver.resolve(0);
        assert_eq!(
            attr.foreground,
            Some([171.0 / 255.0, 205.0 / 255.0, 239.0 / 255.0, 1.0])
        );
    }
}
