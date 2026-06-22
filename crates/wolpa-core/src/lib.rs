//! ## wolpa-core — Neovim UI protocol client
//!
//! This crate implements the Neovim UI protocol lifecycle:
//!
//! 1. **RPC transport** — Spawn `nvim --embed`, communicate via msgpack over stdin/stdout.
//! 2. **Event decoding** — Parse redraw notifications (`grid_line`, `hl_attr_define`, etc.)
//!    into Rust types.
//! 3. **Grid state machine** — Maintain a 2D cell grid that accumulates redraw events
//!    into a consistent screen state.
//! 4. **Highlight resolution** — Map `hl_id` values to render attributes (colors, bold,
//!    italic, underline, etc.) with support for deferred resolution.
//! 5. **Input translation** — Convert macOS key events to Neovim `nvim_input()` notation.
//!
//! ## Platform support
//!
//! This crate is **platform-independent**. It compiles and runs on any target.
//! macOS-specific rendering lives in `wolpa-render`.

pub mod event;
pub mod grid;
pub mod highlight;
pub mod input;
pub mod rpc;
