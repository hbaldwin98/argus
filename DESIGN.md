# Argus: Current Design

Argus is a terminal workspace for running shells and AI command-line agents against Git
checkouts. This document describes the code as it exists today. Desired behavior lives in
[`TARGET.md`](TARGET.md); unfinished work lives in [`ROADMAP.md`](ROADMAP.md).

## Process model

Argus has three binaries:

- `argus`: the ratatui/crossterm client.
- `argusd`: the daemon that owns PTYs, terminal state, Git state, and runtime persistence.
- `argus-hook`: the helper used by managed Claude hooks.

The client starts the daemon lazily when it cannot connect. On Unix the daemon process calls
`setsid`; on Windows it starts in a detached process group. Closing the client does not stop the
daemon or its child processes.

The daemon listens on a Unix socket or Windows named pipe. Messages are named MessagePack
records framed by a four-byte big-endian length. Frames larger than 64 MiB are rejected.
Several clients may connect at once and each connection may subscribe to several pane screens.
There is no protocol negotiation or transport authentication.

## Navigation model

The runtime hierarchy is:

```text
Workspace scope -> Project -> Checkout -> Pane
```

A workspace is a daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The TUI draws project, checkout, and pane
columns followed by the selected pane's terminal.

A project configures one or more repository paths. Each path currently becomes a flat primary
checkout under the project; there is no repository node in the protocol. Linked worktrees are
discovered under the project, but multi-repository worktree operations still assume the first
primary checkout.

## Configuration

Configuration uses `ARGUS_CONFIG_DIR` when set and the platform config directory otherwise.

- `projects.toml` declares workspaces, projects, and agent templates.
- `client.toml` stores theme and editor settings.
- `open-workspace` stores the daemon-wide selected workspace name.
- `session.json` stores descriptions of panes to relaunch.

The current project and agent schema is:

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
env = { KEY = "value" }
```

Projects without a workspace use `default`. When no agents are configured, `claude`, `codex`,
and `opencode` templates are supplied. Adding a project at runtime appends a single-repository
project to `projects.toml` in the open workspace.

## Panes and terminal state

Shells, agents, and editors use the same PTY primitive. Editors exist in daemon state while open
but are omitted from the normal pane list and counts.

Each PTY starts at 24 by 80 cells. A blocking reader thread sends output to a Tokio task, which
drains output on a 16 ms interval, feeds a `vt100` parser, and broadcasts changed horizontal cell
spans. The parser retains 4,000 scrollback lines, though the client has no scrollback navigation.
An exiting process gets a 500 ms output-flush grace period.

Clients receive a full grid when they subscribe, then incremental damage. Resize changes both the
PTY and parser and emits another full grid. If several clients resize one pane, the latest request
wins; ownership is not yet defined.

The current pane states are `Idle`, `Working`, `Waiting`, and `Exited { code }`. Claude hooks map
prompt submission to working, stop to idle, and notification to waiting. Other harnesses rely on
process state. Hook files are checkout-wide, so concurrent Claude panes in one checkout do not yet
have independent hook routing.

## Session restore

`session.json` records each pane's checkout path, kind, and title when the daemon broadcasts a
structural tree change. Recording is enabled by `main` only after startup restore, so tests do not
write the user's session.

On daemon startup:

- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells;
- agents start as new processes from the saved template name;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

This is relaunch, not process reattachment or conversation resume. PIDs and harness session IDs
are not stored. Exited panes are omitted. Session replacement uses a same-directory temporary file,
flush, and atomic rename; on Unix it also syncs the parent directory. A read or parse failure disables
recording for that daemon run so the recoverable file is not overwritten.

## Git and checkouts

Read-only Git work uses `git2`. Every two seconds the daemon refreshes:

- branch or detached-HEAD state;
- dirty state and changed-file count, including untracked files;
- ahead/behind against the tracking branch;
- linked worktrees added or removed outside Argus.

Branch and file pickers run in process. Branches are local branches, current first. File discovery
uses `ignore`, follows Git ignore rules, and caps the result at 50,000 files.

Git mutations use the `git` executable:

- switch to an existing branch;
- create and switch to a branch;
- add a worktree and branch;
- force-remove a linked worktree and best-effort delete its branch.

Argus-created worktrees live under `<primary>/.argus/worktrees/<branch>`. Branches without a
checkout are not represented. Dirty-checkout switching relies on Git's own conflict checks.

## Review

`R`, `Tab`, or leader-Tab requests a checkout review. The daemon computes the diff on a blocking
task with `git2`; the client opens it in a floating overlay while leaving the terminal behind it
subscribed.

Three bases cycle with `b`:

- `uncommitted`: `HEAD` against index and working tree.
- `this branch`: merge base against upstream or a conventional local default branch, falling
  back to `HEAD`.
- `last looked`: the last target explicitly accepted with `A`. With no baseline it falls back to
  uncommitted work and remains uninitialized until accepted, including for an empty review.

Each request captures an immutable synthetic tree from the real index plus working-tree deletions,
edits, and non-ignored untracked content. The objects are written to Git's object database without
changing HEAD, branches, the index, or files. Diffs use three context lines, preserve old and new
line numbers, and detect renames. Binary files and files over 5,000 rendered lines are listed
without their content. Capture and diff failures are reported rather than rendered as empty work.

Every request has an id and the client accepts only the latest exact reply. Accepted last-looked
trees live under a hidden `refs/argus/review/...` ref keyed by the worktree Git directory, so linked
worktrees do not share baselines and daemon restarts retain them. `A` updates the ref with
compare-and-swap against the displayed baseline. Closing, refreshing, changing base, editing, and
commenting do not acknowledge anything.

The client supports line and file navigation, range marking, a changed-file fuzzy picker, refresh,
and opening the selected line in an editor. A comment is flattened and typed into the first agent
PTY in the checkout; it is not durable review state.

There is no vetted state, stage/unstage/revert action, syntax highlighting, or persistent comment
store. Closing the review sends no daemon message.

## Editors and overlays

Files open in one of three modes:

- a floating PTY overlay, the default;
- the rightmost terminal column;
- an external detached process with no PTY.

Known GUI editors always launch externally. Editor lookup uses `$VISUAL`, then `$EDITOR`, then an
installed terminal editor fallback. Daemon-side validation rejects absolute and parent-traversing
paths. Editor command text is split on whitespace, so quoted arguments and executable paths with
spaces are not supported yet.

Floating panes close through leader-Escape, F12, clicking outside, or process exit. Closing a
floating editor kills it; closing a window over a listed pane only hides the window.

Settings save immediately to `client.toml`. The client ships Catppuccin Mocha, Macchiato, Frappe,
and Latte themes. `ARGUS_THEME` overrides the stored startup theme.

## Testing

`cargo test` covers protocol framing, grid damage, key and mouse behavior, navigation state
machines, UI rendering, Git status and diffs, worktree reconciliation, hooks, workspace scoping,
session restore, and real short-lived PTY processes. Tests live in `#[cfg(test)]` modules because
the binary crates currently have no library targets.
