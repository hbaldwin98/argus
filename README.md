# Argus

[![CI](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml/badge.svg)](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml)

Argus is a terminal workspace for running shells and AI command-line agents across Git checkouts.
One persistent daemon owns the processes and PTYs; the TUI can detach and reconnect without stopping
them.

The workspace builds three executables:

- `argus`: the terminal client.
- `argusd`: the daemon that owns panes, Git state, and session restore.
- `argus-hook`: a helper that lets agent harnesses report status, notes, and pane titles.

## Features

- Organizes projects into workspaces, repositories, checkouts, and shell or agent panes.
- Keeps panes running when the client closes.
- Starts and discovers Git worktrees and switches branches from the TUI.
- Runs Claude Code, Codex, OpenCode, Google Antigravity (AGY), Cursor Agent (`agent`), or custom command-line agent templates.
- Names each agent pane from the user's latest prompt, so a column of running agents is not a list of identical template names.
- Shows Git status, changed-file counts, and ahead/behind state.
- Reviews staged and unstaged work as two separate diffs, the way Git itself keeps them apart.
- Captures deleted, renamed, and non-ignored untracked content for review.
- Reads a diff unified or split side by side, with comments meaning the same thing in either.
- Opens files in a floating terminal editor, the terminal column, or an external editor.
- Restores non-exited shell and agent panes after a daemon restart, reopening each agent's last
  conversation where its CLI can be asked to.
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
- Perl, used to compile the vendored OpenSSL dependency.
- A full-screen terminal with true-color support.

Common platform setup:

**Ubuntu or Debian**

```sh
sudo apt update
sudo apt install build-essential pkg-config git curl perl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**macOS**

```sh
xcode-select --install
brew install git pkg-config perl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows**

1. Install Git for Windows.
2. Install Visual Studio 2022 Build Tools with **Desktop development with C++**.
3. Install Strawberry Perl and ensure `perl` is on `PATH`.
4. Install Rust from [rustup.rs](https://rustup.rs/) using the default MSVC toolchain.
5. Run the following commands from PowerShell or Windows Terminal.

OpenSSL is built from vendored source, so a separate OpenSSL installation is not required.

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
directory: every Git repository at or beneath it becomes one of the project's repositories, so
naming a repository adds that one and naming the directory a dozen of them live in adds the dozen.

If the repository does not exist yet, press `i` in the repositories column instead: browse to where
it should live, give it a name, and Argus creates the directory, runs `git init` in it, and adds it
to the project.

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

### Release archive

Download the archive for your platform from [GitHub Releases](https://github.com/hbaldwin98/argus/releases):

- Linux: `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` `.tar.gz`.
- macOS: `x86_64-apple-darwin` or `aarch64-apple-darwin` `.tar.gz`.
- Windows: `x86_64-pc-windows-msvc` `.zip`.

Each archive contains `argus`, `argusd`, and `argus-hook` together. Extract the archive and add its
directory to `PATH`; do not move only one executable. A matching `.sha256` file is published for
each archive.

Linux and macOS binaries are not code-signed. macOS may require approval in Privacy & Security after
the first launch. Windows binaries are not Authenticode-signed and may show a SmartScreen warning.

### Build from source

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
`argusd`, and agent harness integration requires `argus-hook` beside `argusd`.

## Configuration

Set `ARGUS_CONFIG_DIR` to use an explicit directory. Otherwise Argus uses the platform application
configuration directory, typically:

- Linux: `$XDG_CONFIG_HOME/argus`, usually `~/.config/argus`.
- macOS: `~/Library/Application Support/argus`.
- Windows: `%APPDATA%\argus\config`.

The directory may contain:

| File | Purpose |
|---|---|
| `projects.toml` | Workspaces, projects, repositories, agent templates, and harnesses. Yours to edit; Argus only reads it |
| `client.toml` | Editor, theme, layout, and notification preferences |
| `runtime.db` | Everything Argus writes: panes to relaunch, projects and repositories added or removed from the TUI, workspaces created at runtime, and the last workspace open |

The daemon watches `projects.toml` and reloads project and agent-template changes. Existing panes
keep the command and harness they started with.

Anything you do to the panel — adding a directory as a project, adding a repository by path,
removing either with `D`, creating a workspace — is recorded in `runtime.db` rather than written
back into your config. A project `projects.toml` declares is *hidden* when you remove it, not
deleted from the file, so nothing you wrote by hand is ever rewritten. Adding the same directory
back is the undo.

Upgrading from a version before `runtime.db`, the old `session.json`, `excluded-repos`, and
`open-workspace` files are imported on first start and renamed to `*.imported`.

### `projects.toml`

```toml
[[workspace]]
name = "work"

[[project]]
name = "src"
root = "~/src"

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

A project finds its repositories under `root`, or names them one at a time in `repos`, or does
both. Each becomes a repository with a primary checkout at that path.

`root` is scanned for the Git repositories at or beneath it, at startup and every ten seconds
after, so a repository you clone into it turns up without restarting anything, and one you delete
leaves once nothing is running in it. The scan stops at each repository it finds, so a submodule
stays part of the repository containing it; it skips `.git`, `.argus`, `node_modules`, and
`target`; it does not follow directory symlinks; and it looks no more than eight directories deep.
Linked worktrees are not repositories of their own — they are checkouts of the repository they
belong to, and appear in its checkout column.

`repos` is taken at its word: a path there need not be a Git repository at all, which is how a
plain directory gets a row and panes, and no scan will ever remove it. A repository reached both
ways appears once.

`~/` and `~\` are expanded. Projects without a `workspace` use the always-present `default`
workspace.

When no `[[agent]]` entries exist, Argus supplies these templates:

- `claude`, running `claude`.
- `codex`, running `codex`.
- `opencode`, running `opencode`.
- `agy`, running `agy`.
- `agent`, running Cursor's `agent` CLI.

Adding any `[[agent]]` entry replaces the built-in list, so include every template you want to keep.
Install and authenticate each agent CLI separately, and ensure it is available in the daemon's
inherited `PATH`. Custom commands are argument arrays; the first item is the executable. An agent's
optional `harness` selects a matching built-in or configured harness; without it, Argus tries the
agent name and then falls back to the generic environment-only harness.

Claude Code reports through hooks Argus writes into `.claude/settings.local.json`, Codex through a
SessionStart adapter in `.codex/hooks.json`, OpenCode through a plugin module Argus writes to
`.opencode/plugin/argus-status.js`, AGY through `.agents/hooks.json` under the `argus` hook key, and
Cursor's `agent` CLI through `.cursor/hooks.json` plus an always-on rule at `.cursor/rules/argus.mdc`.
All are removed when
the last agent pane in the checkout closes and swept from every configured checkout at startup;
adding them to a repository's `.gitignore` keeps them out of its status while an agent is running.
Codex treats project hooks as untrusted until the user approves them. Argus writes the correct hook,
but cannot approve that trust decision; exact Codex identity capture starts after approval.

Custom JSON-hook harnesses use this schema:

```toml
[[harness]]
name = "herdr"
settings = ".herdr/hooks.json"
hooks_key = "hooks"
shape = "flat" # use "matcher" for Claude Code-style nesting
context_event = "session_start"
resume = ["--continue"] # appended to the agent command when a pane is restored
resume_id = ["--resume", "{session_id}"] # exact resume when identity was captured

[harness.events]
turn_start = "working"
turn_end = "idle"
ask = { reports = "waiting", note = true }
prompt = { reports = "working", title = true }
start = { reports = "idle", session_id = "session_id" }

[[agent]]
name = "herdr"
cmd = ["herdr"]
harness = "herdr"
```

`settings` is relative to the checkout. Event values are `working`, `idle`, `waiting`,
`needs-review`, `done`, or `failed`.
Set `note = true` when the harness sends a useful JSON or text explanation to the hook on stdin.
Set `title = true` when that stdin carries the user's prompt, so the daemon can name the pane
without waiting for the model to run `argus-hook title`.
Set `session_id` to the top-level stdin JSON key containing its stable conversation identity.
`resume_id` is an argv template; every `{session_id}` placeholder is replaced without invoking a
shell. `resume` remains the broad fallback for session records created before identity capture.
Omit `settings` for an environment-only harness. Omit `resume` for a CLI that cannot be asked to
continue its last conversation; its panes still come back, just empty. A block that reuses a
built-in name replaces it outright, including a plugin module and resume arguments the built-in
shipped.

### `client.toml`

```toml
editor = "overlay"
editor_cmd = ""
theme = "mocha"
notifications = "off"
```

- `editor`: `overlay`, `column`, or `external`.
- `editor_cmd`: an editor command, or empty to try `$VISUAL`, `$EDITOR`, then installed terminal
  editors.
- `theme`: `mocha`, `macchiato`, `frappe`, or `latte`.
- `notifications`: `off` or `bell`; `bell` rings for a background transition into waiting,
  needs-review, or failed.

The settings panel writes this file immediately. Editor commands currently split on whitespace, so
quoted arguments and executable paths containing spaces are not yet supported.

### Environment variables

| Variable | Effect |
|---|---|
| `ARGUS_CONFIG_DIR` | Overrides the configuration directory for client and daemon |
| `ARGUS_INSTANCE` | Names this process's instance: scopes the pipe/socket and carves the config directory into `instances/<name>`, so a second Argus can run beside the first |
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

### Running a development build beside an installed one

From any checkout or `.argus/worktrees/*` worktree:

```powershell
.\scripts\dev.ps1          # builds this checkout and opens its Argus
.\scripts\dev.ps1 -Stop    # ends this checkout's daemon
```

The script sets `ARGUS_INSTANCE=dev`, so the worktree daemon listens on its own
endpoint under its own slice of the config directory and the installed Argus
keeps running, untouched. The daemon outlives the client, so the next launch
connects instantly and a rebuild only pays for what changed.

## Controls

### Navigation

| Key | Action |
|---|---|
| `j` / `k`, arrows | Move within the selected column |
| `l`, Right, Enter | Open or descend; on a branch row, switch the primary checkout to it |
| `h`, Left, Escape | Go back |
| `s` | Start a shell |
| `a` | Choose and start an agent |
| `n` | Add a project, add a repository to one, create a worktree, or give a branch row a worktree, depending on the column |
| `i` | In the repositories column, make a repository that does not exist yet: browse to where it should go, name it, and Argus creates the directory, runs `git init` in it, and adds it to the project. An empty name uses the chosen directory itself, which is how a folder that is already there gets initialized |
| `D` | Remove what the column selects, after confirmation: a project or repository (out of the panel only — nothing on disk is touched), a linked worktree (deleted), or a branch row (the local branch, deleted; the remote is untouched) |
| `w` | Switch workspace |
| `b` | Open the branch picker |
| `B` | Show or hide the branches no checkout is sitting on, including the ones only a remote has; the main branch keeps its row either way |
| `F` | Fetch every remote (`--prune`), which is what makes the remote's branches appear as rows |
| `P` | Pull the selected checkout, fast-forward only |
| `f` | Open the file picker |
| `R` / Tab | Open review |
| `N` | Jump to the next pane, or parent of a child, waiting, failed, or ready for review |
| `S` | Open settings |
| `t` | Choose a theme for this client process |
| `x` | Kill the selected pane |
| `q` | Detach the client |

### Terminal panes

`Ctrl-Space` is the leader key:

| Chord | Action |
|---|---|
| `Ctrl-Space`, Escape | Leave terminal input or close a floating pane |
| `Ctrl-Space`, `f` | Toggle fullscreen for the selected pane |
| `Ctrl-Space`, `x` | Kill the pane |
| `Ctrl-Space`, Tab | Open review |
| `Ctrl-Space`, `N` | Jump to the next pane waiting, failed, or ready for review |
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
| `c` | Send a comment to an agent in the checkout; choose one when several are running |
| `e` | Open the selected line in the editor |
| `b` | Toggle between staged and unstaged changes |
| `s` | Toggle between the unified and split diff |
| `r` / `R` | Refresh |
| `h`, Left, Escape, `q` | Close review |

The split view puts each removal beside what replaced it, numbered from the old file on the left
and the new one on the right; where one side has no counterpart it is left empty. It wants width,
so it pairs well with a fullscreen pane. A comment means the same thing in either view, because a
comment already records both line numbers.

The chosen side and the chosen view both persist across reopens. Review covers uncommitted work only; comparing against
committed history is left to a dedicated Git tool.

Diffs are syntax highlighted for Rust, TypeScript, TSX and JavaScript, Python, C#, CSS, YAML, TOML,
JSON, and Markdown, picked by file extension; anything else is shown as plain text. Added and
removed lines are marked by a background wash and by their `+` and `-` markers, leaving the
foreground to the syntax colours.

Comments are stored in `runtime.db` before Argus notifies the selected live agent. They remain
visible to every live agent in that checkout through `argus-hook comments`; the command returns the
newest 100 comments in order. If terminal delivery fails after the save, the comment remains stored.

## Sessions and Git Data

Argus records non-exited shell and agent panes, in worktrees as well as primary checkouts. After a
daemon restart it launches fresh processes in the recorded checkouts; it does not reattach old PIDs.
When a pane has a captured harness session ID, Argus appends the harness's exact `resume_id`
arguments: `--resume <id>` for Claude Code, `resume <id>` for Codex, `--session <id>` for
OpenCode, and `--conversation <id>` for AGY. Every identified pane resumes independently, including several of one harness in the same
checkout. Legacy records without an ID use the broad `resume` arguments (`--continue`,
`resume --last`, or none). Because those mean "the last conversation in this directory", only one
legacy pane per checkout and harness may claim broad resume; aliases of the same harness share that
claim. An agent that exits non-zero within five seconds of a resumed start is taken to have had
nothing to continue and is replaced by a plain new agent. Editors are not restored. Set
`ARGUS_NO_RESTORE=1` to start clean.

Review captures write blobs and trees to the repository's Git object database. Argus does not
change `HEAD`, branch refs, the real index, or working files while capturing a review.

Read-only Git operations use embedded libgit2. Branch switching and worktree creation/removal require
the `git` executable.

Argus-created worktrees are stored under:

```text
<primary-checkout>/.argus/worktrees/<branch>
```

Add `/.argus/` to the repository's ignore rules if it is not already ignored. A project may set
`worktree_root` to put them somewhere else instead — one directory per repository under it — and
`setup` commands to run in each worktree Argus creates.

## Agent Harnesses

Every agent pane receives `ARGUS_HOOK`, `ARGUS_HOOK_URL`, `ARGUS_HOOK_TOKEN`, `ARGUS_PANE`, and
`ARGUS_INSTRUCTIONS`. A CLI or wrapper can report without a configured hook file:

```sh
"$ARGUS_HOOK" status working
"$ARGUS_HOOK" status waiting "needs database access"
"$ARGUS_HOOK" status needs-review "ready for review"
"$ARGUS_HOOK" status done "reviewed and complete"
"$ARGUS_HOOK" status failed "tests failed"
"$ARGUS_HOOK" title "repairing session restore"
"$ARGUS_HOOK" checkout
"$ARGUS_HOOK" session "harness-session-id"
"$ARGUS_HOOK" comments
```

`argus-hook say "text"` writes text to stdout for harnesses that inject command output into the
agent's context. Lifecycle reporting forms are silent, and `comments` prints the checkout's stored
review feedback. All forms always exit successfully so a stopped daemon cannot break an agent turn.
Run `argus-hook checkout` after changing to another checkout in the same project;
Argus moves the existing pane under that checkout without restarting it. An explicit path may follow
`checkout`, but reporting the current directory is the normal form. `needs-review` marks work ready
to inspect; `done` marks reviewed, completed work. A later `working` report resumes either state.

Claude Code, Codex, OpenCode, AGY, Cursor Agent (`agent`), and the generic environment-only harness are built in. The Claude
harness manages
`UserPromptSubmit`, `Stop`, `Notification`, and `SessionStart` entries in
`<checkout>/.claude/settings.local.json`; its SessionStart hook captures top-level `session_id`.
Codex uses `<checkout>/.codex/hooks.json` with its required command-string handler shape. Its handler
reads pane routing from the process environment, keeping its trust-sensitive content stable across
pane starts and daemon restarts. OpenCode's
plugin reports the root session ID and updates it when the process creates a new root. AGY manages
`PreInvocation` and `Stop` hooks in `<checkout>/.agents/hooks.json` under the `argus` hook key and captures
top-level `conversationId`. The `agent` (Cursor) harness manages `sessionStart`, `beforeSubmitPrompt`,
`preToolUse`, `beforeShellExecution`, and `stop` hooks in
`<checkout>/.cursor/hooks.json` (with top-level `version: 1`) and captures top-level `conversation_id`
or `session_id` without posting `idle` from `sessionStart`. It also installs
`.cursor/rules/argus.mdc`. Tool-start hooks mark `working` when the CLI skips
lifecycle events; only `stop` returns the pane to `idle`. Commands bake the helper path and pane
URL because Cursor's hook process does not inherit `ARGUS_HOOK_*` (shell-run `argus-hook title`
still does). The helper answers Cursor tool hooks with `{"permission":"allow"}` and does not wait
for stdin EOF after one JSON object. Managed
entries preserve user settings and are removed when the last agent pane closes or during the next
daemon startup sweep. A same-named `[[harness]]` block replaces a built-in.

Hook files are checkout-wide, but handlers use or rebase to the valid pane-specific
`ARGUS_HOOK_URL` inherited by each process. Concurrent panes therefore report to their own rows.

## Development

GitHub Actions runs check, tests, and clippy on Linux x86_64, macOS arm64, and Windows x86_64 for
pull requests and pushes to `main`. The workflow intentionally omits rustfmt until the existing
workspace-wide formatting drift is resolved.

To make CI mandatory, create a branch ruleset for `main` under **Settings > Rules > Rulesets** and
require the `Linux x86_64`, `macOS arm64`, and `Windows x86_64` status checks. A workflow reports
failures, but repository rules are what prevent an unverified merge.

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

## Publishing a Release

Releases are created from version tags. The tag must exactly match `workspace.package.version` in
`Cargo.toml`; for version `0.1.0`, use `v0.1.0`.

1. Update `workspace.package.version` and commit the change.
2. Ensure CI passes on `main`.
3. Create and push the matching tag:

```sh
git tag -a v0.1.0 -m "Argus 0.1.0"
git push origin v0.1.0
```

The release workflow builds five native packages, generates SHA-256 checksums, and creates a GitHub
Release with generated notes. A mismatched tag fails before any packages are built.

## License

Argus is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT License](LICENSE-MIT), at your option.
