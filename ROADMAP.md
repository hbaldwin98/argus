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
- Fix the store's known gap: a committed write must not answer with an error
  (`store::tests::faults`).

## Desktop

- Packaging: `.app`, `.msi` and AppImage targets, icons (`.icns` is missing), a CI job that stages
  the sidecars and bundles each platform, signing, and the updater (`DESKTOP.md` §Bundling).
- Persist the sizes of the Workspace's pane grid across restarts.
- Finish the GPUI window's parity with the Tauri one and retire `argus-desktop` (`GPUI.md`).

## Panels in the grid

A cell of the Workspace's grid holds a pane or a panel. `Panel::cells` admits the browsers, whose
tiles reflow into any room, and the changes, which lay themselves out by their own width. Each
other panel joins once the grid can size it.

- A panel's cell reports its room, as a pane's does, so the grid's room check
  (`workspace_grid::has_room`) counts it.
- Each panel's own floor in the grid, beside a pane's 20 columns by 6 rows. A saved view's
  columns, the console's terminal and an extension's page each need more than the browsers.
- A cell's gutters tell the panel its width, and a panel that keeps its own widths (a saved view's
  dragged columns, the console's sides) fits them to the cell rather than overflowing it.
- The keys for a panel that types: the console and an editable view in a cell take them, and the
  grid's focus keys reach them. Today a panel's cell never has the keys.
- Then admit the panels one at a time: the views, a saved view, an extension's view, the console.

## Float anything

Anything on screen docks, floats or sits in a cell: today a panel can, if it `floats`, and a pane
cannot. The person wants agents' panes, the browsers and every panel to float as the changes do.

- Every panel floats: drop `Panel::floats`' exceptions once each body is drawn well at a dialog's
  size (the console's terminal and the views are the ones left).
- A pane in a dialog: a dialog that holds an `Occupant::Pane`, reporting its room as a cell does,
  so the pane's program is sized to the dialog, and taking the keys when pressed.
- One carry for all: a pane's header, a cell's header and a dock's tab each drag to a dock, onto
  a cell, or out over the stage to float, as the browsers' tab does today.

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
