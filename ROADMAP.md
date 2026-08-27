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

- Define fallback state detection for unsupported harnesses, as the tiering TARGET.md already
  promises — explicit events authoritative, process state next, output matching last. Settle
  precedence (a real event locks out inference for that session), decay for an inferred `working`
  that will never be told to stop, the Windows answer where there is no foreground process group,
  and whether an inferred state is visually distinct from a reported one.
- Surface the active tool in the pane note. Tool-start hooks already stand in for missing
  lifecycle events — Cursor's `agent` reports `working` from `preToolUse` and
  `beforeShellExecution`, with `stop` still the sole authority for `idle` — so what is left is
  displaying which tool, not detecting that one ran. Extend the same fallback to any other
  harness whose lifecycle hooks prove unreliable.
- Add daemon-arbitrated auto-titling.
- Expand the template schema only after lifecycle and permission semantics are stable.

## P5: Complete Review

Review is the half of the premise that is still a viewer. Vetting and staging are what turn Argus
from somewhere you watch agents work into somewhere you accept or reject what they produced.

- Track vetted content and invalidate it on edits.
- Stage, unstage, and revert addressed hunks safely.
- Persist review comments and let agents read them.
- Select a comment recipient when several agents share a checkout.
- Add syntax highlighting and lazy rendering where measurements justify them.
- Decide whether to retain the current scrolling surface or adopt separate file and diff columns.

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
- Unix-first delivery versus equal Windows support, which the state-detection tiering forces first.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
