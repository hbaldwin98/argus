# Argus

Argus is a terminal workspace for running shells and AI command-line agents across Git checkouts.
One persistent daemon owns the processes and PTYs; the TUI can detach and reconnect without stopping
them.

The workspace builds three executables:

- `argus`: the terminal client.
- `argusd`: the daemon that owns panes, Git state, and session restore.
- `argus-hook`: a helper used for Claude Code status events.

## Features

- Organizes projects into workspaces, checkouts, and shell or agent panes.
- Keeps panes running when the client closes.
- Starts and discovers Git worktrees and switches branches from the TUI.
- Runs Claude Code, Codex, OpenCode, or custom command-line agent templates.
- Shows Git status, changed-file counts, and ahead/behind state.
- Reviews uncommitted work, branch work, or changes since the last explicitly accepted snapshot.
- Captures staged, unstaged, deleted, renamed, and non-ignored untracked content for review.
- Opens files in a floating terminal editor, the terminal column, or an external editor.
- Restores non-exited shell and agent descriptions after a daemon restart.
- Supports multiple daemon-wide workspaces and multiple attached clients.

See [`DESIGN.md`](DESIGN.md) for exact current behavior, [`TARGET.md`](TARGET.md) for the intended
product contract, and [`ROADMAP.md`](ROADMAP.md) for unfinished work.

## Quick Start

### 1. Install prerequisites

Install:

- A current stable Rust toolchain through [rustup](https://rustup.rs/). Rustup is the official
  recommended installer and includes Cargo.
- Git on `PATH`.
- A native C/C++ build toolchain.
- A full-screen terminal with true-color support.

Common platform setup:

**Ubuntu or Debian**

```sh
sudo apt update
sudo apt install build-essential pkg-config libssl-dev git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**macOS**

```sh
xcode-select --install
brew install git pkg-config openssl@3
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows**

1. Install Git for Windows.
2. Install Visual Studio 2022 Build Tools with **Desktop development with C++**.
3. Install Rust from [rustup.rs](https://rustup.rs/) using the default MSVC toolchain.
4. Run the following commands from PowerShell or Windows Terminal.

If `openssl-sys` cannot find OpenSSL on Windows, install OpenSSL through `vcpkg` and set
`VCPKG_ROOT`, or set the `OPENSSL_DIR` environment variable to an existing OpenSSL installation.

This repository does not pin a minimum Rust version. Use the current stable toolchain:

```sh
rustup default stable
rustup update stable
```

### 2. Clone and build

```sh
git clone https://github.com/hbaldwin98/argus.git
cd argus
cargo build --workspace --release --locked
```

Building the whole workspace matters. `argus` looks for `argusd` beside itself, and `argusd` looks
for `argus-hook` beside itself.

### 3. Configure a project

Start Argus once to create the configuration directory, or create `projects.toml` yourself as shown
under [Configuration](#configuration). You can also press `n` in the projects column and enter a
directory.

### 4. Run

Linux and macOS:

```sh
./target/release/argus
```

Windows:

```powershell
.\target\release\argus.exe
```

The client starts `argusd` in the background when needed. Press `q` from the navigation columns to
detach; existing panes continue running.

## Install on `PATH`

Install both packages so all three executables share Cargo's binary directory:

```sh
cargo install --locked --path crates/argusd
cargo install --locked --path crates/argus-client
argus
```

Cargo normally installs them under `~/.cargo/bin`. Ensure that directory is on `PATH`.

For a custom location, use the same root for both packages:

```sh
cargo install --locked --root /opt/argus --path crates/argusd
cargo install --locked --root /opt/argus --path crates/argus-client
```

Then add `/opt/argus/bin` to `PATH`. Do not copy only `argus`: lazy daemon startup requires
`argusd`, and Claude status hooks require `argus-hook` beside `argusd`.

## Configuration

Set `ARGUS_CONFIG_DIR` to use an explicit directory. Otherwise Argus uses the platform application
configuration directory, typically:

- Linux: `$XDG_CONFIG_HOME/argus`, usually `~/.config/argus`.
- macOS: `~/Library/Application Support/argus`.
- Windows: `%APPDATA%\argus\config`.

The directory may contain:

| File | Purpose |
|---|---|
| `projects.toml` | Workspaces, projects, repositories, and agent templates |
| `client.toml` | Editor and theme preferences |
| `open-workspace` | Last daemon-wide workspace |
| `session.json` | Pane descriptions used for relaunch |

Configuration is loaded when the daemon starts; live reload is not implemented.

### `projects.toml`

```toml
[[workspace]]
name = "work"

[[project]]
name = "argus"
repos = ["~/src/argus"]
workspace = "work"

[[agent]]
name = "claude"
cmd = ["claude"]
env = { CLAUDE_PROJECT_DIR = "." }

[[agent]]
name = "opencode"
cmd = ["opencode"]
env = {}
```

Each repository path becomes a primary checkout. `~/` and `~\` are expanded. Projects without a
`workspace` use the always-present `default` workspace.

When no `[[agent]]` entries exist, Argus supplies these templates:

- `claude`, running `claude`.
- `codex`, running `codex`.
- `opencode`, running `opencode`.

Adding any `[[agent]]` entry replaces the built-in list, so include every template you want to keep.
Install and authenticate each agent CLI separately, and ensure it is available in the daemon's
inherited `PATH`. Custom commands are argument arrays; the first item is the executable.

### `client.toml`

```toml
editor = "overlay"
editor_cmd = ""
theme = "mocha"
```

- `editor`: `overlay`, `column`, or `external`.
- `editor_cmd`: an editor command, or empty to try `$VISUAL`, `$EDITOR`, then installed terminal
  editors.
- `theme`: `mocha`, `macchiato`, `frappe`, or `latte`.

The settings panel writes this file immediately. Editor commands currently split on whitespace, so
quoted arguments and executable paths containing spaces are not yet supported.

### Environment variables

| Variable | Effect |
|---|---|
| `ARGUS_CONFIG_DIR` | Overrides the configuration directory for client and daemon |
| `ARGUS_NO_RESTORE` | Starts the daemon without relaunching recorded panes when present |
| `ARGUS_THEME` | Overrides the configured theme for that client process |
| `RUST_LOG` | Controls daemon tracing when `argusd` is run in the foreground |

To diagnose daemon startup, run it in one terminal before opening the client:

```sh
RUST_LOG=argusd=debug cargo run -p argusd --bin argusd
```

```sh
cargo run -p argus --bin argus
```

## Controls

### Navigation

| Key | Action |
|---|---|
| `j` / `k`, arrows | Move within the selected column |
| `l`, Right, Enter | Open or descend |
| `h`, Left, Escape | Go back |
| `s` | Start a shell |
| `a` | Choose and start an agent |
| `n` | Add a project or create a worktree, depending on the column |
| `D` | Remove a linked worktree after confirmation |
| `w` | Switch workspace |
| `b` | Open the branch picker |
| `f` | Open the file picker |
| `R` / Tab | Open review |
| `S` | Open settings |
| `t` | Choose a theme for this client process |
| `x` | Kill the selected pane |
| `q` | Detach the client |

### Terminal panes

`Ctrl-Space` is the leader key:

| Chord | Action |
|---|---|
| `Ctrl-Space`, Escape | Leave terminal input or close a floating pane |
| `Ctrl-Space`, `x` | Kill the pane |
| `Ctrl-Space`, Tab | Open review |
| F12 | Emergency close for a floating window |

Other supported keys are forwarded to the child PTY.

### Review

| Key | Action |
|---|---|
| `j` / `k`, arrows | Move through changed lines |
| `d` / `u`, Page Down / Page Up | Move ten lines |
| `]` / `[` | Next or previous changed file |
| `g` / `G`, Home / End | First or last changed line |
| `v` / `V` | Start or clear a line-range selection |
| `f` | Open the changed-file picker |
| `c` | Send a comment to the first agent in the checkout |
| `e` | Open the selected line in the editor |
| `b` | Cycle uncommitted, branch-point, and last-looked bases |
| `r` / `R` | Refresh |
| `A` | Accept the exact displayed snapshot as last looked |
| `h`, Left, Escape, `q` | Close review without accepting it |

Review acceptance is explicit. Closing, refreshing, changing bases, commenting, or opening an editor
does not advance the last-looked baseline.

## Sessions and Git Data

Argus records non-exited shell and agent panes. After a daemon restart it launches fresh processes in
the recorded checkouts; it does not reattach old PIDs or resume agent conversations. Editors are not
restored. Set `ARGUS_NO_RESTORE=1` to start clean.

Review captures write blobs and trees to the repository's Git object database. Accepted last-looked
baselines are retained under worktree-specific `refs/argus/review/...` refs. Argus does not change
`HEAD`, branch refs, the real index, or working files while capturing a review.

Read-only Git operations use embedded libgit2. Branch switching and worktree creation/removal require
the `git` executable. Argus-created worktrees are stored under:

```text
<primary-checkout>/.argus/worktrees/<branch>
```

Add `/.argus/` to the repository's ignore rules if it is not already ignored.

## Claude Status Hooks

An agent template named exactly `claude` receives working, waiting, and idle updates. Argus manages
three entries in `<checkout>/.claude/settings.local.json` and removes them when the last Claude pane
closes or during the next daemon startup sweep.

Hook configuration is checkout-wide. Multiple Claude panes in one checkout currently route status to
the newest pane, so their individual statuses are not reliable. Other agent templates expose process
lifecycle state only.

## Development

Useful checks from the repository root:

```sh
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Some tests launch real PTYs and Git processes. `cargo test` can finish all assertions while a spawned
test process briefly delays command teardown.

The repository currently has pre-existing rustfmt drift, so `cargo fmt --all -- --check` may report
files unrelated to a change. Avoid applying a workspace-wide format pass in an otherwise focused
patch.
