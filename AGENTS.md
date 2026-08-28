# Repository Guidelines

## Project Structure & Module Organization

Argus is a Rust 2021 workspace. `crates/argus-client/` contains the `argus` terminal UI, `crates/argusd/` contains the daemon plus `argus-hook`, and `crates/argus-protocol/` owns everything the three binaries have to agree on.

One rule decides where code goes: **a module is named after the question it answers, and answers only that one.** Nothing is split across two files, and no file holds two subjects. If you cannot name a module's single responsibility in a sentence, it is two modules. `DESIGN.md` §"Where each responsibility lives" has the table; keep it current when you add or move a module.

Two boundaries are load-bearing. Cross-process contracts go in `argus-protocol` — the message enums, the tree, the pane API's URL grammar, environment and flags — because three binaries share no types unless they live there, and a contract written twice drifts in silence. And the client never predicts the result of a request: `app` changes when the daemon's tree says it changed, so a refused action leaves the panel showing what is true.

Tests live beside the code they cover, in a `#[cfg(test)] mod tests` block for a small module or a `tests/` directory split the same way as the module itself. `scripts/dev.ps1` supports isolated local development; `target/` is generated output. Consult `DESIGN.md` for current behavior and `TARGET.md`/`ROADMAP.md` for intended direction.

## Build, Test, and Development Commands

- `cargo check --workspace --all-targets --locked` performs a fast compile check using the committed lockfile.
- `cargo test --workspace --locked` runs the complete workspace test suite.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` applies the same strict lint policy as CI.
- `cargo build --workspace --release --locked` creates optimized release binaries.
- `.\scripts\dev.ps1` builds and launches an isolated Windows development instance; use `-Stop` to stop its daemon.

## Coding Style & Naming Conventions

Use rustfmt defaults (four-space indentation) and keep Clippy warning-free. Follow Rust conventions: `snake_case` for modules, functions, and tests; `UpperCamelCase` for types and traits; `SCREAMING_SNAKE_CASE` for constants. Prefer focused modules and explicit protocol types over loosely structured cross-process data. Preserve platform-specific behavior with `cfg(unix)` or `cfg(windows)` where appropriate.

Comment the *why*, never the *what*. A doc comment that restates the signature is noise; one that records a decision, a constraint, or a trap saves the next reader a rediscovery. Every module opens with a `//!` header naming its responsibility in a line or three. When a rule is spelled out in two places, one of them is about to go stale — move it to where it belongs and delete the copy.

## Testing Guidelines

Add regression tests with each behavior change, colocated with the affected module. Name tests after the observable rule, for example `restores_session_after_reconnect`. Exercise protocol changes in both producer and consumer crates. No numeric coverage threshold is configured. CI runs `cargo test` and `cargo clippy -D warnings`; it does not check formatting, and `cargo fmt --all` reflows files this repository has deliberately hand-formatted, so do not run it.

## Commit & Pull Request Guidelines

Recent commits use short, imperative, sentence-case subjects such as `Wake a pane's pump on the byte`. Keep commits narrowly scoped and explain non-obvious design choices in the body. Pull requests should summarize user-visible behavior, identify affected crates, link relevant issues, and list verification commands. Include terminal screenshots or recordings for TUI changes, and update `README.md` or design documents when commands, configuration, controls, or architecture change.
