//! ## msgpack-RPC client for Neovim
//!
//! Communicates with an `nvim --embed` process via msgpack-encoded messages
//! over stdin/stdout. Handles message framing, request/response matching,
//! and dispatching of redraw notifications.
//!
//! ### Message format (Neovim msgpack-RPC)
//!
//! All messages are bare msgpack arrays (no length prefix):
//!
//! ```text
//! Request:      [0, msgid, "method", [args...]]
//! Response:     [1, msgid, error, result]
//! Notification: [2, "method", [args...]]
//! ```
//!
//! ### Redraw lifecycle
//!
//! 1. `RpcClient::spawn()` — Start `nvim --embed --clean`.
//! 2. `ui_attach(width, height)` — Send `nvim_ui_attach`. Neovim resizes grid 1
//!    and starts sending redraw batches bounded by `flush`.
//! 3. `command()` / `input()` — Send editing commands.
//! 4. The caller reads redraw events (via `parse_notification`) and applies them
//!    to `GridState` and `HighlightResolver`.
//! 5. `shutdown()` — Kill the nvim process.

use rmpv::Value;
use std::collections::HashMap;
use std::io;
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use super::event::{
    CtermAttr, GridLineCell, HighlightInfo, ModeInfo, MsgChunk, PopupMenuItem, RedrawEvent,
    RgbAttr, TablineInfo, UiEvent,
};

/// Errors that can occur during RPC communication.
#[derive(Error, Debug)]
pub enum RpcError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("msgpack decode error: {0}")]
    Decode(#[from] rmpv::decode::Error),
    #[error("msgpack encode error: {0}")]
    Encode(#[from] rmpv::encode::Error),
    #[error("serde encode error: {0}")]
    SerdeEncode(String),
    #[error("nvim error response: {0:?}")]
    NvimError(Value),
    #[error("unexpected message format")]
    UnexpectedMessage,
}

pub type Result<T> = std::result::Result<T, RpcError>;

/// A connected Neovim instance running in `--embed` mode.
///
/// Owns the child process and its stdin/stdout pipes. All communication
/// is async via `tokio`. The client is not `Clone` — a single client
/// corresponds to a single nvim instance.
pub struct RpcClient {
    child: Child,
    stdin: tokio::io::BufWriter<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
    msg_id: u64,
    /// Persistent buffer for incremental msgpack decoding across multiple
    /// read_message calls. Accumulates data until a complete value is decoded,
    /// then retains unconsumed bytes for the next call.
    read_buf: bytes::BytesMut,
    /// Notifications received during `call()` that have not been consumed
    /// by the caller yet. `drain_events()` returns these first.
    pending_events: Vec<UiEvent>,
}

impl RpcClient {
    /// Spawn `nvim --embed --clean` and set up async I/O pipes.
    ///
    /// `--clean` ensures a predictable initial state (no plugins, no config).
    /// `--headless` prevents nvim from trying to open a terminal UI.
    pub async fn spawn() -> Result<Self> {
        let mut child = Command::new("nvim")
            .args(["--embed", "--headless", "--clean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin not captured");
        let stdout = child.stdout.take().expect("stdout not captured");

        Ok(RpcClient {
            child,
            stdin: tokio::io::BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            msg_id: 0,
            read_buf: bytes::BytesMut::with_capacity(8192),
            pending_events: Vec::new(),
        })
    }

    fn next_msg_id(&mut self) -> u64 {
        let id = self.msg_id;
        self.msg_id += 1;
        id
    }

    async fn write_message(&mut self, msg: &Value) -> Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        rmpv::encode::write_value(&mut buf, msg)?;
        self.stdin.write_all(&buf).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Read a single msgpack value from nvim's stdout.
    ///
    /// Uses a persistent buffer (`self.read_buf`) to handle the case where
    /// multiple msgpack values arrive in one OS read. After decoding a value,
    /// the consumed bytes are removed from the buffer so the next call can
    /// decode the next value from the remaining data.
    async fn read_message(&mut self) -> Result<Value> {
        loop {
            // Try to decode from existing buffered data first
            {
                let mut cursor = std::io::Cursor::new(&self.read_buf[..]);
                let pos_before = cursor.position();
                match rmpv::decode::read_value(&mut cursor) {
                    Ok(val) => {
                        let consumed = cursor.position() as usize;
                        let _ = self.read_buf.split_to(consumed);
                        return Ok(val);
                    }
                    Err(rmpv::decode::Error::InvalidDataRead(_))
                    | Err(rmpv::decode::Error::InvalidMarkerRead(_)) => {
                        // Incomplete — restore position, read more data
                        cursor.set_position(pos_before);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            // Read more data from stdout
            let mut chunk = vec![0u8; 4096];
            let n = self.stdout.read(&mut chunk).await?;
            if n == 0 {
                return Err(RpcError::UnexpectedMessage);
            }
            self.read_buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Send a request and wait for the matching response.
    ///
    /// Notifications received while waiting are silently discarded.
    /// In the current implementation, the caller should separately read
    /// redraw events (see `parse_notification`) before or after calling this.
    pub async fn call(&mut self, method: &str, args: Vec<Value>) -> Result<Value> {
        let msg_id = self.next_msg_id();
        let msg = Value::Array(vec![
            Value::from(0u8),
            Value::from(msg_id),
            Value::String(method.into()),
            Value::Array(args),
        ]);
        self.write_message(&msg).await?;

        loop {
            let response = self.read_message().await?;
            let arr = response.as_array().ok_or(RpcError::UnexpectedMessage)?;
            let msg_type = arr
                .first()
                .and_then(|v| v.as_u64())
                .ok_or(RpcError::UnexpectedMessage)?;

            match msg_type {
                1 => {
                    let resp_id = arr
                        .get(1)
                        .and_then(|v| v.as_u64())
                        .ok_or(RpcError::UnexpectedMessage)?;
                    let error = arr.get(2).cloned().unwrap_or(Value::Nil);
                    let result = arr.get(3).cloned().unwrap_or(Value::Nil);
                    if resp_id != msg_id {
                        continue;
                    }
                    if !error.is_nil() {
                        return Err(RpcError::NvimError(error));
                    }
                    return Ok(result);
                }
                2 => {
                    // Save notification for later retrieval via drain_events
                    if let Some(event) = parse_ui_notification(arr) {
                        self.pending_events.push(event);
                    }
                }
                _ => continue,
            }
        }
    }

    /// Send `nvim_ui_attach` to connect as a UI client.
    ///
    /// Enables all recommended UI extensions: `ext_linegrid`, `ext_hlstate`,
    /// `ext_messages`, `ext_popupmenu`, `ext_tabline`, `ext_termcolors`.
    /// After this call succeeds, Neovim will send redraw events.
    pub async fn ui_attach(&mut self, width: u64, height: u64) -> Result<Value> {
        let mut opts: HashMap<String, Value> = HashMap::new();
        opts.insert("ext_linegrid".into(), Value::Boolean(true));
        opts.insert("ext_multigrid".into(), Value::Boolean(true));
        opts.insert("ext_hlstate".into(), Value::Boolean(true));
        opts.insert("ext_messages".into(), Value::Boolean(true));
        opts.insert("ext_popupmenu".into(), Value::Boolean(true));
        opts.insert("ext_tabline".into(), Value::Boolean(true));
        opts.insert("ext_termcolors".into(), Value::Boolean(true));

        let options =
            rmpv::ext::to_value(&opts).map_err(|e| RpcError::SerdeEncode(e.to_string()))?;

        self.call(
            "nvim_ui_attach",
            vec![Value::from(width), Value::from(height), options],
        )
        .await
    }

    /// Send an `nvim_input` call to inject keystrokes.
    ///
    /// The `keys` parameter should use Neovim's key notation (e.g., `"<C-w>l"`).
    /// See `:help key-notation`.
    pub async fn input(&mut self, keys: &str) -> Result<Value> {
        self.call("nvim_input", vec![Value::String(keys.into())])
            .await
    }

    /// Send an `nvim_command` call to execute an Ex command.
    pub async fn command(&mut self, cmd: &str) -> Result<Value> {
        self.call("nvim_command", vec![Value::String(cmd.into())])
            .await
    }

    /// Force a redraw flush by running `:redraw`.
    ///
    /// Useful in tests to ensure redraw events have been sent before checking
    /// grid state.
    pub async fn redraw_flush(&mut self) -> Result<Value> {
        self.command("redraw").await
    }

    /// Kill the nvim process and wait for it to terminate.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.stdin.flush().await;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        Ok(())
    }

    /// Read a single msgpack value from nvim's stdout and parse it as a `UiEvent`.
    ///
    /// Returns `Ok(None)` if the message is not a notification (e.g., a response).
    /// Returns `Err` on I/O or decode errors.
    pub async fn read_event(&mut self) -> Result<Option<UiEvent>> {
        let value = self.read_message().await?;
        Ok(parse_notification(&value))
    }

    /// Drain all pending notifications from nvim's stdout.
    ///
    /// Returns events previously collected during `call()` first, then reads
    /// from stdout until a short timeout with no data.
    pub async fn drain_events(&mut self) -> Result<Vec<UiEvent>> {
        let mut events: Vec<UiEvent> = std::mem::take(&mut self.pending_events);
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(50), self.read_event())
                .await
            {
                Ok(Ok(Some(event))) => events.push(event),
                Ok(Ok(None)) => continue,
                Ok(Err(e)) => return Err(e),
                Err(_elapsed) => break,
            }
        }
        Ok(events)
    }
}

// ── Event parsing helpers ──────────────────────────────────────────────

/// Parse the cells array from a `grid_line` event.
///
/// Each cell is a tuple `[text, hl_id?, repeat?]`. Handles run-length encoding.
fn parse_grid_line_cells(cells: &[Value]) -> Vec<GridLineCell> {
    let mut result = Vec::new();
    for cell in cells {
        let arr = match cell {
            Value::Array(a) => a,
            _ => continue,
        };
        let text = arr
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or(" ")
            .to_string();
        let hl_id = arr.get(1).and_then(|v| v.as_u64());
        let repeat = arr.get(2).and_then(|v| v.as_u64()).unwrap_or(1);
        result.push(GridLineCell {
            text,
            hl_id,
            repeat,
        });
    }
    result
}

fn map_get_u64(map: &[(Value, Value)], key: &str) -> Option<u64> {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .and_then(|(_, v)| v.as_u64())
}

#[allow(dead_code)]
fn map_get_i64(map: &[(Value, Value)], key: &str) -> Option<i64> {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .and_then(|(_, v)| v.as_i64())
}

#[allow(dead_code)]
fn map_get_f64(map: &[(Value, Value)], key: &str) -> Option<f64> {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .and_then(|(_, v)| v.as_f64())
}

fn map_get_str<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a str> {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .and_then(|(_, v)| v.as_str())
}

fn map_get_bool(map: &[(Value, Value)], key: &str) -> bool {
    map.iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v.as_bool().unwrap_or(false))
        .unwrap_or(false)
}

fn parse_rgb_attr(map: &[(Value, Value)]) -> RgbAttr {
    RgbAttr {
        foreground: map_get_u64(map, "foreground").map(|v| v as u32),
        background: map_get_u64(map, "background").map(|v| v as u32),
        special: map_get_u64(map, "special").map(|v| v as u32),
        reverse: map_get_bool(map, "reverse"),
        italic: map_get_bool(map, "italic"),
        bold: map_get_bool(map, "bold"),
        strikethrough: map_get_bool(map, "strikethrough"),
        underline: map_get_bool(map, "underline"),
        undercurl: map_get_bool(map, "undercurl"),
        underdouble: map_get_bool(map, "underdouble"),
        underdotted: map_get_bool(map, "underdotted"),
        underdashed: map_get_bool(map, "underdashed"),
        blend: map_get_u64(map, "blend").map(|v| v as u8).unwrap_or(0),
    }
}

fn parse_cterm_attr(map: &[(Value, Value)]) -> CtermAttr {
    CtermAttr {
        foreground: map_get_u64(map, "foreground").map(|v| v as u16),
        background: map_get_u64(map, "background").map(|v| v as u16),
    }
}

fn parse_highlight_info(arr: &[Value]) -> Vec<HighlightInfo> {
    arr.iter()
        .filter_map(|v| v.as_map())
        .map(|map| HighlightInfo {
            kind: map_get_str(map, "kind").unwrap_or("").to_string(),
            id: map_get_u64(map, "id"),
            hi_name: map_get_str(map, "hi_name").map(|s| s.to_string()),
        })
        .collect()
}

fn parse_mode_info(arr: &[Value]) -> Vec<ModeInfo> {
    arr.iter()
        .filter_map(|v| v.as_map())
        .map(|map| ModeInfo {
            name: map_get_str(map, "name").unwrap_or("").to_string(),
            short_name: map_get_str(map, "short_name").unwrap_or("").to_string(),
            cursor_shape: map_get_str(map, "cursor_shape").map(|s| s.to_string()),
            cell_percentage: map_get_u64(map, "cell_percentage"),
            blink_wait: map_get_u64(map, "blinkwait"),
            blink_on: map_get_u64(map, "blinkon"),
            blink_off: map_get_u64(map, "blinkoff"),
            hl_id: map_get_u64(map, "hl_id"),
            hl_lens: map_get_u64(map, "hl_lens"),
            attr_id: map_get_u64(map, "attr_id"),
            attr_id_lm: map_get_u64(map, "attr_id_lm"),
        })
        .collect()
}

fn parse_msg_chunks(arr: &[Value]) -> Vec<MsgChunk> {
    arr.iter()
        .filter_map(|v| v.as_array())
        .filter_map(|chunk| {
            let kind = chunk.first().and_then(|v| v.as_u64())?;
            let content = chunk.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let hl_id = chunk.get(2).and_then(|v| v.as_u64());
            if kind == 0 || kind == 1 {
                Some(MsgChunk {
                    text: content.to_string(),
                    hl_id,
                })
            } else {
                None
            }
        })
        .collect()
}

fn parse_popupmenu_items(arr: &[Value]) -> Vec<PopupMenuItem> {
    arr.iter()
        .filter_map(|v| v.as_array())
        .filter_map(|item| {
            Some(PopupMenuItem {
                word: item.first().and_then(|v| v.as_str())?.to_string(),
                kind: item.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
                menu: item.get(2).and_then(|v| v.as_str()).map(|s| s.to_string()),
                info: item.get(3).and_then(|v| v.as_str()).map(|s| s.to_string()),
            })
        })
        .collect()
}

fn parse_tabline_item(v: &Value) -> TablineInfo {
    if let Some(map) = v.as_map() {
        TablineInfo {
            name: map_get_str(map, "name").unwrap_or("").to_string(),
            tabpage_id: map_get_u64(map, "tab").unwrap_or(0),
        }
    } else {
        TablineInfo {
            name: String::new(),
            tabpage_id: 0,
        }
    }
}

/// Parse a single redraw event from a `[event_name, [arg1, arg2, ...]]` array.
///
/// The event format (nvim 0.10+) packs all arguments into a single sub-array.
/// For backward compatibility with older nvim, if the second element is _not_
/// an array, the remaining elements are treated as individual args.
pub fn parse_redraw_event(arr: &[Value]) -> Option<RedrawEvent> {
    let name = arr.first()?.as_str()?;

    // Event format: [event_name, [arg1, arg2, ...]] (nvim 0.10+)
    // For backward compat, also handle [event_name, arg1, arg2, ...]
    let args: Vec<&Value> = if let Some(args_arr) = arr.get(1).and_then(|v| v.as_array()) {
        args_arr.iter().collect()
    } else {
        arr.iter().skip(1).collect()
    };

    match name {
        "grid_resize" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let width = args.get(1).and_then(|v| v.as_u64())?;
            let height = args.get(2).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::GridResize {
                grid,
                width,
                height,
            })
        }
        "grid_line" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let row = args.get(1).and_then(|v| v.as_u64())?;
            let col_start = args.get(2).and_then(|v| v.as_u64())?;
            let cells = args
                .get(3)
                .and_then(|v| v.as_array())
                .map(|a| parse_grid_line_cells(a))
                .unwrap_or_default();
            Some(RedrawEvent::GridLine {
                grid,
                row,
                col_start,
                cells,
            })
        }
        "grid_clear" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            Some(RedrawEvent::GridClear { grid })
        }
        "grid_cursor_goto" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let row = args.get(1).and_then(|v| v.as_u64())?;
            let col = args.get(2).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::GridCursorGoto { grid, row, col })
        }
        "grid_scroll" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let top = args.get(1).and_then(|v| v.as_u64())?;
            let bot = args.get(2).and_then(|v| v.as_u64())?;
            let left = args.get(3).and_then(|v| v.as_u64())?;
            let right = args.get(4).and_then(|v| v.as_u64())?;
            let rows = args.get(5).and_then(|v| v.as_i64())?;
            let cols = args.get(6).and_then(|v| v.as_i64()).unwrap_or(0);
            Some(RedrawEvent::GridScroll {
                grid,
                top,
                bot,
                left,
                right,
                rows,
                cols,
            })
        }
        "grid_destroy" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            Some(RedrawEvent::GridDestroy { grid })
        }
        "hl_attr_define" => {
            let id = args.first().and_then(|v| v.as_u64())?;
            let rgb_attr = args
                .get(1)
                .and_then(|v| v.as_map())
                .map(|m| parse_rgb_attr(m))
                .unwrap_or_default();
            let cterm_attr = args
                .get(2)
                .and_then(|v| v.as_map())
                .map(|m| parse_cterm_attr(m))
                .unwrap_or_default();
            let info = args
                .get(3)
                .and_then(|v| v.as_array())
                .map(|a| parse_highlight_info(a))
                .unwrap_or_default();
            Some(RedrawEvent::HlAttrDefine {
                id,
                rgb_attr,
                cterm_attr,
                info,
            })
        }
        "default_colors_set" => {
            let rgb_fg = args.first().and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let rgb_bg = args.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let rgb_sp = args.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let cterm_fg = args.get(3).and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let cterm_bg = args.get(4).and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            Some(RedrawEvent::DefaultColorsSet {
                rgb_fg,
                rgb_bg,
                rgb_sp,
                cterm_fg,
                cterm_bg,
            })
        }
        "mode_info_set" => {
            let cursor_style_enabled = args
                .first()
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let mode_info = args
                .get(1)
                .and_then(|v| v.as_array())
                .map(|a| parse_mode_info(a))
                .unwrap_or_default();
            Some(RedrawEvent::ModeInfoSet {
                cursor_style_enabled,
                mode_info,
            })
        }
        "mode_change" => {
            let name = args.first().and_then(|v| v.as_str())?.to_string();
            let index = args.get(1).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::ModeChange { name, index })
        }
        "option_set" => {
            let name = args.first().and_then(|v| v.as_str())?.to_string();
            let value = args.get(1).copied().cloned().unwrap_or(Value::Nil);
            Some(RedrawEvent::OptionSet { name, value })
        }
        "msg_show" => {
            let kind = args.first().and_then(|v| v.as_str())?.to_string();
            let content = args
                .get(1)
                .and_then(|v| v.as_array())
                .map(|a| parse_msg_chunks(a))
                .unwrap_or_default();
            let replace_last = args
                .get(2)
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            Some(RedrawEvent::MsgShow {
                kind,
                content,
                replace_last,
            })
        }
        "msg_clear" => Some(RedrawEvent::MsgClear),
        "busy_start" => Some(RedrawEvent::BusyStart),
        "busy_stop" => Some(RedrawEvent::BusyStop),
        "popupmenu_show" => {
            let items = args
                .first()
                .and_then(|v| v.as_array())
                .map(|a| parse_popupmenu_items(a))
                .unwrap_or_default();
            let selected = args.get(1).and_then(|v| v.as_i64())?;
            let row = args.get(2).and_then(|v| v.as_u64())?;
            let col = args.get(3).and_then(|v| v.as_u64())?;
            let grid = args.get(4).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::PopupMenuShow {
                items,
                selected,
                row,
                col,
                grid,
            })
        }
        "popupmenu_select" => {
            let selected = args.first().and_then(|v| v.as_i64())?;
            Some(RedrawEvent::PopupMenuSelect { selected })
        }
        "popupmenu_hide" => Some(RedrawEvent::PopupMenuHide),
        "tabline_update" => {
            let curtab = parse_tabline_item(args.first()?);
            let tabs = args
                .get(1)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().map(parse_tabline_item).collect())
                .unwrap_or_default();
            let show = args
                .get(2)
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            Some(RedrawEvent::TablineUpdate { curtab, tabs, show })
        }
        "win_pos" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let win = args.get(1).and_then(|v| v.as_u64())?;
            let start_row = args.get(2).and_then(|v| v.as_u64())?;
            let start_col = args.get(3).and_then(|v| v.as_u64())?;
            let width = args.get(4).and_then(|v| v.as_u64())?;
            let height = args.get(5).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::WinPos {
                grid,
                win,
                start_row,
                start_col,
                width,
                height,
            })
        }
        "win_close" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            Some(RedrawEvent::WinClose { grid })
        }
        "win_hide" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            Some(RedrawEvent::WinHide { grid })
        }
        "win_float_pos" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let win = args.get(1).and_then(|v| v.as_u64())?;
            let anchor_dir = args.get(2).and_then(|v| v.as_str())?.to_string();
            let anchor_grid = args.get(3).and_then(|v| v.as_u64())?;
            let anchor_row = args.get(4).and_then(|v| v.as_f64())?;
            let anchor_col = args.get(5).and_then(|v| v.as_f64())?;
            let focusable = args
                .get(6)
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            Some(RedrawEvent::WinFloatPos {
                grid,
                win,
                anchor_dir,
                anchor_grid,
                anchor_row,
                anchor_col,
                focusable,
            })
        }
        "win_external_pos" => {
            let grid = args.first().and_then(|v| v.as_u64())?;
            let win = args.get(1).and_then(|v| v.as_u64())?;
            Some(RedrawEvent::WinExternalPos { grid, win })
        }
        "mouse_on" => Some(RedrawEvent::MouseOn),
        "mouse_off" => Some(RedrawEvent::MouseOff),
        "flush" => Some(RedrawEvent::Flush),
        _ => None,
    }
}

/// Parse a UI notification: `[2, "method", ...args]`.
///
/// For `"redraw"` notifications, the single argument is an array of event arrays:
/// `[2, "redraw", [[event1], [event2], ...]]`. Each inner array is parsed by
/// `parse_redraw_event`.
pub fn parse_ui_notification(msg: &[Value]) -> Option<UiEvent> {
    let method = msg.get(1)?.as_str()?;
    let args: Vec<&Value> = msg.iter().skip(2).collect();

    match method {
        "redraw" => {
            let events: Vec<RedrawEvent> = args
                .iter()
                .filter_map(|v| v.as_array())
                .flat_map(|ev_arr| ev_arr.iter())
                .filter_map(|ev| ev.as_array())
                .filter_map(|arr| parse_redraw_event(arr))
                .collect();
            Some(UiEvent::Redraw(events))
        }
        _ => Some(UiEvent::Other(
            method.to_string(),
            args.into_iter().cloned().collect(),
        )),
    }
}

/// Parse a msgpack value as a notification array and convert it to a `UiEvent`.
///
/// This is the main entry point for consuming raw msgpack values from nvim's stdout
/// and turning them into typed UI events.
pub fn parse_notification(value: &Value) -> Option<UiEvent> {
    value.as_array().and_then(|arr| parse_ui_notification(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_grid_resize() {
        // New format: [event_name, [args...]]
        let event = Value::Array(vec![
            Value::String("grid_resize".into()),
            Value::Array(vec![
                Value::Integer(1.into()),
                Value::Integer(80.into()),
                Value::Integer(24.into()),
            ]),
        ]);
        let parsed = parse_redraw_event(event.as_array().unwrap());
        assert!(matches!(
            parsed,
            Some(RedrawEvent::GridResize {
                grid: 1,
                width: 80,
                height: 24
            })
        ));
    }

    #[test]
    fn test_parse_grid_resize_old_format() {
        // Old format (backward compat): [event_name, arg1, arg2, ...]
        let event = Value::Array(vec![
            Value::String("grid_resize".into()),
            Value::Integer(1.into()),
            Value::Integer(80.into()),
            Value::Integer(24.into()),
        ]);
        let parsed = parse_redraw_event(event.as_array().unwrap());
        assert!(matches!(
            parsed,
            Some(RedrawEvent::GridResize {
                grid: 1,
                width: 80,
                height: 24
            })
        ));
    }

    #[test]
    fn test_parse_grid_line() {
        let event = Value::Array(vec![
            Value::String("grid_line".into()),
            Value::Array(vec![
                Value::Integer(1.into()),
                Value::Integer(0.into()),
                Value::Integer(0.into()),
                Value::Array(vec![
                    Value::Array(vec![Value::String("H".into()), Value::Integer(0.into())]),
                    Value::Array(vec![Value::String("i".into())]),
                ]),
            ]),
        ]);
        let parsed = parse_redraw_event(event.as_array().unwrap());
        match parsed {
            Some(RedrawEvent::GridLine {
                grid,
                row,
                col_start,
                cells,
            }) => {
                assert_eq!(grid, 1);
                assert_eq!(row, 0);
                assert_eq!(col_start, 0);
                assert_eq!(cells.len(), 2);
                assert_eq!(cells[0].text, "H");
                assert_eq!(cells[0].hl_id, Some(0));
                assert_eq!(cells[1].text, "i");
                assert_eq!(cells[1].hl_id, None);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn test_parse_grid_resize_correct_args() {
        // Test the new format parsing logic directly
        let event = Value::Array(vec![
            Value::String("grid_resize".into()),
            Value::Array(vec![
                Value::Integer(1.into()),
                Value::Integer(80.into()),
                Value::Integer(24.into()),
            ]),
        ]);
        let arr = event.as_array().unwrap();
        // verify the sub-array extraction
        let args_arr = arr.get(1).and_then(|v| v.as_array()).unwrap();
        assert_eq!(args_arr.len(), 3);
        assert_eq!(args_arr[0].as_u64(), Some(1));
        assert_eq!(args_arr[1].as_u64(), Some(80));
        assert_eq!(args_arr[2].as_u64(), Some(24));
    }

    #[test]
    fn test_parse_redraw_notification() {
        let notification = Value::Array(vec![
            Value::Integer(2.into()),
            Value::String("redraw".into()),
            Value::Array(vec![
                Value::Array(vec![
                    Value::String("grid_resize".into()),
                    Value::Array(vec![
                        Value::Integer(1.into()),
                        Value::Integer(80.into()),
                        Value::Integer(24.into()),
                    ]),
                ]),
                Value::Array(vec![Value::String("flush".into()), Value::Array(vec![])]),
            ]),
        ]);

        let parsed = parse_notification(&notification);
        match parsed {
            Some(UiEvent::Redraw(events)) => {
                assert_eq!(events.len(), 2);
                assert!(matches!(events[0], RedrawEvent::GridResize { .. }));
                assert!(matches!(events[1], RedrawEvent::Flush));
            }
            other => panic!("unexpected: {:?}", other),
        }
    }
}
