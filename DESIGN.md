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
Workspace scope -> Project -> Repository -> Checkout -> Pane
```

A workspace is a daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The TUI draws project, repository, checkout,
and pane columns followed by the selected pane's terminal.

A project takes its repositories from a root directory, from paths named one at a time, or from
both. Each becomes a repository with its own primary checkout and linked worktrees. Repository
identity is carried through daemon state and the protocol, so worktree discovery, creation, and
removal stay scoped to the repository that owns the selected checkout.

A root is scanned for the Git repositories at or beneath it. The scan stops at each repository it
finds, so a submodule or a vendored checkout stays part of the repository containing it rather than
becoming a sibling of it. It skips `.git`, `.argus`, `node_modules`, and `target`; it does not
follow directory symlinks; it goes no more than eight directories below the root; and it treats
neither a linked worktree nor a bare repository as a repository of its own. Like the rest of the
read-only Git work it uses `git2` rather than the `git` executable, and it returns its repositories
in path order.

The scan runs at startup and every ten seconds after, on the blocking pool and never under the
daemon mutex. A repository cloned into a root therefore arrives on its own, and one deleted out of
it leaves once it holds no panes — a repository still running an agent stays until it is empty,
because a directory can go missing for reasons that have nothing to do with the operator's intent.
A path the configuration names outright is taken at its word: one that is not a Git repository at
all still becomes a row, and no scan removes it. A root with no repositories under it is a project
all the same.

Checkout rows use the branch currently occupying their path as their display name, including when
another process switches the branch outside Argus. A live agent can report that it has started
working in another known checkout in the same project. Argus then moves the existing pane under that
checkout without restarting its PTY or changing its id, title, status, or conversation. The client
follows the pane when the pane list or terminal has focus; project and checkout navigation stays put.

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
name = "src"
root = "~/src"

[[project]]
name = "argus"
repos = ["~/src/argus"]
workspace = "work"

[[agent]]
name = "claude"
cmd = ["claude"]
env = { KEY = "value" }
```

Projects without a workspace use `default`. A project may set `root`, `repos`, or both; a path
reached both ways is one repository, and the two lists are joined with `repos` first. When no
agents are configured, `claude`, `codex`, and `opencode` templates are supplied. Adding a project
at runtime appends a block whose `root` is the directory given, in the open workspace, so what it
holds is discovered again on each start rather than frozen into the file.

## Panes and terminal state

Shells, agents, and editors use the same PTY primitive. Editors exist in daemon state while open
but are omitted from the normal pane list and counts.

Each PTY starts at 24 by 80 cells. A blocking reader thread sends output through a bounded queue to
a Tokio task, which processes a bounded batch on a 16 ms interval, feeds a `vt100` parser, and
broadcasts changed horizontal cell spans plus the child cursor's position and visibility.

A cell's grapheme is stored inline rather than on the heap, and a cell the parser holds nothing in
is read as a blank without asking the parser to build one. Both exist because a grid is rebuilt,
diffed, shipped and applied sixty times a second per pane, and an allocation per cell at each of
those steps was most of what that cost — the second one especially, since most of a screen is blank
and `vt100` allocates for a blank cell as readily as for a full one. A blank still carries the
attributes it was cleared to, so a TUI's coloured bars survive. The encoding is unchanged: a cell is
a plain string on the wire. The
client also bounds incoming daemon messages and coalesces redraws to the same interval. Cursor-only
changes are broadcast even when no cell changed. The client places its hardware cursor there only
while that pane has typing focus. The parser retains 4,000 scrollback lines, though the client has
no scrollback navigation. An exiting process gets a 500 ms output-flush grace period.

Clients receive a full grid when they subscribe, then incremental damage. The grid and the damage
stream are taken under one hold of the parser lock, so no frame can be published between them and
be missed by both. Resize changes both the PTY and parser and emits another full grid. If several
clients resize one pane, the latest request wins; ownership is not yet defined.

The client drops a pane's cached grid the moment it stops drawing it, so a subscription it takes
back is never redundant even when the daemon never stopped streaming: only a snapshot can rebuild
a grid, because incremental damage has no rows to land on. Subscription changes are coalesced over
one frame and reduced to the settled selection, so crossing a column of panes costs one full grid
rather than one per pane — but that settled selection is always sent.

The client enables bracketed paste and forwards each paste as one protocol message. The daemon
consults the pane parser and wraps the text in bracketed-paste delimiters only when the child has
requested that mode. Keyboard, paste, mouse, and daemon updates all share the client's 16 ms redraw
tick, so input bursts cannot trigger an unbounded number of full UI renders.

The current pane states are `Idle`, `Working`, `Waiting`, `NeedsReview`, `Done`, `Failed`, and
`Exited { code }`. `NeedsReview` means work is ready for the operator to inspect; `Done` means it
has been reviewed and completed. `Failed` means the agent said something went wrong while still
running, so the row is worth going to rather than worth closing. A pane also carries a `note`: one
line from the agent about the state it is in — the question it is blocked on, or what failed — drawn
as the row's second line so a stalled pane explains itself without being opened. A note is set
alongside a status report and cleared by the next accepted report that carries none, so it can never
outlive the state it explains.
Automatic `Idle` events do not erase `Waiting`, `NeedsReview`, `Done`, or `Failed`; the agent reports
`Working` when it resumes.

When the client itself runs in a Herdr pane, it reports one aggregate `argus` agent for the open
workspace. Its message names that workspace and groups every live pane by harness, with each pane's
name and status. `Waiting`, `NeedsReview`, and `Failed` map to Herdr's blocked state, `Done` maps to
idle, and `Working` takes precedence over idle; a blocked pane's name and note lead the message so
truncation cannot hide why it needs attention. A newly attached client reports the tree it receives
even when every agent was already running, and releases the report when it detaches. Herdr context
is removed from nested PTY processes so individual harness integrations cannot claim the outer pane.
This aggregate is limited to the open workspace because background workspace summaries carry pane
counts, not individual statuses.

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
`resume`, the legacy arguments that continue the last conversation, and `resume_id`, an exact argv
template containing `{session_id}`. Both are used only when a recorded pane is restored. Claude
Code, Codex, OpenCode and `generic` are built in. Codex uses a project-local `.codex/hooks.json`
SessionStart adapter; Codex requires the user to trust project hooks before it runs. A `[[harness]]`
block in `projects.toml` adds or replaces one, and an `[[agent]]` template selects one with
`harness = "..."`, defaulting to a harness matching its own name. A block cannot supply a plugin,
so replacing a built-in by name also gives up its module and its resume arguments.

The daemon's loopback receiver is a small pane API rather than a hook endpoint: `POST
/pane/<id>/status/<working|idle|waiting|needs-review|done|failed>` with an optional body as the note,
`POST /pane/<id>/title`, `POST /pane/<id>/session` with a validated harness session ID, and `POST
/pane/<id>/checkout` with a known checkout path. The checkout
endpoint changes affiliation only: the agent runs `argus-hook checkout` from the directory it has
already moved to. The status is named in the URL rather than the harness's event name, because the
installer already resolved that — which is what makes a new harness config instead of a match arm. Managed
blocks are per-boot: they name an ephemeral port and a per-boot token, so they are swept from every
configured checkout at startup and removed when the last agent pane in a checkout goes away, along
with any directory Argus made only to hold them. Moving a pane performs the same cleanup in its old
checkout and installs its harness in the new one. Hook files are checkout-wide; the helper rebases a
generated URL to a valid `ARGUS_HOOK_URL` on the same loopback listener, so each process still routes
to its own pane. The helper reads hook stdin once and can extract both a note and a configured
top-level session ID key. Claude captures `session_id` at SessionStart. OpenCode's plugin reports only
the root ID and reports again when a newly created root replaces it.

Agents name their own rows. At session start a harness with a `context_event` is handed
instructions telling it to run `argus-hook title "..."` once it knows what it is working on, and
`argus-hook status waiting "..."` when it needs a human, `needs-review` when work is ready to
inspect, and `done` once reviewed and complete. The instructions also ask it to run
`argus-hook checkout` after moving to another checkout. The same text is in `ARGUS_INSTRUCTIONS`,
which is where a plugin harness picks it up — OpenCode's module appends it to the system prompt.
Titles arriving from a model are flattened to one line and cut to 48 characters. Neither a rename,
status report, nor checkout move can touch a pane that has exited. A renamed row keeps showing its
template on its second line, so a column of agents that have all named themselves still says which
CLI each one is.

## Session restore

`session.json` records each pane's checkout path, kind, title, status, note, and optional harness
session ID when the daemon
broadcasts a structural tree change. Recording is enabled by `main` only after startup restore, so
tests do not write the user's session. Older files without status fields restore panes as `Idle`.

On daemon startup:

- linked worktrees are reconciled against Git first, because only primary checkouts come from the
  config and a pane in a worktree would otherwise look like a pane whose checkout is gone;
- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells;
- the saved status and note are reapplied after each pane starts;
- an agent with an ID starts with its harness's `resume_id` argv template expanded to that exact ID;
- every identified pane resumes independently;
- records without an ID retain broad `resume`; one legacy pane per checkout and harness claims it,
  so different template aliases of one harness cannot reopen the same last conversation;
- an agent started that way that exits non-zero within five seconds is taken to have had nothing to
  continue, and is replaced by a plain new agent in the same checkout;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

Exact templates are Claude `--resume {session_id}`, Codex `resume {session_id}`, and OpenCode
`--session {session_id}`. Their legacy broad forms are `--continue`, `resume --last`, and
`--continue`; `generic` has neither. A `[[harness]]` block sets `resume_id`, `resume`, and an
event-level `session_id` stdin JSON key. Replacing a built-in gives up all of its defaults.

This is relaunch, not process reattachment. PIDs are not stored. Captured IDs are nonempty, bounded,
and control-free, and enter the child command as argv rather than shell interpolation. Exited panes
are omitted. Session replacement uses a same-directory temporary file,
flush, and atomic rename; on Unix it also syncs the parent directory. A read or parse failure disables
recording for that daemon run so the recoverable file is not overwritten.

## Git and checkouts

Read-only Git work uses `git2`. Every two seconds, on a blocking-pool thread, the daemon refreshes:

- branch or detached-HEAD state;
- dirty state and changed-file count, including untracked files;
- ahead/behind against the tracking branch;
- linked worktrees added or removed outside Argus.

On a slower ten-second beat it also rescans each project root for repositories added or removed
there. Both run on the blocking pool.

Status is cached on the checkout rather than read when a tree is snapshotted. A snapshot is taken
under the daemon's one lock, and a keystroke needs that same lock to find the pty it belongs to, so
reading git there put several milliseconds of blocking I/O per checkout in front of the next key —
on every structural change, not just on the poll. The refresh collects paths, reads git with the
lock down, and stores results back by checkout id, dropping any whose checkout moved meanwhile. A
branch switch, a new worktree, and a scan that found new repositories refresh what they changed, so
a row does not name the branch it just left for the rest of the tick; anything changed outside Argus
waits for the poll.

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
- `this branch`: merge base against a conventional local default branch, then the branch's
  upstream when no default exists, falling back to `HEAD`.
- `last looked`: the last target explicitly accepted with `A`. With no baseline it falls back to
  uncommitted work and remains uninitialized until accepted, including for an empty review.

Each request captures an immutable synthetic tree from the real index plus working-tree deletions,
edits, and non-ignored untracked content. The objects are written to Git's object database without
changing HEAD, branches, the index, or files. Diffs use three context lines, preserve old and new
line numbers, and detect renames. Binary files, files over 1 MiB, files over 5,000 rendered lines,
and content beyond the review's 20,000-line budget are listed without their content. Capture and
diff failures are reported rather than rendered as empty work.

Every request has an id and the client accepts only the latest exact reply. Accepted last-looked
trees live under a hidden `refs/argus/review/...` ref keyed by the worktree Git directory, so linked
worktrees do not share baselines and daemon restarts retain them. `A` updates the ref with
compare-and-swap against the displayed baseline. Closing, refreshing, changing base, editing, and
commenting do not acknowledge anything. Review capture is globally serialized, and a connection
drops an older queued capture when a newer request replaces it.

The client supports line and file navigation, single-file range marking, a changed-file fuzzy
picker, refresh, and opening the selected line in an editor. A comment is flattened and typed into
the first agent PTY in the checkout; it is not durable review state.

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
