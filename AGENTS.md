# AGENTS.md

## Build & Test Commands

- Build: `cargo build`
- Check/Lint: `cargo clippy --all-targets --all-features` (pedantic + restriction lints enabled)
- Format: `cargo +nightly fmt -- --config imports_granularity=Crate --config group_imports=StdExternalCrate`
- Test: `cargo test --all-features`
- Single test: `cargo test <test_name>`

## Code Style

- Rust 2024 edition, MSRV 1.88 (can be increased as needed)
- Strict Clippy: pedantic + many restriction lints (see `[lints.clippy]` in Cargo.toml)
- No `unwrap`/`expect`/`panic` in non-test code; use `anyhow` for errors
- Imports:
  - Group std imports first, then external crates, then local modules
  - Never use fully-qualified paths (e.g., `std::path::Path` or `crate::ui::foo()`) in code; always import namespaces via `use` statements and refer to symbols by their short name
  - Import deep `std` namespaces aggressively (e.g., `use std::path::PathBuf;`, `use std::collections::HashMap;`), except for namespaces like `io` or `fs` whose symbols have very common names that may collide — import those at the module level instead (e.g., `use std::fs;`)
  - For third-party crates, prefer importing at the crate or module level (e.g., `use anyhow::Context as _;`, `use clap::Parser;`) rather than deeply importing individual symbols, to keep the origin of symbols clear when reading code — only import deeper when needed to avoid very long fully-qualified namespaces
- Prefer `log` macros for logging; no `dbg!` or `todo!`
- Prefer `default-features = false` for dependencies
- In tests:
  - Use `use super::*;` to import from the parent module
  - Prefer `unwrap()` over `expect()` for conciseness
  - Do not add custom messages to `assert!`/`assert_eq!`/`assert_ne!` — the test name is sufficient
  - Prefer full type comparisons with `assert_eq!` over selectively checking nested attributes or unpacking; tag types with `#[cfg_attr(test, derive(Eq, PartialEq))]` if needed
- When moving or refactoring code, never remove comment lines — preserve all comments and move them along with the code they document

## Architecture

- Single-binary Gotify desktop notification daemon (`gotify-desktop`)
- `src/main.rs`: entry point, event loop connecting WebSocket messages to desktop notifications
- `src/config.rs`: TOML config parsing (server URL, token, notification settings, on-message commands)
- `src/gotify.rs`: Gotify REST/WebSocket client (uses `ureq` for HTTP, `tungstenite` for WS)
- `src/notif.rs`: Desktop notification display via `notify-rust` (D-Bus/zbus)
- Error handling: `anyhow` for propagation, `thiserror` for typed errors, `backon` for retries

## Version control

- This repository uses the jujutsu VCS. **Never use any `jj` command that modifies the repository**.
- You can also use read-only git commands for inspecting repository state. **Never use any git command that modifies the repository**.
