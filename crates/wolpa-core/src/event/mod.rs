use rmpv::Value;

/// A single cell in a grid_line event.
#[derive(Debug, Clone, PartialEq)]
pub struct GridLineCell {
    pub text: String,
    pub hl_id: Option<u64>,
    pub repeat: u64,
}

/// RGB highlight attribute.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RgbAttr {
    pub foreground: Option<u32>,
    pub background: Option<u32>,
    pub special: Option<u32>,
    pub reverse: bool,
    pub italic: bool,
    pub bold: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub undercurl: bool,
    pub underdouble: bool,
    pub underdotted: bool,
    pub underdashed: bool,
    pub blend: u8,
}

/// CTerm highlight attribute (16-color fallback).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CtermAttr {
    pub foreground: Option<u16>,
    pub background: Option<u16>,
}

/// Highlight info from hl_attr_define.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightInfo {
    pub kind: String,
    pub id: Option<u64>,
    pub hi_name: Option<String>,
}

/// Single mode info from mode_info_set.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeInfo {
    pub name: String,
    pub short_name: String,
    pub cursor_shape: Option<String>,
    pub cell_percentage: Option<u64>,
    pub blink_wait: Option<u64>,
    pub blink_on: Option<u64>,
    pub blink_off: Option<u64>,
    pub hl_id: Option<u64>,
    pub hl_lens: Option<u64>,
    pub attr_id: Option<u64>,
    pub attr_id_lm: Option<u64>,
}

/// A message chunk (for msg_show).
#[derive(Debug, Clone, PartialEq)]
pub struct MsgChunk {
    pub text: String,
    pub hl_id: Option<u64>,
}

/// A popupmenu item.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupMenuItem {
    pub word: String,
    pub kind: Option<String>,
    pub menu: Option<String>,
    pub info: Option<String>,
}

/// Tabline info.
#[derive(Debug, Clone, PartialEq)]
pub struct TablineInfo {
    pub name: String,
    pub tabpage_id: u64,
}

/// Redraw events from Neovim.
///
/// These correspond to the events in a `["redraw", [...]]` notification
/// (see :help ui-events).
#[derive(Debug, Clone, PartialEq)]
pub enum RedrawEvent {
    /// Grid was resized.
    GridResize { grid: u64, width: u64, height: u64 },
    /// A line of cells was updated.
    GridLine {
        grid: u64,
        row: u64,
        col_start: u64,
        cells: Vec<GridLineCell>,
    },
    /// Grid was cleared.
    GridClear { grid: u64 },
    /// Cursor moved.
    GridCursorGoto { grid: u64, row: u64, col: u64 },
    /// Region was scrolled.
    GridScroll {
        grid: u64,
        top: u64,
        bot: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    },
    /// Grid was destroyed.
    GridDestroy { grid: u64 },
    /// Highlight attribute was defined.
    HlAttrDefine {
        id: u64,
        rgb_attr: RgbAttr,
        cterm_attr: CtermAttr,
        info: Vec<HighlightInfo>,
    },
    /// Default colors were set.
    DefaultColorsSet {
        rgb_fg: u32,
        rgb_bg: u32,
        rgb_sp: u32,
        cterm_fg: u16,
        cterm_bg: u16,
    },
    /// Mode information was set.
    ModeInfoSet {
        cursor_style_enabled: bool,
        mode_info: Vec<ModeInfo>,
    },
    /// Mode changed.
    ModeChange { name: String, index: u64 },
    /// Option was set (e.g. guifont).
    OptionSet { name: String, value: Value },
    /// Message shown (command-line, echo, etc.).
    MsgShow {
        kind: String,
        content: Vec<MsgChunk>,
        replace_last: bool,
    },
    /// Messages cleared.
    MsgClear,
    /// nvim is busy.
    BusyStart,
    /// nvim is no longer busy.
    BusyStop,
    /// Popupmenu was shown.
    PopupMenuShow {
        items: Vec<PopupMenuItem>,
        selected: i64,
        row: u64,
        col: u64,
        grid: u64,
    },
    /// Popupmenu selection changed.
    PopupMenuSelect { selected: i64 },
    /// Popupmenu was hidden.
    PopupMenuHide,
    /// Tabline was updated.
    TablineUpdate {
        curtab: TablineInfo,
        tabs: Vec<TablineInfo>,
        show: bool,
    },
    /// Window position.
    WinPos {
        grid: u64,
        win: u64,
        start_row: u64,
        start_col: u64,
        width: u64,
        height: u64,
    },
    /// Window close.
    WinClose { grid: u64 },
    /// Window hide.
    WinHide { grid: u64 },
    /// Window float position.
    WinFloatPos {
        grid: u64,
        win: u64,
        anchor_dir: String,
        anchor_grid: u64,
        anchor_row: f64,
        anchor_col: f64,
        focusable: bool,
    },
    /// Window external position.
    WinExternalPos { grid: u64, win: u64 },
    /// Mouse enabled.
    MouseOn,
    /// Mouse disabled.
    MouseOff,
    /// Redraw batch boundary.
    Flush,
}

/// All supported UI events (including non-redraw notifications).
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Redraw(Vec<RedrawEvent>),
    /// Notification that something changed, not strictly a redraw.
    Other(String, Vec<Value>),
}

/// Resolved render attribute for a cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellAttr {
    pub foreground: Option<[f32; 4]>,
    pub background: Option<[f32; 4]>,
    pub special: Option<[f32; 4]>,
    pub reverse: bool,
    pub italic: bool,
    pub bold: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub undercurl: bool,
    pub underdouble: bool,
    pub underdotted: bool,
    pub underdashed: bool,
    pub blend: u8,
}

impl CellAttr {
    /// A default attribute (typically represents hl_id 0, "Normal").
    pub fn default_colors(fg: [f32; 4], bg: [f32; 4]) -> Self {
        CellAttr {
            foreground: Some(fg),
            background: Some(bg),
            ..Default::default()
        }
    }

    /// Pack a 24-bit RGB value into [f32; 4] for Metal.
    pub fn pack_color(rgb: u32) -> [f32; 4] {
        [
            ((rgb >> 16) & 0xff) as f32 / 255.0,
            ((rgb >> 8) & 0xff) as f32 / 255.0,
            (rgb & 0xff) as f32 / 255.0,
            1.0,
        ]
    }
}
