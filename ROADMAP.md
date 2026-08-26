# Argus Roadmap

This file orders unfinished work. Current behavior is in [`DESIGN.md`](DESIGN.md), and the desired
contract is in [`TARGET.md`](TARGET.md).

## P2: Repository Model

- Show shared-checkout warnings and support optional exclusivity.
- Replace or supplement the two-second poll with Git metadata watching.
- Reload configuration without dropping running panes.
- Let a project root include or exclude paths, now that scanning one is how projects are built.
  The scan currently has one fixed skip list and no per-project control.

## P3: Agent State and Identity

- Separate template, harness, display title, harness session id, and restart policy.
- Add transitions and notifications around explicit agent states.
- Make hook events pane-specific and subagent-aware.
- Define fallback state detection for unsupported harnesses.
- Add accessible glyphs, transition flashes, and optional notifications.
- Add daemon-arbitrated auto-titling.
- Expand the template schema only after lifecycle and permission semantics are stable.

## P4: Complete Review

- Track vetted content and invalidate it on edits.
- Stage, unstage, and revert addressed hunks safely.
- Persist review comments and let agents read them.
- Select a comment recipient when several agents share a checkout.
- Add syntax highlighting and lazy rendering where measurements justify them.
- Decide whether to retain the current scrolling surface or adopt separate file and diff columns.

## P5: Notes, Context, and Runtime Storage

- Introduce transactional runtime storage for review state, notes metadata, links, UI selection,
  and resumable agent ids.
- Implement Markdown notes and todo/pinned rollups.
- Add project feature boards where agents can claim tasks, report progress or blockers, and submit
  completion evidence for human review and acceptance.
- Implement explicit note forwarding.
- Add scoped context read APIs, then policy-gated writes with audit records.
- Add `argus ctx` and MCP adapters over the same implementation.
- Add delegation approval or fan-out controls.
- Move runtime-added project overlays out of raw TOML appends if transactional storage owns them.

## P6: Terminal and Performance

- Expose scrollback navigation.
- Add child-negotiated mouse behavior, bracketed paste, focus events, OSC 52, and extended keys.
- Replace idle 16 ms pane wakeups with event-driven work where possible.
- Implement packed, byte-bounded scrollback, then cold eviction, spill, and redaction.
- Benchmark frame time, startup, RSS, pane scaling, high-output children, and slow clients.
- Add protocol deltas only where measurements show they help.

## P7: Platform and Remote Work

- Restrict socket and named-pipe access and harden stale-daemon startup races.
- Add per-template sandboxing.
- Qualify Windows ConPTY resize and performance behavior.
- Define clean daemon service and shutdown management.
- Add protocol versioning and authentication before remote hosts.
- Explore a self-installing SSH transport only after the local protocol is stable.

## Open Decisions

- True child-process reattachment versus guaranteed termination plus harness resume.
- Multi-repository features as coordinated checkout sets.
- Delegation approval versus a fan-out cap.
- Unix-first delivery versus equal Windows support.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
