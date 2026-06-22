//! ## FFI function declarations
//!
//! C-callable entry points consumed by the Swift AppKit layer.
//! These functions are thin wrappers that delegate to `wolpa-core` and
//! `wolpa-render`. No business logic lives here.
//!
//! ### Naming convention
//!
//! All functions use the `wolpa_` prefix. Parameters use C-compatible types.
//! Callbacks from Rust to Swift use `extern "C"` function pointers.

// To be implemented in Phase 3.
