# Argus Roadmap

This file orders unfinished work. Current behavior is in [`DESIGN.md`](DESIGN.md), and the desired
contract is in [`TARGET.md`](TARGET.md).

Ordering is by dependency, not by appetite.

## P3: Runtime Storage — landed

Storage came first because review state, notes, context, and durable agent artifacts all need
somewhere transactional to live, and each one built ahead of it would have become another bespoke
file with its own compatibility ladder. That is now `runtime.db` (DESIGN.md, "Runtime storage"):
SQLite in WAL mode holding panes to relaunch, project and repository overlays, exclusions,
runtime-created workspaces, and the workspace last open. `session.json`, `excluded-repos`, and
`open-workspace` are imported once and retired; `projects.toml` is back to being configuration
Argus only reads.

- Give review state and links their tables as those features land. The schema is
  versioned on `user_version`, so each is a migration rather than a new file.

## P4: Agent State and Identity

- Extend tool-start hooks as a lifecycle fallback to any other harness whose own lifecycle events
  prove unreliable. Cursor's `agent` already reports `working` from `preToolUse` and
  `beforeShellExecution`, with `stop` still the sole authority for `idle`.
- Daemon-arbitrated auto-titling has landed (DESIGN.md, "Panes and terminal state"): prompt-submit
  events name the row from the user's text; `argus-hook title` still refines it.
- Expand the template schema only after lifecycle and permission semantics are stable.

## P5: Complete Review

Review shows both sides of uncommitted work — staged and unstaged, each with its own endpoint —
and a checkout's recent commits one at a time against their first parent. Comments are durable and
agents can read checkout-scoped feedback; the client selects a recipient when several agents share
the checkout. What remains is presentation work.

- Add lazy rendering where measurements justify it. Syntax highlighting has landed (DESIGN.md,
  "Review"): tree-sitter in the daemon, ten grammars linked in and chosen by extension, spans on
  the wire carrying token roles rather than colours.
- Decide whether to retain the current scrolling surface or adopt separate file and diff columns.
- The split view has landed (DESIGN.md, "Review"): `s` reflattens the diff so each hunk's removals
  sit beside what replaced them, built in the client over `Hunk` data the wire already carried.
  Anchors survived it, as expected — a line comment records both old and new numbers, so the two
  views produce the same anchor for the same change.

Deliberately out of scope: staging, unstaging, and reverting hunks, and any base that compares a
branch against a fork point or a remembered snapshot. Dedicated Git tools do those better.

## P6: Notes and Context — removed

Project and checkout Markdown notes, their checkbox rollups, note forwarding, and the
`argus-hook context`/`todo` commands landed here and were later removed as unused. Feature briefs,
tasks, and the decision board (P6.5) carry durable context instead.

- Add `argus ctx` and MCP adapters over the scoped agent reads that remain.

## P6.5: Agent Memory

Argus has durable feature, decision, and task storage, but its first presentation treated that
storage as a project-management board. That makes the operator maintain a second tracker and leaves
agents to guess whether whichever feature a checkout selected applies to their current request.
The target is agent-maintained working context: people supply and correct information, agents build
up durable artifacts, and later agents receive the relevant subset without browsing a board.

- Views have landed (DESIGN.md, "Views"): the content area holds one named view at a time, with
  the project spine as the default and a one-row tab strip naming the rest. The strip lives in the
  page's top gutter, so it costs the view underneath nothing; digits open a view, the leader plus a
  digit does it from inside a pane, and the open view is per-client and never sent to the daemon.
- Features and the decision board have landed (DESIGN.md, "Features and the decision board"):
  schema v5's `decision` table and v6's `feature`/`feature_scope`, `argus-hook feature` to read and
  move the scope, `argus-hook decisions` to read the current feature's tree and `argus-hook decide`
  to append to it, and a view that draws the tree with superseded branches dimmed. Ungated — the
  board exists for agents to write and attributes every row. A decision is filed
  under the feature its checkout is on, and `decide` from a checkout on none is refused.
- Feature, decision, and task boards now default to the checkout's repository and branch. Linked
  worktrees on that branch share a durable board; other branches and repositories do not. The TUI
  sends its selected checkout with every artifact request, and agent helpers can opt into a shared
  workspace board with `ARGUS_ARTIFACT_SCOPE=workspace`. Workspace visibility remains separate from
  artifact scope.
- The Kanban board is gone (DESIGN.md, "What a feature row says"). Its five columns were the one
  surface in Argus showing an assertion rather than an observation, `argus-hook` never exposed the
  move, and so the only thing that maintained them was a person dragging cards. Schema v9 collapses
  `FeatureState` to `open` and `done`, mapping the four old names onto `open` and leaving
  `feature_event` alone, since it records what was believed at the time. A feature's line is now
  read off the agent panes running in its checkouts and the state of its tasks; `Feature` carries
  `checkouts` and `tasks` for it, so a feature is no longer an island describing work with no way
  to tell whether anything is happening to it. `done` stays stored and stays the human's, because
  the agent that did the work cannot accept it.
- The four views are two (DESIGN.md, "The feature view"). The decision board, the feature board and
  the task board were one object drawn three times with a selection each, and selections that can
  disagree did: opening the tasks from the decisions showed whichever card the board was sitting
  on. One `feature_sel` scopes the brief, the tasks and the tree together, `Tab` and `h`/`l` cross
  the three panels, and tasks are one ordered list with their state marked on the row rather than
  three columns spending the order to say what a glyph says. Tasks can now be nested recursively:
  the wire carries parent ids, the TUI and `argus-hook task` render a depth-first tree, and removing
  a parent removes its discovered subtree.
- Replace the feature-as-assignment contract with a work context. A checkout may suggest context,
  but selection must not silently assign unrelated work or make the sole feature authoritative.
- Define the durable artifact contract around what later agents need: decisions, tasks, findings,
  assumptions, open questions, and summaries, all with source, session, checkout, and time.
- Build one bounded context-packet read for agents. It combines explicitly forwarded material with
  applicable artifacts, explains the scope, excludes stale or unrelated records, and exposes a
  revision so a running agent knows when to refresh.
- Make artifact writeback part of the managed agent workflow. Agents should record durable results
  as they discover them without turning routine steps or transcript into memory.
- Give human corrections explicit precedence. Preserve superseded reasoning and provenance while
  ensuring corrected or withdrawn material no longer guides later agents.
- Extend the feature view into an inspection and correction surface over work contexts, as the
  artifact types land. The Kanban half of this is done; what remains is that the view still shows
  one feature's stored records rather than the packet an agent was actually given.
- Reuse the landed tables and migrations where their semantics fit. Change storage only after the
  context packet and correction behavior establish what must persist.

## P7: Terminal and Performance

- Scrollback navigation and line-anchored parked views have landed (DESIGN.md, "Panes and terminal
  state"): the daemon numbers the lines that go up past the live screen, and a parked view scrolls
  from its own line however much output arrives.
- Extended keys have landed for the desktop (DESIGN.md, "Panes and terminal state"): the daemon relays
  a child's kitty keyboard flags and the shared encoder emits `CSI u` for the disambiguate level. The
  terminal client still types legacy bytes.
- Focus events have landed (DESIGN.md, "Panes and terminal state"): a child that enabled DECSET 1004
  hears when the focused view moves to or from its pane.
- Add child-negotiated mouse behavior, bracketed paste, and OSC 52.
- Idle pane wakeups are gone (DESIGN.md, "Panes and terminal state"): a thread waits on each child's
  exit, so a quiet pane's task sleeps until its child speaks or exits.
- Implement packed, byte-bounded scrollback, then cold eviction, spill, and redaction.
- Benchmark frame time, startup, RSS, pane scaling, high-output children, and slow clients.
- Add protocol deltas only where measurements show they help.

## P8: Platform and Remote Work

- Restrict socket and named-pipe access and harden stale-daemon startup races.
- Add per-template sandboxing.
- Qualify Windows ConPTY resize and performance behavior.
- Define clean daemon service and shutdown management.
- Add protocol versioning and authentication before remote hosts.
- SSH remote hosts landed (DESIGN.md, "Process model"). The client never installs argus on a
  remote machine; a missing or older argus there is reported with what to install.

## P9: Mobile and Web Client — removed

`argus web` and the phone client it served — pairing and device tokens, the conversation and
Terminal pages, the installable page and Web Push — landed here with the daemon-wide tree it
followed every agent by, and with the live channels a pane got only while it was connected: Codex's
app-server, which Argus joined as a second client, and the tee, a loopback proxy that read a live
Claude pane's reply off its API traffic. So did what the phone alone read and sent: conversation
following, which read each harness's transcript, took pushed entries and drafted replies, and the
outbox, which held a message until its agent was back at its prompt, cancelled one, interrupted an
agent and answered its harness's questions. All of it was later removed. The terminal client
carries the role, on this machine or another through `argus --host`: the pane is the conversation,
the status bar (and the bell, when it is on) says when an agent needs someone, and a review comment
reaches an agent through its harness's inbox.

## P10: Desktop Client — at parity with the terminal client

A Tauri desktop app as a second renderer of the daemon beside the terminal client
([`DESKTOP.md`](DESKTOP.md) is the plan). It reaches the daemon through `argus bridge`, and the
parity issues under HUN-181 have landed: checkouts, review and history, the command palette and
shortcuts, settings and notifications, projects, feature editing, starting an agent, mouse,
selection and scrollback, the full transcript, and console shells that belong to no checkout. The
Feature stage reads a feature through its Work, Record and Flows lenses, lays the index out as a
list or a board, and draws a flow's sequence diagram, whose steps and lifelines a drag reorders by
rewriting its Mermaid (HUN-172). A feature's tickets carry their role, and a reconcile with them
is proposed by an agent and carried out only as far as the person ticks it (HUN-173).

Agent views (HUN-216) are data an agent read with its own tools, handed to the window as a fixed
set of blocks it draws: never markup, and never kept in the store.

Remote hosts landed: the switcher's Connect to a host… reaches a daemon through `ssh … argus bridge`,
names the host in the sidebar and in notifications, and refuses an editor that would open its own
window on the remote machine.

Remaining: several panes at once; and packaging as
.app, .msi and AppImage. Landed for packaging: the desktop finds the `argus` beside it before PATH
(what a GUI launch without a shell PATH needs), and `tauri.bundle.conf.json` lists the sidecars
`argus`, `argusd` and `argus-hook`, read only by a bundle build. Still to do: icons (only
`icon.png` and `icon.ico` exist, no `.icns` for macOS), enabling the `app`, `msi` and `appimage`
targets (`bundle.active` is still `false`), a CI job that stages the sidecars and bundles per
platform, signing, and the updater. See DESKTOP.md §Bundling. The window's stage has moved out of `session` into `stage` (the stage, and the stage each page opened
over); the console's fullscreen is still the console's own state.

Extensions (HUN-191) landed for the daemon and the desktop window. An outside process publishes
views of rows (running dev servers, a repository's containers) in the agent views' vocabulary,
which the window lists under Extensions in its sidebar and opens as a page or docked beside the
stage; registers commands with keys that join the keymap, the shortcuts sheet and the palette;
raises dialogs (confirm, prompt, pick, form) over the current surface; and posts notices. What the
person does comes back to it. The daemon owns extensions rather than the window, so they work
under `--host`: an extension publishes to the pane API and holds its events open as server-sent
events, the daemon launches each from config with a token of its own and restarts it, and only
clients that listed extensions in `Hello` are sent them. A dialog goes to the client the focus
names, so one window asks. An extension's key never shadows a built-in one; a conflict is refused
when it registers. Remaining: the terminal client drawing them (HUN-217).

## Open Decisions

- True child-process reattachment versus guaranteed termination plus harness resume.
- How workspace-wide contexts present evidence from several repositories without implying that one
  checkout owns the whole context.
- Unix-first delivery versus equal Windows support, now forced by ConPTY and named-pipe behavior
  rather than by state detection.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
