# Argus: Current Design

Argus is a terminal workspace for running shells and AI command-line agents against Git
checkouts. This document describes the code as it exists today. Desired behavior lives in
[`TARGET.md`](TARGET.md); unfinished work lives in [`ROADMAP.md`](ROADMAP.md).

## Process model

Argus has three binaries:

- `argus`: the ratatui/crossterm client, and `argus web`, which serves the phone client.
- `argusd`: the daemon that owns PTYs, terminal state, Git state, and runtime persistence.
- `argus-hook`: the helper managed hooks run, and the command an agent runs to report on itself.

The client starts the daemon lazily when it cannot connect. On Unix the daemon process calls
`setsid`; on Windows it starts in a detached process group. Closing the client does not stop the
daemon or its child processes. Before restoring panes or starting its hook server, the daemon holds
an OS-level instance lock and binds the endpoint without replacing a live socket. A second start
exits without restoring panes; a Unix socket pathname is removed only after a bind failure proves
that no daemon can connect to it.

`argus server restart` is the explicit clean replacement path. The client sends a protocol control
message, waits for the daemon to flush its acknowledgement and release the endpoint, then starts
the replacement through the same lazy-start path. The daemon removes managed hooks before exit;
the replacement restores non-exited panes from the runtime session store.

`argus server stop` uses a separate control message. The daemon acknowledges the request, tells
every connected client to exit so their reconnect loops cannot launch a replacement, removes
managed hooks, and releases the endpoint. The next ordinary `argus` launch starts the daemon and
restores non-exited panes from the runtime session store.

The daemon is started with its stderr on the null device, since it shares no console with the
client and anything it printed would either land in the middle of the TUI or open a console window
of its own. So it logs to a file beside its config as well, keeping the previous run's log — a
daemon that is starting has often just stopped in a way somebody wants to read about. `RUST_LOG`
sets the level, `info` by default. Failing to open the log is never a reason not to start.

The daemon listens on a Unix socket or Windows named pipe. Messages are named MessagePack
records framed by a four-byte big-endian length. Frames larger than 64 MiB are rejected.
Several clients may connect at once and each connection may subscribe to several pane screens.
There is no transport authentication.

A message a side cannot decode — one a newer peer added — is skipped, and the connection carries
on. The whole frame is read before it is decoded, so the stream is still aligned; only a frame over
the size cap, which is never read, or an I/O error ends a connection. Every reader goes through
`read_known_msg`. Peers from before this skip unknown messages by hanging up, so nothing new may be
sent to one.

What is new is negotiated by a greeting, shaped around never sending an older peer a message it
cannot read. The client's first message is `Hello`: a protocol number, its version, and the optional
messages and encodings it can take, by name. The daemon sends its opening messages — tree,
templates, workspaces — as it always has, and answers a `Hello` with its own whenever one arrives;
it never sends one unasked, so a client that does not greet is never sent it. The client reads on
until the answer, keeping what came first for the app. A daemon from before the greeting hangs up on
it right after its opening messages, and the client connects again without greeting. One that takes
the greeting and says nothing for three seconds is kept, with nothing optional assumed. Either side
uses an optional message or encoding only when the other listed it, and the protocol number is
raised only for a change nothing can be negotiated across.

A client told of a daemon from another build says so on the status bar, with both versions and
`argus server restart`, which starts the daemon installed beside the client. A daemon on another
protocol number is an alarm rather than a note.

A client can attach to the daemon on another machine as well as its own. `argus --host <host>`
runs `ssh -T -o BatchMode=yes -- <host> argus bridge`, and the bridge on that machine carries its
daemon — started there the way any client starts one — on ssh's stdin and stdout. Everything the
daemon does stays beside the agents it runs: git, hook installs and the loopback hook server need
nothing new. ssh is in batch mode, keys or an agent only, because the terminal is the client's and
ssh has nowhere to ask; when it fails, what it printed becomes what to do — a key to add, a host
key to accept with `ssh <host>` in a terminal, argus to install there. Connecting never installs
argus on the other machine, as VS Code Server does: a client that drops binaries on another host
looks like malware to antivirus, and installing software is the user's act, so a missing or older
argus there is an error that says so. The `--` keeps a host name from passing for an ssh option,
and keepalives notice a link that died quietly within a minute.

Each daemon is a host with an app of its own — its tree, grids, selections and connection — rather
than one app with every id qualified by the daemon it came from, since hosts sit a level above
workspaces and one is on screen at a time. The host on screen is drawn and takes input; the others
keep their connections. A host that loses its daemon reconnects the way it first connected, and a
remote daemon from another build is named along with the host to update.

`W` opens the host picker, a level above the workspace picker. It lists this machine first, then
the hosts attached, then those this client has reached before — remembered in `client.toml` — and
the plain `Host` names in `~/.ssh/config` and the files it `Include`s; patterns name no host anyone
could pick and are left out. The host on screen and the attached ones are marked. Choosing an
attached host puts it on screen; choosing any other, or typing a name nobody listed, connects to it
in the background, puts it on screen once connected, and remembers it, while ssh's failure lands on
the status bar. The product mark names the host on screen when it is not this machine. A host off
screen holds no pane subscriptions — nothing draws its grids, and a remote one would stream them
for nobody — and takes them back, sizes and all, when it returns. `client.toml`'s list of hosts is
written only when a host is remembered: each host's app holds a copy of the settings, and saving a
theme from one keeps the list on disk rather than its own. On a host elsewhere an editor always opens in a floating
pane: an external editor would be launched on that machine, where nobody is looking, and this
machine's editor command is left out so the daemon there picks its own.

`argus web` serves a phone-sized client over HTTP and one WebSocket. It is a foreground client of
the local daemon, started through the same connect-or-start as the TUI, so the daemon itself never
listens on a network and exposure lasts exactly as long as the process. It binds `127.0.0.1:7420`
by default — fixed, because an installed page belongs to one origin — and does no TLS; `--listen`
binds elsewhere and warns when that is reachable without HTTPS. It greets with `wide-tree`, which
the TUI never lists, and is sent `WideTree` — every workspace's projects — wherever the TUI is sent
`Tree`, so following every agent never re-scopes the terminals. A phone pairs with a six-digit code
printed with a QR code: single use, five minutes, five wrong guesses. It is given a 256-bit device
token as an `HttpOnly`, `SameSite=Strict` cookie, which `web-devices.json` in the config directory
keeps only as a SHA-256 hash; `argus web devices` lists them and `argus web revoke` removes one,
which a running server notices within ten seconds. The pairing request and the WebSocket upgrade
must carry an `Origin` naming the host they were sent to, and the upgrade a known cookie; a site
that rebinds its own name to this machine has no cookie for it. Every response carries a CSP that
allows no script or style but the page's own, `X-Frame-Options: DENY` and
`Referrer-Policy: no-referrer`. The page is plain JavaScript embedded in the binary, with no build
step, and speaks JSON defined beside the server rather than the daemon's protocol: the binary that
serves the page is the one that speaks it, so the two cannot drift. Replies arrive as HTML rendered
in Rust with raw HTML escaped, links kept only for `http`, `https` and `mailto`, and images turned
into links; every other string is set as text. Every phone shares one daemon connection, a
conversation several phones watch is watched once, and when the daemon says it is stopping
`argus web` exits rather than start another.

A request that makes something — a shell, an agent, an editor, a worktree, a project, a
repository — may name itself with a `request_id`, and the daemon answers that client alone with
`Created`: the id of what it made, or nothing when it refused (the reason still arrives as an
`Error`). The client matches what was made by that id, never by where a new row turns up in the
next tree, since the tree is broadcast for everything and the first to arrive need not carry the
row; it acts once it holds both the answer and a tree containing the id, in either order. An
unnamed request (id zero, what a client from before this sends) is never answered, so an older
client never receives a message it cannot read, and an older daemon ignores the name — a newer
client talking to it simply does not move to what it made.

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
| `hello` | what each side of a connection can take, said before anything else |
| `transcript` | a pane's conversation as entries, and the updates that keep a copy of it current |
| `tree` | what a client renders, which pane state outranks which, and what a row standing for several shows |
| `hook` | the pane API's URLs, environment, headers and flags — `argus-hook` builds what the daemon parses |
| `cell`, `framing`, `transport` | a screen cell, a frame, and the endpoint they travel over |
| `damage` | what changed on a pane's screen, as runs of cells and regions that scrolled |
| `review`, `ids`, `paths` | the shapes review, identity and location data travel in |
| `features`, `tasks`, `decisions`, `diagrams` | a feature board's parts: its scope, what is left to do, why it is built that way, and how it flows |
| `artifacts`, `memory` | how shared work is bounded when read or written, and the working memory an agent receives for one request |

`argusd` owns PTYs, Git, and everything that outlives a client. Its state is one `Daemon` type
behind a small set of mutexes; the split below is which file a concern is *read* in, not a change
to the type or its locking.

| module | answers |
| --- | --- |
| `main` | daemon startup, and running until asked to stop |
| `state` | what the tree is, and what a client is shown of it |
| `state/build` | turning what the config declares into the tree the daemon runs on |
| `state/panes` | a pane's lifecycle: spawned, restarted, closed, written to |
| `state/agents` | what an agent reports about itself, and what it is told |
| `state/viewers` | the one pty size reconciled out of what every client asks for |
| `state/git_ops` | the writes to Git |
| `state/sync` | the polls and watchers that keep the tree level with the disk |
| `state/panel` | the rows the user adds and removes by hand |
| `state/workspaces` | which scope is open, daemon-wide |
| `state/hook_server` | the loopback receiver agents report to |
| `state/session` | what survives a daemon restart |
| `state/transcripts` | which file a pane's conversation is read from, and the clients following it |
| `state/outbox` | what a client asked to say to an agent, and when it is typed |
| `state/tree` | finding your way around the tree |
| `state/features`, `state/tasks`, `state/decisions`, `state/diagrams` | a checkout's feature board, translated between client ids and store keys |
| `state/board_parts` | which feature a task or diagram request lands in, which rows it may touch, and who hears of the change |
| `conn` | one client connection, and which task each message runs on |
| `conn/dispatch` | what each client message does |
| `pty`, `pty/job`, `pty/vt` | a pane's child process; launching it, bounding what it starts and ending all of it; and its terminal emulator with the translation of its screen |
| `harness`, `harness/install`, `harness/hooks` | what a CLI is, what gets written into a checkout for it, and the command lines in it |
| `harness/skill` | the skill package an agent receives and the short message that leads it there |
| `harness/transcript`, `harness/transcript/claude` | what a harness's own transcript says, read one line at a time into entries, and how each harness writes its own |
| `store`, `store/schema`, `store/legacy` | `runtime.db`, its tables, and the files it replaced |
| `store/boards`, `store/reviews` | the feature boards and the review comments, as stored |
| `store/panel`, `store/session` | runtime changes to the panel, and the panes to bring back after a restart |
| `git`, `diff`, `browse`, `highlight` | the read-only questions asked of a repository |
| `gitignore` | the repository-local excludes for generated Argus files |
| `config` | `projects.toml`, which is read and never written |
| `daemon_lock`, `editor`, `watch`, `command`, `logging`, `paths` | the one daemon allowed to own an instance, and the small services the rest of the daemon uses |
| `dll_search` | where the daemon loads libraries from on Windows: its own directory and System32, never the directory it was started in |

`argus` is a replaceable renderer over one model. `app` holds the state and never predicts the
result of a request; `ui` is a pure function of it.

| module | answers |
| --- | --- |
| `main`, `redraw`, `terminal`, `wire`, `launch` | the event loop, the screen and socket it runs over, and the daemon lifecycle command |
| `bridge` | this machine's daemon on stdin and stdout, for a client on another machine to reach over ssh |
| `web` | the `argus web` command: its flags, and handing the web server this machine's daemon |
| `hosts` | the daemons this client is attached to, each with an app of its own, and which one is on screen |
| `remote` | reaching a daemon on another machine through ssh and its bridge, and saying why when ssh cannot |
| `ssh_hosts` | the hosts the user's ssh config names, for the host picker |
| `app` | the model: the tree, the selection, and which modal is up |
| `app/rows`, `app/layout`, `app/modal` | naming a row independently of its index, where the last frame put things, and the layers that float over it |
| `app/nav`, `app/input`, `app/mouse`, `app/scroll` | what the operator's gestures mean |
| `app/rail` | what the rail lists, in what order, which repository is open, and what choosing a row does |
| `app/mode` | which mode has the keys — the one answer key dispatch, the status bar and `?` all match on |
| `app/actions`, `app/pickers` | what is asked of the daemon, and the modal layers that ask it |
| `app/awaited` | requests waiting on something the daemon is making, and what to do with it once the tree holds it |
| `app/server` | what arrives back, and what it does to the selection |
| `app/views` | which surface the content area holds, and the one feature selection everything on it is read at |
| `ui` | the frame, and where the cursor goes on it |
| `ui/card` | a card and the rows in it: its frame, one row, and how a list longer than the card scrolls |
| `ui/command_center` | the HTML-specified shell: where the rail and stage go, the stage heading, the first-run state, and the status vocabulary they share |
| `ui/command_center/rail` | drawing the rail — its summary, the rows `app/rail` lists, and the live agents — and which row a click lands on |
| `ui/command_center/workspace`, `ui/command_center/feature`, `ui/command_center/panes`, `ui/command_center/checkouts` | one stage each, with the hit-test that shares its layout |
| `ui/text` | fitting text to a width |
| `ui/help`, `ui/prose` | the keymap window, and markdown styled where it stands |
| `ui/views` | the tab strip naming the four stages, and which tab a click lands on |
| `ui/review`, `ui/history`, `ui/status`, `ui/overlay`, `ui/modals`, `ui/term` | one drawn surface each |
| `review`, `history`, `brief`, `dirpicker` | the view state behind each overlay |
| `diagram` | the Mermaid sequence-diagram view and its responsive overlay layout |
| `grid`, `selection`, `pty_input`, `pty_input/keys`, `pty_input/mouse`, `paste`, `clipboard`, `fuzzy` | a pane's screen, its selected text, and the input primitives |
| `motion` | animation arithmetic: how far along a transition is at a given instant |
| `settings`, `theme`, `backend`, `herdr`, `profile` | preferences, palette, the ratatui backend, and what is reported outward |
| `fixtures` | the tree builders every test module shares |

`argus-web` is the server behind `argus web`: a library the `argus` binary calls with its own way of
reaching the daemon, so there is one way a daemon gets started. Its root serves until interrupted
or the daemon stops, and answers `argus web devices` and `revoke`. The page it serves lives in
`assets/`, and its tests run the server end to end against a daemon played over an in-memory
stream.

| module | answers |
| --- | --- |
| `daemon` | the one connection to the daemon, and what the server keeps of it for the phones |
| `routes` | what each request gets, and the checks guarding everything that can reach an agent |
| `pairing` | who may connect: the one-time code, and the device tokens it is traded for |
| `phone` | what the page and the server say to each other |
| `markdown` | a reply's markdown as HTML that can run nothing |

## Views

The production client follows `Argus Command Center.dc.html`: a two-row product-and-navigation
band, a stable 32-cell contextual rail, one dominant stage, and a one-row command band. Four tabs
name the stage surfaces: Workspace, Feature, Panes, and Checkouts. Their digits still open them;
from inside a pane the digits belong to the child, so a view is reached through the leader
(`Ctrl-Space`, then the digit). The empty tree replaces the shell with the first-run surface rather
than pretending first-run is a permanent view.

The rail is stable across all four surfaces. It summarizes the open workspace and selected project,
lists that project's repositories, and rolls live agents up at the bottom. `o` or a click on the
project name switches projects through a picker. Selecting a repository
changes the existing daemon-tree selection; the rail owns no parallel workspace model. Workspace
holds the selected pane's real terminal beneath a flat breadcrumb header. Panes is a responsive
card overview derived from the same pane locations, Checkouts is an operational table for the
selected repository, and Feature reads the selected feature as a document of brief, tasks, and
decisions. The command band follows the active surface.

Switching views changes the stage and nothing else. Every pane keeps running, its subscription
stands, and the Workspace stage is one keystroke back. Focus moves to the stage while another view
is open — a key left reaching a pane that is not on screen is a key nobody can see the effect of —
and returns to where it was when Workspace comes back. Which view is open is this client's own
state and is never sent to the daemon: two people attached to one daemon are not necessarily
reading the same thing.

There are four views: Workspace, Feature, Panes, and Checkouts. Review, history, settings, briefs,
and editors remain overlays because they are temporary work over the current surface. The command
center is the only presentation, and the one the client's tests draw: the five-column spine it
replaced, with its folding columns and draggable gutters, is gone rather than kept compiled beside
it, since a second presentation nobody sees was the one every test was running against.

## Navigation model

The runtime hierarchy is:

```text
Workspace scope -> Project -> Repository -> Checkout -> Pane
```

A workspace is daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The production TUI projects the selected
project's repositories and agents into the stable rail and gives the selected pane's terminal the
remaining stage. Repository rows are ordered active-first and stay collapsed until chosen; the one
open repository shows its checkouts with panes running and those panes, with the pane being viewed
marked in place. Checkout and pane identity remain the same indices and IDs used by the daemon
tree. While typing in a pane, `Ctrl-Space`, `f` lets its terminal take the main content area;
repeating the chord restores the shell. The command band remains visible in both layouts.

The keys walk the rows the rail draws, in the order it draws them, so anything a click reaches the
keys reach too. `j`/`k` and the wheel step through the project heading, the repositories, and the
open repository's checkouts and panes; `l`/Enter opens a repository and moves into it, goes to a
checkout's first pane, or types into a pane; `h`/Escape climbs to the row above. Stepping past a
repository does not open it — walking the list would otherwise unfold every row it crossed — and
opening one closes the last. On the project heading `n` adds a project and `D` removes this one.
The rail's cursor is not stored: it is the selection read at the depth focus names, so a new tree
cannot leave the two disagreeing. Checkouts with nothing running, and branches no checkout is on,
are not rail rows; the Checkouts stage draws every one, and there `n`, `D`, `s`, `a`, `F`, `P`,
`R`/Tab, `H` and Enter act on the row its table has selected — Enter opens a checkout in the workspace, or switches
the primary checkout to a branch.

The status bar's keymaps are written as tiers and the widest that fits is drawn, so a narrow bar
shows fewer keys rather than one cut mid-word. A card or table holding more rows than it can show
puts a thumb in the blank cell between its rows and its edge, sized and placed like a scrollbar's:
without it a list scrolls silently, and twenty checkouts in a table that fits six look exactly
like six.

The left of the bar counts the fleet — how many agents are working, how many are done, how many
are waiting on a person — ordered so the count you have to act on is the one nearest the corner.
It held a breadcrumb before, which said the path already spelled out across the workspace
heading. What is *not* written anywhere else on
screen is the state of the agents you are not currently looking at, which is the reason the
program has a pane list at all. Idle and exited panes are left out: a tally that counts them
reads the same whether anything is happening or not, and when nothing is happening the breadcrumb
comes back rather than the bar spending a row on `0 working`.

The bar is a reminder rather than a reference. It carries the handful of keys worth having in
front of you and ends, at every width and in every mode, with the one key that lists the rest —
appended after the tiers are chosen, so it is never the thing a narrow bar drops. `?` opens that
list as a window sized to its own content, grouped by what the keys act on rather than by
character, and answering for the mode you are actually in: the diff's keys in a diff, the
rail's on the rail. It opens *over* whatever raised the question rather than instead of it,
so the review you asked about is still there when you close it, and any key that is not a scroll
key closes it — having to hunt for the way out of a window you opened to be told something is the
problem it exists to solve. `?` is only this where nothing is taking text; on a prompt, in a
pane, in a filter or a feature line, or in a brief being written it is a character, and the leader
chord reaches the list from inside a pane. Which mode has the keys is decided once (`app/mode`)
and key dispatch, the bar and `?` each match on that answer exhaustively, so a mode one of them
forgets does not compile — the three used to work it out separately, in different orders, and
disagreed.

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
- `client.toml` stores theme, editor, and client layout settings.
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

Schema v3 and v4 added `note` and `note_audit` for project and checkout notes. Notes have since
been removed; the tables are still created on the way up from an older store, and nothing reads them.
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
agents are configured, `claude`, `codex`, `opencode`, `pi`, `agy`, and `agent` templates are supplied. Adding a project
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

On Windows, each agent process tree runs in its own Job Object with a 32 GiB committed-memory limit
and a 256-process limit. Closing the pane or dropping its runtime terminates the whole job rather
than only the template's immediate process. Shell and editor panes are not subject to these limits.

Windows panes run on the ConPTY built into Windows unless `conpty.dll` and `OpenConsole.exe` sit
beside `argusd.exe`, which portable-pty prefers. Argus does not ship them: nothing works worse
without them, and shipping them means a third-party executable per pane and binaries to keep
current. portable-pty loads that library by bare name, and Windows' default search would go on to
the current directory — the daemon inherits the one `argus` was run from, often a repository — so
the daemon's first act is to confine loads by name to its own directory and System32 (`dll_search`).

Each PTY starts at 24 by 80 cells. A blocking reader thread sends output through a bounded queue to
a Tokio task, which feeds the pane's terminal emulator (`alacritty_terminal`) and broadcasts what
changed plus the child cursor's position, visibility and shape. The task wakes on the first byte, takes whatever else is
queued up to a bounded batch, and after a frame waits 16 ms before the next, so a stream is
coalesced while a lone keystroke's echo goes out at once. A second thread owns the child and blocks
until it exits, so an idle pane wakes for nothing: the task used to tick every 16 ms for the life of
the pane only to ask whether its child had gone. The reader cannot stand in for that thread, since
it need not reach EOF while the pane holds the pty master open.

The task keeps its own copy of the screen as protocol cells and converts only the lines the
emulator reports damaged since the last frame: a keystroke's echo converts a cell or two rather than
every cell on the screen. That is why the emulator is `alacritty_terminal` and no longer `vt100`,
which tracks no damage, so every frame rebuilt the whole grid. Measured at 200 by 50, a frame cost
310 µs for an echo, 341 for an Ink redraw, 416 for a full-screen app and 655 for forty scrolled
lines under `vt100`; 63, 182, 63 and 517 under alacritty, which damages the whole screen on any
scroll. What the watchers hold is tracked the way they track it — the last frame's scroll and runs
laid over the grid before — so the next frame is diffed against exactly what they have. A cell's
grapheme is stored inline rather than on the heap, and a blank still carries the attributes it was
cleared to, so a TUI's coloured bars survive.

The daemon is the pane's terminal, so it answers what a terminal is asked — cursor position,
device status and attributes — as the emulator reaches each question mid-stream, and the pump
writes the answer back through the pane's input. A child that asks waits for the answer: ConPTY
opened to inherit the cursor, as portable-pty 0.9 opens it, starts no child until it has one, and a
line editor that asks at every prompt otherwise stalls there until its own timeout. It honours synchronized updates (`CSI ?
2026 h`), holding a frame back until the child ends it, and the pump wakes at the update's deadline
so a child that never ends one cannot freeze its pane. The cursor's shape (DECSCUSR) is the
emulator's; a shape it reports only when nothing asked for one stands for the host's own.

What changed travels compactly (`damage`). Each frame is diffed against the grid the client holds
into runs of cells — a run's graphemes as one string, its looks as counted styles of three small
integers, and a count of default blanks at its end — with an unchanged gap of a few cells riding
inside a run rather than starting another. A region that moved is sent as a move: rows are compared
by hash, and the scroll that saves the most rows is taken, a region rather than the whole screen
because an agent's output scrolls above an input box that stays put. A scroll is looked for only
when more than one row changed, since it saves only changed rows, and the row hash is FxHash rather
than SipHash; together they took the diff for an echoed keystroke on a 200 by 50 screen from about
570 µs to under 60. The frame is then diffed against the grid after that scroll, so a wrong guess —
a hash collision included — costs bytes and never a wrong screen. A 200 by
50 screen of text is a few kilobytes rather than the 620 KB it was as per-cell records, and a one
line scroll of it under 200 bytes rather than 413 KB. A client that did not greet with `cell-runs`
is sent the per-cell form, built by its connection from the runs; a scroll, which that form cannot
say, puts it behind, and it catches up with a fresh grid as a scroll always cost it. Rows read from
a pane's history come back as runs to a client that reads them, and as cells to one that does not.

The client also bounds incoming daemon messages and coalesces redraws to the same interval. Cursor-only
changes are broadcast even when no cell changed. The client places its hardware cursor there only
while that pane has typing focus. The parser retains 4,000 scrollback lines. An exiting process gets a 500 ms output-flush grace period.

A client can park a pane's view above its live screen. It asks for an offset in lines and the
daemon answers with the rows there, the offset it could actually reach, and how far back the buffer
goes; the parked rows are drawn in place of the live grid until the view returns to the bottom. The
rows are read from the emulator's history by line, so the live screen every other watcher is diffed
against never moves. The alternate screen
keeps no scrollback of its own, so a full-screen child answers with a depth of zero rather than
showing the shell's history underneath it.

Damage keeps landing on the live grid the whole time a pane is parked, so returning to the bottom is
immediate and never needs a fresh subscription. The parked rows are deliberately not re-read as that
damage arrives: they are what the operator scrolled up to read, and a pane still producing output
would otherwise shift the text out from under them. Scrolling further is asked by line rather than
by offset: the daemon numbers every line that goes up past the live screen, evicted ones included,
answers a read with the number of its first row, and takes that number back, so a view on a pane
still printing moves from the text on screen rather than from wherever the live screen has got to.
The emulator counts none of this, so the daemon reads it off the history growing: the history may
run past its cap by two reads' worth — a read is at most 8 KiB and a byte scrolls at most one line —
and is trimmed back after each. Lines scrolled on the normal screen by a read that then switches to
the alternate one go uncounted, and a change of width rewraps lines under their numbers; a line
evicted since its number was handed out reads as the oldest one kept. A daemon that numbers no
lines answers without one, and the client asks by offset as before.

A wheel over a pane on the normal screen moves that view rather than reaching the child, since that
is the screen with history behind it. Shift-PageUp and Shift-PageDown page by a screen less a line,
leaving the child its own unshifted paging keys. Typing returns the pane to the live screen: the
child's echo lands there, and the parked view is not somewhere input can be seen. A parked pane
leads its title with how far back it is, because it is otherwise indistinguishable from a quiet one.

Clients receive a full grid when they subscribe, then incremental damage. The grid and the damage
stream are taken under one hold of the parser lock, so no frame can be published between them and
be missed by both. Resize changes both the PTY and parser and emits another full grid.

A connection holds two queues to its socket. Everything a pane sends — its grids, its damage, its
copies and its exit — waits in one bounded queue of 32 frames, in order, since damage ahead of its
grid lands on nothing and an exit ahead of the last frame removes the grid that frame was for. The
subscription's first grid is queued before its stream starts forwarding, so no frame can overtake it.
Everything else — trees, replies, errors, acknowledgements — waits in an unbounded queue that is
written first, so a slow screen never holds up the answer to a request. A frame that finds the pane
queue full is dropped rather than waited for: the pane falls behind, its frames are skipped, and the
first free slot carries a fresh grid with the stream that continues it. A client too slow to keep up
therefore sees the latest screen late, not every screen later and later, and the daemon holds no
more than 32 frames for it. A copy or an exit is never dropped; it waits for room. Falling behind
the pane's own broadcast recovers the same way.

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

A selection dragged in a pane is copied two ways at once: to the desktop clipboard, and to the
host terminal as OSC 52, which is the only route out over SSH or through a multiplexer (herdr
forwards it; tmux needs `set-clipboard on`). A child's own OSC 52 copy (Claude Code's `/copy`, a
shell's `osc52` helper) is caught by the pane's parser and sent to the client as
`ServerMsg::Clipboard`, which puts it on the clipboard the same two ways. Ctrl-V reads the desktop clipboard; where there is
none, as over SSH, the terminal's own paste key arrives as a bracketed paste instead. Argus never
asks the terminal to read its clipboard over OSC 52: most terminals refuse, and the reply would
reach the key parser as typed text.

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
quiet baseline; a later effective-state change into `Waiting`, `NeedsReview`, or `Failed` is announced. Effective
state includes child agents because their parent pane is the selectable place the operator can open.
Every row that stands for several panes — a checkout, a repository and its rail dot, the rail's need
badge, the command band's tally — rolls up each listed pane and each child the same way, through
`tree`'s one ranking: a failed exit outranks a working agent, and only a clean exit is calm.
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
plugin module into the checkout and remove it on the same schedule. OpenCode and pi are the built-in
cases: OpenCode has no hook table, while pi exposes lifecycle events through project TypeScript
extensions. Both modules read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` at run time rather than having
a pane baked into them. A harness also carries
`resume`, the legacy arguments that continue the last conversation, and `resume_id`, an exact argv
template containing `{session_id}`. Both are used only when a recorded pane is restored. Claude
Code, Codex, OpenCode, pi, AGY, Cursor Agent (`agent`) and `generic` are built in. Codex uses a project-local `.codex/hooks.json`
adapter (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SessionEnd`) whose command reads routing from the pane environment, so its content hash stays
stable after the user trusts it. On Windows, Codex runs `commandWindows` through PowerShell, so those strings use `& $env:ARGUS_HOOK` rather than cmd-style `%VAR%` quoting, and build hook URLs with `($env:ARGUS_HOOK_URL + '/status/…')` so Constrained Language mode does not treat `/status` as division. Codex requires the user to trust project hooks before it runs. AGY uses
`.agents/hooks.json` with flat `PreInvocation` and `Stop` hooks. Cursor's `agent` CLI uses `.cursor/hooks.json` with
flat `sessionStart`, `beforeSubmitPrompt`, `preToolUse`, `postToolUse`, `beforeShellExecution`, and `stop` hooks
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

Telemetry rides the same mechanisms. `PaneInfo::telemetry` is one harness-neutral record — model,
context tokens and window, cumulative input and output tokens, cost, the running tool, and a tool
count — and every field is optional, because each harness exposes a different subset. Adapters POST
partial reports to `/pane/<id>/telemetry` as JSON; the daemon merges each set field over the last,
treats an empty `tool` as the tool finishing, counts tool starts that carry no count, drops reports
from child sessions, and clears everything but the model when the pane's conversation changes.
Telemetry is live state and is not persisted. The installed hook form derives a report from every
event it reads: `tool_name` and `hook_event_name` (Claude Code, Codex, Cursor), `model` (Codex,
Cursor), and the tail of `transcript_path` — Claude Code's assistant `usage` for context, Codex's
`turn_context` and `token_count` records for model, window, and totals. The OpenCode plugin and pi
extension report from their message and tool events. Claude Code tool hooks are answered with `{}`
so the user's permission rules still apply. Clients show the report on the workspace breadcrumb and
the pane cards.

A report arrives with every tool call, so it does not travel in a tree. The daemon publishes the
pane's merged record on a channel of its own, and each connection sends it as its client can take
it: `ServerMsg::PaneTelemetry`, the one pane's record, to a client that greeted with
`pane-telemetry`, and the whole tree to any other. A connection handles trees before telemetry, so a
tree taken before a report cannot land after it and wind the numbers back, and one that falls behind
the records is sent the tree, which holds them all.

A pane's conversation is read from the transcript its harness already writes; Argus stores none.
Any report may carry `X-Argus-Transcript` naming the file, and `argus-hook` adds it to every report
the installed form makes when the event's JSON has a `transcript_path` (Claude Code, Codex, Cursor).
The daemon takes it only from the pane's own session, after the report itself is applied, so a
session claim that makes a new conversation the pane's own lands first. It keeps the files a pane's
conversation has lived in, oldest first; `PaneInfo::has_transcript` says whether there is one. The
harness names a dialect (`transcript = "claude"`; only Claude Code's exists so far), and
`harness/transcript` reads one JSON line at a time into the harness-neutral entries of
`argus_protocol::transcript`: prompts, replies, thinking, tool calls and results, notices, turn
ends and dividers. An entry is named after its file and byte offset, or after the harness's own id
for a tool call, so reading a line again replaces rather than repeats it. Text is clipped before it
leaves the daemon, and a harness's bookkeeping records, subagent sidechains and what the CLI adds
on the person's behalf are left out.

A client that greeted with `transcripts` sends `WatchTranscript` and is answered with a fresh
`Transcript` — the last megabyte of the current file — and then every update after it. One task per
watched pane polls the file's length four times a second while anyone watches: a harness appends
many small writes per turn, so a filesystem watcher would report each one only for the length to be
read anyway. Following starts at a line boundary fixed when the first watcher joins, before any tail
is read, so a tail and the stream after it can overlap but never leave a gap. A connection that
falls behind is sent the tail again. When the pane moves to a new file the watchers are sent a
divider and that file's tail below what they hold. `EarlierTranscript` pages back a megabyte at a
time, and across into the file before, until the start. The terminal client never watches.

A client that greeted with `outbox` can say something to an agent with `SendToAgent`, answered with
`Sent` to that client alone. Typing into a pane is typing into whatever has the agent's keyboard,
and a dialog that has it takes the text as its answer, so a message waits in the pane's queue —
`PaneInfo::queued`, shown on the TUI's pane card — until the agent is at its prompt: idle, done,
failed or wanting a review, and so for 300 ms, since a turn's last hook can arrive just before its
prompt is drawn. Codex's and Cursor's approval prompts report as working, so they hold a message
too. One message goes out per turn: the next waits until the pane has been busy and come back, or
two seconds for a harness that reports nothing. A message is typed as one paste and Enter. `now`
types it straight away, which is how a person steers an agent mid-turn. One task does the typing,
started by the first message queued, and wakes whenever the tree changes and four times a second
while anything waits. The queue is not persisted, and an exited agent's queue is dropped.
`CancelQueued` takes a waiting message back, from the phone or with `u` on the TUI's Panes stage,
and the card changes when the tree says so. `Interrupt` types the harness's interrupt key: Esc,
unless a `[[harness]]` block sets `interrupt`, and never Ctrl-C, which pressed twice quits most of
these CLIs.

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
contains only environment references and remains identical across boots. Argus adds every generated
harness and skill file, and `.argus` (the default worktree root), to the repository's local `.git/info/exclude`; this changes no tracked file,
never enters a commit, and is refreshed whenever a checkout is discovered. Hook files are checkout-wide;
the helper uses or rebases to a valid`ARGUS_HOOK_URL`, so each process still routes to its own pane.
The helper reads hook stdin once and can extract both a note and a configured
top-level session ID key. Claude captures`session_id` at SessionStart. OpenCode's plugin tags root
and child reports with their session IDs; only a root claims `/session`, and a newly created root
reports again when it replaces the previous root. Pi's extension claims
`SessionManager.getSessionId()`, reports input and low-level agent starts as working, settled runs as
idle, and blocking extension prompts as waiting. AGY captures`conversationId` at PreInvocation.
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
`PreInvocation`, OpenCode `chat.message`, or pi `input` — carries the user's text; the helper
posts it to `/title` the same way an explicit `argus-hook title` does. Tool-start
events are not titles: a working pane named "Shell" says less than the template
already does. An agent can still refine the name once it knows the task; the
next prompt replaces it. Children still cannot rename the parent row.

Agent workflow guidance lives in the bundled `crates/argusd/skills/argus/SKILL.md` and its
`references/`, embedded into the daemon at build time. The skill explains titles, the statuses
hooks cannot infer, and checkout moves; its references cover features, tasks, decisions, and
diagrams, each read only before writing to that part of the board.

Every tool call re-reads the conversation, so startup is designed to cost none. `argus-hook
context` prints review comments, the feature brief (decisions counted, not listed) and the open
tasks (titles only) in one answer. Claude's context hook runs `context <instructions>`, and Codex's
`instructions` prints the inherited message followed by the same context, so both agents begin
with it in front of them. The bootstrap tells an agent to load the skill only before reporting
status or changing the board, not to answer a question. Lifecycle hooks still capture session
identity and report their existing events. A stopped turn is not proof of completed work.

Before starting a built-in agent, Argus installs the package in `.claude/skills/argus` for
Claude Code, `.pi/skills/argus` for pi, and `.agents/skills/argus` for Codex, OpenCode, AGY, and Cursor. The latter also gives
harnesses without a native skill loader a file they can read directly. `ARGUS_INSTRUCTIONS`
now holds a short bootstrap pointing at the installed `SKILL.md`. Claude's context event,
Codex's additional SessionStart context hook (including compaction), OpenCode's and pi's system-prompt
adapters, and AGY/Cursor's rules deliver that bootstrap through their existing context surfaces.
Pi discovers both managed files only after its normal project-trust approval.
The Codex context command runs `argus-hook instructions`, which prints the inherited message
without interpreting it as shell code, then the pane's context; the message still prints when the
daemon is unreachable. Its command string is stable
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
session ID and harness name whenever a tree broadcast changes one of them. The daemon remembers the
rows it last wrote, so a broadcast that changes none of them — telemetry, a child agent's status —
writes nothing. The harness is the one the pane actually ran under rather than
whatever its template names now, since that is who wrote the conversation a restore claims; a record
without it falls back to the template. Recording is suppressed while a restore is in flight, so the
panes it is starting do not rewrite the rows it is reading.

On daemon startup:

- linked worktrees are reconciled against Git first, because only primary checkouts come from the
  config and a pane in a worktree would otherwise look like a pane whose checkout is gone;
- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells; on Windows Argus uses a runnable `SHELL`
  first, then `pwsh`, `powershell`, and finally `cmd.exe`;
- the saved display title, status, and note are reapplied after each pane starts;
- an agent with an ID starts with its harness's `resume_id` argv template expanded to that exact ID;
- every identified pane resumes independently;
- records without an ID retain broad `resume`; one legacy pane per checkout and harness claims it,
  so different template aliases of one harness cannot reopen the same last conversation;
- an agent started that way that exits non-zero within five seconds is taken to have had nothing to
  continue, and is replaced by a plain new agent in the same checkout;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

Exact templates are Claude `--resume {session_id}`, Codex `resume {session_id}`, OpenCode and pi
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
- dirty state and changed-file count, including untracked files, with how many paths are staged
  and how many unstaged counted apart — a path staged and then edited again is in both;
- ahead/behind against the tracking branch;
- linked worktrees added or removed outside Argus.

The first of those ticks is immediate, overlapping session restore, so dirty counts follow by the
time the tree has been on screen a moment.

The rail's checkout rows and the Checkouts stage draw that state the same way, as parts rather
than a first match: `↑2 ↓1 +3 !4` is two ahead, one behind, three staged, four unstaged, and
`clean` stands in for the last two. A dirty checkout therefore still says it has unpushed commits.
"No upstream" and "in sync" both read as no arrows.

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

On any branch row, `a` and `s` give it a worktree and start the agent or shell there once the
daemon says which checkout it made, Enter switches the primary checkout to it, `n` gives it a worktree, and `D`
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

Switching a checkout follows Git's rules: uncommitted changes that do not conflict move across the
switch, while a conflicting switch is refused and Git's reason is shown to the user. A worktree is
still available when the user wants the work kept in its own directory. Linked worktrees are Argus's
own and switch under the same Git rules.

## Review

`R`, `Tab`, or leader-Tab requests a checkout review. The daemon computes the diff on a blocking
task with `git2`; the client opens it in a floating overlay while leaving the terminal behind it
subscribed.

Review shows uncommitted work split into the two sides Git itself keeps apart. `b` toggles:

- `unstaged`: the index against the working tree, plus non-ignored untracked content — `git diff`.
- `staged`: `HEAD` against the index — `git diff --cached`.

The chosen side is a setting rather than a per-visit choice, and it survives closing and reopening
the overlay. An empty side never strands anyone: the toggle lives inside the overlay, so a fresh
review whose side is empty asks once for the other side and opens on that, and only when both are
empty says so and opens nothing. `b` onto an empty side keeps the diff already on screen and its
side. Closing a review or history returns the keys to where it was opened from, the rail or the
Checkouts stage.

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

## Features and the decision board

Work is scoped to a **feature**: a short document saying what is being built, with the decisions
taken while building it hanging off it. The board used to be one tree per project, which answered
"what has this project ever decided" — a question nobody asks. What an agent picking up work needs
is the handful of choices made about the thing it is about to touch, and everything else on a
project-wide board is noise it reads past. So a decision is filed under a feature, and a board is
read one feature at a time.

The board containing those features defaults to one Git repository. Every linked worktree shares it,
so a feature can move from the primary checkout to a feature worktree and remain readable after that
worktree is removed. The daemon derives a durable key from the repository's shared Git directory
instead of persisting its runtime ids. Synthetic and non-Git checkouts use their repository path.
For work that deliberately crosses repository boundaries, the pane API accepts an explicit workspace scope,
selected by `ARGUS_ARTIFACT_SCOPE=workspace` in `argus-hook`. This scope is independent of the open
workspace in the TUI: opening a workspace changes visibility, not where an artifact is filed.

A feature is stored, not derived: schema v6's `feature` table holds a slug, a title, the document
body, and the checkout and branch it originated in. The slug is derived from the title once, at
creation, and made unique by suffix inside the transaction — a title someone later rewords must not
orphan the decisions under it, and two agents opening the same-sounding feature must not silently
share one scope. Schema v12 merges older branch-keyed boards into their repository board; colliding
slugs receive deterministic numeric suffixes while tasks, decisions and events follow them. The
document is the one part that is edited rather than appended to as
a tree: it is prose both sides write, bounded at 8 KiB, because a brief that has outgrown a screen
has become the design document it was meant to point at.

An agent adds to a brief a paragraph at a time — `argus-hook feature note` — and replaces one only on
purpose, with `feature brief <slug> "<text>"`, which is how a paragraph it appended in error is taken
back: an agent can undo any change to the board it can make. `feature retitle` renames a feature,
keeping its slug, and `feature drop` removes one with nothing under it — the undoing of `open`. A
feature with tasks, decisions or diagrams under it is refused, since those may be other agents'
work; an agent empties it a task at a time, each of which it can undo, and a person removes a whole
one from the view. A human replaces a brief in the view: `e` on a feature, from either board, opens
the brief in the brief editor and `ClientMsg::SetFeatureBody` writes back whatever it says. It is the same editor a
task's brief gets because it is the same job, prose a person reads and corrects. The editor is
modal: `j`/`k`, `0`/`$` and `g`/`G` navigate, `i`, `a` or `o` start typing, and `Esc` leaves insert
mode and saves; `q` saves and closes. The work a brief might have listed lives in its tasks. The decision view draws the
brief above the tree, bounded to a third of the panel, which is the order `argus-hook feature`
prints them in and the order they are read.

Which feature an agent is on is resolved from the checkout and artifact scope, not from a flag.
Schema v10's `artifact_feature_scope` maps a scope and checkout path to a slug, so repository
and workspace work can select different features in the same checkout. A checkout that was never
pointed anywhere falls back to the one
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
appends to it, answering with the id the next decision hangs off. `argus-hook task` reads the
current task tree; `task add "<what to do>" [--key <tracker-key>] [--under <id>]` adds a task, and
the other task verbs change its state, title, brief, or presence.

Every one of those — `feature`, `task`, `decisions`, `decide`, `diagram` — can name the feature it
is about with `--feature <slug>`, and `feature <slug>` alone reads one. The checkout's pointer is
shared by every pane in it, so an agent reaching another feature by moving it moves everyone's; a
named request reads or writes that feature and leaves the pointer where it was. The name travels as
`feature=<slug>` beside the scope in the request, a name that is not a slug never leaves the
helper, and one the board does not have is refused by name rather than read as naming none. A
board read by name is about that feature: its decisions, and `current` naming it.

The decision board is append-only. Nothing is ever edited, and there is no delete. A decision that a
later finding invalidates is *superseded*: the replacement is a new row that takes the old one's
place in the tree — its parent, not its children — and the old row's `superseded_by` records what
replaced it. The old node stays on the board and the view draws it dimmed, because the road not
taken is most of what a reader came back for. A decision recorded in error rather than overtaken is
*withdrawn*: `argus-hook decisions withdraw <id>` sets its `withdrawn_at` (schema v17), agents stop
reading it, and the view keeps it, dimmed and marked, so the history still says it was once decided.
`decisions restore <id>` undoes that. Either is refused for a decision not under the feature the
request is about. A withdrawn replacement replaces nothing — what it superseded reads as standing
again until it is restored — and `--supersedes` and `--under` name only a decision on the same
feature, since a number from another feature's board is a mistyped one. Decisions recorded before features existed keep a
NULL `feature` and are reported as unfiled rather than dragged under a feature nobody chose.

There is no policy flag on these writes: the board exists for agents to write, is append-only, and
attributes every row, so there is nothing for a gate to protect.

Clients read the selected checkout's repository board — every feature and every decision in
that scope — with `ClientMsg::GetDecisions`. Every client artifact request carries both project and
checkout ids; the daemon validates their relationship and resolves the durable board key. Boards are
pushed with `ServerMsg::Decisions` whenever that scope changes, since a tree is meant to be
watched being built and the daemon deliberately does not track which view a client has open; a
client with another project open drops it by name. `DecisionBoard::scoped` is what narrows that to
one feature in the client, so switching scope costs no round trip. The tree is drawn two lines per
decision with branch rails and elbows connecting every child to its parent, and `argus-hook
decisions` draws the same topology for agents. Decisions from before features existed are reported
as unfiled by the daemon, but the feature list offers features only.

### The feature view

The Feature stage draws the selected checkout's repository features at the top and whichever one
is selected, whole, beneath them: its brief, the tasks left under it, its sequence diagrams, and
the decision tree. Tasks can contain subtasks recursively, so newly discovered
work remains under the task that exposed it instead of becoming an unrelated root. That is the
order they are read and the order `argus-hook feature` prints them in — a
decision without what the feature is for explains half of itself, and a task list without either
says what to do and never why.

It replaced three views. A decision board, a feature board and a task board were the same object
drawn three times, and each carried a selection of its own: `App` held `board_feature_sel` for the
decision view's list and a `(column, card)` pair for the board, and the task list resolved the
card first. Opening the tasks while reading a feature's decisions therefore showed a different
feature's tasks — whichever card the board was sitting on, which by default was the first of
`proposed`. One selection is the fix, and there is now exactly one: `feature_sel`, an index into
the same list every panel is scoped by, so the three cannot disagree about which feature is on
screen.

The three panels take keys in turn rather than each having their own set. `h` and `l` cross between
the list and the feature being read — `l` is a direction and stops at the last panel rather than
wrapping — `Tab` steps through them in the order they are drawn, and `j`/`k` move in whichever has
them. `a`, `s`, `e` and `x` act on what has the keys: a new feature or root task, a subtask under
the selected task, the brief or the task's text, remove the feature or drop the task. The decision
panel refuses both in prose, because
the board is append-only and agents are what write it. Every panel draws its own selection whether
or not it has the keys, the way the rail does — the selections are how a reader traces where they
are, and one that vanished when the keys left would make crossing back a hunt. A line being typed —
a new feature, task or subtask, or a rewrite — takes the stage's last row, so what is being written
and what is already there are readable at once.

The brief takes what its wrapped text needs, and the wheel scrolls what does not fit; the rest is
one `e` away in the editor. In the panel that has the keys, the selected feature, task, and
decision grow to their full wrapped text, and the window they scroll in is sized after paying for
that growth — rounding up there is what let an expanded row start on the last line and run off the
bottom.

### Sequence diagrams

Sequence diagrams are stored as Mermaid source on a feature and opened from its Diagrams panel in
a floating overlay. The client removes the dependency renderer's decorative block-fill glyphs so
`alt`, `opt`, and related frames retain their borders and arrows without filling the panel with
visual noise. The rendered grid is recalculated against the overlay's current inner width after a
resize; when the natural diagram is still wider than the panel, `h`/`l` or the left/right arrows
pan it horizontally, while `j`/`k` scroll vertically. `q` and Escape close the overlay.

### What a feature row says

A feature's line is read off what Argus already observes rather than maintained by anyone:

- the agent panes running in its checkouts, summarized as the one thing worth knowing — `waiting:
  <what for>`, `failed`, `needs review`, `2 working`, or `1 idle` — and coloured like the pane that
  stopped, since a list of features is a list of places work might be stuck;
- how its tasks stand, as `3/7 tasks`;
- how many decisions are filed under it;
- `after <slug>` for each feature it comes after that is not yet done;
- `held` when someone has held it;
- and, only when there is nothing else to say, the branch it was cut on.

`Feature` carries `checkouts` and `tasks` for this. The checkouts are every path `feature_scope`
points at plus the one it originated in, so a feature a person wrote down and an agent later picked
up with `feature use` gains a place where work on it can be seen happening; without them the
feature list is an island describing work with no way to tell whether anything is happening to it.
The counts are one grouped query in the daemon rather than every task of every feature on the wire.
Both are pushed with the board, and a task change re-broadcasts the features so the heading over a
list cannot contradict the list.

This replaced five drag-maintained columns. `proposed`, `active`, `blocked` and `submitted` were the
one surface in Argus showing an assertion rather than an observation — a pane's status comes from
its harness, a checkout row from the branch actually occupying the path, a diff from Git, and a
board column from whoever last pressed `H`. `argus-hook` never exposed the move at all, so the only
thing that ever maintained them was a person dragging cards, and a column nobody updates is stale
rather than wrong-but-current, which is worse.

Schema v9 collapses `FeatureState` to `open` and `done` and maps the four old names onto `open`.
`feature_event` is not rewritten — it records what was believed at the time, which is the whole
reason it is a table — so `FeatureState::parse` still reads `submitted` and answers `open`. The
`claimed_by`, `claimed_at`, `blocker` and `evidence` columns are cleared and left in place; nothing
writes them, and the claim they held is what the panes now say directly.

`done` stays stored and stays the human's decision. The normal list shows open features; `v`
replaces it with accepted history so completed work remains available without crowding current
work. `.` accepts the selected feature and reopens one already accepted over
`ClientMsg::MoveFeature`; the pushed board is the only state change the client trusts. An agent can
carry the decision out — `argus-hook feature done <slug>`, and `feature reopen <slug>` to take it
back — and the bundled skill says to only on a person's word, never because its own tasks are
finished. `feature_event` records every move with who made it, so one an agent made is logged as the
agent's, with its session, rather than claiming the person moved it themselves.

A feature can be held, and can come after other features (schema v18). A hold is the one thing a
row says that is maintained rather than observed — "until the user answers" shows on no pane and in
no task — so it is never bare: `feature.held` is the reason, NULL meaning not held, and a reader
who sees the reason no longer stands can lift it. That is the difference from the `blocked` column,
which said only that somebody once dragged a card there. What a feature comes after is a
`feature_wait` row per prerequisite, and whether it still waits is read off the prerequisite's
state, so accepting one releases what came after it without anyone remembering to. A wait that
would close a loop is refused, since two features each waiting for the other to be accepted never
will be, and removing a feature clears its waits both ways. A held or waiting feature's title is
drawn dim, and its hold and open prerequisites lead its brief. In the view, `p` asks why and holds
the selected feature, or lifts its hold, and `w` picks a feature it comes after — open features,
and any it already waits on — where confirming one it waits on takes the wait back. An agent uses
`feature hold <slug> "<why>"`, `feature unhold <slug>`, `feature wait <slug> <on>` and `feature
unwait <slug> <on>`; `feature list` prints a hold and the open prerequisites under the row, and
`feature` and the pane context lead with them.

`m` transfers the selected feature's active checkout association to another checkout in the same
repository over `ClientMsg::TransferFeature`. The daemon validates both runtime ids, removes the
source association and writes the destination in one store transaction. This changes where work is
happening, not where it began: `origin_checkout` and `origin_branch` remain historical. Removing a
worktree removes neither the repository-owned feature nor its accepted history.

A feature is opened with `a`, renamed with `R`, and removed with `x` — the list is where a person
writes down work, so it cannot be a surface only agents can add to. A feature opened here records
no origin checkout, because work a person wrote down has not been cut anywhere yet; whichever agent
picks it up says so with `argus-hook feature use`, and that is when it gains a home. A rename
changes the title and freezes the slug: every decision row, task row and `feature_scope` entry
points at the slug, so re-deriving it from the new title would orphan exactly the work the feature
is about. Removing one takes its tasks, events and checkout scope with it and **unfiles its
decisions** rather than destroying them — the board is append-only because it records what was
believed at the time, and that outlives the feature it was believed about. The torn-off decisions
survive as unfiled and stay readable through `argus-hook decisions`, but they are not a feature and
the feature pane draws no row for them.

### Tasks

A feature says what is being built and its decision tree says why it is being built that way.
Neither says what is *left* to do, which is what a human actually hands an agent. So a feature
carries a list of tasks, drawn as the middle panel of the feature view.

A task is a row, not a checkbox line in the feature's document. Its title stays a compact line, and
an optional multiline brief carries context, boundaries, acceptance criteria and verification when
the title is not enough. The selected row expands to show that brief; Enter opens it in the same
multiline editor used for feature briefs, while `e` keeps the fast title editor. A checkbox line in
a document would be addressed by line number, and a line number moves whenever the text around it
is edited — which is the one thing a list cannot take, since a row has to stay the same row while a
human rewrites the list. Schema v8's `task` holds the title, the state, the
claim, a `position` and an `external` key; schema v11 adds its optional body and schema v15 adds
its nullable `parent` task id. `position` is per sibling list, so moving a child never reorders its
parent's other children or the feature's root tasks.

The states are still todo, doing and done, and unlike the feature columns they sat beside they are
maintained by whoever is doing the work: an agent takes a task up and finishes it as part of the
job, so `doing` says which row somebody is actually on rather than which was last dragged. They are
drawn as a mark on the row — `○`, `▶`, `✓` — rather than as three columns, which is what lets the
list keep the order a person put it in. That order is most of what a list is for, and three columns
spent it saying what a glyph already says. Distinct shapes rather than colour alone, since a state
you can only see by telling two greys apart is a state half the readers cannot see.

`external` is whatever key the team's tracker uses. Argus stores it and knows nothing else about it:
an agent with access to Jira, Linear, GitHub Issues or a spreadsheet is what puts tasks here, which
is why Argus works the same with any of them and needs credentials for none. `argus-hook task add
"<what to do>" --key ORION-412` is the whole of the integration. When work reveals more work,
`argus-hook task add "<child>" --under <id>` keeps that discovery beneath the task whose id `task`
printed.

Both sides write, and here they write the same things — there is no acceptance step and so no move
either side is refused. That ceremony belongs to the feature the tasks are under, which is where a
human accepts the work as a whole. An agent reads with `argus-hook task` and writes with `task add`,
optionally using `--under <id>`, `task doing <id>`, `task done <id>`, `task todo <id>`, `task retitle
<id> <text>`, `task brief <id> <text>` and `task drop <id>`; the states are named as verbs rather than
hidden behind a state argument, so what an agent types is what a reader of the transcript understands
happened. Taking a task up is what claims it and finishing it is what releases it, so a row always
says who is on it without anyone claiming by hand.

`task move <id>` changes where a task sits, never its state: `--under <id>`, `--top`, `--before
<id>` or `--after <id>` within its feature, or `--to <feature>` onto another feature on the same
board, where it arrives at the top level. `TaskAction::Place` names the place against another task
rather than as an index, so the daemon, which holds the tree, works out the numbers. The subtasks go
with it, a task placed beside another takes that one's parent, and a place inside its own subtree is
refused. Both sibling lists it touches are renumbered dense, and a move to another feature pushes
that feature's list as well as the one it left.

Ids are database-wide and an agent numbers its tasks from what it last read, so every change naming
a task or a diagram is refused when the row is not under the feature the request lands in — the one
the agent's checkout is on, or the one the client names: a stale id would otherwise let one
feature's agent tick off another's work by arithmetic. Both sides and both parts take that one path
(`state/board_parts`), so the guard cannot differ by who asked.

From the view, `H`/`L` move a task along todo → doing → done and `J`/`K` move it earlier or later among
its siblings. The order is usually a human's statement of what to do first, and the skill tells
agents to leave one a person arranged; they can still set it, so a task an agent put in the wrong
place is one it can put back. `>` puts the selected task under the sibling above it and `<` lifts it
out to sit after its parent, and `m` in the tasks moves it, with its subtasks, to another open
feature. `s` starts a line for a subtask under the selected task; `a` starts a
root task. Both are refused unless the tasks have the keys, so a capital `H` on the feature list does
not move a task the cursor is nowhere near. Lists are pushed whole on `ServerMsg::Tasks` whenever one
changes, and the wire stays flat: every row carries its parent id while the client and hook project
the rows into depth-first branches. The cursor is held by task id across the push rather than by row
number: a card you have to go looking for reads as having been lost. Removing a task removes its
descendants with it, so a discovered subtree cannot become orphaned work.

Adding and rewriting type on one `LineInput`, shared with the feature list because adding a feature,
a root task, and a subtask are the same gesture on the same kind of surface and two of them would
drift. It
takes a row off the bottom of the view rather than floating over it, so what you are writing and
what is already there stay readable together, and while it is up it swallows every key: a title with
an `x` in it does not delete the row behind it, and the first `Esc` puts the line away rather than
the view. The prompt names itself — `new task`, `rewrite`, `new feature`, `rename` — which is the
whole of the affordance, since there is no other cue that the keys have changed meaning.

### Limits of the current model

A feature is still both a context container and a unit of work. Its selection is shared by every
pane in a checkout, and a checkout with one originating feature selects it without an explicit
assignment. The managed skill asks agents to read that feature, its decisions, and its tasks, but
the daemon does not decide whether they apply to the user's current request. Agents pull these reads
at startup or task changes; unlike clients, they receive no board updates while running.

The stored briefs, decisions, tasks, attribution, and supersession are useful durable material, and
retiring the Kanban states removed the second tracker an operator had to maintain. What is still
missing is the rest of TARGET.md, "Agent context and memory": work contexts that a checkout may
suggest but never silently assign, typed artifacts beyond decisions and tasks — findings,
assumptions, open questions, summaries — and a bounded context packet that says why each durable
item was included. Until those land, an agent still infers its work from whichever feature its
checkout is on.

## Editors and overlays

Files open in one of three modes:

- a floating PTY overlay, the default;
- as a pane in the checkout — though an editor is never a listed pane, so the workspace cannot
  hold it and it floats as the first mode does;
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
