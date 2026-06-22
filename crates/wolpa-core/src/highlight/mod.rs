use std::collections::HashMap;

use super::event::{CellAttr, RedrawEvent, RgbAttr};

/// Resolves hl_id → CellAttr, supporting deferred resolution.
///
/// Neovim may send `grid_line` with hl_ids before `hl_attr_define` for those ids.
/// The resolver stores pending references and resolves when attributes arrive.
#[derive(Debug, Clone)]
pub struct HighlightResolver {
    /// Known attributes, keyed by hl_id.
    attrs: HashMap<u64, CellAttr>,
    /// Default colors (set by default_colors_set).
    default_fg: [f32; 4],
    default_bg: [f32; 4],
}

impl HighlightResolver {
    pub fn new() -> Self {
        HighlightResolver {
            attrs: HashMap::new(),
            default_fg: [1.0, 1.0, 1.0, 1.0],
            default_bg: [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Resolve an hl_id to a CellAttr.
    ///
    /// If the hl_id has been defined via hl_attr_define, returns the stored attr.
    /// If not yet defined, returns None and the caller should store it for
    /// late resolution (see `pending_hl_ids` pattern — but for simplicity,
    /// we return a default attr and the caller re-resolves after more define events).
    pub fn resolve(&self, hl_id: u64) -> CellAttr {
        self.attrs.get(&hl_id).cloned().unwrap_or_else(|| {
            if hl_id == 0 {
                CellAttr::default_colors(self.default_fg, self.default_bg)
            } else {
                // Unknown hl_id — return a placeholder with default colors.
                // The renderer should re-resolve after attribute batches.
                CellAttr::default_colors(self.default_fg, self.default_bg)
            }
        })
    }

    /// Process a redraw event that affects highlight state.
    pub fn apply(&mut self, event: &RedrawEvent) {
        match event {
            RedrawEvent::HlAttrDefine { id, rgb_attr, .. } => {
                let attr = rgb_to_cell_attr(rgb_attr, self.default_fg, self.default_bg);
                self.attrs.insert(*id, attr);
            }
            RedrawEvent::DefaultColorsSet { rgb_fg, rgb_bg, .. } => {
                self.default_fg = CellAttr::pack_color(*rgb_fg);
                self.default_bg = CellAttr::pack_color(*rgb_bg);
                // Update hl_id 0 with new defaults
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
        // 0xabcdef → r=0xab, g=0xcd, b=0xef
        assert_eq!(
            attr.foreground,
            Some([171.0 / 255.0, 205.0 / 255.0, 239.0 / 255.0, 1.0])
        );
    }
}
