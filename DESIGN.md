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

The daemon's state is one `Daemon` type behind a small set of mutexes, but it is not one file.
`state.rs` holds the tree itself — the types, the workspace and session handling, and the snapshot
clients render — and four sibling modules hold `impl Daemon` blocks for the concerns that only touch
it: `state/panes.rs` for pane lifecycle, sizing, and what an agent reports about itself;
`state/sync.rs` for the polls and watchers that keep the tree level with the disk; `state/git_ops.rs`
for the writes to Git; and `state/hook.rs` for the loopback receiver. The type and its locking are
unchanged by the split — only which file a concern is read in.

The pane API's URL grammar lives in `argus-protocol` rather than in the daemon, because
`argus-hook` builds the paths the daemon parses. Written once, a new endpoint cannot compile on one
side and fail at runtime against the other.

## Navigation model

The runtime hierarchy is:

```text
Workspace scope -> Project -> Repository -> Checkout -> Pane
```

A workspace is a daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The TUI draws project, repository, checkout,
and pane columns followed by the selected pane's terminal. While typing in a pane, `Ctrl-Space`,
`f` lets its terminal take the main content area; repeating the chord restores the columns. The
status bar remains visible in both layouts.

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
`/pane/<id>/checkout` with a known checkout path. `POST /pane/<id>/delegate` accepts a JSON task and
optional agent template, opens an independent agent pane in the source pane's checkout, and returns
the new pane ID. The task is flattened to one line and limited to 2 KiB. Delegation requires a live
agent source, honors project exclusivity, and is refused when the checkout already has four live
agents. A malformed body returns 400, a policy or launch refusal returns 409, and success returns 201.
`POST /pane/<id>/handoff` applies the same source, template, checkout, and live-agent rules but accepts
up to 32 KiB of context. The daemon flattens it to one line and delivers it to the new pane's PTY as
the fresh harness's first message. This keeps the handoff in memory and needs no harness-specific
startup flag.

A first message is not written the moment the pane is spawned. The harness process is seconds from
having a prompt, and a CLI that has not reached one swallows what is typed at it: the text would
survive in the PTY buffer and turn up in the input box, but the Return that submits it would not,
leaving a pane holding a task it never ran. Delivery therefore waits for the pane to report
bracketed paste mode — the one thing every agent TUI turns on when it is reading keys, and so a
readiness signal that needs no harness knowledge — then pastes the message and sends Return
separately. It gives up waiting after 30 seconds and sends anyway, because a message that might be
missed beats one never sent. Return is repeated up to three more times at two-second intervals while
the pane is still idle, since a pane going to `working` is the only report that the prompt was
actually submitted. The new pane's row is named after the delegated task, or `handoff`, until the
agent reads it and renames itself, so a pane that opens says what it was opened for.
`POST /pane/<id>/comments` requires a live agent source and returns the newest 100 durable review
comments for that pane's checkout as JSON, oldest first. The checkout comes from the pane rather
than request input, so callers cannot select another checkout through this endpoint.
It broadcasts the normal tree update but does not focus the pane in any attached client. The checkout
endpoint changes affiliation only: the agent runs `argus-hook checkout` from the directory it has
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

Agents name their own rows. At session start a harness with a `context_event` is handed
instructions telling it to run `argus-hook title "..."` once it knows what it is working on, and
`argus-hook status waiting "..."` when it needs a human, `needs-review` when work is ready to
inspect, and `done` once reviewed and complete. The instructions also ask it to run
`argus-hook checkout` after moving to another checkout. The same text is in `ARGUS_INSTRUCTIONS`,
which is where a plugin harness picks it up — OpenCode's module appends it to the system prompt.
Titles arriving from a model are flattened to one line and cut to 48 characters. Neither a rename,
status report, nor checkout move can touch a pane that has exited. A renamed row keeps showing its
template on its second line, so a column of agents that have all named themselves still says which
CLI each one is. Because panes may share a checkout, the standing instructions prohibit agents from
switching that checkout's branch in place. An agent that needs another branch creates a linked
worktree, continues from its path, and reports that checkout move. This keeps one agent's branch
choice from changing the files and `HEAD` seen by every other pane in the original checkout.
The instructions also expose `argus-hook delegate [--template NAME] "task"` for bounded code or
document reviews. A delegated pane is a peer, not one of the nested harness sessions described above.
It inherits the source template by default and shares the checkout's files; read-only review is a
prompt convention rather than an enforced sandbox, so editing work that needs isolation belongs in a
separate worktree.
For full-session continuation, the instructions expose `argus-hook handoff [--template NAME]`, which
reads the handoff from stdin and starts a fresh peer pane without writing the document to the checkout
or a temporary file.

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

Read-only Git work uses `git2`. Every two seconds, on a blocking-pool thread, the daemon refreshes:

- branch or detached-HEAD state;
- dirty state and changed-file count, including untracked files;
- ahead/behind against the tracking branch;
- linked worktrees added or removed outside Argus.

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
refused while the branch holds commits nothing else does. The main branch is refused outright, and
so is a remote-only row: deleting one of those would be a push, which nothing in this column does.
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
- delete a local branch, refusing an unmerged one;
- fetch every remote, and pull one checkout fast-forward-only;
- force-remove a linked worktree and best-effort delete its branch.

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

Review shows uncommitted work only, split into the two sides Git itself keeps apart. `b` toggles:

- `unstaged`: the index against the working tree, plus non-ignored untracked content — `git diff`.
- `staged`: `HEAD` against the index — `git diff --cached`.

The chosen side is a setting rather than a per-visit choice, and it survives closing and reopening
the overlay. Committed history is out of scope: reviewing a branch against its fork point, or
against a remembered snapshot, is what a Git client is for.

Each request captures the index as a tree, and for the unstaged side also an immutable synthetic
tree built from the index plus working-tree deletions, edits, and non-ignored untracked content.
Both are written to Git's object database without changing HEAD, branches, the index, or files.
Diffs use three context lines, preserve old and new line numbers, and detect renames. Binary files,
files over 1 MiB, files over 5,000 rendered lines, and content beyond the review's 20,000-line
budget are listed without their content. Capture and diff failures are reported rather than
rendered as empty work.

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

Review is a viewer. There is no vetted state, no stage/unstage/revert action, no syntax
highlighting, or comment-resolution lifecycle. Closing the review sends no daemon message.

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
