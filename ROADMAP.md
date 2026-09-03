# Argus Roadmap

This file orders unfinished work. Current behavior is in [`DESIGN.md`](DESIGN.md), and the desired
contract is in [`TARGET.md`](TARGET.md).

Ordering is by dependency, not by appetite.

## P3: Runtime Storage — landed

Storage came first because review state, notes, context, and boards all need somewhere
transactional to live, and each one built ahead of it would have become another bespoke file with
its own compatibility ladder. That is now `runtime.db` (DESIGN.md, "Runtime storage"): SQLite in
WAL mode holding panes to relaunch, project and repository overlays, exclusions, runtime-created
workspaces, and the workspace last open. `session.json`, `excluded-repos`, and `open-workspace` are
imported once and retired; `projects.toml` is back to being configuration Argus only reads.

- Give review state, notes metadata, and links their tables as those features land. The schema is
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

## P6: Notes and Context

Closes the loop: information currently flows only upward, from agents reporting to humans reading.

- Markdown notes and todo/pinned rollups have landed (DESIGN.md, "Notes"): projects and checkouts
  hold plain Markdown, the checkbox line is the one construct read out of it, and its three states
  roll up checkout to repository to project. Storage is schema v3, keyed by name and path so a note
  outlives the ids it was written under.
- Scoped context reads have landed (DESIGN.md, "Notes"): `argus-hook context` returns the project
  and checkout notes of the pane that asked, and nothing else.
- Policy-gated writes have landed (DESIGN.md, "Notes"): `argus-hook todo` adds and ticks off
  checkboxes on the asking pane's checkout note, refused unless the project sets `agent_todos`,
  never reaching the project note or a `- [!]` line, and recorded in schema v4's `note_audit`
  alongside the change itself. The decision board was built on it: the write path, the agent scope,
  and the attributed record are the parts it would otherwise have invented.
- Explicit note forwarding has landed (DESIGN.md, "Notes"): `f` stages the current line and `F`
  stages the whole visible note in a chosen in-scope agent's prompt without submitting it.
- Add `argus ctx` and MCP adapters over the same implementation.

## P6.5: Views and Boards

Boards are the first thing Argus has wanted that is not a column. A decision tree and a work board
are read at project scope, all at once, and neither says anything useful in a 30-column strip
beside a pane — so the client needed a second top-level surface before either could be built. That
ordering was the whole of this section: the view mechanism first, then the two boards on it, then
the link between them. The first two have landed.

- Views have landed (DESIGN.md, "Views"): the content area holds one named view at a time, with
  the project spine as the default and a one-row tab strip naming the rest. The strip lives in the
  page's top gutter, so it costs the view underneath nothing; digits open a view, the leader plus a
  digit does it from inside a pane, and the open view is per-client and never sent to the daemon.
- Features and the decision board have landed (DESIGN.md, "Features and the decision board"):
  schema v5's `decision` table and v6's `feature`/`feature_scope`, `argus-hook feature` to read and
  move the scope, `argus-hook decisions` to read the current feature's tree and `argus-hook decide`
  to append to it, and a view that draws the tree with superseded branches dimmed. Ungated, unlike
  note writes — the board exists for agents to write and attributes every row. A decision is filed
  under the feature its checkout is on, and `decide` from a checkout on none is refused.
- Add the feature board (TARGET.md, "Boards"): items in columns by state, claimed by an agent,
  carrying progress, blockers, and submitted completion evidence, accepted or sent back by a human
  and never by the agent that did the work. Both sides can add an item — the human from the view,
  over a client message the way note edits go, and an agent over the pane API — so the write path
  is the note's dual shape rather than the decision board's agent-only one. It has to read as well as it writes — an agent starting
  work infers what is in flight and what is already decided from the board, which is the reason it
  exists rather than a checklist in a note.
- Give the board its own reads for a client: it is pushed whole on every change, which is right
  while a board is a few dozen rows and wrong once it is a few thousand.
- Link the two: a decision is taken about an item, an item carries the decisions taken under it,
  and either one reaches the other.
- Decide how a board reaches an agent: whole-board reads will not fit a prompt for long, so the
  read wants scoping — this item, its ancestors, its blockers — the way `argus-hook context` scopes
  notes to the asking pane.

## P7: Terminal and Performance

- Anchor a parked scrollback view to a line rather than to the live screen, so a pane still printing
  does not shift the rows out from under a reader. Navigation itself has landed (DESIGN.md, "Panes
  and terminal state"): the daemon answers an offset with the rows there, and the wheel,
  Shift-PageUp/PageDown, and typing move between history and live.
- Add child-negotiated mouse behavior, bracketed paste, focus events, OSC 52, and extended keys.
- Replace idle 16 ms pane wakeups with event-driven work where possible.
- Implement packed, byte-bounded scrollback, then cold eviction, spill, and redaction.
- Benchmark frame time, startup, RSS, pane scaling, high-output children, and slow clients.
- Add protocol deltas only where measurements show they help.

## P8: Platform and Remote Work

- Restrict socket and named-pipe access and harden stale-daemon startup races.
- Add per-template sandboxing.
- Qualify Windows ConPTY resize and performance behavior.
- Define clean daemon service and shutdown management.
- Add protocol versioning and authentication before remote hosts.
- Explore a self-installing SSH transport only after the local protocol is stable.

## Open Decisions

- True child-process reattachment versus guaranteed termination plus harness resume.
- Multi-repository features as coordinated checkout sets.
- Unix-first delivery versus equal Windows support, now forced by ConPTY and named-pipe behavior
  rather than by state detection.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
