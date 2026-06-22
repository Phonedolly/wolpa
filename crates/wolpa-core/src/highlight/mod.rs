// Highlight attribute resolver.
// Maps hl_id (u64) to resolved CellAttr (foreground, background, bold, italic, etc.)
// Handles deferred resolution: hl_id can appear in grid_line before hl_attr_define.
