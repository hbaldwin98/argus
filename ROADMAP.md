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
- Add daemon-arbitrated auto-titling.
- Expand the template schema only after lifecycle and permission semantics are stable.

## P5: Complete Review

Review shows both sides of uncommitted work and nothing else — staged and unstaged, each with its
own endpoint. Comments are durable and agents can read checkout-scoped feedback; the client selects
a recipient when several agents share the checkout. What remains is presentation work.

- Add syntax highlighting and lazy rendering where measurements justify them. Tree-sitter is the
  intended parser, chosen by file extension. Its grammars are compiled-in crates rather than
  anything fetched at runtime, so "whatever the filename needs" means a fixed set linked at build
  time and a quiet fall back to unhighlighted text for everything else; weigh that binary cost
  against `syntect`, which loads its grammars at runtime instead. Highlight whole blobs rather than
  hunks — review already captures both endpoints into the object store, so the parser can be given a
  complete file instead of guessing at a fragment torn out of one.
- Decide whether to retain the current scrolling surface or adopt separate file and diff columns.
- Offer a split view alongside the unified one, toggled. The wire needs nothing: a split pairs each
  hunk's removed and added lines into rows over the `Hunk` data the client already has. What it
  needs is width, and review draws in the live pane's column at roughly a third of the screen — so
  it belongs behind the fullscreen pane, which is where two readable sides fit. Line comments
  already carry both old and new numbers, so anchors survive the change.

Deliberately out of scope: staging, unstaging, and reverting hunks, and any base that reaches into
committed history. Dedicated Git tools do those better.

## P6: Notes, Context, and Delegation

Closes the loop: information currently flows only upward, from agents reporting to humans reading.

- Implement Markdown notes and todo/pinned rollups.
- Add scoped context read APIs, then policy-gated writes with audit records.
- Add project feature boards where agents can claim tasks, report progress or blockers, and submit
  completion evidence for human review and acceptance.
- Implement explicit note forwarding.
- Add `argus ctx` and MCP adapters over the same implementation.
- Decide whether delegation needs approval on top of its cap. Agents can already open peer
  review panes, bounded to four live agents per checkout and gated on project exclusivity.

## P7: Terminal and Performance

- Expose scrollback navigation.
- Add child-negotiated mouse behavior, bracketed paste, focus events, OSC 52, and extended keys.
- Add fullscreen mode for a pane (hide navigation columns, give terminal full window).
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
