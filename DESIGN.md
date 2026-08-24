# Argus: Current Design

Argus is a terminal workspace for running shells and AI command-line agents against Git
checkouts. This document describes the code as it exists today. Desired behavior lives in
[`TARGET.md`](TARGET.md); unfinished work lives in [`ROADMAP.md`](ROADMAP.md).

## Process model

Argus has three binaries:

- `argus`: the ratatui/crossterm client.
- `argusd`: the daemon that owns PTYs, terminal state, Git state, and runtime persistence.
- `argus-hook`: the helper managed hooks run, and the command an agent runs to report on itself.

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
spans plus the child cursor's position and visibility. Cursor-only changes are broadcast even when
no cell changed. The client places its hardware cursor there only while that pane has typing focus.
The parser retains 4,000 scrollback lines, though the client has no scrollback navigation. An
exiting process gets a 500 ms output-flush grace period.

Clients receive a full grid when they subscribe, then incremental damage. Resize changes both the
PTY and parser and emits another full grid. If several clients resize one pane, the latest request
wins; ownership is not yet defined.

The current pane states are `Idle`, `Working`, `Waiting`, `Failed`, and `Exited { code }`.
`Failed` means the agent said something went wrong while still running, so the row is worth going
to rather than worth closing. A pane also carries a `note`: one line from the agent about the state
it is in — the question it is blocked on, or what failed — drawn as the row's second line so a
stalled pane explains itself without being opened. A note is set alongside a status report and
cleared by the next report that carries none, so it can never outlive the state it explains.

When the client itself runs in a Herdr pane, it reports one aggregate `argus` agent for the open
workspace. `Waiting` and `Failed` map to Herdr's blocked state, then `Working` takes precedence over
`Idle`; the blocked pane's note or title explains why. A newly attached client reports the tree it
receives even when every agent was already running, and releases the report when it detaches. Herdr
context is removed from nested PTY processes so individual harness integrations cannot claim the
outer pane. This aggregate is limited to the open workspace because background workspace summaries
carry pane counts, not individual statuses.

Status is harness-agnostic. A *harness* is a description of how a particular agent CLI can be
asked to report, and there are three mechanisms; a harness may use any combination of them.

Every agent pane is handed `ARGUS_HOOK_URL`, `ARGUS_HOOK_TOKEN`, `ARGUS_PANE`, `ARGUS_HOOK` and
`ARGUS_INSTRUCTIONS`. That is the universal floor: a CLI that can run one command at some point in
its lifecycle can report without Argus knowing anything about its config format. On top of that,
a harness whose hooks live in JSON in the checkout can have Argus write and remove a managed block
itself — `settings` says where the file is, `shape` says how an entry nests (`matcher` for Claude
Code, `flat` otherwise), and `events` maps the harness's own event names onto the statuses Argus
draws. Third, a harness that extends through code rather than through JSON can have Argus write a
plugin module into the checkout and remove it on the same schedule. OpenCode is the built-in case:
it has no hook table, so its module carries the event mapping itself and reads `ARGUS_HOOK_URL`
and `ARGUS_HOOK_TOKEN` at run time rather than having a pane baked into it. A harness also carries
`resume`, the arguments that make its CLI continue its last conversation, used only when a recorded
pane is restored — see Session restore. Claude Code, Codex, OpenCode and `generic` are built in;
Codex is there only to say how it resumes, having no hook mechanism at all. A `[[harness]]` block
in `projects.toml` adds or replaces one, and an `[[agent]]` template selects one with
`harness = "..."`, defaulting to a harness matching its own name. A block cannot supply a plugin,
so replacing a built-in by name also gives up its module and its resume arguments.

The daemon's loopback receiver is a small pane API rather than a hook endpoint: `POST
/pane/<id>/status/<working|idle|waiting|failed>` with an optional body as the note, and `POST
/pane/<id>/title`. The status is named in the URL rather than the harness's event name, because
the installer already resolved that — which is what makes a new harness config instead of a match
arm. Managed blocks are per-boot: they name an ephemeral port and a per-boot token, so they are
swept from every configured checkout at startup and removed when the last agent pane in a checkout
goes away, along with any directory Argus made only to hold them. Hook files are checkout-wide, so
concurrent panes of the same harness in one checkout do not yet have independent hook routing;
a plugin module does not have that problem, because it reads the pane out of its own environment.

Agents name their own rows. At session start a harness with a `context_event` is handed
instructions telling it to run `argus-hook title "..."` once it knows what it is working on, and
`argus-hook status waiting "..."` when it needs a human; the same text is in `ARGUS_INSTRUCTIONS`,
which is where a plugin harness picks it up — OpenCode's module appends it to the system prompt.
Titles arriving from a model are flattened to one line and cut to 48 characters. Neither a rename
nor a status report can touch a pane that has exited. A renamed row keeps showing its template on
its second line, so a column of agents that have all named themselves still says which CLI each
one is.

## Session restore

`session.json` records each pane's checkout path, kind, and title when the daemon broadcasts a
structural tree change. Recording is enabled by `main` only after startup restore, so tests do not
write the user's session.

On daemon startup:

- linked worktrees are reconciled against Git first, because only primary checkouts come from the
  config and a pane in a worktree would otherwise look like a pane whose checkout is gone;
- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells;
- agents start as new processes from the saved template name, with their harness's `resume`
  arguments appended to the template's own command;
- one pane per checkout and template resumes; a second agent of the same kind in the same checkout
  starts fresh, because a resume argument names the last conversation in a directory rather than a
  particular one;
- an agent started that way that exits non-zero within five seconds is taken to have had nothing to
  continue, and is replaced by a plain new agent in the same checkout;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

`claude` and `opencode` resume with `--continue`, `codex` with `resume --last`, and `generic` not at
all; a `[[harness]]` block sets its own `resume`, and a block that replaces a built-in by name gives
up the built-in's along with its plugin.

This is relaunch, not process reattachment. PIDs are not stored, and neither are harness session
IDs: the conversation comes back only as far as a CLI's own "continue the last session" flag
reaches. Exited panes are omitted. Session replacement uses a same-directory temporary file,
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
