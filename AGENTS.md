# Repository Guidelines

## Project Structure & Module Organization

Argus is a Rust 2021 workspace of four crates. `crates/argus-client/` contains the `argus` terminal UI, `crates/argusd/` contains the daemon plus `argus-hook`, `crates/argus-desktop/` contains the `argus-desktop` window, and `crates/argus-protocol/` owns everything the binaries have to agree on.

One rule decides where code goes: **a module is named after the question it answers, and answers only that one.** Nothing is split across two files, and no file holds two subjects. If you cannot name a module's single responsibility in a sentence, it is two modules. A folder is named after its domain, by the same name in every crate that has a folder for it, and its parent is always `dir/mod.rs`; only a leaf whose tests are split out keeps `name.rs` beside `name/tests.rs`. That `mod.rs` is either an umbrella, the existing module the whole folder is about, with its `pub mod` lines straight after its `//!` header, or a pure group, a one-clause `//!` header naming the folder's question and then its sorted `pub mod` lines and nothing else. Adding, moving or deleting a module fails `cargo test -p argus-protocol --test design_table` until the table in `DESIGN.md` §"Where each responsibility lives" has its row, so change the row in the same edit.

Two boundaries are load-bearing. Cross-process contracts go in `argus-protocol` — the message enums, the tree, the pane API's URL grammar, environment and flags — because the binaries share no types unless they live there, and a contract written twice drifts in silence. And the client never predicts the result of a request: `app` changes when the daemon's tree says it changed, so a refused action leaves the panel showing what is true.

Tests live beside the code they cover, in a `#[cfg(test)] mod tests` block for a small module or a `tests/` directory split the same way as the module itself. Consult `DESIGN.md` for current behavior and `TARGET.md`/`ROADMAP.md` for intended direction.

## Build, Test, and Development Commands

- Package names for `-p` are `argus` (`crates/argus-client`), `argusd`, `argus-desktop` and `argus-protocol`.
- The desktop page is TypeScript in `crates/argus-desktop/page` (`npm ci` once there). After editing `page/src`, `npm run build` and commit `assets/js` with it; `npm run check` is CI's compile, oxlint and oxfmt check. After changing `src/page.rs`, `cargo test -p argus-desktop` rewrites `page/src/bindings`; commit those too.
- `argus-desktop` is not a default member: plain `cargo build`/`cargo test` skip it, `--workspace` builds it. On Linux it needs `libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config`. Under a headless X server, start it inside `dbus-run-session` or it stalls before opening its window.
- `rustup update stable` before a batch of work: CI lints with the newest stable, so an older local clippy passes what CI rejects.
- `cargo test -p <pkg> --locked 2>&1 | grep -E '^error|^test .* FAILED|panicked at|^test result'` runs one crate's tests and prints only failures and totals; `cargo test --workspace --locked` is the full suite.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` is CI's lint. A crate is clippy-clean before it is handed to another agent as a contract.
- `ARGUS_INSTANCE=dev` runs this checkout beside the installed Argus without touching its store; `README.md` §"Running a development build beside an installed one" has the build, run and stop commands. Its daemon outlives the client, so stop it when done.
- `scripts/reload.sh` puts this checkout on the installed daemon: it backs up the store and binaries first, and detaches the restart when run inside an Argus pane, where a plain `argus server restart` kills the shell that issued it.
- `scripts/ci-failure.sh [run-id]` prints only the failure lines of a CI run; the raw `gh run view --log-failed` output is ANSI-coloured and buries them.

## Coding Style & Naming Conventions

Files are hand-formatted: four-space indent, and match the line breaking of the file you are in; rustfmt is disabled in `rustfmt.toml`. Prefer explicit protocol types over loosely structured cross-process data. Preserve platform-specific behavior with `cfg(unix)` or `cfg(windows)` where appropriate.

Comment the *why*, never the *what*. A doc comment that restates the signature is noise; one that records a decision, a constraint, or a trap saves the next reader a rediscovery. Every module opens with a `//!` header naming its responsibility in a line or three. When a rule is spelled out in two places, one of them is about to go stale — move it to where it belongs and delete the copy.

## Testing Guidelines

Add regression tests with each behavior change, colocated with the affected module. Name tests after the observable rule, for example `restores_session_after_reconnect`. Exercise protocol changes in both producer and consumer crates.

CI builds and lints the tests on Windows; a Linux machine without mingw-w64 cannot. A test that runs a Unix program is `#[cfg(unix)]`, and so are the imports and helpers only it uses.

To look at a TUI screen, run its ignored dump test: find it with `grep -rn 'fn dump_' crates/argus-client/src/ui/tests` and run one by name, `cargo test -p argus dump_frame -- --ignored --nocapture`. Add one for a new surface. This text dump stands in for a screenshot.

## Commits, Releases & Review

Recent commits use short, imperative, sentence-case subjects such as `Wake a pane's pump on the byte`. Keep commits narrowly scoped and explain non-obvious design choices in the body. Maintainer work is committed on `main` and pushed; open a branch or pull request only when asked. An outside contributor's pull request summarizes the user-visible behavior, lists its verification commands, and includes a screen dump for a TUI change. Update `README.md` or the design documents when commands, configuration, controls, or architecture change.

A release follows `README.md` §"Publishing a Release". Its version commit is titled `Bump version to X for release` and carries `Cargo.lock` along with `Cargo.toml`.

Code written by a subagent is read by the parent, at least its schema and store diffs, before it is installed or committed.
