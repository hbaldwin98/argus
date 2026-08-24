# Argus Roadmap

This file orders unfinished work. Current behavior is in [`DESIGN.md`](DESIGN.md), and the desired
contract is in [`TARGET.md`](TARGET.md).

## P0: Correctness

- Define multi-client PTY resize ownership.
- Recover lagged pane subscribers with a full snapshot and close the subscribe/snapshot race.
- Route Claude hooks to the correct pane when several agents share a checkout.
- Make worktree removal transactional so a failed Git command does not destroy its panes.
- Validate worktree branch names as strictly as in-place branch creation.
- Prevent review ranges from crossing files, or represent multi-file comments explicitly.
- Parse editor commands without breaking quoted arguments or executable paths containing spaces.

## P2: Repository Model

- Add repository identity and repository nodes to daemon state and protocol.
- Correct worktree discovery and mutation for multi-repository projects.
- Show branches without checkouts and offer switch or worktree creation.
- Add explicit dirty-primary protection.
- Add configurable worktree roots and setup hooks.
- Show shared-checkout warnings and support optional exclusivity.
- Replace or supplement the two-second poll with Git metadata watching.
- Reload configuration without dropping running panes.
- Decide whether project scan globs remain part of the product.

## P3: Agent State and Identity

- Separate template, harness, display title, harness session id, and restart policy.
- Add `needs-review`, `done`, and explicit `failed` semantics.
- Make hook events pane-specific and subagent-aware.
- Define fallback state detection for unsupported harnesses.
- Add accessible glyphs, transition flashes, and optional notifications.
- Record each pane's harness session id and resume by it, rather than by the CLI's own "continue the
  last conversation here" flag, which cannot tell two agents in one checkout apart.
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
- Implement explicit note forwarding.
- Add scoped context read APIs, then policy-gated writes with audit records.
- Add `argus ctx` and MCP adapters over the same implementation.
- Add delegation approval or fan-out controls.
- Move runtime-added project overlays out of raw TOML appends if transactional storage owns them.

## P6: Terminal and Performance

- Expose scrollback navigation.
- Add child-negotiated mouse behavior, bracketed paste, focus events, OSC 52, and extended keys.
- Bound the reader-to-parser output queue.
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
