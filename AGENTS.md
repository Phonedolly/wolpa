# wolpa — macOS native Neovim GUI

## Project
Rust core (msgpack RPC, grid state, Metal renderer) +
Swift thin shell (AppKit NSView, IME, menu).

## Architecture
- `crates/wolpa-core/`   — RPC client, grid state machine, highlight resolver, event types
- `crates/wolpa-render/` — Core Text shaping, glyph atlas, Metal draw pipeline
- `crates/wolpa-bridge/` — C FFI boundary. Translation only. No business logic.
- `app/`                 — SwiftPM AppKit app. Minimal AppKit code.

## Commands
- Build:  `make build`
- Run:    `make run`
- Test:   `cargo test && swift test --package-path app`
- Lint:   `cargo clippy -- -D warnings && cargo fmt --check`

## Rules
1. Standard Rust conventions (clippy, rustfmt). No `unsafe` without justification.
2. `wolpa-bridge` is translation-only. Logic in `wolpa-core` or `wolpa-render`.
3. Swift code is minimal. Never reimplement Rust logic in Swift.
4. Always write tests for new protocol handling code. Prefer fixture tests.
5. Never commit build artifacts or generated headers. Run `make gen-header`.
6. Pre-commit: `cargo clippy -- -D warnings && cargo fmt --check && cargo test`
7. New crate → add to workspace `Cargo.toml` `[workspace].members`.
8. FFI surface changed → regenerate header + verify Swift compiles.
9. macOS-only code (render, bridge crates) gate with `#[cfg(target_os = "macos")]`.
