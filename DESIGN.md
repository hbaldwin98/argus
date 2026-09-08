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

The daemon is started with its stderr on the null device, since it shares no console with the
client and anything it printed would either land in the middle of the TUI or open a console window
of its own. So it logs to a file beside its config as well, keeping the previous run's log — a
daemon that is starting has often just stopped in a way somebody wants to read about. `RUST_LOG`
sets the level, `info` by default. Failing to open the log is never a reason not to start.

The daemon listens on a Unix socket or Windows named pipe. Messages are named MessagePack
records framed by a four-byte big-endian length. Frames larger than 64 MiB are rejected.
Several clients may connect at once and each connection may subscribe to several pane screens.
There is no protocol negotiation or transport authentication.

## Where each responsibility lives

One rule decides which file a thing goes in: a module is named after the question it answers, and
answers only that one. Nothing is split across two files, and no file holds two subjects. Tests sit
beside the code they cover, in a `tests/` module split the same way.

`argus-protocol` is everything the three binaries have to agree on, and nothing else. Anything
written on both sides of a boundary belongs here, because three binaries share no types unless they
live in this crate and a contract written twice drifts in silence.

| module | answers |
| --- | --- |
| `message` | what a client asks for, and what the daemon sends back |
| `tree` | what a client renders, and which pane state outranks which |
| `hook` | the pane API's URLs, environment, headers and flags — `argus-hook` builds what the daemon parses |
| `cell`, `framing`, `transport` | a screen cell, a frame, and the endpoint they travel over |
| `review`, `notes`, `ids`, `paths` | the shapes review, note, identity and location data travel in |

`argusd` owns PTYs, Git, and everything that outlives a client. Its state is one `Daemon` type
behind a small set of mutexes; the split below is which file a concern is *read* in, not a change
to the type or its locking.

| module | answers |
| --- | --- |
| `state` | what the tree is, and what a client is shown of it |
| `state/build` | turning what the config declares into the tree the daemon runs on |
| `state/panes` | a pane's lifecycle: spawned, restarted, closed, written to |
| `state/agents` | what an agent reports about itself, and what it is told |
| `state/viewers` | the one pty size reconciled out of what every client asks for |
| `state/git_ops` | the writes to Git |
| `state/sync` | the polls and watchers that keep the tree level with the disk |
| `state/panel` | the rows the user adds and removes by hand |
| `state/workspaces` | which scope is open, daemon-wide |
| `state/notes` | what is written down against a row |
| `state/hook_server` | the loopback receiver agents report to |
| `state/session` | what survives a daemon restart |
| `state/tree` | finding your way around the tree |
| `conn` | one client connection, and which task each message runs on |
| `pty`, `pty/job`, `pty/vt` | a pane's child process, its resource bounds, and the vt100 translation |
| `harness`, `harness/install`, `harness/hooks` | what a CLI is, what gets written into a checkout for it, and the command lines in it |
| `harness/skill` | the skill package an agent receives and the short message that leads it there |
| `store`, `store/schema`, `store/legacy` | `runtime.db`, its tables, and the files it replaced |
| `git`, `diff`, `browse`, `highlight` | the read-only questions asked of a repository |
| `config` | `projects.toml`, which is read and never written |
| `editor`, `watch`, `command`, `logging`, `paths` | the small services the rest of the daemon uses |

`argus` is a replaceable renderer over one model. `app` holds the state and never predicts the
result of a request; `ui` is a pure function of it.

| module | answers |
| --- | --- |
| `main`, `redraw`, `terminal`, `wire`, `launch` | the event loop, and the screen and socket it runs over |
| `app` | the model: the tree, the selection, and which modal is up |
| `app/nav`, `app/input`, `app/mouse`, `app/scroll` | what the operator's gestures mean |
| `app/actions`, `app/pickers` | what is asked of the daemon, and the modal layers that ask it |
| `app/server` | what arrives back, and what it does to the selection |
| `ui` | the frame, and where the cursor goes on it |
| `ui/columns`, `ui/rows`, `ui/text` | the spine, the vocabulary of a row, and fitting text to a width |
| `ui/review`, `ui/history`, `ui/status`, `ui/overlay`, `ui/modals`, `ui/term` | one drawn surface each |
| `review`, `history`, `notes`, `dirpicker` | the view state behind each overlay |
| `grid`, `pty_input`, `paste`, `clipboard`, `fuzzy` | a pane's screen, and the input primitives |
| `settings`, `theme`, `backend`, `herdr`, `profile` | preferences, palette, the ratatui backend, and what is reported outward |

## Views

The content area holds one *view* at a time. The spine — the five columns below — is the default
view and the one every other view is left back to. A one-row tab strip along the top of the frame
names the views that exist and marks the open one; each tab carries the digit that opens it, and a
click on a tab does the same. From inside a pane the digits belong to the child, so a view is
reached through the leader (`Ctrl-Space`, then the digit) the way review and history are. The strip
is drawn in the blank row the page is already inset by, so it costs the view underneath nothing.

While a view other than the spine is open, the content area is that view's: a click in it selects a
row there and keeps the keyboard on the view, rather than resolving against the pane whose column
used to occupy the same cells. The status bar follows: each view advertises its own keys, since a
bar still offering the spine's columns is worse than one saying nothing.

Switching views changes the screen and nothing else. Every pane keeps running, its subscription
stands, and the spine is one keystroke back. Focus moves out of the columns while another view is
open — a view has no columns to move between, and a key left reaching a pane that is not on screen
is a key nobody can see the effect of — and returns to the column it left. Which view is open is
this client's own state and is never sent to the daemon: two people attached to one daemon are not
necessarily reading the same thing.

The views are the spine, the decision board, the feature board, and one feature's tasks (see
"Features and the decision board").

## Navigation model

The runtime hierarchy is:

```text
Workspace scope -> Project -> Repository -> Checkout -> Pane
```

A workspace is a daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The TUI draws project, repository, checkout,
and pane columns followed by the selected pane's terminal. Pressing `p` folds the leading
columns away one at a time to disclosure tabs on the left edge, ceding their width to the rest,
and wraps back to none; clicking a tab brings that column back. While typing in a pane,
`Ctrl-Space`, `f` lets its terminal take the main content area; repeating the chord restores the
columns. The status bar remains visible in both layouts.

A nav column is as wide as what it holds. It asks for its widest row — name line or detail line,
whichever is longer — and for its own title, floored, capped at what a list of names is worth, and
rounded up to a step so that renaming a pane or changing a note does not drag the whole spine
sideways. Everything left over goes to the live view. Fixed percentage shares were the reason the
same screen managed to look empty and truncated at once: they gave a column holding one project
more room than it could use and a column holding twelve panes less than it needed, and no width of
terminal changed the ratio. A dragged gutter still wins over all of this — it is an absolute
preference, not a share.

The layout answers a small terminal by folding rather than by squeezing. Every nav column has a
floor, the live view has its own and much larger one, and the width at which a column folds away
is derived from those floors rather than picked separately, so the two cannot drift: the spine
folds exactly when the widths it would otherwise hand out stop being honest. Shortfall is
reclaimed from the nav columns before the live view, since a squeezed list is still a list where
a squeezed terminal is a program that has stopped drawing. A resize only ever folds further —
widening does not undo a layout the user chose — and `p` overrides in either direction at any
width, so a folded column is never unreachable. Nothing is lost while folded: the live view's
title is the full breadcrumb.

Short terminals are answered the same way, in the other axis. A row is normally two lines, a name
and a dimmer line of what is true about it, but on a card too short to afford both the detail line
is dropped and the items are kept. Where a row carries a count badge, the badge is reserved before
the name is fitted — a count survives truncation better than the tail of a name does — unless
doing so would leave too little room to identify the row at all, in which case the badge is what
goes. The status bar's keymaps are written as tiers and the widest that fits is drawn, so a narrow
bar shows fewer keys rather than one cut mid-word. A card holding more rows than it can show puts
a thumb in the blank cell between its rows and its border, sized and placed like a scrollbar's:
without it a column scrolls silently, and twenty checkouts in a card that fits six look exactly
like six.

Weight recedes with focus. Five cards drawn in one weight read as five of the same thing rather
than as a path with a working end, so a column nobody is in drops its names from `text` to `muted`
and loses its bold; the selected row keeps full weight in every column, since the selections are
how the path is traced. A row's detail line is indented to start under its own name rather than by
a fixed amount, because a checkout carries a kind mark in front of its name and the other columns
do not — but only while the indent is free: a detail that would be ellipsized to pay for a tidy
left edge has traded something read for something merely noticed, and `no checkout` beats
`no checko…`. A nav card is padded on all four sides; the modals keep a tighter bottom, since
their height is chosen for the rows they hold.

The left of the bar counts the fleet — how many agents are working, how many are done, how many
are waiting on a person — ordered so the count you have to act on is the one nearest the corner.
It held a breadcrumb before, which said either the word already written on the card above it or
the path already spelled out across the live view's title. What is *not* written anywhere else on
screen is the state of the agents you are not currently looking at, which is the reason the
program has a pane list at all. Idle and exited panes are left out: a tally that counts them
reads the same whether anything is happening or not, and when nothing is happening the breadcrumb
comes back rather than the bar spending a row on `0 working`.

The bar is a reminder rather than a reference. It carries the handful of keys worth having in
front of you and ends, at every width and in every mode, with the one key that lists the rest —
appended after the tiers are chosen, so it is never the thing a narrow bar drops. `?` opens that
list as a window sized to its own content, grouped by what the keys act on rather than by
character, and answering for the mode you are actually in: the diff's keys in a diff, the
column's in the columns. It opens *over* whatever raised the question rather than instead of it,
so the review you asked about is still there when you close it, and any key that is not a scroll
key closes it — having to hunt for the way out of a window you opened to be told something is the
problem it exists to solve. `?` is only this where nothing is taking text; on a prompt, in a
pane, or in a note being written it is a character, and the leader chord reaches the list from
inside a pane.

A project takes its repositories from a root directory, from paths named one at a time, or from
both. Each becomes a repository with its own primary checkout and linked worktrees. Repository
identity is carried through daemon state and the protocol, so worktree discovery, creation, and
removal stay scoped to the repository that owns the selected checkout.

A root is scanned for the Git repositories at or beneath it. The scan stops at each repository it
finds, so a submodule or a vendored checkout stays part of the repository containing it rather than
becoming a sibling of it. It skips `.git`, `.argus`, `node_modules`, and `target`, plus whatever the
project's own `exclude` adds and minus whatever its `include` overrides; it does not
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

`i` in the repositories column is the one gesture that makes a repository rather than finding one:
the directory browser picks where it goes, a prompt names it, and the daemon creates the directory,
runs `git init` in it, and adds it to the project the same way a named path is added. An empty name
means the chosen directory itself, so a folder that already exists gets initialized where it
stands; a directory that is already a repository is added without being re-inited, since rewriting
the hooks of a repository that exists is not what the gesture asked for. `git init` does the work
rather than a `.git` written by hand, so the result is whatever the user's own git configuration
would have produced. The walk probes for a `.git` entry (or a bare Git directory) before opening libgit2,
so a root of ordinary directories is not a libgit2 open per folder.

Checkout rows use the branch currently occupying their path as their display name, including when
another process switches the branch outside Argus. A live agent can report that it has started
working in another known checkout in the same project. Argus then moves the existing pane under that
checkout without restarting its PTY or changing its id, title, status, or conversation. The client
follows the pane when the pane list or terminal has focus; project and checkout navigation stays put.

## Configuration

Configuration uses `ARGUS_CONFIG_DIR` when set and the platform config directory otherwise.

- `projects.toml` declares workspaces, projects, and agent templates. Argus only reads it.
- `client.toml` stores theme and editor settings.
- `runtime.db` holds everything Argus writes: panes to relaunch, projects and repositories added
  from the TUI, projects and repositories removed from the panel, workspaces created at runtime,
  and the daemon-wide selected workspace.
- `argusd.log` is the running daemon's log, and `argusd.log.1` the run before it.

## Runtime storage

`runtime.db` is SQLite in WAL mode, opened once per daemon with `synchronous = NORMAL` and a
five-second busy timeout. Its schema version is `user_version`; a store from a newer Argus is
refused rather than migrated backwards, and a store that cannot be opened at all costs the run its
memory rather than its startup — the daemon falls back to an in-memory store, which also cannot
overwrite whatever made the file unreadable.

The dividing line against `projects.toml` is ownership: the file says what exists and is the
user's to edit, comments included; the store says what happened while Argus was running. So a
project added from the TUI is recorded as an overlay keyed on its root directory, an extra
repository is recorded against its project's name, and a project the config declares is recorded
as *hidden* rather than deleted from the file. Startup merges the two, the config winning for any
root that appears in both.

Persistence is a store a daemon is given, not a flag it can be told to set: `Daemon::new` hands out
an in-memory one, and only `main` passes the store on disk. That is what keeps the test suite off
the user's state.

Schema v3 adds `note`, keyed on scope and key rather than on an id: a scope this version does not
recognise is dropped when the table is read, not guessed at. Schema v4 adds `note_audit`, keyed the
same way: one row per change an agent made to a note, kept even after the note itself is deleted.
Schema v5 adds `decision`, keyed by project name: one row per recorded decision, with `parent` a
self-reference rather than a foreign key, since a board is read whole and reassembled by a reader
that already has to survive a parent it cannot see.

`session.json`, `excluded-repos`, and `open-workspace` are read once, on the first start that finds
them, and renamed to `*.imported` afterwards. A `session.json` that will not parse is left where it
is, since a file this version cannot read is one a later version might.

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
worktree_root = "~/worktrees"
setup = ["pnpm install"]
exclusive = true
agent_todos = true
exclude = ["vendor"]
include = ["target/scratch"]

[[agent]]
name = "claude"
cmd = ["claude"]
env = { KEY = "value" }
restart = "on-failure"
```

`projects.toml` is watched, and a save reloads it into the running tree — projects, repositories,
their per-project settings, and agent templates. Nothing is rebuilt: what the file still names is
matched and updated in place, so ids stay valid and panes keep running. What it no longer names is
removed unless it is holding panes, in which case the row stays until it is empty. A file caught
half-written fails to parse and is logged and ignored rather than allowed to take the tree with it.
Harnesses are not reloaded: a running agent's hooks on disk were written by the harness it started
under.

Projects without a workspace use `default`. A project may set `root`, `repos`, or both; a path
reached both ways is one repository, and the two lists are joined with `repos` first. When no
agents are configured, `claude`, `codex`, `opencode`, `agy`, and `agent` templates are supplied. Adding a project
at runtime records the directory given as its `root`, in the open workspace, so what it holds is
discovered again on each start rather than frozen into the record.

## Panes and terminal state

Shells, agents, and editors use the same PTY primitive. Editors exist in daemon state while open
but are omitted from the normal pane list and counts.

An agent whose process exits leaves its row as `Exited` unless its template sets `restart`:
`on-failure` starts it again on a non-zero exit, `always` on any exit, and `never` — the default —
leaves the row alone. A pane closed by the operator is removed before it is killed, so closing is
never a restart. Three restarts of one template in one checkout within a minute stops the cycle and
leaves the exited row, which is what says what happened.

On Windows, each agent process tree runs in its own Job Object with an 8 GiB committed-memory limit
and a 64-process limit. Closing the pane or dropping its runtime terminates the whole job rather
than only the template's immediate process. Shell and editor panes are not subject to these limits.

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
while that pane has typing focus. The parser retains 4,000 scrollback lines. An exiting process gets a 500 ms output-flush grace period.

A client can park a pane's view above its live screen. It asks for an offset in lines and the
daemon answers with the rows there, the offset it could actually reach, and how far back the buffer
goes; the parked rows are drawn in place of the live grid until the view returns to the bottom. The
read moves the parser's scrollback offset and puts it straight back under one hold of the lock,
because that offset is parser-global: left set, it would drag every other subscriber's frames back
with one client's view, and the pump would broadcast the difference as damage. The alternate screen
keeps no scrollback of its own, so a full-screen child answers with a depth of zero rather than
showing the shell's history underneath it.

Damage keeps landing on the live grid the whole time a pane is parked, so returning to the bottom is
immediate and never needs a fresh subscription. The parked rows are deliberately not re-read as that
damage arrives: they are what the operator scrolled up to read, and a pane still producing output
would otherwise shift the text out from under them. The consequence is that an offset is relative to
the live screen at the moment it is requested, so on a pane that is actively printing, scrolling
again lands lower than the arithmetic suggests. Anchoring an offset to a line rather than to the
screen needs the daemon to count what it evicts, which the parser does not report.

A wheel over a pane on the normal screen moves that view rather than reaching the child, since that
is the screen with history behind it. Shift-PageUp and Shift-PageDown page by a screen less a line,
leaving the child its own unshifted paging keys. Typing returns the pane to the live screen: the
child's echo lands there, and the parked view is not somewhere input can be seen. A parked pane
leads its title with how far back it is, because it is otherwise indistinguishable from a quiet one.

Clients receive a full grid when they subscribe, then incremental damage. The grid and the damage
stream are taken under one hold of the parser lock, so no frame can be published between them and
be missed by both. Resize changes both the PTY and parser and emits another full grid.

Each client's requested size for each pane it is showing is recorded against its connection, and a
pane's PTY is sized to the smallest request in each dimension, so no client is ever sent more rows
or columns than it has room to draw. A client with a larger window pads; sizing to the largest
would truncate content out of the smaller one instead. Unsubscribing releases that client's claim,
and so does disconnecting, so a pane grows back once whatever was holding it small stops showing it.
A pane no client is showing keeps the size it has rather than reverting to the default. A request
that does not change the effective size is not applied, so a second client agreeing with the first
costs no snapshot. On the client side, a pane leaving the screen forgets its remembered size, so
returning to the screen re-sends it.

The client drops a pane's cached grid the moment it stops drawing it, so a subscription it takes
back is never redundant even when the daemon never stopped streaming: only a snapshot can rebuild
a grid, because incremental damage has no rows to land on. Subscription changes are coalesced over
one frame and reduced to the settled selection, so crossing a column of panes costs one full grid
rather than one per pane — but that settled selection is always sent.

The client enables bracketed paste and forwards each paste as one protocol message. The daemon
consults the pane parser and wraps the text in bracketed-paste delimiters only when the child has
requested that mode. A wheel over a pane whose child is on the alternate screen and has not asked
for mouse reporting is sent as a cursor key (xterm alternate-scroll); a child that has asked for
mouse reporting still gets the mouse sequence. Keyboard, paste, mouse, and daemon updates all share
the client's 16 ms redraw tick, so input bursts cannot trigger an unbounded number of full UI renders.

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

Each client compares consecutive tree snapshots by pane ID. The first snapshot after attaching is a
quiet baseline; a later effective-state change flashes the owning pane for 900 ms. Effective state
includes child agents because their parent pane is the selectable place the operator can open.
Transitions into `Waiting`, `NeedsReview`, or `Failed` also put the pane or child note in the status
bar. A client can optionally ring its terminal bell for those transitions when the pane is not the
active input pane; notifications default to off and are saved in `client.toml`.

When the client itself runs in a Herdr pane, it reports one aggregate `argus` agent for the open
workspace. Its message names that workspace and groups every live pane by harness, with each pane's
name and status, including child agents under the name `parent / child`. `Waiting`, `NeedsReview`,
and `Failed` map to Herdr's blocked state, `Done` maps to
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
Code, Codex, OpenCode, AGY, Cursor Agent (`agent`) and `generic` are built in. Codex uses a project-local `.codex/hooks.json`
SessionStart adapter whose command reads routing from the pane environment, so its content hash stays
stable after the user trusts it. Codex requires the user to trust project hooks before it runs. AGY uses
`.agents/hooks.json` with flat `PreInvocation` and `Stop` hooks. Cursor's `agent` CLI uses `.cursor/hooks.json` with
flat `sessionStart`, `beforeSubmitPrompt`, `preToolUse`, `beforeShellExecution`, and `stop` hooks
(schema `version: 1`) plus `.cursor/rules/argus.mdc`. `sessionStart` claims the conversation
(`conversation_id` or `session_id`) without posting `idle`, because that event is fire-and-forget
and can arrive after a tool has already marked the pane working. Tool-start events mark `working`
because the CLI often skips prompt/stop lifecycle hooks; only `stop` clears to `idle`. The helper
answers Cursor tool hooks with `permission: allow` (and Claude's `toolCall` with `decision: allow`)
and stops reading stdin at one JSON object so a runner that waits for stdout without closing the
pipe cannot deadlock the POST. Hook commands bake the helper path and pane URL — Cursor's runner
does not inherit the pane environment, unlike the shell where `argus-hook title` still works. A `[[harness]]`
block in `projects.toml` adds or replaces one, and an `[[agent]]` template selects one with
`harness = "..."`, defaulting to a harness matching its own name. A block cannot supply a plugin,
so replacing a built-in by name also gives up its module and its resume arguments.

The daemon's loopback receiver is a small pane API rather than a hook endpoint: `POST
/pane/<id>/status/<working|idle|waiting|needs-review|done|failed>` with an optional body as the note,
`POST /pane/<id>/title`, `POST /pane/<id>/session` with a validated harness session ID, and `POST
`/pane/<id>/checkout` with a known checkout path.
`POST /pane/<id>/comments` requires a live agent source and returns the newest 100 durable review
comments for that pane's checkout as JSON, oldest first. The checkout comes from the pane rather
than request input, so callers cannot select another checkout through this endpoint.
The checkout endpoint changes affiliation only: the agent runs `argus-hook checkout` from the directory it has
already moved to. The status is named in the URL rather than the harness's event name, because the
installer already resolved that — which is what makes a new harness config instead of a match arm. Managed
blocks are generally per-boot: they name an ephemeral port and a per-boot token, so they are swept from every
configured checkout at startup and removed when the last agent pane in a checkout goes away, along
with any directory Argus made only to hold them. Moving a pane performs the same cleanup in its old
checkout and installs its harness in the new one. Codex is the exception: its trust-sensitive command
contains only environment references and remains identical across boots. Hook files are checkout-wide;
the helper uses or rebases to a valid `ARGUS_HOOK_URL`, so each process still routes to its own pane.
The helper reads hook stdin once and can extract both a note and a configured
top-level session ID key. Claude captures `session_id` at SessionStart. OpenCode's plugin tags root
and child reports with their session IDs; only a root claims `/session`, and a newly created root
reports again when it replaces the previous root. AGY captures `conversationId` at PreInvocation.
Cursor's `agent` CLI captures `conversation_id` or `session_id` at `sessionStart` without moving the
pane to idle.

Every report carries the session it came from, and the pane belongs to one of them. The session that
claims a pane first owns it; a report from any other session — a CLI started from inside the pane,
which inherits the same hook URL and token and cannot be stopped from calling home — is recorded as a
child of that pane instead. Children are listed as indented rows beneath the parent's, each with its
own status and note, and are not separately selectable: clicking one selects the pane it runs in,
because a child is something happening inside that pane rather than somewhere else to go. A child can
change nothing about the pane it reports through:
not its title, not its status, not its checkout, and not the conversation it resumes. A new session
ID may take the pane over only while the pane is not working, which is what an agent starting a fresh
conversation looks like and what an agent spawned mid-turn does not; taking over clears the child
list. A child stops being listed three ways: it reports idle, its parent reports idle — the turn that
spawned it is over, and most children never report an ending of their own because the subagent's
harness fires the *parent's* hooks — or it goes ten minutes without reporting anything, which is the
backstop for one that was killed mid-turn. A background agent outliving its parent's turn is not lost
by the second of those: its next report lists it again. A pane lists at most eight. A report with no
session at all — `argus-hook status` run by hand — is the pane's own voice, as before.

Agents name their own rows, and the daemon names them first. A prompt-submit
event — Claude `UserPromptSubmit`, Cursor `beforeSubmitPrompt`, AGY
`PreInvocation`, OpenCode `chat.message` — carries the user's text; the helper
posts it to `/title` the same way an explicit `argus-hook title` does. Tool-start
events are not titles: a working pane named "Shell" says less than the template
already does. An agent can still refine the name once it knows the task; the
next prompt replaces it. Children still cannot rename the parent row.

Agent workflow guidance lives in the bundled `crates/argusd/skills/argus/SKILL.md` and its
`references/work.md`, embedded into the daemon at build time. The skill explains titles,
semantic status reports, checkout moves, and reading context and review feedback; its reference
covers features, tasks, decisions, and note writes. Lifecycle hooks still capture session
identity and report their existing events. A stopped turn is not proof of completed work.

Before starting a built-in agent, Argus installs the package in `.claude/skills/argus` for
Claude Code and `.agents/skills/argus` for Codex, OpenCode, AGY, and Cursor. The latter also gives
harnesses without a native skill loader a file they can read directly. `ARGUS_INSTRUCTIONS`
now holds a short bootstrap pointing at the installed `SKILL.md`. Claude's context event,
Codex's additional SessionStart context hook (including compaction), OpenCode's system-prompt
adapter, and AGY/Cursor's rules deliver that bootstrap through their existing context surfaces.
The Codex context command runs `argus-hook instructions`, which prints the inherited message
without contacting the daemon or interpreting it as shell code. Its command string is stable
across panes, boots, and changes to skill content; its separate session-identity event keeps its
existing matcher, so compaction does not reset pane status.

A generic or custom harness without `skill_dir` receives compact fallback guidance in
`ARGUS_INSTRUCTIONS`, covering context, titles, status, and checkout isolation. Custom harnesses
can set `skill_dir` to a checkout-relative directory to opt into the same package. Missing,
incomplete, or conflicting packages also use the fallback. A user-owned skill or reference is
never overwritten; a symlinked skill directory or file is left alone. Managed files carry an
ownership marker and are removed with the hooks after the last agent leaves, on checkout moves,
or during startup cleanup; unrelated files remain. After moving, agents resolve the skill's
references in the new checkout. Removing the marker makes a replacement file user-owned.

Titles arriving from a model are flattened to one line and cut to 48 characters. Neither a rename,
status report, nor checkout move can touch a pane that has exited. A renamed row keeps showing its
template on its second line. The skill directs agents to use linked worktrees for branch changes,
since switching a shared checkout in place changes files and HEAD for every pane using it.

## Session restore

The store records each pane's checkout path, kind, title, status, note, and optional harness
session ID and harness name when the daemon
broadcasts a structural tree change. The harness is the one the pane actually ran under rather than
whatever its template names now, since that is who wrote the conversation a restore claims; a record
without it falls back to the template. Recording is suppressed while a restore is in flight, so the
panes it is starting do not rewrite the rows it is reading.

On daemon startup:

- linked worktrees are reconciled against Git first, because only primary checkouts come from the
  config and a pane in a worktree would otherwise look like a pane whose checkout is gone;
- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells;
- the saved display title, status, and note are reapplied after each pane starts;
- an agent with an ID starts with its harness's `resume_id` argv template expanded to that exact ID;
- every identified pane resumes independently;
- records without an ID retain broad `resume`; one legacy pane per checkout and harness claims it,
  so different template aliases of one harness cannot reopen the same last conversation;
- an agent started that way that exits non-zero within five seconds is taken to have had nothing to
  continue, and is replaced by a plain new agent in the same checkout;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

Exact templates are Claude `--resume {session_id}`, Codex `resume {session_id}`, OpenCode
`--session {session_id}`, and AGY `--conversation {session_id}`. Their legacy broad forms are `--continue`, `resume --last`, and
`--continue`; `generic` has neither. A `[[harness]]` block sets `resume_id`, `resume`, and an
event-level `session_id` stdin JSON key. Replacing a built-in gives up all of its defaults.

This is relaunch, not process reattachment. PIDs are not stored. Captured IDs are nonempty, bounded,
and control-free, and enter the child command as argv rather than shell interpolation. Exited panes
are omitted. Each recording replaces the pane table in one transaction, because the tree is the
truth and the table follows it: a pane that closed has no row to update.

## Git and checkouts

Read-only Git work uses `git2`. At daemon startup only HEAD is read for each checkout, so the first
client gets branch names without a workdir walk of every repository under a project root. Every two
seconds, on a blocking-pool thread, the daemon then refreshes:

- branch or detached-HEAD state;
- dirty state and changed-file count, including untracked files;
- ahead/behind against the tracking branch;
- linked worktrees added or removed outside Argus.

The first of those ticks is immediate, overlapping session restore, so dirty counts follow by the
time the tree has been on screen a moment.

On a slower ten-second beat it also rescans each project root for repositories added or removed
there. Both run on the blocking pool.

Alongside the poll, each repository's Git metadata is watched with `notify`, so a branch switch, a
commit, or a worktree made in a shell reaches clients as it happens rather than up to a tick later.
The watched set is the Git directory itself (HEAD, index, packed-refs) non-recursively plus its
`refs` and `worktrees` trees — never `objects`, which takes thousands of writes per commit for a
change `refs` reports once. Events are coalesced for 150 ms, then run the same reconcile, status,
and branch refresh the poll runs. The watched set is re-derived on the ten-second beat, since
repositories come and go. The poll is not replaced: editing a file touches nothing under `.git`, so
dirty state and changed-file counts still come from the sweep, and a platform where the watch cannot
start logs and leaves the poll on its own.

Status is cached on the checkout rather than read when a tree is snapshotted. A snapshot is taken
under the daemon's one lock, and a keystroke needs that same lock to find the pty it belongs to, so
reading git there put several milliseconds of blocking I/O per checkout in front of the next key —
on every structural change, not just on the poll. The refresh collects paths, reads git with the
lock down, and stores results back by checkout id, dropping any whose checkout moved meanwhile. A
branch switch, a new worktree, and a scan that found new repositories refresh what they changed, so
a row does not name the branch it just left for the rest of the tick; anything changed outside Argus
waits for the poll.

A status that could not be read is unknown, not empty. `git switch` in another terminal rewrites
HEAD, and a poll landing in that window used to report a checkout on no branch, which is how a
detached HEAD reads: the row fell back to the directory the worktree was created as, and the branch
it was really on turned up in the free-branch list, rearranging the column under the user. A failed
read now leaves the cached status alone, and only a genuine detached HEAD reports no branch. A
repository with no commits yet is a settled answer rather than a failure, and reports one too.

The checkouts column is ordered around the repository's main branch, which leads it whether that
is a checkout sitting on the branch or a row offering one. Which branch that is comes from
`origin/HEAD` where the remote has said, and from the conventional names only where nothing ever
set it — so a repository whose trunk is called something else still gets the same treatment.

The repository's other local branches — the ones no checkout is sitting on — are cached on the same
poll but stay out of the column until `B` asks for them: the column is for what is running, and a
repository with forty branches would bury the two checkouts that are the point of it. Reaching a
branch that has no row is what the `b` picker is for. Expanding also shows the branches that exist
on a remote and nowhere here, as `origin/feature`: what the last fetch turned up.

On any branch row, Enter switches the primary checkout to it, `n` gives it a worktree, and `D`
deletes it — `git branch -d` in the primary checkout, so the deletion is local, never pushed, and
refused while the branch holds commits nothing else does. That refusal is the one the user has an
answer to, so it comes back as a second confirmation rather than an alert: an unmerged branch is
reported as such, the popup reopens asking whether to delete it anyway, and yes reruns the deletion
as `git branch -D`. Every other refusal is still an error, because forcing would not have helped
it. The main branch is refused outright, and so is a remote-only row: deleting one of those would
be a push, which nothing in this column does.
A remote-only row answers to the local name it would take, so Enter and `n` name `feature` and let
git take it from `origin/feature` — a worktree for one starts from the remote branch rather than
from this checkout's HEAD.

`F` fetches every remote and prunes, and `P` pulls the selected checkout fast-forward-only; a
branch row runs both in the repository's primary checkout, having none of its own. Both refresh the
tree on the way out rather than waiting for the poll, since a fetch that appears to have done
nothing for two seconds reads as a fetch that failed. A merge that needs a decision is git's to
refuse, and its message is what the user sees.

Branch and file pickers run in process. Branches are local branches, current first. File discovery
uses `ignore`, follows Git ignore rules, and caps the result at 50,000 files.

Git mutations use the `git` executable:

- switch to an existing branch;
- create and switch to a branch;
- add a worktree, creating the branch unless it already exists;
- delete a local branch, refusing an unmerged one until the user confirms the forced deletion;
- fetch every remote, and pull one checkout fast-forward-only;
- force-remove a linked worktree and best-effort delete its branch;
- `git init` a repository that does not exist yet.

A root scan skips `.git`, `.argus`, `node_modules`, and `target` for every project. `exclude` adds to
that and `include` overrides it, both taking either a bare directory name, matched anywhere under
the root, or a root-relative `/`-separated path, matched once; `include` wins over both `exclude`
and the built-in list, so a repository kept where the defaults would never look is still reachable.

A project may set `worktree_root`, which holds one directory per repository, so two repositories in
one project can have a branch of the same name. Its `setup` commands run in a worktree Argus has
just created, in order, parsed into arguments without a shell like the editor command; the row is
broadcast before they run, and a failure is reported without removing the worktree.

A checkout may hold several agents. That is shown — a warning glyph on the row, and "shared by N"
where the column is wide enough — rather than prevented, unless the project sets `exclusive`, which
turns a second fresh agent in one checkout into a refusal naming the one already there. Shells never
count, and restoring a session is exempt: those panes were already running together.

Argus-created worktrees live under `<primary>/.argus/worktrees/<branch>` by default; a branch name that is not
a plain path component is refused before it becomes a directory, and a name starting with a dash is
refused before it reaches a command line. Removing a worktree decides what git would refuse — a
locked worktree, a path that is not a linked worktree of this repository — before killing the
checkout's panes, because the panes have to die first for the directory to be deletable on Windows
and a refusal afterwards would cost them for nothing. A registration whose directory is already
gone is pruned instead.

Switching a dirty primary checkout is refused, and the refusal names the worktree alternative: git
carries uncommitted changes across a switch whenever they do not conflict, which moves work off the
branch it was done on. Linked worktrees are Argus's own and switch under Git's rules alone.

## Review

`R`, `Tab`, or leader-Tab requests a checkout review. The daemon computes the diff on a blocking
task with `git2`; the client opens it in a floating overlay while leaving the terminal behind it
subscribed.

Review shows uncommitted work split into the two sides Git itself keeps apart. `b` toggles:

- `unstaged`: the index against the working tree, plus non-ignored untracked content — `git diff`.
- `staged`: `HEAD` against the index — `git diff --cached`.

The chosen side is a setting rather than a per-visit choice, and it survives closing and reopening
the overlay.

A third snapshot is one commit against its first parent, reached through the history overlay rather
than the side toggle. `H` lists the newest 100 commits on the checkout's HEAD — identities only,
which is one revwalk and no diffs at all. Naming the files a commit touched means diffing it
against its parent, so that is asked for one commit at a time, when `l` drills into its row, and
kept while the overlay stays open; `h` folds a commit back up before it closes the overlay.
Drilling into a commit that is already unfolded, or into one of its file rows, asks for that commit
as an ordinary review, so a commit diff is the same viewer, the same navigation, and the same
comment path as uncommitted work, with the comment's anchor recording which commit it was made
against. `h` from a commit review returns to the list it was opened from; escape closes both. An
unborn branch is an empty history rather than an error. Comparing a branch against its fork point,
or against a remembered snapshot, remains out of scope: that is what a Git client is for.

Each request captures the index as a tree, and for the unstaged side also an immutable synthetic
tree built from the index plus working-tree deletions, edits, and non-ignored untracked content.
Both are written to Git's object database without changing HEAD, branches, the index, or files.
Diffs use three context lines, preserve old and new line numbers, and detect renames. Binary files,
files over 1 MiB, files over 5,000 rendered lines, and content beyond the review's 20,000-line
budget are listed without their content. Capture and diff failures are reported rather than
rendered as empty work.

Diff lines carry syntax spans, produced by tree-sitter in the daemon. The daemon parses because
the daemon is the side holding whole blobs: the client is sent hunks, and a parser given a bare
hunk reads a fragment torn out of its syntax. Each side is parsed from its own tree, so a removed
line is read in the file it was removed from rather than the one that replaced it. A span carries
what a token *is* — keyword, string, comment, number, type, function, constant, property,
operator, punctuation — never a colour, so the client's theme keeps the palette. Identifiers are
deliberately untagged: colouring every name buries which lines changed.

Ten grammars link in and are chosen by file extension: Rust, TypeScript, TSX (which also serves
JavaScript), Python, C#, CSS, YAML, TOML, JSON, and Markdown. Both TypeScript grammars are
configured with the JavaScript query concatenated underneath their own, which is only its
additions; without it they parse correctly and highlight nothing. Anything else is plain text, and
so is a file over 512 KiB, an unreadable blob, or a parse that fails — highlighting is decoration
and never costs a review. The client validates every offset against the line before slicing it.

`s` flattens the same diff the other way. Unified gives every diff line a row; split pairs a
hunk's removals against the additions that replaced them, one row holding both sides, and ends a
run at each context line because that is where the two sides are known to line up again. Where a
run of one side is longer than the other, the surplus rows leave the far side empty and recessed.
Nothing is asked of the daemon: the rows are rebuilt from the hunks the client already holds. Each
side is ellipsized at its own half so one long line cannot push the other off the row, and each is
numbered from its own tree — the old file on the left, the new one on the right. The cursor stays
on the line it was on across a toggle, a half-made range is dropped, and the choice is a setting
that persists like the side toggle. A row holding both sides anchors a comment to both, which is
the anchor the unified view produces for the same two lines selected together.

Every request has an id and the client accepts only the latest exact reply. Review capture is
globally serialized, and a connection drops an older queued capture when a newer request replaces
it.

The client supports line and file navigation, single-file range marking, a changed-file fuzzy
picker, refresh, and opening the selected line in an editor. A comment records the review side,
paths, separate old and new ranges, quoted diff text, and body. The client chooses among the live
agents in the checkout when necessary. The daemon validates that recipient, persists the comment
under the checkout path, then sends a flattened one-line notification to the recipient's PTY. A
failed PTY write does not discard the stored comment. Live agents in the checkout can read the
newest 100 comments with `argus-hook comments`.

Added and removed lines are marked by a background wash rather than by foreground colour, which
syntax now owns; the `+` and `-` markers keep their own colour so the signal survives a terminal
that drops backgrounds. Selecting a line brightens its wash instead of replacing it, so a
selected range still shows which side each line was on.

Review is a viewer. There is no vetted state, no stage/unstage/revert action, and no
comment-resolution lifecycle. Closing the review sends no daemon message.

## Notes

`m` opens a note on whatever the spine has selected: the project when focus is in the projects
column, otherwise the checkout, falling back to the project on a branch row that has no directory
of its own. Notes are plain Markdown the user owns; the daemon stores the text and reads exactly
one construct out of it.

That construct is the checkbox line — GitHub's task list grammar plus one state. Optional indent, a
bullet (`-`, `*`, or `+`), a space, `[`, one state character, `]`, and then end of line or a space
before the text:

- `- [ ]` open: outstanding, and the only one of the three that is a claim on anybody.
- `- [x]` done.
- `- [!]` pinned: a standing instruction, meant to be read by an agent without being asked for.
  Ticking a box never unpins one, since a pinned item was set deliberately.

Counts are derived from the body on every read rather than stored beside it, and ride the tree: a
checkout row shows its own note's counts, a repository row sums its checkouts, and a project row
sums everything beneath it plus its own note. A row with a note but nothing to count still says so,
because "nothing open" and "nothing written down" are different answers.

The window is modal, because a note is read and ticked far more often than it is written. View mode
navigates with `j`/`k`, `0`/`$`, and `g`/`G`, ticks the box under the cursor with `space`, and
starts typing with `i`, `a`, or `o`. Insert mode is text, and `Esc` leaves it and saves. Closing
with `q` saves too; nothing goes out mid-word.

`f` forwards the current line and `F` forwards the whole visible note. A checkout note can reach
only a live agent in that checkout; a project note can reach any live agent in that project. One
eligible agent is chosen directly and several open a recipient picker. The client sends the
visible text, including unsaved edits, and preserves its Markdown and whitespace. The daemon
checks the scope and live-agent status again, then uses bracketed paste without Enter, so the text
remains editable in the agent's prompt rather than silently commanding it.

Ticking sends the line number and the new state rather than a body, because the counts a client is
acting on arrived with the tree rather than from opening the note, and a whole-body write from a
stale copy would discard whatever an agent wrote in the meantime. The client sends its own text
first when it has unsaved edits, so the line number means what the daemon thinks it means. Every
write is answered with the stored note, so the editor shows what the daemon holds rather than what
it predicted; a note arriving while there are unsent edits is ignored in favour of them.

Notes are stored in `runtime.db` under durable identity — a project by its name, a checkout by its
path — because ids are handed out fresh on every start. Saving an empty body deletes the note;
there is no separate delete. A note over 64 KiB is refused.

A live agent reads those notes with `argus-hook context`, which posts to `/pane/<id>/context` on
the same loopback pane API its status reports use. What comes back is its project's note and its
checkout's note, in that order, each with its body and its parsed checkboxes; an unwritten or blank
note is left out rather than sent as an empty document. Scope is the point. The pane the request
arrives on decides which two notes exist for it, so there is no target to name and nothing else to
reach, and the caller must be a live agent pane — the same rule review comments are read under, and
for the same reason: pane ids are runtime-only, so anything durable keys off the checkout directory.

The helper prints the pinned lines once above the bodies they came from. That repetition is
deliberate: `- [!]` is Argus's spelling rather than Markdown's, and an agent reading a note cold has
no reason to know it means "standing instruction".

Writes are the same surface narrowed. `argus-hook todo add "<text>"` appends one open checkbox to
the *checkout's* note, and `argus-hook todo done <line>` or `todo open <line>` moves an existing
one, by the line number a person reads off the note. Three things gate it, cheapest first: the
caller must be a live agent pane, the project must carry `agent_todos = true`, and the change must
be one the note can take. The default is off, because a note is what the human wrote down.

Neither write can reach the project note — there is no target to name — and neither can touch a
`- [!]` line: ticking off a standing instruction would delete the instruction rather than complete
a task, and pinning is a human's judgement about what an agent should read. An item's text is
folded to a single line, so one claim cannot arrive as several checkboxes.

The change and its record commit together. Each record holds the time, the harness session that
asked, what it did (`add`, `done`, `reopen`), and the item's text; the twenty most recent travel
with the note whenever a client reads one, and the note window's footer names the last of them. The
records outlive the note, since a note being emptied is exactly when "who wrote that" is worth
answering. A note already open in a client does not yet redraw when an agent writes to it — the
counts on the tree do, and reopening the note shows the change — because a stored note is answered
only to the client that asked for it. Unlike every other pane-API write, this one answers in prose — the agent has to be able
to tell the user it was refused, and "this project does not allow it" is a different situation from
"that line is not a checkbox".

Not yet implemented: pinned-note injection into a template's prompt, `argus ctx`, and the feature
board's write path from the client (TARGET.md, "Boards"). Features, the decision board and the
columns view have landed (see "Features and the decision board"); what is missing is moving a card
from the view rather than only over the pane API, and the scoped read an agent starts work from.

## Features and the decision board

Work is scoped to a **feature**: a short document saying what is being built, with the decisions
taken while building it hanging off it. The board used to be one tree per project, which answered
"what has this project ever decided" — a question nobody asks. What an agent picking up work needs
is the handful of choices made about the thing it is about to touch, and everything else on a
project-wide board is noise it reads past. So a decision is filed under a feature, and a board is
read one feature at a time.

A feature is stored, not derived: schema v6's `feature` table holds a slug, a title, the document
body, and the checkout and branch it originated in. The slug is derived from the title once, at
creation, and made unique by suffix inside the transaction — a title someone later rewords must not
orphan the decisions under it, and two agents opening the same-sounding feature on two branches must
not silently share a board. The document is the one part that is edited rather than appended to as
a tree: it is prose both sides write, bounded at 8 KiB, because a brief that has outgrown a screen
has become the design document it was meant to point at.

The two sides write it differently, and deliberately. An agent *appends* — `argus-hook feature note`
adds a paragraph — because an agent adding to a brief mid-task should not be able to erase what it
is working from. A human *replaces*: `e` on a feature, from either board, opens the brief in the
note editor and `ClientMsg::SetFeatureBody` writes back whatever it says. It is the same editor a
note gets because it is the same job, prose a person reads and corrects, and a second editor for a
second kind of document would only be somewhere for the two to drift apart. What the editor will
not do to a brief is tick a checkbox or forward it: a brief's `NoteTarget` points at the project so
nothing reading it has to branch, so both are refused rather than quietly writing into the project's
note — and the work a brief might have listed lives in its tasks now. The decision view draws the
brief above the tree, bounded to a third of the panel, which is the order `argus-hook feature`
prints them in and the order they are read.

Which feature an agent is on is resolved from the checkout, not from a flag. `feature_scope` maps a
checkout path to a slug, and a checkout that was never pointed anywhere falls back to the one
feature that originated there — which is what makes worktree-per-feature need no ceremony. The
fallback deliberately gives up when a checkout has two features to its name: guessing would file a
decision under whichever happened to be older, which is worse than asking. `decide` from a checkout
on no feature is **refused**, in prose that says how to open one, because a decision nobody can find
again is the pile this scoping exists to end.

Agents reach it through four pane API endpoints. `argus-hook feature` reads the current feature —
its brief, then its decision tree, as one answer, since a decision without what the feature is for
explains half of itself. `argus-hook feature list`, `feature open "<title>"`, `feature use <slug>`
and `feature note "<text>"` are the writes. `argus-hook decisions` reads the same tree alone, and
`argus-hook decide "<chose>" [--over ...] [--because ...] [--under <id>] [--supersedes <id>]`
appends to it, answering with the id the next decision hangs off.

The board itself is append-only. Nothing is ever edited, and there is no delete. A decision that a
later finding invalidates is *superseded*: the replacement is a new row that takes the old one's
place in the tree — its parent, not its children — and the old row's `superseded_by` records what
replaced it. The old node stays on the board and the view draws it dimmed, because the road not
taken is most of what a reader came back for. Decisions recorded before features existed keep a
NULL `feature` and are reported as unfiled rather than dragged under a feature nobody chose.

There is no policy flag on these writes, unlike note writes. A note is the human's document and an
agent writing to it needs permission; the board exists for agents to write, is append-only, and
attributes every row, so there is nothing for a gate to protect.

Clients read the whole project — every feature and every decision — with `ClientMsg::GetDecisions`
and are pushed `ServerMsg::Decisions` whenever any board changes, since a tree is meant to be
watched being built and the daemon deliberately does not track which view a client has open; a
client with another project open drops it by name. `DecisionBoard::scoped` is what narrows that to
one feature in the client, so switching scope costs no round trip. The board view is two columns: the project's features on the
left, and the tree of whichever one is selected on the right, drawn two lines per decision with
branch rails and elbows connecting every child to its parent. `argus-hook decisions` draws the same
topology for agents. `h`/`l` cross between the columns and `j`/`k` move through whichever has the
keys, a click selects the row it lands on, and `r` re-asks by hand. Decisions from before features
existed get a row of their own at the end of the feature column rather than being hidden.

### The feature board

The third view draws every feature of the project in a column by state — proposed, active, blocked,
submitted, done — which is the "what is in flight" question the decision view does not answer. It is
its own view rather than a mode of the decision view because the two are read for different reasons,
and a toggle would hide whichever one you were not on. It reads the same pushed `DecisionBoard`,
which already carries every feature with its state, so switching between them costs no round trip.

The columns are equal width. What is worth width here is whichever column is full, and that changes
hour to hour — a layout that tracked it would move the columns around under a reader who is using
their position to find them. `h`/`l` cross columns and `j`/`k` walk the cards in one, the same
gesture the spine takes, and `Enter` opens the decisions under the selected card: you see what is in
flight, then ask why. Moving between columns lands on the first card rather than carrying the row
across, since the same depth of an unrelated column is not where you were looking.

A card's second line answers the question its column raises — a blocked card says why, a submitted
one says what was offered, and one nobody has picked up says the branch it was cut on.

A feature is opened from the board with `a`, renamed with `R`, and removed with `x` — the board is
where a person writes down work, so it cannot be a surface only agents can add to. A feature opened
here records no origin checkout, because work a person wrote down has not been cut anywhere yet;
whichever agent picks it up says so with `argus-hook feature use`, and that is when it gains a home.
A rename changes the title and freezes the slug: every decision row, task row and `feature_scope`
entry points at the slug, so re-deriving it from the new title would orphan exactly the work the
feature is about. Removing one takes its tasks, events and checkout scope with it and **unfiles its
decisions** rather than destroying them — the board is append-only because it records what was
believed at the time, and that outlives the feature it was believed about, so those decisions
reappear on the "before features" row.

`H` and `L` move the selected card a column along, over `ClientMsg::MoveFeature`, and the selection
follows it there — a move you have to go looking for reads as having lost the card. The move is
applied to the client's own copy at once and the pushed board is what actually redraws it, so a
refusal puts the card back where it really is. `s` sends a card back to whoever is on it, and has a
key of its own because it is the one human verb the layout does not teach: accepting is a step right
from `submitted`, but sending back is two columns left, and stepping through `blocked` on the way
would post a blocker nobody claimed. The human write names the feature outright rather than being
resolved from a checkout, since the board is the project's and the checkout a reader happens to have
selected has nothing to do with the card under the cursor.

Schema v7 holds this: `state`, `claimed_by`, `claimed_at`, `blocker` and `evidence` on `feature`,
and a `feature_event` row for every move. The state is a column because the board is read whole on
every change; the moves are a table for the reason `note_audit` is one, since a column reading
`submitted` cannot say who submitted it or what they claimed. `blocker` and `evidence` are cleared
on the way out of their states, so a feature that was unblocked stops explaining why it was stuck,
and the claim is taken entering `active`, kept through `blocked` and `submitted` — a stuck feature
still belongs to whoever carried it there — and released on `done`.

`submitted` and `done` are separate states and an agent is refused the second: the review column has
to be where work stops rather than somewhere it passes through, and the agent that did the work is
the one party that cannot accept it. So the two write paths differ in what they may reach rather
than in what they do — an agent moves its own work over `FeatureAction::Move`, scoped to the feature
its checkout is on, and a human moves any card over `ClientMsg::MoveFeature`, acceptance included.
Every move is recorded with which side made it, so a column reading `done` says who accepted it.

### Tasks

A feature says what is being built and its decision tree says why it is being built that way.
Neither says what is *left* to do, which is what a human actually hands an agent. So a feature
carries a list of tasks, drawn as a board of its own: `Enter` on a feature card goes into it, and
the columns are todo, doing and done.

A task is a row, not a checkbox line in the feature's document. The note checkbox already has three
states and an agent write path, but it is addressed by line number, and a line number moves whenever
the text around it is edited — which is the one thing a board cannot take, since a card has to stay
the same card while a human rewrites the list. Schema v8's `task` holds the title, the column, the
claim, a `position` and an `external` key.

`external` is whatever key the team's tracker uses. Argus stores it and knows nothing else about it:
an agent with access to Jira, Linear, GitHub Issues or a spreadsheet is what puts tasks here, which
is why Argus works the same with any of them and needs credentials for none. `argus-hook task add
"<what to do>" --key ORION-412` is the whole of the integration.

Both sides write, and here they write the same things — there is no acceptance step and so no move
either side is refused. That ceremony belongs to the feature the tasks are under, which is where a
human accepts the work as a whole. An agent reads with `argus-hook task` and writes with `task add`,
`task doing <id>`, `task done <id>`, `task todo <id>`, `task retitle <id> <text>` and `task drop
<id>`; the columns are named as verbs rather than hidden behind a `move`, so what an agent types is
what a reader of the transcript understands happened. Taking a task up is what claims it and
finishing it is what releases it, so the doing column always says who is on each card without
anyone claiming by hand.

Ids are project-wide and an agent numbers its tasks from what it last read, so every agent-side
change is refused when the task is not under the feature its checkout is on: a stale id would
otherwise let one feature's agent tick off another's work by arithmetic.

From the view, `H`/`L` move a task a column and `J`/`K` move it earlier or later in the list — the
order is a human's statement of what to do first, so it is theirs to set and there is no agent-side
equivalent. `a` starts a new task and `e` rewrites the selected one.

Both boards type on the same line — one `LineInput`, not one per board, because adding a feature and
adding a task are the same gesture on the same kind of surface and two of them would drift. It takes
a row off the bottom of the view rather than floating over it, so what you are writing and what is
already there stay readable together, and while it is up it swallows every key: a title with an `x`
in it does not delete the card behind it, and the first `Esc` puts the line away rather than the
view. The prompt names itself — `new task`, `rewrite`, `new feature`, `rename` — which is the whole
of the affordance, since there is no other cue that the keys have changed meaning.

Lists are pushed whole on `ServerMsg::Tasks` whenever one changes, the way a board is, and a client
holding another feature's list open drops it by name.

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
