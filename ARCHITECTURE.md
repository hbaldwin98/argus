# Argus: Architecture

The rules the code is built to and the reasons behind them. What each control does is in
[`docs/`](docs); what is still to build is in [`ROADMAP.md`](ROADMAP.md); what Argus is for is in
[`TARGET.md`](TARGET.md). Where each question is answered in the code is each module's `//!` header.

## Processes

- `argusd` owns everything that outlives a window: PTYs and their terminal state, Git observation,
  agent integration, extensions, browsers, and the runtime store.
- `argus` is the terminal client. It starts the daemon lazily when it cannot connect: detached
  (`setsid` on Unix, a detached process group on Windows), so closing the client stops nothing.
- `argus-hook` is what managed hooks run and what an agent runs to report on itself.
- `argus-desktop-next` (`crates/argus-gpui`) is the desktop window, and `argus-desktop` the Tauri
  window it replaces. Both build on `argus-desktop-core`, which holds what the window holds and says
  with no window in it.

The daemon takes an OS-level instance lock and binds its endpoint without replacing a live socket
before it restores a pane, so a second start exits having touched nothing. A Unix socket path is
removed only after a failed bind proves no daemon answers on it. `argus server restart` hands over
explicitly: the daemon acknowledges, releases the endpoint and removes its managed hooks, and the
replacement starts through the same lazy path and restores the panes. `argus server stop` also tells
every client to exit, so no reconnect loop starts a replacement.

**One way to start a daemon.** The desktop window never opens the endpoint and never starts `argusd`.
It runs `argus bridge` and speaks the protocol over the child's stdin and stdout, exactly as
`argus --host <host>` runs `ssh -T -o BatchMode=yes -- <host> argus bridge`. A client never starts
again a daemon somebody stopped: after `Stopping` the window waits for its Reconnect button.

The daemon's stderr is the null device, since it shares no console with the client. It logs to
`argusd.log` beside its config, keeping the previous run's as `argusd.log.1`, because a daemon that
is starting has often just stopped in a way somebody wants to read about.

**The client never predicts.** A client changes what it shows when the daemon's tree says it
changed, so a refused action leaves the screen showing what is true.

## Protocol

Everything the binaries must agree on is in `argus-protocol`, since they share no other types and a
contract written twice drifts in silence. Messages are named MessagePack records framed by a
four-byte big-endian length; frames over 64 MiB are refused. The socket or pipe is the only
transport, and it is not authenticated.

**Never send an older peer a message it cannot read.** A whole frame is read before it is decoded,
so a message a newer peer added is skipped and the stream stays aligned (`read_known_msg`). What is
new is negotiated: the client's first message is `Hello`, naming its protocol number, version and
the optional messages and encodings it takes. The daemon answers a `Hello` only when one arrives,
and either side uses an optional message only when the other listed it. A daemon from before the
greeting hangs up on it, and the client reconnects without greeting. The protocol number is raised
only for a change nothing can be negotiated across. A client tells a daemon from another build on
its status bar.

**Topics recover by state, not by backlog.** What a client is pushed comes in topics (the tree, the
workspaces, the boards, the focus, views, extensions), each forwarded in order on a task of its own.
A client that falls behind is sent the topic's current state whole, never the queue it missed.
Order holds within a topic and not across them.

**A request that makes something names itself.** A `request_id` is answered with `Created` to that
client alone, and the client acts once it holds both the answer and a tree containing what was made,
in either order, never by guessing which new row is its. A named request that is turned down is
`Refused` by that id. An unnamed one (id zero, what an older client sends) is never answered.

**Where the person is working** is the client's to know and the daemon's to be told. A client sends
a `View` when its selection changes or its terminal regains focus; the focus is the most recently
active client's view. `argus focus` reads it for scripts and status lines, and never starts a
daemon.

**Remote hosts.** Everything the daemon does stays beside the agents it runs. ssh runs in batch mode
because the terminal is the client's and ssh has nowhere to ask. Connecting never installs argus on
the other machine: a client that drops binaries on another host looks like malware, and installing
software is the user's act. Each host is an app of its own; a host off screen holds no pane
subscriptions.

## Storage

**`projects.toml` says what exists; `runtime.db` says what happened.** The config file is the
user's, comments included, and Argus only reads it. Everything Argus writes goes to the store: a
project added from a client is an overlay keyed on its root, and a project the config declares is
recorded as hidden rather than deleted from the file. Each client keeps its own settings
(`client.toml`, `desktop.json`), since a shared file would be a contract both binaries had to keep.

`runtime.db` is SQLite in WAL mode, versioned on `user_version`. A store from a newer Argus is
refused rather than migrated backwards. A store that cannot be opened costs the run its memory, not
its startup: the daemon falls back to an in-memory store, which also cannot overwrite whatever made
the file unreadable. Persistence is a store the daemon is handed, never a flag: only `main` passes
the file, which is what keeps the test suite off the user's state.

**A write is a transaction.** A write that fails leaves the store as it was. The pane table is
replaced in one transaction, since the tree is the truth and the table follows it.
`store::tests::faults` fails every write the store makes at each of its steps and checks every row
is as it was before or as the write leaves it. Two are left out of it: a migration runs in one
transaction with the version it reaches, and the one-time import of legacy files inserts with
`OR IGNORE` and retires a file only once it is in, so either finishes when run again.
Known gap: a write whose commit landed can still return an error if a read after the commit fails,
and a client that retries then writes twice. The intended fix is that a write answers from what it
committed and logs the failed read.

## Panes and terminals

Shells, agents, editors and consoles share one PTY primitive. A console belongs to no checkout and
travels beside the tree, not in it. An agent whose process exits stays `Exited` unless its template
sets `restart`; three restarts in a minute stop the cycle.

**The pump.** A reader thread feeds a bounded queue; a task wakes on the first byte, takes up to a
batch, and waits 16 ms after a frame, so a stream is coalesced while a keystroke's echo goes out at
once. A second thread waits on the child's exit, so an idle pane wakes for nothing. The emulator is
`alacritty_terminal` because it tracks damage: only damaged lines are converted, and each frame is
diffed against the grid the client holds into runs of cells, with a scrolled region sent as a move.
A wrong scroll guess costs bytes, never a wrong screen. The daemon is the pane's terminal, so it
answers what a terminal is asked (cursor position, device attributes) as the emulator reaches each
question, and honours synchronized updates with a deadline.

**A slow client sees the latest screen late, not every screen later and later.** Each connection
has a bounded queue of 32 pane frames and an unbounded one for everything else, written first, so
a slow screen never holds up the answer to a request. A frame that finds the pane queue full is
dropped and the next free slot carries a fresh grid. A grid and its damage stream are taken under
one lock, so nothing falls between them.

**Size follows the person.** A pane is sized to the client the person was last at, as tmux's
`window-size latest`; other clients pad or crop. Falling back to the smallest request only when no
client has reported a view keeps a window from padding a terminal-sized grid.

**Paste is never a command.** The daemon wraps a paste in bracketed-paste markers only when the child
asked for them, and strips markers from the text first, so a pasted end marker cannot turn the rest
into typed commands. A paste with nowhere to land is reported, never replayed as keys. Argus never
asks the terminal for its clipboard over OSC 52; the reply would arrive as typed text. An image
crosses as a file the daemon keeps under its own directory, since the pane's process reads the
daemon's disk, not the client's.

Keys are legacy xterm bytes until a child negotiates the kitty keyboard protocol; the flags ride on
every frame. Focus reports reach only a child that enabled them.

**Windows.** Each pane's console server and child run in a Job Object of its own that kills what it
holds when its last handle closes, so however the owner dies the OS ends them. Jobs cap neither
memory nor processes (a cap ended agents mid-task). The daemon's first act is to confine library
loads by name to its own directory and System32, since portable-pty loads `conpty.dll` by bare name
and the inherited working directory is often a repository.

## Agents

**State comes only from explicit harness events.** Process activity cannot separate an agent
thinking from one stopped at a prompt, and matching output reads an agent's prose as its status. A
harness that reports nothing stays `idle` until it exits. A note travels with a status and is
cleared by the next report without one, so it never outlives the state it explains. Rows that stand
for several panes all roll up through one ranking in `tree`.

**How a harness reports.** Every agent pane gets `ARGUS_HOOK_URL`, `ARGUS_HOOK_TOKEN`, `ARGUS_PANE`,
`ARGUS_HOOK` and `ARGUS_INSTRUCTIONS`, so any CLI that can run a command can report. On top of that,
a harness may have Argus write a managed hook block or a plugin module. Hook files name a per-boot
port and token, so they are swept from every checkout at startup and removed when the last agent in a
checkout leaves; Codex is the exception, since its trust-sensitive command must stay identical. A
write that would change nothing is skipped, because Claude Code stops firing a running session's
hooks once their file changes. Generated paths go in `.git/info/exclude`, never a tracked file. What
each endpoint takes and answers is declared once, in `argus_protocol::hook::call`.

**A pane belongs to the session that claimed it.** A report from any other session (a CLI started
inside the pane, which inherits its URL and token) is listed as a child and can change nothing about
the pane: not its title, status, checkout or conversation. Claude Code's subagents and workflow
agents are not listed at all, but a turn that ends with background work still running reads as
working until it finishes.

**What Argus says to an agent goes through its harness when it can.** Typing goes to whatever has
the agent's keyboard, and a dialog takes it as an answer. A plugin inside the agent (OpenCode, pi,
the Claude Code mod) holds the pane's inbox open and turns a message into a queued prompt. A harness
without one is typed into.

**The row follows the agent.** A turn that ends in another checkout's directory moves the row there,
without restarting the PTY. A tool call's directory is not followed, since an agent reading another
worktree has not moved. A worktree removed under a running agent moves its pane to the primary
checkout rather than killing it mid-command.

**Restore is relaunch, not reattachment.** PIDs are not stored. After a restart each recorded pane
is started fresh in its checkout, an agent with its harness's exact `resume_id` and the session id
its hooks captured; an id enters argv, never a shell. An agent resumed that exits non-zero within
five seconds is replaced by a fresh one. Editors and browsers are not restored.

Argus never stores a transcript. It reads the harness's own file when asked, and records only what it
saw of the sessions of a harness that keeps none.

## Git

Read-only work uses `git2`; mutations run the `git` executable, so the result is what the user's own
configuration would have produced. Argus never pushes, never merges, and deletes a branch only
locally, asking again before forcing an unmerged one.

Status is cached on the checkout and refreshed off the daemon's lock, because a keystroke needs that
lock to find its PTY. A two-second poll keeps dirty counts; a watch on each repository's Git metadata
(never `objects`) and a recursive watch on checkouts with panes make changes arrive as they happen.
**A status that could not be read is unknown, not empty**: a poll landing mid-`git switch` once
reported a checkout on no branch and rearranged the rail.

A root is rescanned every ten seconds. A repository deleted from it leaves only once nothing runs in
it, since a directory can vanish for reasons unrelated to the person's intent. A path named in
`repos` is taken at its word and no scan removes it.

## Review and code

A review is two snapshots, the sides Git keeps apart: `HEAD` against the index, and the index
against the working tree including untracked files. Each request writes its trees to the object
database without touching `HEAD`, branches, the index or files. Comparing against a fork point or a
remembered snapshot is a Git client's job, and so are staging, unstaging and reverting.

Highlighting happens in the daemon, which holds whole blobs; a parser given a hunk reads a fragment.
Spans carry token roles, never colours, so the client's theme keeps the palette. Identifiers are left
untagged so changed lines stay visible. Unified and split views are the client's, over the same
hunks, and a comment anchors to both line numbers so it means the same in either.

A comment is stored before its recipient is told, and a failed delivery keeps it. Only a live agent
in the checkout can be its recipient.

**When editing, the file on disk is the authority and the daemon referees it.** An edit is written
only when its base is the version the disk still holds; otherwise the client is sent what changed
and makes the edit again on top. An agent's write is sent the moment it settles. The page holds the
buffer, the one piece of window state Rust does not, because a key must be drawn in the handler that
heard it.

## Durable context

Features, tasks and decisions are written mainly by agents and corrected by people.

- **A board belongs to a repository**, keyed on its shared Git directory, so every worktree shares it
  and it outlives the worktree a feature was built in. Cross-repository work opts in with
  `ARGUS_ARTIFACT_SCOPE=workspace`.
- **Nothing is deleted from the decision record.** A decision overtaken is superseded and kept,
  dimmed, because the road not taken is most of what a reader comes back for. One made in error is
  withdrawn and kept. An open decision is a question for a person, and every read carries it.
- **One path for both sides.** An agent and a person change the board through the same apply, with
  who asked as a parameter, so nothing a person can do is closed to an agent or guarded differently.
- **Standing is observed, not maintained.** What a feature row says comes from the agents running in
  its checkouts and its tasks' states. The only state a person sets is accepting the work, because
  the agent that did it cannot.
- **Numbers are never reused**, so a `#7` said last week still names the same feature.
- **Trackers are not synchronized.** A feature may link tickets; reconciling them is an agent's
  proposal, carried out only as far as the person ticks it.
- **Views are data.** What an agent or extension shows is a fixed vocabulary of blocks the window
  draws, never markup.

What is still missing is the rest of [`TARGET.md`](TARGET.md) §"Agent context and memory": an agent
still infers its work from whichever feature its checkout is on.

## Browsers and extensions

A browser is a pane owned by the agent that opened it. It is headless Chrome on a fresh profile,
driven over anonymous pipes, so no other process on the machine can reach it. The agent gets a
DevTools URL on the pane API carrying its token, and brings whatever tool it likes. The daemon, not
the window, reads the screencast, so a window on another host sees the same browser. While the
person holds a browser, an agent's input and navigation are refused.

An extension is a program the daemon starts from `[[extension]]` with a token minted for it, so only
an extension the daemon started may publish and none can speak as another. Everything it says is
declarative JSON in `argus_protocol::extensions`. The daemon owns it so it works under `--host`. Its
keys never shadow Argus's, and while it is down everything it published is withdrawn.

## Testing

Tests live beside the code. Beyond unit tests and proptest properties:

- **Characterization transcripts.** `test_host` runs the daemon in-process behind its real accept
  loop, and `test_host/tests` pins what it answers in the conversations clients have most. A changed
  snapshot needs a person's approval.
- **Request fuzz.** Arbitrary sequences of every `ClientMsg` against a prepared host: the daemon
  never hangs up, hangs or panics, every named request is answered or refused by its id, and an
  unnamed one is never answered.
- **Store faults.** See Storage.
- **Mutation testing** of the daemon's store and board rules, and `fuzz/` for input from outside the
  process. `AGENTS.md` has the workflow.
