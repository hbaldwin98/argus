# Repository Guidelines

## Project Structure & Module Organization

Argus is a Rust 2021 workspace. `crates/argus-client/` contains the `argus` terminal UI, `crates/argusd/` contains the daemon plus `argus-hook`, and `crates/argus-protocol/` owns shared messages, framing, IDs, and transport types. Keep behavior near its owning crate and put cross-process contracts in `argus-protocol` — the pane API’s URL grammar lives there for exactly that reason. The daemon’s `state.rs` owns the tree; `state/panes.rs`, `state/sync.rs`, `state/git_ops.rs`, and `state/hook.rs` carry the `impl Daemon` blocks for panes, disk reconciliation, Git writes, and the loopback receiver. Unit tests generally live beside implementation code in `#[cfg(test)] mod tests` blocks. `scripts/dev.ps1` supports isolated local development; `target/` is generated output. Consult `DESIGN.md` for current behavior and `TARGET.md`/`ROADMAP.md` for intended direction.

## Build, Test, and Development Commands

- `cargo check --workspace --all-targets --locked` performs a fast compile check using the committed lockfile.
- `cargo test --workspace --locked` runs the complete workspace test suite.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` applies the same strict lint policy as CI.
- `cargo fmt --all -- --check` verifies standard Rust formatting; run `cargo fmt --all` to fix it.
- `cargo build --workspace --release --locked` creates optimized release binaries.
- `.\scripts\dev.ps1` builds and launches an isolated Windows development instance; use `-Stop` to stop its daemon.

## Coding Style & Naming Conventions

Use rustfmt defaults (four-space indentation) and keep Clippy warning-free. Follow Rust conventions: `snake_case` for modules, functions, and tests; `UpperCamelCase` for types and traits; `SCREAMING_SNAKE_CASE` for constants. Prefer focused modules and explicit protocol types over loosely structured cross-process data. Preserve platform-specific behavior with `cfg(unix)` or `cfg(windows)` where appropriate.

## Testing Guidelines

Add regression tests with each behavior change, colocated with the affected module. Name tests after the observable rule, for example `restores_session_after_reconnect`. Exercise protocol changes in both producer and consumer crates. No numeric coverage threshold is configured, but CI requires the full test, Clippy, formatting, and all-target check suite to pass.

## Commit & Pull Request Guidelines

Recent commits use short, imperative, sentence-case subjects such as `Wake a pane's pump on the byte`. Keep commits narrowly scoped and explain non-obvious design choices in the body. Pull requests should summarize user-visible behavior, identify affected crates, link relevant issues, and list verification commands. Include terminal screenshots or recordings for TUI changes, and update `README.md` or design documents when commands, configuration, controls, or architecture change.
