# Argus

[![CI](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml/badge.svg)](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml)

Argus is a terminal workspace for running shells and AI command-line agents across Git checkouts.
One persistent daemon owns the processes and PTYs; the TUI can detach and reconnect without stopping
them.

The workspace builds four executables:

- `argus`: the terminal client.
- `argusd`: the daemon that owns panes, Git state, and session restore.
- `argus-hook`: a helper that lets agent harnesses report status and pane titles.
- `argus-desktop`: a desktop window onto the same daemon, opened with `argus desktop`.

## Features

- Organizes projects into workspaces, repositories, checkouts, and shell or agent panes.
- Keeps panes running when the client closes.
- Starts and discovers Git worktrees and switches branches from the TUI.
- Runs Claude Code, Codex, OpenCode, pi, Google Antigravity (AGY), Cursor Agent (`agent`), or custom command-line agent templates.
- Names each agent pane from the user's latest prompt, so a column of running agents is not a list of identical template names.
- Shows each checkout's Git state: `↑` ahead of and `↓` behind its upstream, `+` staged and `!`
  unstaged files, or `clean`.
- Reviews staged and unstaged work as two separate diffs, the way Git itself keeps them apart.
- Captures deleted, renamed, and non-ignored untracked content for review.
- Reads a diff unified or split side by side, with comments meaning the same thing in either.
- Scrolls a pane back through what its child has already printed, by wheel or Shift-PageUp.
- Stores feature briefs, tasks, and a visibly connected tree of what agents chose, what they chose
  it over, and what forced it. Later agents can read that durable context, and reversing a decision
  supersedes it rather than erasing it.
- Shows a feature whole in one view — its brief, its tasks and its reasoning — and says what is
  happening to it from what it already observes: the agents running in its checkouts and how far
  its tasks have got. Nothing to drag between columns and no second tracker to keep true; the only
  state anyone sets by hand is accepting the work.
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
- A full-screen terminal with true-color support.

Common platform setup:

**Ubuntu or Debian**

```sh
sudo apt update
sudo apt install build-essential pkg-config git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**macOS**

```sh
xcode-select --install
brew install git pkg-config
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows**

1. Install Git for Windows.
2. Install Visual Studio 2022 Build Tools with **Desktop development with C++**.
3. Install Rust from [rustup.rs](https://rustup.rs/) using the default MSVC toolchain.
4. Run the following commands from PowerShell or Windows Terminal.

The desktop window is a webview. On Linux, building it needs WebKitGTK's development files, and
running it needs WebKitGTK itself:

```sh
sudo apt install libwebkit2gtk-4.1-dev libdbus-1-dev   # to build
sudo apt install libwebkit2gtk-4.1-0                   # to run
```

macOS and Windows need nothing extra; Windows uses the WebView2 runtime it ships with. A plain
`cargo build` or `cargo test` leaves the desktop crate out, so the terminal client and the daemon
build without WebKitGTK; `--workspace` or `-p argus-desktop` builds it.

libgit2 is built from vendored source, and Argus reads repositories only locally — fetching and
pulling go through the `git` on `PATH` — so neither OpenSSL nor a system libgit2 is required.

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
under [Configuration](#configuration). You can also press `n` in the projects column and browse to a
directory: every Git repository at or beneath it becomes one of the project's repositories, so
choosing a repository adds that one and choosing the directory a dozen of them live in adds the dozen.

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

To add a directory of repositories as a project without opening the TUI, run:

```sh
argus init ~/src      # or `argus init` for the current directory
```

It scans the directory into the open workspace, prints the repositories and checkouts it found,
and refuses a directory that is already a project.

To restart a running daemon without opening the TUI, run:

```sh
argus server restart
```

The command waits for the replacement daemon to be ready. Non-exited panes are restored from the
runtime session store.

To stop the daemon without starting a replacement, run:

```sh
argus server stop
```

The command waits for the endpoint to be released. Connected Argus clients exit, and the next
`argus` launch restores non-exited panes from the runtime session store.

To ask where you are working, for a script or a status line, run:

```sh
argus focus           # one JSON object, then exits
argus focus --watch   # a JSON object per line, each time it changes
```

It prints what the Argus client you were last at is showing: the workspace, project, repository,
checkout (path, branch, whether it is the primary checkout and dirty) and pane (id, kind, title,
agent, status, note, model). Each part is `null` when nothing is selected there. The whole line is
`null` while no client is open, and in `--watch` while the daemon is down; the watch connects
again once it is back. `argus focus` never starts the daemon.

```sh
argus focus | jq -r '"\(.checkout.branch) · \(.pane.agent // "shell") \(.pane.status)"'
```

To open the desktop window beside the terminal client, run:

```sh
argus desktop
```

It starts `argus-desktop` from beside `argus`, or from `PATH`, detached so closing the terminal
leaves the window open, and returns at once. The window reaches the same daemon through
`argus bridge`, so it shows what the terminal client shows: the rail of projects, repositories,
checkouts and panes, and one live pane that takes keys, pastes and resizes. It draws its own title
bar: the sidebar runs to the top of the window with Search, New and the three stages above the
rail, the connection, the chime and the settings at its foot. New (`Ctrl-N`) opens the composer: pick the
repository, checkout and agent, type the agent's first message, and `Ctrl-Enter` starts it, or
Shell starts a shell there instead. `+` on a checkout opens a menu
that starts a shell (`S`) or an agent from one of the daemon's templates (`A` for the first, the
default; the arrows and Enter for the rest). The template the window last started is listed first,
and remembered across runs. A checkout several agents share says so: `⚠2` on its rail row, and
"shared by 2" on the Checkouts stage. A project with `exclusive` set refuses a second agent there, and
the window says which agent holds it. `×` on a pane, shown while it is pointed at and in
the stage header, closes it. The Checkouts stage (`3`) lists the chosen repository's checkouts, and
with `B` the branches without one: Enter opens a checkout on its first pane or switches the primary
checkout to a branch, `F` fetches, `P` pulls, and `S`/`A` start a shell or the default agent in the
selected checkout. `N` gives a branch row a worktree, or a checkout a new branch in one; `D`
deletes a branch row (asking again, before `git branch -D`, when git refuses an unmerged one) or
removes a worktree once asked. `R` opens the checkout's review: its unstaged
and staged changes beside the diff, where `v` picks lines, `c` comments to the checkout's agent,
`Ctrl-P` jumps to a file by name and `E` opens the line in your editor. It opens on the side and
layout last chosen, unless that side has nothing and the other does. `H` opens its history:
commits unfolding to their files, filtered with `/`, and any of them opened as a review. The clock
on a repository's rail row opens its recent agent sessions across every checkout, live ones first:
`j`/`k` move, `/` searches, `H` and `C` step the harness and checkout filters, and the transcript
of the one under the cursor shows beside the list. `O` opens it in full in place of the list: your
turns as a jump list (`j`/`k`), a find (`/`, then `n` and `N`), `Y` copying the session id, `R`
resuming it, and Escape back to the list; a harness that keeps no transcript lists its sessions but
has none to open. Enter goes to a live session's pane, and on an ended one resumes it in a new pane
(`R`), by the command the footer names; the pane is selected under its checkout once the daemon
has made it, and its header says when the session was last active and how many turns it had. A
session from a removed worktree, or from a harness that cannot resume by id, offers only its
transcript. The project heading at the top of the rail opens the switcher: every project with
how many of its agents need you or are working, Add a project… (a folder of repositories, one
repository, or a new one made with `git init`, previewing what it would add before anything is
written, then a name and folders to skip), Switch workspace…, and Remove, which asks first and
leaves the folders where they are. Connect to a host… lists the hosts in `~/.ssh/config` and the ones used before, and reaches the
chosen one's daemon through ssh as `argus --host` does (keys or ssh-agent only; argus must already
be installed there). `ARGUS_DESKTOP_HOST` names one on a first run. After `argus server stop` it waits for its Reconnect button rather than starting the daemon
again. The rest of the terminal client's surfaces are still to come; `DESKTOP.md` has the plan.

Its keys are the terminal client's where the two share a surface. In a pane every key goes to the
program except the zoom keys and the leader, `Ctrl-Space`: after it, `f` fills the window with the pane and again
gives the rail back, `x` closes the pane, `Tab` reviews the pane's checkout, `1`, `2` and `3` open
the Workspace, the Feature and the Checkouts stage, `n` opens the composer, and a second `Ctrl-Space` sends one to the program. Outside a pane `1`, `2` and `3`
open them directly, and the
Feature stage takes `j`/`k`, Home/End, `v`, `z`, `/`, and on the feature under the cursor `m`,
`h`, `A` and `g`. `N` writes a new feature as one note — the first line its title, a paragraph its
brief, `-` a task (indented, a subtask), `?` an open decision with its options indented beneath,
`!` a decision made with its reason beneath — beside what the note reads as by the daemon's own
grammar, naming any line it cannot place; `Ctrl-Enter` creates it, and it opens once the daemon
has it. Enter opens the feature under the cursor: its tasks and decisions as one outline (`j`/`k`)
beside the row under the cursor. There `e` edits a task's title and description or a decision as its
record, `b` the brief, `a` adds a task, `s` a subtask or sub-decision, `d` raises a decision, `o`
answers an open one in words (or pick one of its options and Take it), `x` withdraws a decision or
restores it, and Escape goes back to the index. A task's done-when criteria are checked off, added and
removed in place. `Ctrl-V` (`Cmd-V` on a Mac), `Ctrl-Shift-V` and `Alt-V` paste, and
Shift-PageUp/PageDown page through the history.

The console is shells that belong to no checkout: each starts in your home directory, and every
window shows the same ones, since they are the daemon's. `4` (from a pane, the leader and `4`), or
Console in the sidebar, gives it the stage; `` ` `` (from a pane, the leader and `` ` ``), or the `›_`
beside the connection, shows it docked beside the Workspace, Feature and Checkouts stages or hides
it, and its sash sets its size. It docks at the bottom until moved: the chevron in its head, or the
palette's Dock the console on the left, on the right or at the bottom, moves it to that edge of the
stage, as does dragging its header there. The leader and `z` fills the window with it, all but the sidebar's
projects. A tab per console, `+` for another and a tab's `✕` to end one. Docked, the console sits
beside the Workspace's pane with both on screen, so an agent and a shell are side by side: a click
on either, or the pane chosen on the rail, gives it the keys, and from a pane the leader and `o`
hands them to the next one on screen. The split button (from a pane, the leader and `\`) shows two
consoles side by side; a click on a side's header or screen gives it the keys, a tab picked replaces
the side with the keys, and a side's `✕` takes it out of the split, its console staying a tab. Where it is docked, whether it
shows, and its size are kept in `desktop.json`; the split lasts the run. A daemon restarted starts as many
consoles again as were running, each a fresh shell in the home directory.

`Ctrl-K` (from a pane, the leader and `k`) opens the palette: one search over the window's commands,
the project's panes and checkouts and the chosen repository's features, each with its key or its
status beside it. A leading `>` searches only commands, `#` only features. `?` (from a pane, the
leader and `?`) opens the shortcuts sheet, every key grouped by where it works.

`Ctrl-=` and `Ctrl--` (`Cmd` on a Mac) zoom the whole window in and out, a pane's text with the
rest, and `Ctrl-0` puts it back; they work everywhere, a pane included, and the zoom is kept in
`desktop.json`.

`Ctrl-,` (from a pane, the leader and `,`) opens the settings, saved as each one changes: the theme
or following the system, the background's opacity and, on Windows, whether Mica or Acrylic shows
through it, where a file opens and the editor command (empty uses `$VISUAL`, then `$EDITOR`), a
system notification naming the pane when an agent needs you (by default only while the window is
in the background), and whether a repository's Feature index opens as a list or a board until it
remembers its own.

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

On Windows, panes use the ConPTY built into Windows. To run them on a newer one — the console host
Windows Terminal ships, which re-draws less and passes more escape sequences through, most
noticeably on Windows 10 — put `conpty.dll` and `OpenConsole.exe` from Microsoft's
`Microsoft.Windows.Console.ConPTY` package beside `argusd.exe` and restart the daemon. Argus does
not ship them. The daemon loads libraries only from its own directory and System32, so a
`conpty.dll` anywhere else, such as a repository Argus was started in, is never picked up.

### Build from source

Install the packages so the executables share Cargo's binary directory:

```sh
cargo install --locked --path crates/argusd
cargo install --locked --path crates/argus-client
cargo install --locked --path crates/argus-desktop   # optional: the desktop window
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
| --- | --- |
| `projects.toml` | Workspaces, projects, repositories, agent templates, and harnesses. Yours to edit; Argus only reads it |
| `client.toml` | Editor, theme, layout, pane-view, and notification preferences |
| `desktop.json` | The desktop window's own: each repository's feature layout, whether it chimes when an agent needs you, whether the console is docked and how tall, its zoom, and its settings (theme, opacity, editor, notifications). Kept apart from `client.toml`, so each client's editor and theme are set in it |
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
- `opencode`, running `opencode --auto` (auto-approving permissions that are not explicitly denied).
- `pi`, running `pi`.
- `agy`, running `agy`.
- `agent`, running Cursor's `agent` CLI.

Adding any `[[agent]]` entry replaces the built-in list, so include every template you want to keep.
Install and authenticate each agent CLI separately, and ensure it is available in the daemon's
inherited `PATH`. Custom commands are argument arrays; the first item is the executable. An agent's
optional `harness` selects a matching built-in or configured harness; without it, Argus tries the
agent name and then falls back to the generic environment-only harness.

Claude Code reports through hooks Argus writes into `.claude/settings.local.json`, Codex through
session, prompt, tool, and session-end hooks in `.codex/hooks.json`, OpenCode through a plugin module Argus writes to
`.opencode/plugin/argus-status.js`, and pi through a project extension at
`.pi/extensions/argus-status.ts`. AGY uses `.agents/hooks.json` under the `argus` hook key, and
Cursor's `agent` CLI uses `.cursor/hooks.json` plus an always-on rule at `.cursor/rules/argus.mdc`.
All are removed when
the last agent pane in the checkout closes and swept from every configured checkout at startup.
Argus also adds the generated hook and skill paths, and `.argus` (where linked worktrees live by
default), to the repository-local `.git/info/exclude` when it discovers a checkout, so they stay out of status without changing the tracked `.gitignore`.
Codex treats project hooks as untrusted until the user approves them. Argus writes the correct hook,
but cannot approve that trust decision; exact Codex identity capture starts after approval.

Claude Code 2.1.287 and later also run a small mod (a Claude Code plugin of function hooks) that
Argus writes into its own config directory, never the checkout, and names in the pane's
`CLAUDE_CODE_PLUGIN_DIRS` after any folders already there. Through it, as through the OpenCode
plugin and the pi extension, what Argus says to the agent — a review comment, say — goes in through
the harness rather than as typing, so it can never land in a dialog. An older Claude Code ignores
the folder, and is typed into as before.

Agent panes also show telemetry: the tool running now, context used against the model's window,
cumulative cost, and the model. Each harness supplies what it can. Claude Code and Codex hooks
read it from the hook payload and the transcript it names (Claude Code does not record its
context window or cost there, so its mod reports those from inside the session, and Codex prices
nothing). The OpenCode plugin and pi extension
report model, tokens, and cost from their message events, and Cursor and AGY hooks report the
model and tool their payloads carry. Any other agent can run
`argus-hook telemetry --model M --context N --window N --cost USD --tool NAME`.

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
transcript = { format = "neutral", path = "~/.herdr/sessions/{session_id}.jsonl" } # for session previews

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
`transcript` says where the CLI keeps a session's transcript and in which format: `format` is
`claude`, `codex`, `cursor`, `pi` or `neutral`, and `path` a template with `~`, `{session_id}`,
`{checkout}`, `{project}` and `*`. `transcript = "claude"` alone reads Claude Code's format where
Claude Code keeps it. The daemon lists a repository's sessions from these files and reads one when
asked, for the desktop window's recent-sessions dialog; the neutral format and the
templates are spelled out in DESIGN.md, "Transcripts". A CLI that keeps no transcript can have its
hooks write the neutral one with `argus-hook transcript FILE user|agent|title TEXT` or
`argus-hook transcript FILE tool NAME SUMMARY [--failed]` (`--stdin` reads the text from stdin), with
`path` naming the same file. A harness with no `transcript` still lists its sessions, from what Argus
saw of their panes — title, checkout, and when it first and last saw them — and its preview says it
keeps no transcript.

#### Extensions

An extension is a program the daemon runs beside itself that adds to the interface: a tab, keys
and palette commands, dialogs, status-bar notices, and panes it opens.

```toml
[[extension]]
name = "docker"                       # lowercase letters, digits and dashes
command = ["python3", "docker-extension.py"]
dir = "~/argus-extensions"            # where it runs; the config directory by default
env = { DOCKER_HOST = "unix:///var/run/docker.sock" }
```

The blocks are read when the daemon starts, so restart it after changing one. Its output goes to
`extension-<name>.log` in the config directory.

How to write one, from an empty file to a running extension and then every endpoint, field, event
and limit, is printed by the `argus` binary itself, so it matches the build you run:

```sh
argus extension guide         # the walkthrough, then the reference
argus extension list          # the extensions running, and what each publishes
argus extension show docker   # one of them: its own guide, its commands and its views
```

`list` and `show` ask the daemon, and take `--host HOST` before `extension` for another
machine's. `scripts/docker-extension.py` is a fuller example.

In the desktop window, every view an extension publishes is listed under **Extensions** in the
sidebar, after Workspace, Features, Checkouts and Console, while its scope matches. A view opens
docked on the left, the right or the bottom of the stage, or as a page of its own — where its
`place` says, until you move it with its header's dock menu, **Open as page** or **Dock** (`>` and
`<` while the view has the keys), which the window remembers per view. A dock holding several
panels — the console, the agents' views, extension views — shows a tab for each, and a tab's
right-click moves it to another dock. Any panel can also be dragged by its tab or header, or an
extension's view from the sidebar, to an edge of the stage to dock it there, or onto the stage to
open it as a page. The palette's **Go to …** opens one too. A middle click closes what it lands on,
as a browser's tab: a saved view's tab or header, an agent's view tab, a console tab, or a pane's
row in the rail, which ends its program as its `✕` does.

Views an agent saved (`argus-hook view save`) are listed under **Views** in the sidebar, grouped by
scope and filtered to those about the chosen repository and open feature (**Here**) or all of them,
a stale one marked. A view another one links to is folded under it; the count beside a view unfolds
them. One opens as a panel docked on the right, which is moved and dragged like the
others. A link in a view (a `view` value, or an arrow beside a heading, card, list item and the
like) opens its target as a new column beside the one it was followed in, and a panel of several
scrolls sideways; a column's `✕` closes it and its pop-out button makes it a panel of its own. The
`✕` on the panel's header or on its dock tab closes the whole panel.

A link followed inside a saved view opens its target as a dialog floating over the stage, so the
view it came from keeps its dock. Drag a dialog by its header and resize it by its bottom-right
corner; the window remembers where. Its dock menu docks it, and the same menu on a docked view
(or a column's dialog button) floats it. To have links open as the next column instead, set
**Settings → Views → Links in a saved view open** to **Column**. A
column's header offers **Refresh** (asks the agent that saved it to build it again), **Mark stale**,
**Delete** and **Details**: what it was built from. The Feature stage lists the saved views scoped or
attached to the open feature beside its tickets.

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
| --- | --- |
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

```sh
cargo build -q -p argus -p argusd
ARGUS_INSTANCE=dev target/debug/argus               # opens this checkout's Argus
ARGUS_INSTANCE=dev target/debug/argus server stop   # ends this checkout's daemon
```

For the desktop window, build it too and open it the same way:

```sh
cargo build -q -p argus -p argusd -p argus-desktop
ARGUS_INSTANCE=dev target/debug/argus desktop
```

`scripts/reload.sh` installs only the daemon and the terminal client.

Both forms set `ARGUS_INSTANCE=dev`, so the worktree daemon listens on its own
endpoint under its own slice of the config directory and the installed Argus
keeps running, untouched. The daemon outlives the client, so the next launch
connects instantly and a rebuild only pays for what changed.

## Controls

In the directory browser, use Up and Down to select a folder, Enter to open it, and Left to return
to its parent. Enter on the `add this directory` or `make it in this directory` row confirms the
location. Typing filters the folders in the current directory.

### Navigation

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move along the rail — the project, its repositories, the open repositories' checkouts and panes, then the consoles at its foot — in the order it is drawn |
| `l`, Right, Enter | Open the repository and move into it, go to the checkout's first pane (unfolding it), or type into the pane or console |
| `h`, Left, Escape | Up to the row this one hangs under; on an open repository, fold it first |
| `z` | Fold the repository or checkout under the cursor — on a pane, its checkout — or unfold it. A folded checkout keeps its row, with how many panes it hides coloured by the loudest of them; a second click on a checkout does the same, and in the desktop window so do the chevron at its left and its context menu |
| `s` | Start a shell — on a branch row, in a new worktree for that branch; on a console, another console |
| `c` | Start a console: a shell in your home directory that belongs to no checkout, listed under *consoles* at the foot of the rail once the daemon runs it, and typed into straight away. A daemon from before consoles draws no such section, and `c` says to restart it |
| `a` | Choose and start an agent — on a branch row, in a new worktree for that branch |
| `n` | Add a project on the project heading, a repository on a repository row, or a worktree on a checkout — or, in the Checkouts stage, give a branch row a worktree |
| `i` | On a repository row, make a repository that does not exist yet: browse to where it should go, name it, and Argus creates the directory, runs `git init` in it, and adds it to the project. An empty name uses the chosen directory itself, which is how a folder that is already there gets initialized |
| `D` | Remove what the row is, after confirmation: a project or repository (out of the panel only — nothing on disk is touched), a linked worktree (deleted), or a branch row (the local branch, deleted; the remote is untouched). An unmerged branch is refused and asked about again, and confirming that second prompt force-deletes it |
| `w` | Switch workspace |
| `o` | Switch project (command center rail; also click the project name) |
| `b` | Open the branch picker |
| `B` | In the Checkouts stage, show or hide the branches no checkout is sitting on — the main branch's offer first, and the ones only a remote has last |
| `F` | Fetch every remote (`--prune`), which is what makes the remote's branches appear as rows |
| `P` | Pull the selected checkout, fast-forward only |
| `f` | Open the file picker |
| `R` / Tab | Open review, from the rail or the Checkouts stage |
| `H` | Open the checkout's commit history, from the rail or the Checkouts stage |
| `N` | Jump to the next pane, or parent of a child, waiting, failed, or ready for review |
| `S` | Open settings |
| `t` | Choose a theme for this client process |
| `x` | Kill the selected pane or console |
| `q` | Detach the client |

### Views

The stage beside the rail holds one view at a time: `1` Workspace (the selected pane's terminal),
`2` Feature, `3` Checkouts, and `4` Views. The tab strip along the top names them and carries the
digit that opens each; clicking a tab does the same. Switching views never stops a pane.
Drag the rail's right border to widen or narrow it; the width is saved to `client.toml` as
`rail_width`.

The Views stage lists what agents showed, then the saved views, one list. `j`/`k` scroll the open
view, `n`/`p` move along the list, and `x` closes a shown view. A saved view is drawn with its
scope, a `stale` mark and when it was built; `l` lists the views it links to, `Enter` opens the one
under the cursor, and `Backspace` or `h` goes back along the links followed. `r` asks the agent that
saved it to refresh it, and `s` marks it out of date.

The Feature view opens on an index of the selected repository's features, grouped by how they
stand — active, queued, done — with the one under the cursor peeked at beside the list. Held and
archived features are folded away until `z`. `Enter` opens a feature, which is read through three
lenses: `1` work, the tasks and the decisions they raised as one outline; `2` record, every
decision as a tree with the selected one read as an ADR; and `3` flows, its sequence diagrams step
by step. A breadcrumb says where you are and `Esc` climbs it one level at a time. Features belong
to the repository rather than a branch, so they remain available after a feature worktree is
removed.

Agent helper commands use that same repository scope. For a change that intentionally
crosses repositories or branches, set `ARGUS_ARTIFACT_SCOPE=workspace` for the `argus-hook feature`,
`task`, `decisions`, and `decide` commands involved. Workspace scope is explicit rather than
inherited from whichever workspace the TUI currently shows.

| Key | Action |
| --- | --- |
| `j` / `k` | Move in whatever has the keys; PageUp/PageDown ten at a time, Home/End for the ends |
| Enter | Open it: a feature, a task's brief, a decision's record, a flow |
| `1` `2` `3` | Inside a feature: read it as work, as a record, as flows |
| Tab | Into the detail beside the list, and back |
| `a` | A new feature written as one note; inside one, a root task, or a flow on the flows lens |
| `i` | Write more of the open feature as one note |
| `s` | A subtask under the selected task, or a sub-decision under the selected decision |
| `d` | A decision, raised under the task the cursor is on. Start it with `?` to leave it open. Flows: name the decision binding the step, `D2.1` or `#id` |
| `u` / `o` / `y` | Supersede the decision, answer an open one in words, copy it as ADR markdown |
| `c` / `n` | A done-when criterion for the selected task, or a note on it |
| Space | Mark the task done or not; in the detail, check a criterion off |
| `e` | Rewrite it: the brief, the task's title, the decision's record, the flow's Mermaid source |
| `h` / `l` | Work: fold or unfold the task. Flows: the flow before or after |
| `H` / `L`, `J` / `K`, `>` / `<` | Move the task along todo, doing, done; among its siblings; in or out a level |
| `g` | Go to the pane working the task or the feature |
| `R` | Rename the feature — the title only; its slug is frozen so nothing filed under it is orphaned |
| `x` | Remove the feature or task. A decision is withdrawn instead, and `x` again restores it |
| `v` / `z` | Archive the feature or bring it back; show or fold the held and archived ones |
| `m` / `p` / `w` | Move the feature to another checkout, hold it with a reason, choose what it comes after |
| `t` / `T` | Open the feature's ticket in the browser (over SSH, copy its URL); link it to a ticket by pasting the URL |
| `M` | On the index: the repository's tracker map, edited like a brief; when an agent has drafted one, the draft, which `A` approves as it reads and `D` turns down |
| `r` | Re-ask the daemon for all of it |
| Escape | Up one level; `q` goes straight back to the workspace |

In the note a feature is written as, the first line is its title and the prose under it its brief;
a `-` line is a task and an indented one a subtask, a `?` line an open decision with its options
indented beneath, and a `!` line a decision already made with the reason beneath. `Ctrl-S` creates
it, Tab and Shift-Tab indent, Escape throws it away.

On the flows lens each step names the decisions and tasks bound to it, the selected step is lit in
the diagram, and the decisions binding it are listed under the steps with the tasks that carry
them out. A binding is a Mermaid comment under the step's message, `%% decision #4 task #9`, which
`d` writes for you. `Enter` opens the selected Mermaid sequence diagram whole. `j`/`k`
scroll vertically and `h`/`l` (or the arrows) pan horizontally when the diagram is wider than the
overlay; `q` or Escape closes it.

### Terminal panes

`Ctrl-Space` is the leader key:

| Chord | Action |
| --- | --- |
| `Ctrl-Space`, Escape | Leave terminal input or close a floating pane |
| `Ctrl-Space`, `f` | Toggle fullscreen for the selected pane |
| `Ctrl-Space`, `x` | Kill the pane |
| `Ctrl-Space`, Tab | Open review |
| `Ctrl-Space`, `N` | Jump to the next pane waiting, failed, or ready for review |
| `Ctrl-Space`, `1` / `2` / `3` | Open a view — the digits belong to the child while you are typing in a pane |
| F12 | Emergency close for a floating window |
| `Ctrl-V` or `Alt-V` | Paste the clipboard; an image on it, or an image file dropped on the terminal, reaches the pane as a file even when the daemon is on another machine |

Other supported keys are forwarded to the child PTY. Use `Alt-V` in Windows Terminal, which keeps
`Ctrl-V` for itself. Images and dropped files cross only from where the client runs: in an ssh
session the client has no clipboard, so run `argus --host <machine>` on the desktop instead.

### Review

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move through changed lines |
| `d` / `u`, Page Down / Page Up | Move ten lines |
| `]` / `[` | Next or previous changed file |
| `g` / `G`, Home / End | First or last changed line |
| Enter / `z` | On a fold, the lines the diff left out: show twenty more of them, or all |
| `v` / `V` | Start or clear a line-range selection |
| `f` | Open the changed-file picker |
| `c` | Send a comment to an agent in the checkout; choose one when several are running |
| `e` | Open the selected line in the editor |
| `b` | Toggle between staged and unstaged changes; an empty side leaves the diff on screen |
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

In the desktop window, `o` reads the file under the cursor whole in the Code stage, at that line.

Comments are stored in `runtime.db` before Argus notifies the selected live agent. They remain
visible to every live agent in that checkout through `argus-hook comments`; the command returns the
newest 100 comments in order. If terminal delivery fails after the save, the comment remains stored.

### Code

The desktop window's Code stage (`5`, or **Code** in the sidebar) reads a checkout's working tree
rather than its diff: a tree of its files with each changed file marked and counted, one file whole
with the lines it changed washed as a review washes them and a mark where lines were removed, a
ruler beside it marking where in the whole file it changed, and that file's outline with the
changes in it listed. A file whose only change is its line endings is not counted as changed.
Opened from a staged review it reads the index, and from the history a commit, and says so in its
header. It is read-only; `e` opens the file in your editor.

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move through the file's lines |
| Home / End | First or last line |
| `]` / `[` | The next or previous change |
| Ctrl P | Go to a file |
| Ctrl F, or **Search** above the tree | Search the checkout; Enter searches, then reads the match picked. Alt C makes case always matter, Alt W matches whole words only, Alt R reads the query as a regular expression, and a second field takes globs of the files to include (`src/**/*.rs`, `!*.lock`) |
| `o` / `O` | Show or hide the outline |
| `v` / `V` | Start or clear a line-range selection |
| `c` | Comment on the picked lines to an agent in the checkout, as a review comment |
| `e` | Open the file in your editor at the cursor's line |
| `r` / `R` | Read the files again |
| Escape | Close what is over the file — a comment, the search, go to file, a picked range — and stay on the file |
| `b` / `B` | Back to the review or history it was opened from, as it was |

The outline lists what the file defines for Rust, TypeScript and JavaScript, Python and C#. In the
history, `c` reads the commit under the cursor in Code, at the file the cursor is on.

## Sessions and Git Data

Argus records non-exited shell and agent panes, in worktrees as well as primary checkouts. After a
daemon restart it launches fresh processes in the recorded checkouts; it does not reattach old PIDs.
When a pane has a captured harness session ID, Argus appends the harness's exact `resume_id`
arguments: `--resume <id>` for Claude Code, `resume <id>` for Codex, `--session <id>` for
OpenCode and pi, and `--conversation <id>` for AGY. Every identified pane resumes independently, including several of one harness in the same
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

Argus installs a small **argus skill** for built-in harnesses: `.claude/skills/argus` for
Claude Code, `.pi/skills/argus` for pi, and `.agents/skills/argus` for Codex, OpenCode, AGY, and
Cursor. Where the harness has a context hook (Claude Code, Codex), the agent starts with the
pane's context already printed by `argus-hook context`: review comments, and the checkout's feature
by name with how much is filed under it and any open decisions waiting on a person. The brief itself is read with `argus-hook feature` when
the work touches it. Elsewhere the startup instructions ask it to run that command once. The
agent loads `SKILL.md` only when it reports status or changes the board, and each reference only
when it writes to that part of it. Hooks keep reporting lifecycle
events and session identity. `argus-hook task add "<what to do>" --under <id>` records newly
discovered work beneath an existing task, and `argus-hook task` prints the resulting tree.
Generic harnesses receive compact fallback instructions instead. Custom `[[harness]]` blocks
can opt in with `skill_dir = ".agents/skills/argus"` and deliver `ARGUS_INSTRUCTIONS` through
their context mechanism.

The skill is embedded in `argusd`, so no separate skill installation is needed. Argus preserves
user-owned files at those paths and falls back to compact instructions if it cannot install the
package. Generated files carry `argus:managed-skill`; remove that marker to preserve a replacement.
Managed skills are cleaned up with the hooks when the last agent leaves the checkout. Their
paths are included in the repository-local `.git/info/exclude` alongside the hook files.

To try this after rebuilding, restart the daemon when your running work is safely stopped, then
start a fresh agent pane. It should start with its pane context and name its pane. In Codex, trust the
new SessionStart context hook when prompted; its command stays stable across subsequent starts.
Existing running agents keep the instructions they started with. Pi must trust the project before
it loads project-local extensions and skills; approve its normal trust prompt on the first run.

`argus-hook instructions` prints the inherited startup message without contacting the daemon.
`argus-hook say "text"` writes text to stdout for harnesses that inject command output into the
agent's context. Lifecycle reporting forms are silent, and `comments` prints the checkout's stored
review feedback. All forms always exit successfully so a stopped daemon cannot break an agent turn.
Run `argus-hook checkout` after changing to another checkout in the same project;
Argus moves the existing pane under that checkout without restarting it. An explicit path may follow
`checkout`, but reporting the current directory is the normal form. `needs-review` marks work ready
to inspect; `done` marks reviewed, completed work. A later `working` report resumes either state.

Claude Code, Codex, OpenCode, pi, AGY, Cursor Agent (`agent`), and the generic environment-only harness are built in. The Claude
harness manages
`UserPromptSubmit`, `Stop`, `Notification`, `SessionStart`, and tool entries in
`<checkout>/.claude/settings.local.json`; its SessionStart hook captures top-level `session_id`.
Subagents and a dynamic workflow's agents run inside the pane's session and are not listed as rows;
a pane whose turn ended with a workflow or background subagent still running reads as working until
it finishes.
Codex uses `<checkout>/.codex/hooks.json` with its required command-string handler shape. Its handler
reads pane routing from the process environment, keeping its trust-sensitive content stable across
pane starts and daemon restarts. OpenCode's
plugin reports the root session ID and updates it when the process creates a new root. Pi's
project extension captures its session ID, reports active and settled agent
runs, marks blocking extension prompts as waiting, and injects the skill bootstrap into the system
prompt. Pi resumes an identified session with `--session <id>`. AGY manages
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

GitHub Actions runs tests and clippy on Linux x86_64, macOS arm64, and Windows x86_64 for
pull requests and pushes to `main`, and checks the desktop page (the `Desktop page` job). The sources are hand-formatted, so rustfmt is disabled in
`rustfmt.toml` and `cargo fmt` changes nothing.

To make CI mandatory, create a branch ruleset for `main` under **Settings > Rules > Rulesets** and
require the `Linux x86_64`, `macOS arm64`, and `Windows x86_64` status checks. A workflow reports
failures, but repository rules are what prevent an unverified merge.

Useful checks from the repository root:

```sh
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The desktop window's page is TypeScript in `crates/argus-desktop/page`, compiled into
`crates/argus-desktop/assets/js`. The compiled JavaScript is committed, so building the crate needs
no Node; editing the page does. From `crates/argus-desktop/page`:

```sh
npm ci            # once: TypeScript, oxlint and oxfmt, at the locked versions
npm run build     # compile src/ into ../assets/js; commit what it writes
npm run fmt       # format the TypeScript and assets/style.css with oxfmt
npm run check     # compile, oxlint with warnings denied, and oxfmt --check, as CI does
```

The types of what Rust sends the page are written from `crates/argus-desktop/src/page.rs` into
`page/src/bindings` by `cargo test -p argus-desktop`; commit them with the change to `page.rs`. A
`tsconfig.json` is in `page/`, so any TypeScript language server reads the page as the compiler does.

Some tests launch real PTYs and Git processes. `cargo test` can finish all assertions while a spawned
test process briefly delays command teardown.

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

The release workflow runs the CI workflow's full matrix (Linux, macOS, and Windows) against the
tag, then builds five native packages, generates SHA-256 checksums, and creates a GitHub Release
with generated notes. A mismatched tag or a failure on any CI platform stops the release before any
packages are built.

## License

Argus is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT License](LICENSE-MIT), at your option.
