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
10. Every file starts with a `//!` module-level doc comment. Every module and
    important function has a multiline `///` doc comment explaining its purpose,
    contracts, and invariants.
11. Commits use [Conventional Commits](https://www.conventionalcommits.org/) with scope:

    ```
    <type>(<scope>): <description>

    <body>
    ```

    | Field | Convention |
    |---|---|
    | type | `feat` `fix` `docs` `refactor` `test` `chore` `ci` `build` `perf` |
    | scope | `core` (wolpa-core), `render` (wolpa-render), `bridge`, `cli`, `app`, `ci`, `repo` |
    | description | lowercase, imperative mood, no period at end |
    | body | blank line after description, wrapped at 72 chars, explains **what** and **why** |
