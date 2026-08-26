# Argus Roadmap

This file orders unfinished work. Current behavior is in [`DESIGN.md`](DESIGN.md), and the desired
contract is in [`TARGET.md`](TARGET.md).

Ordering is by dependency, not by appetite. Runtime storage comes first because review state, notes,
context, and boards all need somewhere transactional to live, and each one built ahead of it becomes
another bespoke file with its own compatibility ladder.

## P3: Runtime Storage

- Introduce transactional runtime storage for review state, notes metadata, links, UI selection,
  and resumable agent ids.
- Absorb `session.json` into it, retiring the per-field `serde(default)` legacy shims.
- Move runtime-added project overlays out of raw TOML appends, so `projects.toml` goes back to being
  configuration the user owns and Argus only reads.
- Finish the `state.rs` split begun by `panes`, `sync`, and `git_ops` while the storage work is
  already touching it, and give `app.rs` the same treatment.

## P4: Agent State and Identity

Lands alongside P3 rather than ahead of it: inferred state has nothing that needs to persist.

- Define fallback state detection for unsupported harnesses, as the tiering TARGET.md already
  promises — explicit events authoritative, process state next, output matching last. Settle
  precedence (a real event locks out inference for that session), decay for an inferred `working`
  that will never be told to stop, the Windows answer where there is no foreground process group,
  and whether an inferred state is visually distinct from a reported one.
- Use tool-start hooks (`PreToolUse` / `preToolUse` / equivalents) across harnesses as a
  `working` signal when lifecycle hooks are missing or unreliable; keep turn-end events as the
  sole authority for `idle`. Optionally surface the active tool in the pane note later.
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
- Add delegation approval or fan-out controls.

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
- Delegation approval versus a fan-out cap.
- Unix-first delivery versus equal Windows support, which the state-detection tiering forces first.
- Whether a future GPU client warrants a richer protocol now.
- PR/link lookup and whether `gh` is an acceptable optional dependency.
