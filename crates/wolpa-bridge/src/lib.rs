//! ## wolpa-bridge — C FFI boundary
//!
//! This crate defines the `extern "C"` functions that the Swift AppKit layer calls
//! to interact with the Rust core. It is a **translation-only** layer: no business
//! logic lives here.
//!
//! ### Design
//!
//! - `crate-type = ["staticlib"]` — Produces `libwolpa_bridge.a` for linking into
//!   the Swift app.
//! - All public functions are `extern "C"` with `#[no_mangle]`.
//! - `cbindgen` generates `wolpa.h` from this crate's public API.
//! - Swift imports `wolpa.h` via a module map and calls these functions directly.

pub mod ffi;
