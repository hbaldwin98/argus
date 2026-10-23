# Argus Roadmap

Open work only, ordered by dependency. What has landed is in the code, its history, and
[`ARCHITECTURE.md`](ARCHITECTURE.md); what Argus is for is in [`TARGET.md`](TARGET.md).

## Agent memory

The main gap against the target (`TARGET.md` §"Agent context and memory").

- Replace the feature-as-assignment contract with work contexts a checkout may suggest but never
  silently assigns.
- Define the artifact contract: findings, assumptions, open questions and summaries beside
  decisions and tasks, each with source, session, checkout and time.
- One bounded context-packet read, explaining why each item is included and carrying a revision.
- Make writeback part of the managed agent workflow, without turning routine steps into memory.
- Give human corrections explicit precedence over agent writes.
- Show the packet an agent was given, not only the stored records, in the feature view.
- `argus ctx` and an MCP adapter over the same scoped reads.
- Reuse the existing tables where their meaning fits; change storage only once the packet settles
  what must persist.

## Terminal

- Mouse reporting the child negotiates, and extended keys (`CSI u`) in the terminal client; the
  desktop already has both.
- Packed, byte-bounded scrollback, then cold eviction, spill and redaction.
- Benchmarks against the budgets in `TARGET.md`: frame time, startup, RSS, pane scaling, noisy
  children, slow clients.

## Safety and platform

- Restrict socket and named-pipe access to the user; harden startup races against a stale daemon.
- Protocol authentication before anything listens beyond the local machine.
- Per-template sandboxing.
- Qualify Windows ConPTY resize and performance.
- Fix the store's known gap: a committed write must not answer with an error (`ARCHITECTURE.md`
  §Storage).

## Desktop

- Packaging: `.app`, `.msi` and AppImage targets, icons (`.icns` is missing), a CI job that stages
  the sidecars and bundles each platform, signing, and the updater (`DESKTOP.md` §Bundling).
- Persist the sizes of the Workspace's pane grid across restarts.
- Finish the GPUI window's parity with the Tauri one and retire `argus-desktop` (`GPUI.md`).

## Terminal client

- Draw extensions (HUN-217).

## Review

- Lazy rendering, where measurements show it is needed.
- Decide between the single scrolling surface and separate file and diff columns.

## Open decisions

- How workspace-wide contexts show evidence from several repositories without implying one checkout
  owns the context.
- Whether `gh` is an acceptable optional dependency for pull-request lookup.
- Unix-first or equal Windows support, now forced by ConPTY and named-pipe behavior.
