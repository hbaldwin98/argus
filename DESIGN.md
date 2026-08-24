# Argus

A terminal multiplexer with one idea: **group repositories into workspaces and run AI agents against their branches.**

Not a general-purpose tmux. Everything below exists to serve a single navigational spine:

```
Project  ──▶  Branch / Checkout  ──▶  Agent (or shell, or editor)
```

Left to right. You always know where you are, and you can always go one level up.

---

## 1. Goals & non-goals

**Goals**

- Sub-16ms frame time, <50 MB RSS for the UI with a dozen live panes.
- Agents survive the UI. Close the terminal, reboot the SSH session — work continues.
- First-class review: see what an agent *changed*, not just what it *said*.
- Real editor (vim/neovim/helix) as a pane, not a shell-out that blanks the screen.
- Agents are participants: they can read project context and write notes back.
- Zero-config for the common case: point at a directory of repos, go.

**Non-goals**

- Being a tmux replacement. No arbitrary window trees, no session sharing, no scripting DSL.
- A GUI. This is a TUI. (A future thin GPU frontend can reuse the daemon protocol.)
- Hosting/executing agents ourselves. We spawn CLIs (`claude`, `aider`, `codex`, whatever) as processes.
- Git porcelain. We list branches, manage worktrees, and read diffs; we don't reimplement git.

---

## 2. Architecture

Two binaries, one protocol.

```
┌─────────────────┐        unix socket / named pipe        ┌──────────────────────┐
│  argusd         │◀──────── length-prefixed msgpack ─────▶│  argus (TUI client)  │
│  (daemon)       │                                        │  ratatui + crossterm │
│                 │                                        └──────────────────────┘
│  • PTY pool     │                                                  ▲
│  • scrollback   │                                        multiple clients may
│  • git watcher  │                                        attach to one daemon
│  • state store  │
│  • ctx server   │◀──── loopback HTTP + MCP ──── spawned agents
└─────────────────┘
       │ forkpty
       ├── claude   (acme-api @ main)
       ├── nvim     (acme-api @ main)
       ├── aider    (acme-api @ feat/rate-limit)
       └── zsh      (acme-api @ feat/rate-limit)
```

**Why a daemon.** Detachment is the whole point. `argusd` owns every PTY and every scrollback
buffer; the client is a pure renderer with no state worth losing. It is started lazily on first
`argus` invocation (double-fork, `setsid`, reparented to init) and idles at near-zero CPU —
epoll/kqueue on PTY fds, no polling loop.

**Why Rust.** Predictable memory, no GC pauses in the render path, and the ecosystem is already
here: `portable-pty` for cross-platform PTYs (ConPTY on Windows), `vt100`/`wezterm-term` for
terminal emulation, `gix` for git without shelling out, `ratatui` for rendering, `notify` for
filesystem watching.

**The protocol.** Length-prefixed msgpack over a Unix domain socket (`$XDG_RUNTIME_DIR/argus.sock`)
or a named pipe on Windows. Client→daemon: `Attach`, `Input{pane, bytes}`, `Resize`, `SpawnAgent`,
`Kill`, `Subscribe{view}`. Daemon→client: `Damage{pane, cell_runs}`, `TreeDelta`, `AgentState`,
`GitDelta`. The daemon sends **damage regions, not full screens** — a spinner in a background
pane costs a handful of bytes per frame, and a client rendering a different view costs nothing.

---

## 3. Memory model

The naive approach — one `Vec<Cell>` per scrollback line — costs ~16 bytes/cell and dominates
everything. Instead:

- Scrollback lines are stored **compressed and packed**: each line is a run-length-encoded byte
  blob (text + SGR runs), decoded only when scrolled into view. Typical agent output compresses
  10–30×. Budget: **2 MB/pane default**, configurable, with oldest-first eviction.
- The live screen (visible rows only) is a dense grid — that is the hot path, keep it flat.
- Panes with no client subscription for N minutes drop their decoded screen and keep only the
  compressed tail; re-attach replays it.
- Scrollback above a threshold spills to a memory-mapped file under `$XDG_CACHE_HOME/argus/`,
  so a runaway `cargo build` log costs address space, not RSS.

Target: ~1.5 MB resident per idle pane, ~40 MB daemon with 16 panes, ~25 MB client.

---

## 4. The three levels

**Floating windows.** Some things do not fit a 38%-wide column: a terminal editor, and a
settings panel with room to say what each option does. Those open as an overlay — a large panel
above the columns, with the tree still framing it, because losing your place is what the whole
layout exists to prevent.

The review is a floating window too, for the same reason: reading a diff should not cost you
sight of the agent that produced it. Only the live pane ever occupies the rightmost column.

An editor is **not a pane in the tree**. It is a way of looking at a file, not something
running in the checkout that you would come back to, so it is left out of the panes column and
out of every count and status rollup. That in turn makes its window the only handle on it —
which is why closing the window ends the editor, while a window over a *listed* pane (a shell,
an agent) only stops you looking at it.

A floating pane streams *alongside* the column, never instead of it: a connection carries one
subscription per pane on screen, and each pty is sized from where that pane is actually drawn.
Opening a file must not cost you sight of the agent running behind it, so opening one also
leaves the pane selection alone — close the window and you are back where you were, not on the
editor. Every key belongs to the editor while it is up, `q` included; the leader is the way
out.

A floating window must always be closable. The leader is the polite way out, but it depends on
the terminal delivering Ctrl-Space and a floating pane deliberately swallows every other key, so
there are three independent escapes: **F12** is checked before any handler and never forwarded
to a child, a **click outside** dismisses the window, and a window whose pane exits or leaves
the tree closes itself. The title carries the way out, because a window you cannot leave is
worse than no window at all.

`S` opens settings. Changes apply and save as they are made, to `client.toml` beside the
daemon's `projects.toml`: what exists is the daemon's business, how one client draws is not.
`editor opens` chooses between the floating window (the default), the rightmost column, and
outside Argus entirely — the last spawns detached with no pty, for editors that bring their own
window. `editor command` names the program, flags and all; left empty it falls back to
`$VISUAL`/`$EDITOR` and then to whichever terminal editor is actually installed.

A **GUI editor is always launched externally**, whatever the mode says. One in a pty is a blank
pane whose child never speaks — indistinguishable from a hung editor, which is precisely how
`notepad` presented before this rule existed.

**Fuzzy pickers.** `b` switches branch, `f` opens a file, and `f` inside the review jumps to a
changed file. All three filter as you type. Ranking is `nucleo-matcher` — fzf's algorithm as a
library — and the file list comes from `ignore`, the crate ripgrep and fd are built on, so
`.gitignore` is honoured with the rules the user already expects. Both are in-process
deliberately: shelling out would mean a console window per invocation on Windows, where the
daemon owns no console (§9 M4, and the note on `git::list_worktrees`), and would make the
features depend on tools that may not be installed. `fzf` itself is not usable here at all —
it is an interactive program that wants a terminal of its own.

Typing a branch name that doesn't exist adds a `+ create` row below the matches, so making a
branch and switching to one are the same gesture. It is a row you can see and aim at, never an
implicit consequence of pressing Enter on an empty list. This is `git switch -c` in place —
distinct from `n`, which puts the new branch in a worktree of its own.

**Mouse.** The two gestures are separate on purpose: clicking anywhere on a card moves focus to
it and leaves every selection where it was, while clicking a row selects that row — and clicking
an already-selected row a second time descends into it, the way `l` would. Moving focus never
disturbs the live pane, so the session you were watching keeps streaming in the rightmost column
while you look around. Rows are two lines tall and either line counts as the same item.

### Level 1 — Projects

A **project** is a named group of repositories. Not one repo — a group, because real work spans a
frontend, a backend, and a shared library.

```toml
# ~/.config/argus/projects.toml
[[project]]
name = "argus"
repos = ["~/src/argus"]

[[project]]
name = "acme"
repos = ["~/src/acme-api", "~/src/acme-web", "~/src/acme-proto"]
scan = "~/src/acme-plugins/*"   # glob, re-scanned on open
```

The project pane lists projects with a live rollup:
`acme  3 repos · 5 checkouts · 2 agents · 4 todo ●`.
A colored dot per project aggregates the worst agent state beneath it (§8b), so an unattended
failure is visible from the top level without drilling in.

### Level 2 — Checkouts (branches)

The unit here is a **checkout**: a working directory sitting at a branch. Git already models this
exactly — `git worktree list` includes the repo's primary working tree — so Argus does too. The
main checkout is a first-class row, not a special case, and a linked worktree is just another
checkout that happens to live elsewhere on disk.

**If you want to work off `main` in the repo you already have cloned, you do.** No worktree gets
created, nothing is moved, and the agent you spawn there runs in `~/src/acme-api` like it would if
you had `cd`'d in yourself. Worktrees are a tool Argus makes cheap, not a toll it charges.

Selecting a project lists every repo and, nested under it, its checkouts:

```
acme-api                                     ~/src/acme-api
  ● main              ⌂  +12 −0     2 agents            ⚠ shared
  ● feat/rate-limit   ⧉  +142 −18   1 agent   ⚠ needs review
    fix/oauth-refresh ⧉  +8 −3                ✓ reviewed
    hotfix/tls-expiry ⌥  (no checkout)
acme-web                                     ~/src/acme-web
    main              ⌂  clean
```

- `⌂` primary checkout · `⧉` linked worktree · `⌥` a branch with no checkout at all.
- Branches without a checkout are listed because they are still things you might want to work on.
  Selecting one asks how to open it: **switch the primary checkout** (`git switch`, if it is
  clean) or **create a worktree** (`n`). Either way you land in a checkout and continue right.
- `git switch` on a dirty primary checkout is refused, with the offer to make a worktree instead.
  This is the one place Argus nudges — not because worktrees are ideologically correct, but
  because clobbering uncommitted work is the failure we actually have to prevent.

**Isolation is a default, not a law.** One agent per checkout is what Argus sets up and what the
UI assumes. Nothing stops you from putting two agents on `main` — Argus marks the row `⚠ shared`,
colors it amber, and shows both agents' names on the checkout row so the risk is visible rather
than discovered at merge time. Per-project `exclusive = true` turns it into a hard block if you
want the guardrail.

`n` creates a worktree: prompts for a branch name (defaulting to a branch off the current one),
runs `git worktree add`, and optionally runs a per-project `setup` hook — `npm ci`, `direnv
allow`, copying `.env`. Worktrees live in `.argus/worktrees/<branch>` beside the repo, or wherever
config says. `D` removes a checkout, its branch, and any agents attached, after a confirmation
showing exactly what disappears — and it refuses outright on the primary checkout, which is yours,
not Argus's to delete.

### Level 3 — Agents

Each checkout holds a list of panes. Three kinds, same primitive underneath:

- **Agent** — an AI CLI, spawned from a template.
- **Shell** — your `$SHELL`, cwd'd into the checkout.
- **Editor** — `$EDITOR`, a full PTY pane. Neovim runs *inside* Argus the same way it runs inside
  tmux, with full mouse, true color, and its own keymaps. See §6 for how we stay out of its way.

Agents are declared as templates, not hardcoded:

```toml
[[agent]]
name = "claude"
cmd  = ["claude"]
idle_when = "esc to interrupt"     # absent ⇒ idle
notes = "pinned"                   # inject `- [!]` constraints at spawn (§5b)
allow = ["note.*", "status.*"]     # context-server permissions (§5c)
env  = { CLAUDE_PROJECT_DIR = "{checkout}" }

[[agent]]
name = "review"
cmd  = ["claude", "-p", "Review this diff. Be terse.\n\n{diff}"]
oneshot = true
```

`a` opens a template picker; the agent spawns detached, in the checkout's directory, and its pane starts
streaming immediately.

---

## 5. Review — the part that has to be good

Vetting output is the reason this exists. `Tab` from any agent pane toggles into **Review** for
that checkout, a two-column view:

```
┌ files ────────────┬ diff ──────────────────────────────────┐
│  M src/pty.rs +42 │  @@ -18,6 +18,9 @@ impl PtyPool {      │
│  M src/ui.rs   +8 │  +    fn reap(&mut self) {             │
│  A src/grid.rs+210│  +        self.children.retain(..)      │
│  D old/legacy.rs  │  ...                                    │
│                   │                                         │
│  ✓ 2 of 4 vetted  │                                         │
└───────────────────┴─────────────────────────────────────────┘
```

- Diffs come from `gix` in-process — no `git diff` subprocess per keystroke. Rendering is
  syntax-highlighted via tree-sitter, incrementally, only for visible hunks.
- Three diff bases, cycled with `b`: **working tree** (uncommitted agent edits), **vs. branch
  point** (everything this branch did), **since I last looked** — the last one is the killer.
  Argus snapshots the tree hash each time you leave Review, so returning shows *only what the
  agent did while you were away* — and it is the base that still works on `main`, where "vs.
  branch point" has nothing to point at. On a checkout with no meaningful fork point, `b` cycles
  between working tree, since-I-last-looked, and vs. upstream (`@{u}`) instead.
- `v` marks a file vetted; `V` marks all. Vetted state is keyed on (checkout, blob hash), so
  re-editing a file un-vets it automatically.
- `s` stages a hunk, `u` unstages, `x` reverts a hunk — the three verbs you actually need.
- `c` leaves a comment on a hunk. Comments are notes (§5b) anchored to a file and line, and an
  agent can read them back via `context.review` — which makes "here is what is wrong with your
  patch" a one-keystroke round trip instead of a retyped paragraph.
- `e` opens the file in an editor pane **at the cursor's line**, the natural exit ramp from
  "this is wrong" to fixing it yourself.

The file watcher (`notify`, debounced 100ms) marks the checkout dirty; the diff recomputes lazily
on next view, never in the background hot path.

---

## 5b. Notes

Every level of the tree can hold notes. A note is a plain markdown file on disk — no database, no
lock-in, greppable, committable if you want it to be.

```
~/.local/share/argus/notes/<project>.md          # project-level
<repo>/.argus/notes/<branch>.md                  # branch-level, next to the code
```

`o` opens the notes pane for whatever is selected — a split alongside the current column, edited
with your `$EDITOR` (it is just another editor pane, §6). Notes at a parent level are visible from
children: standing in a checkout you see that branch's notes *and* the project's, stacked, so
"the staging DB resets nightly" written once is visible everywhere it matters.

Checkbox lines are structured, and only checkbox lines:

```markdown
- [ ] rate limiter needs a jitter test
- [x] drop the legacy /v1 route
- [!] DO NOT touch the migration in 0042 — hand-edited
```

`- [ ]` open, `- [x]` done, `- [!]` a pinned constraint. Open counts roll up the tree the same way
agent state does, so the things you meant to get back to are visible from the project column
rather than lost in a file you forgot you wrote.

**Forwarding to an agent.** Notes never go to an agent implicitly. Three explicit paths:

- `<leader>f` on a note line — forward that line (or a visual selection) into the focused agent's
  input, as text you can still edit before sending.
- `F` on a note file — forward the whole file, wrapped in a labeled block so the agent reads it as
  operator context rather than a fresh instruction.
- Per-template `notes = "pinned"` — inject only `- [!]` lines at spawn, as part of the initial
  prompt. This is the one automatic path, and it is opt-in per template, because pinned
  constraints are exactly the thing every agent should start with.

Anything an agent writes back (`note.append`, §5c) lands in the same file, tagged with the agent
name and a timestamp, so the handoff runs both directions and stays auditable.

---

## 5c. The context server

`argusd` exposes an HTTP + MCP server on a loopback port. Every agent it spawns gets `ARGUS_URL`
and a scoped `ARGUS_TOKEN` in its environment; the token is bound to one checkout, so an agent can
only see and touch its own.

This is what turns Argus from a window manager into something the agents participate in.

**Read side — pull context down.**

| Call | Returns |
|---|---|
| `context.tree` | the project, its repos, and their branches and checkouts |
| `context.notes` | notes visible at this checkout, parents included |
| `context.diff` | the current diff, any of the three bases from §5 |
| `context.review` | which files are vetted, which are not, and reviewer comments |
| `context.siblings` | what other agents in this project are doing, and on what branches |

`context.siblings` is the interesting one: an agent on `feat/rate-limit` can discover that another
agent is mid-flight in the same file on `fix/oauth-refresh` — before both of you find out at merge.

**Write side — participate.**

| Call | Effect |
|---|---|
| `note.append` | add a note or todo at branch or project level |
| `todo.check` | tick a checkbox it finished |
| `status.set` | declare its own state: `working` / `blocked` / `needs-review` (§8b) |
| `review.request` | flag the checkout for review, optionally naming files to look at |
| `worktree.create` | branch off and hand work to a fresh isolated checkout |
| `agent.spawn` | delegate to another agent template in that new checkout |

Write calls are policy-gated per template. `allow = ["note.*", "status.*"]` is the sane default;
`worktree.create` and `agent.spawn` are off unless you enable them, and both are capped by a
configurable fan-out limit so a delegation loop cannot fork bomb the machine.

**Referencing into any harness.** Not every CLI speaks MCP, so the same data is reachable three
ways, all backed by one implementation:

- **MCP** over stdio or HTTP, for harnesses that support it — the good path.
- **`argus ctx <what>`** — a small CLI printing the same payloads to stdout, so any agent that can
  run a shell command can pull context: `argus ctx diff --base branch-point`.
- **Template interpolation** — `{diff}`, `{notes}`, `{pinned}`, `{branch}`, `{siblings}` expand
  directly into a template's argv or prompt file, for one-shot agents that take a prompt and
  nothing else.

---

## 6. Editor panes and the keybinding problem

Running vim inside a multiplexer means fighting over keys. Argus's answer: **a single leader key
and no ambiguity.**

- Default leader: `Ctrl-Space` (rebindable). Nothing else is intercepted, ever. Inside a pane,
  every byte goes to the child — `Ctrl-w`, `Esc`, `Ctrl-a`, all of it.
- `<leader>h/j/k/l` moves focus, `<leader>1..9` jumps to a pane, `<leader>Tab` is review,
  `<leader>o` is notes, `<leader>Esc` steps up a level (agent → checkout → project).
- Navigation panes (the three columns) are modal and vim-native: `hjkl`, `/` to filter, `gg/G`.
  Moving right descends, moving left ascends. That is the whole model.
- Editor panes get full mouse pass-through, bracketed paste, focus events, and OSC 52 clipboard
  forwarded to the outer terminal so yanking works over SSH.

---

## 7. Safety

"Safe" here means an agent cannot quietly ruin your day.

- **Isolation by checkout** is the default posture — an agent's blast radius is one working
  directory, and Argus flags a shared checkout in amber rather than pretending it cannot happen.
- **Scoped tokens.** A context-server token grants exactly one checkout and exactly the calls its
  template allows. There is no ambient authority to reach sideways into another branch.
- **Optional sandbox per template**: `sandbox = "readonly-outside"` wraps the child in the platform
  sandbox (Landlock on Linux, Seatbelt on macOS, Job Objects + restricted token on Windows) so it
  can write to the checkout and nothing else. Off by default, one line to enable.
- **Never auto-push, auto-merge, or auto-delete.** Destructive operations are explicit keystrokes
  with a confirmation showing exactly what disappears.
- **Crash-safe state.** The daemon journals its tree (projects, checkouts, panes, pids) to
  `state.json` via atomic write-and-rename. On restart it reattaches to surviving children and
  marks the dead ones `crashed` rather than silently dropping them.
- **Secrets do not land in scrollback on disk.** Spilled scrollback files are `0600` and live in
  cache, and there is a per-template `redact` list applied before persistence.

---

## 8. Performance discipline

- Render only on damage. Idle Argus issues zero syscalls per second beyond the epoll wait.
- Coalesce PTY reads: drain the fd, parse once, emit one damage set per frame, cap at 60 Hz.
  A pane spewing output at 100 MB/s costs bounded work — we parse and discard, we do not queue.
- No `String` allocation in the render loop; a per-frame arena, reset each pass.
- Diff, syntax highlighting, git status, and context-server requests run on a small rayon pool,
  never on the input thread.
- Startup budget: **<30 ms** from `argus` to first paint when the daemon is warm.

---

## 8b. State, and seeing it

Every pane has one state, the state has one color, and the color appears at every level of the
tree at once. Glanceability is the requirement: from the project column, without pressing
anything, you should know whether something needs you.

| State | Color | Meaning |
|---|---|---|
| `idle` | dim gray `·` | spawned, nothing happening |
| `working` | blue, gently pulsing `◐` | producing output, do not interrupt |
| `waiting` | **amber, blinking** `?` | blocked on you — a permission prompt, a question |
| `needs-review` | **magenta** `⚠` | finished, and touched files you have not vetted |
| `done` | green `✓` | finished, everything vetted |
| `failed` | **red** `✗` | non-zero exit, or `status.set(blocked)` |

Rules that make this trustworthy:

- **Attention states are the loud ones.** `waiting`, `needs-review`, and `failed` are the only
  states that blink, and the only ones that propagate a dot up to the project row. `working` is
  deliberately calm — most of the time an agent is working and you do not care.
- **Parents show the worst child.** A project row's dot is the highest-severity state anywhere
  beneath it, so drilling down always narrows toward the thing that wants you.
- **Transitions flash, briefly.** A pane crossing into `waiting` pulses its border for ~400ms.
  Peripheral vision catches motion far better than color, and the flash is what makes you look at
  the right pane on a screen full of them.
- **Three sources of truth, in priority order:** explicit `status.set` from the agent (§5c), then
  process state (exited / blocked on tty read), then the template's `idle_when` output pattern.
  An agent that speaks the protocol is always right; scraping is the fallback, never the override.
- **Readable without color.** Every state carries a distinct glyph, the palette is configurable
  for colorblind schemes, and colors degrade to the terminal's 16-color palette when true color
  is unavailable.

Optional, off by default: an OSC 9 desktop notification when a pane enters `waiting` or `failed`
while its project is not the focused one.

---

## 8c. Testing

Behaviour is verified by `cargo test`, not by launching the TUI and looking at it. Driving a
real terminal is slow, needs a human to read the screen, and only exercises whatever path
happened to get clicked — a race that loses a short-lived pane's output shows up in a test
loop long before anyone notices it by eye.

Three layers, all in-tree unit tests (both binaries are single-crate, so `#[cfg(test)]` modules
are the only option — there is no library target for `tests/` to import):

- **Pure logic** — wire framing and grid diffing, key encoding, git porcelain parsing, hook
  path/JSON handling, the status roll-up and its glyphs. No I/O, instant.
- **State machines** — `App` is constructed over an `mpsc` channel and driven with synthetic
  key/mouse events; tests assert on the `ClientMsg`s it emits and the focus/selection it lands
  in. `Daemon`'s worktree reconciliation takes its listing as an injected closure, so it is
  tested without a repo or the `git` binary.
- **Real processes** — `PaneRuntime` tests spawn actual pty-backed children, type at them, and
  assert on the grid and the damage broadcast. These are the end-to-end M1 spine checks that
  used to require a terminal.

Rules that keep the suite worth having: no fixed sleeps waiting for output (poll the grid to a
deadline instead), every test names the behaviour it protects rather than the function it
calls, and anything that shells out to `git` or a real config file gets a seam instead.

---

## 9. Roadmap

**M1 — spine.** Daemon + client, PTY panes, three-column navigation, shell panes, detach/reattach.
Nothing AI-specific. If this is not fast, nothing else matters.

**M2 — git.** Branch and checkout listing, `switch` and `worktree add`/`remove`, status rollup,
file watcher.

**M3 — agents.** Templates, spawn, state machine + colors (§8b), per-checkout agent lists.

**M4 — review.** Diff view, the three bases, vetted tracking, stage/revert, comments,
jump-to-editor. Concretely, the shape wanted here:

- Enumerate a checkout's changes as a navigable list of files and hunks, inside the client.
  *(landed)* `R` on a checkout opens the working tree against HEAD in a floating window,
  untracked files included; `]`/`[` move between files, `v` extends a selection over a range.
- Attach a comment to a line or a *range* of lines, the way a pull-request review does, and
  send it straight to the agent working that checkout. *(landed)* `c` in the review composes
  one; it is typed at the agent's pty as `path:line \`+the line\`: your comment`, so it works
  with any harness rather than needing one to know about Argus.
- Open any of them in the user's real editor (`$EDITOR` — vim, nvim, helix, whatever), as a
  pane rather than a shell-out that blanks the screen (§6). *(landed)* `e` on a line opens it
  at that line; the flag syntax is per editor, and an unrecognised one just gets the path.
- Two of the three bases (§5) are in, cycled with `b`: **uncommitted** (HEAD against the
  working tree) and **this branch** (its fork point against the working tree, using the
  upstream where there is one and falling back to HEAD where there is no fork point at all).
- Still open: **since I last looked** — it needs a tree snapshot per checkout, so it waits on
  persistence — plus vetted tracking and stage/revert from the review itself.

That last point is the payoff and the reason review is the part that has to be good: the
review surface and the agent are already side by side in the same tree, so feedback should
not have to leave the tool.

**M5 — notes & context server.** Markdown notes, todo rollups, forwarding, then the read side of
the context server, then the write side behind policy.

**M6 — polish.** Sandboxing, scrollback spill, and config reload.

Session restore is in: `session.json` beside the config records what was running, rewritten on
every structural change, and the daemon starts it all again on launch. Checkouts are matched by
path and projects by name, since ids are reissued every run. Editors are skipped — one belongs
to the floating window it opened in — and so is any pane whose checkout or agent template has
since left the config; a failure costs that pane, never the startup. `ARGUS_NO_RESTORE` starts
clean, for when the restore is itself the problem. Recording is opt-in and only `main` enables
it, so no test can write over a real session.

A restored agent is a *fresh* process in the right checkout, not its previous conversation.
Resuming that needs the per-harness session ids described in §11.

The UI is done for now:

- `theme.rs` keys every color to a semantic role — three elevations (`bg` / `surface` /
  `surface_focus`), accent, text, muted/dim, ok/warn/err, edge, and the two selection fills.
- The four presets are the [Catppuccin](https://catppuccin.com) flavors in truecolor:
  `mocha` (default), `macchiato`, `frappe`, `latte`. Indexed-palette approximations were
  tried first and are the reason the UI read as a 1980s terminal — ANSI cyan/green/red are
  fully saturated, and no layout fixes a palette that shouts.
- Panels are padded cards separated by a gutter, sitting on a page one elevation darker.
  Rows are two lines — what the thing is, then a dimmer line of what is true about it —
  with counts pinned right as badges so the narrow columns don't truncate them away.
- `t` switches theme at runtime; `ARGUS_THEME` still sets the one to start in.

---

## 10. Open questions

- **Agent state detection.** Output-scraping is fragile across CLIs. `status.set` fixes it for
  cooperating agents, but the fallback still matters — is a per-template `state_cmd` hook worth
  supporting, or do we accept coarse granularity from process state alone?
- **Multi-repo branches.** When a project has three repos and a feature spans all three, does a
  "checkout" become a *set* of checkouts sharing a branch name? Powerful, but it complicates the
  clean left-to-right model. Probably M7+.
- **Agents spawning agents.** `agent.spawn` is genuinely useful and genuinely a way to wake up to
  forty panes. Is a fan-out cap enough, or should delegation require interactive approval?
- **Windows.** ConPTY works but is slower and its resize semantics differ. Ship Unix-first?

---

## 11. Further directions

Additional directions for the roadmap, beyond what's in §9:

- **Rename to `argus`.** *(landed)* Crates, binaries (`argus` / `argusd` / `argus-hook`),
  the socket and named pipe, the config directory, and `ARGUS_CONFIG_DIR` / `ARGUS_THEME`.
  `is_hook_helper` still recognises the old `orion-hook` name so a hook block written before
  the rename is swept rather than left firing on every agent turn.

- **Workspaces.** *(landed)* A named group of projects sitting above the project level, with
  exactly one open at a time — daemon-global, so every attached client re-scopes together.
  Deliberately not a fourth navigation column: the left-to-right spine stays
  project → checkout → pane, and the workspace is a scope switch (`w`) above it. Projects
  declare theirs with `workspace = "..."`; anything unassigned lands in `default`, so a config
  that predates the feature keeps working. Panes in a closed workspace keep running, and their
  counts stay visible in the picker.

- **A workspace layer above projects.** Nest Workspace → Project → Checkout → Pane, with exactly
  one workspace *open* at a time (daemon-global, switched from the TUI, broadcast to every
  client) — other workspaces' panes keep running in the background but drop out of the Projects
  panel and search. Argus's tree currently starts at Project; adding Workspace above it is the
  next structural change to the tree, ahead of M2's remaining pieces.
- **Status via hooks, not scraping.** This is the real answer to the M3/§8b open question above.
  At spawn, install managed hooks (e.g. `.claude/settings.local.json`) that `curl` a loopback HTTP
  server with a per-boot bearer token. `UserPromptSubmit` / `Stop` / `Notification` drive the
  state machine directly, gated so a turn isn't marked done while subagents are still going.
  `idle_when` output-pattern scraping becomes the fallback for agents with no hook support, not
  the primary mechanism.
- **SQLite, not TOML-only.** `projects.toml` remains the *declared* config, but runtime state that
  accumulates — notes, links, last UI selection, an agent's resumable session id — needs real
  persistence, not repeated appends to a TOML file. `add_project`'s append-to-file approach (this
  session) is a stopgap; the SQLite move in M5/M6 should absorb it.
- **Worktree auto-discovery.** Poll git metadata so worktrees created outside the tool (a bare
  `git worktree add` from a shell) still show up. Argus only shows worktrees it created itself —
  same gap as the "branches without a checkout" row from §4, worth closing together.
- **Notes and links per checkout**, with a PR row looked up client-side (`gh pr view`) on the git
  poll tick, shown above the stored links but not itself editable — it's derived, not owned.
- **Session resume and auto-titling** via `--resume <session-id>` (stored per agent) and a
  hook-injected instruction that has a freshly spawned agent rename itself once, arbitrated by a
  daemon-side flag so a manual rename always wins.
- **Remote hosts.** A self-installing remote session over `ssh -t`, with a small recents list the
  TUI can pick from. Not urgent, but the kind of thing that's easy to bolt on once the core loop
  is solid — worth keeping the socket/protocol layer host-agnostic so it isn't a rewrite later.
