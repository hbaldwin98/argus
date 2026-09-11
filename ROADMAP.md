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
  three columns spending the order to say what a glyph says.
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
- How workspace-wide contexts present evidence from several repositories without implying that one
  checkout owns the whole context.
- Unix-first delivery versus equal Windows support, now forced by ConPTY and named-pipe behavior
  rather than by state detection.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
