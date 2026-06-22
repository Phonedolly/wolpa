//! ## Neovim UI event types
//!
//! This module defines the Rust representations of all Neovim UI protocol events
//! (see `:help ui-events`). Neovim sends these as msgpack arrays within `"redraw"`
//! notifications after `nvim_ui_attach`.
//!
//! ## Event categories
//!
//! | Category | Events |
//! |---|---|
//! | Grid | `grid_resize`, `grid_line`, `grid_clear`, `grid_scroll`, `grid_destroy` |
//! | Cursor | `grid_cursor_goto` |
//! | Highlight | `hl_attr_define`, `default_colors_set` |
//! | Mode | `mode_info_set`, `mode_change` |
//! | Messages | `msg_show`, `msg_clear` |
//! | Popupmenu | `popupmenu_show`, `popupmenu_select`, `popupmenu_hide` |
//! | Tabline | `tabline_update` |
//! | Windows | `win_pos`, `win_close`, `win_hide`, `win_float_pos`, `win_external_pos` |
//! | State | `busy_start`, `busy_stop`, `mouse_on`, `mouse_off` |
//! | Sync | `flush` (redraw batch boundary) |

use rmpv::Value;

/// A single cell in a `grid_line` event.
///
/// Each cell consists of a text string (one grapheme cluster),
/// an optional highlight ID, and a repeat count for run-length encoding.
/// A cell with `text = " "` and `repeat = 5` means "5 consecutive space chars".
#[derive(Debug, Clone, PartialEq)]
pub struct GridLineCell {
    pub text: String,
    pub hl_id: Option<u64>,
    pub repeat: u64,
}

/// RGB (true-color) highlight attributes.
///
/// Sent as the second argument to `hl_attr_define`. Colors are packed 24-bit RGB
/// integers (`0xRRGGBB`). A `None` value means "use the default color from
/// `default_colors_set`".
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
    /// Alpha blend factor (0–100). Used for floating windows.
    pub blend: u8,
}

/// CTerm highlight attributes (16-color terminal palette fallback).
///
/// These mirror `RgbAttr` but use 0–255 indexed colors. Sent as the third
/// argument to `hl_attr_define`. Used only when `ext_termcolors` is enabled.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CtermAttr {
    pub foreground: Option<u16>,
    pub background: Option<u16>,
}

/// Semantic highlight group info from `hl_attr_define`.
///
/// Each entry describes how the hl_id was created — from a named highlight
/// group (`hi_name`), as a blend of two groups (`kind = "ui"`), or as a computed
/// attribute from the color scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightInfo {
    pub kind: String,
    pub id: Option<u64>,
    pub hi_name: Option<String>,
}

/// A single mode descriptor from `mode_info_set`.
///
/// Neovim sends these once after attach and again when options change.
/// Each mode has a cursor shape (block, horizontal, vertical) and blink timings.
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

/// A single chunk in a message display (`msg_show`).
///
/// Each chunk has a text segment and an optional highlight ID.
/// Neovim uses these for command-line display, echo messages, and statusline.
#[derive(Debug, Clone, PartialEq)]
pub struct MsgChunk {
    pub text: String,
    pub hl_id: Option<u64>,
}

/// A single item in the completion popupmenu.
///
/// Each item has the completion word, an optional kind (e.g., `f` for function,
/// `v` for variable), an optional menu string (extra info displayed next to the word),
/// and optional info string (documentation shown in the preview window).
#[derive(Debug, Clone, PartialEq)]
pub struct PopupMenuItem {
    pub word: String,
    pub kind: Option<String>,
    pub menu: Option<String>,
    pub info: Option<String>,
}

/// Information about a single tab in `tabline_update`.
#[derive(Debug, Clone, PartialEq)]
pub struct TablineInfo {
    pub name: String,
    pub tabpage_id: u64,
}

/// A redraw event from a Neovim `"redraw"` notification.
///
/// Each variant corresponds to a named event in the UI protocol.
/// Events are processed in batches delimited by `Flush`.
#[derive(Debug, Clone, PartialEq)]
pub enum RedrawEvent {
    /// Grid was resized to `width × height`.
    GridResize { grid: u64, width: u64, height: u64 },
    /// A range of cells on row `row`, starting at `col_start`, was updated.
    /// Cells use run-length encoding via `GridLineCell::repeat`.
    GridLine {
        grid: u64,
        row: u64,
        col_start: u64,
        cells: Vec<GridLineCell>,
    },
    /// Grid was cleared (all cells reset to space with hl_id 0).
    GridClear { grid: u64 },
    /// Cursor moved to `(row, col)`.
    GridCursorGoto { grid: u64, row: u64, col: u64 },
    /// A rectangular region `[top, bot) × [left, right)` was scrolled by
    /// `rows` lines (positive = down, negative = up) and `cols` columns.
    GridScroll {
        grid: u64,
        top: u64,
        bot: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    },
    /// Grid was destroyed (e.g., window closed).
    GridDestroy { grid: u64 },
    /// A highlight attribute was defined for `id`.
    /// Contains both true-color (`rgb_attr`) and terminal palette (`cterm_attr`) versions.
    HlAttrDefine {
        id: u64,
        rgb_attr: RgbAttr,
        cterm_attr: CtermAttr,
        info: Vec<HighlightInfo>,
    },
    /// Default foreground, background, and special colors were set.
    /// `hl_id = 0` and undefined hl_ids fall back to these.
    DefaultColorsSet {
        rgb_fg: u32,
        rgb_bg: u32,
        rgb_sp: u32,
        cterm_fg: u16,
        cterm_bg: u16,
    },
    /// Mode information was set or changed.
    /// `cursor_style_enabled` indicates whether `guicursor` styling is active.
    ModeInfoSet {
        cursor_style_enabled: bool,
        mode_info: Vec<ModeInfo>,
    },
    /// Vim mode changed (e.g., normal → insert).
    /// `name` is the full mode name, `index` indexes into `ModeInfoSet::mode_info`.
    ModeChange { name: String, index: u64 },
    /// A UI option was set (e.g., `guifont`, `linespace`).
    OptionSet { name: String, value: Value },
    /// A message was displayed (command-line, echo, etc.).
    /// `kind` is one of: `""` (unknown), `"confirm"`, `"confirm_sub"`, `"emsg"`,
    /// `"echo"`, `"echomsg"`, `"lua_error"`, `"rpc_error"`, `"return_prompt"`,
    /// `"quickfix"`, `"search_count"`, `"wmsg"`.
    MsgShow {
        kind: String,
        content: Vec<MsgChunk>,
        replace_last: bool,
    },
    /// All messages were cleared.
    MsgClear,
    /// Neovim started a long-running operation.
    BusyStart,
    /// The long-running operation finished.
    BusyStop,
    /// The popupmenu was displayed with `items`, the given `selected` item,
    /// at position `(row, col)` on `grid`.
    PopupMenuShow {
        items: Vec<PopupMenuItem>,
        selected: i64,
        row: u64,
        col: u64,
        grid: u64,
    },
    /// Popupmenu selection changed to `selected` (0-indexed, -1 = none).
    PopupMenuSelect { selected: i64 },
    /// Popupmenu was hidden.
    PopupMenuHide,
    /// Tabline was updated. `curtab` is the current tab, `tabs` is all tabs.
    /// `show` indicates whether the tabline should be visible.
    TablineUpdate {
        curtab: TablineInfo,
        tabs: Vec<TablineInfo>,
        show: bool,
    },
    /// Window `win` at grid `grid` was positioned at `(start_row, start_col)`
    /// with the given `width × height`.
    WinPos {
        grid: u64,
        win: u64,
        start_row: u64,
        start_col: u64,
        width: u64,
        height: u64,
    },
    /// Grid was closed (associated window was removed).
    WinClose { grid: u64 },
    /// Grid was hidden (associated window is still alive but not visible).
    WinHide { grid: u64 },
    /// A floating window was positioned.
    /// `anchor_dir` is one of `"NW"`, `"NE"`, `"SW"`, `"SE"`.
    WinFloatPos {
        grid: u64,
        win: u64,
        anchor_dir: String,
        anchor_grid: u64,
        anchor_row: f64,
        anchor_col: f64,
        focusable: bool,
    },
    /// An external window (e.g., `nvim_open_win` with `external = true`) was
    /// positioned. The GUI should open a native window for this grid.
    WinExternalPos { grid: u64, win: u64 },
    /// Mouse events are now enabled.
    MouseOn,
    /// Mouse events are now disabled.
    MouseOff,
    /// End of a redraw batch. All events since the last `Flush` should be
    /// applied atomically before rendering.
    Flush,
}

/// A decoded UI event from Neovim.
///
/// UI events come in two forms:
/// - `Redraw(batch)` — A `"redraw"` notification containing one or more `RedrawEvent`s.
/// - `Other(method, args)` — Any other notification (may be ignored or logged).
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Redraw(Vec<RedrawEvent>),
    Other(String, Vec<Value>),
}

/// A fully resolved render attribute for a single grid cell.
///
/// This is the render-ready representation after `hl_id` → `RgbAttr` mapping
/// and default color application. Colors are packed as `[r, g, b, a]` with
/// floating-point values 0.0–1.0 for Metal shader consumption.
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
    /// Construct a default attribute with the given foreground and background.
    ///
    /// This typically represents `hl_id = 0` (the `Normal` highlight group).
    pub fn default_colors(fg: [f32; 4], bg: [f32; 4]) -> Self {
        CellAttr {
            foreground: Some(fg),
            background: Some(bg),
            ..Default::default()
        }
    }

    /// Pack a 24-bit RGB value (`0xRRGGBB`) into a `[f32; 4]` color array
    /// suitable for Metal fragment shader uniforms.
    pub fn pack_color(rgb: u32) -> [f32; 4] {
        [
            ((rgb >> 16) & 0xff) as f32 / 255.0,
            ((rgb >> 8) & 0xff) as f32 / 255.0,
            (rgb & 0xff) as f32 / 255.0,
            1.0,
        ]
    }
}
