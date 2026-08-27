# Argus: Target Design

This document defines the intended product. It does not claim these systems are implemented;
[`DESIGN.md`](DESIGN.md) is the current behavior and [`ROADMAP.md`](ROADMAP.md) orders the gap.

## Product boundary

Argus is a terminal workspace built around one navigation spine:

```text
Project -> Repository -> Checkout -> Agent, shell, or editor
```

A workspace scopes the visible projects without adding another column. Argus is not a general
tmux replacement, a GUI, an agent host, or a Git porcelain. It runs existing command-line
harnesses and makes their checkouts, state, and review work visible.

The target budgets are sub-16 ms client frames, less than 50 MiB client RSS with twelve active
panes, about 1.5 MiB resident per idle pane, and less than 30 ms to first paint against a warm
daemon.

## Ownership and persistence

The daemon owns PTYs, scrollback, runtime state, Git observation, and agent integration. Clients
are replaceable renderers. Closing a client never stops a pane.

Runtime persistence must be atomic and recoverable. It must distinguish:

- an open pane from a running child;
- a process that exited from one that should restart;
- a fresh relaunch from a resumable harness session;
- display title from template and harness identity.

Argus must choose and document one crash policy: reattach surviving processes, or guarantee that
daemon children cannot survive and relaunch/resume them. It must never duplicate an untracked
surviving child.

## Repository and checkout model

A project groups repositories. A repository owns its primary checkout, linked worktrees, and
branches without a checkout. The UI may show, for example:

```text
acme-api
  main              primary     clean
  feat/rate-limit   worktree    +142 -18   needs review
  hotfix/tls-expiry no checkout
```

Selecting a branch without a checkout offers to switch a clean primary checkout or create a
worktree. Switching a dirty primary checkout is refused when it could obscure work, with worktree
creation offered instead. Worktree roots and setup hooks are configurable.

One agent per checkout is the default, not a hidden assumption. Multiple agents are allowed but
shown as shared; optional project exclusivity can make that a hard block.

## Agent templates and state

Templates may define command arguments, environment, prompt interpolation, pinned notes,
permissions, one-shot behavior, sandboxing, redaction, and harness-specific resume behavior.
Template identity and display title are separate persisted fields.

The target state model is:

| State | Meaning |
|---|---|
| `idle` | Running without current work |
| `working` | Producing or processing work |
| `waiting` | Requires operator input |
| `needs-review` | Changed files have not been acknowledged |
| `done` | Finished and reviewed |
| `failed` | Non-zero exit or explicit block/failure |

Explicit harness events are the only source of state. A harness that reports nothing sits at
`idle` until it exits — coarse, but honest: process activity cannot separate an agent thinking
from one stopped at a prompt, and output matching reads an agent's own prose as its status.
Events must identify a pane even when several harnesses share a checkout. Parent rows show the
highest-severity descendant with distinct glyphs as well as color.

## Review contract

Review covers uncommitted work split into the two sides Git keeps apart: `HEAD` against the
index, and the index against the working tree. Each side captures its own endpoint, so a file
staged and then edited again shows the correct, different diff on each. The working-tree endpoint
includes deleted, renamed, and non-ignored untracked content. A checkout's recent commits are
reviewable one at a time against their first parent.

Comparing a branch against a fork point, an upstream, or a remembered snapshot belongs to a Git
client, not here.

Review supports durable line or range comments and live-agent selection. The daemon stores a
comment before sending its terminal notification, and agents can read checkout-scoped feedback.
Syntax highlighting is produced in the daemon as token-role spans; the client theme supplies
colour. Staging, unstaging, and reverting are deliberately not Argus's job.

## Notes and context

Projects and checkouts can hold plain Markdown notes. Checkbox lines provide open, done, and
pinned states whose counts roll up the tree. Forwarding to an agent is explicit except for a
template's opt-in pinned-note injection.

The daemon exposes the same scoped context through MCP, HTTP, and `argus ctx`. A per-checkout token
limits every agent to approved read and write calls. Write operations such as note changes,
review requests, and worktree creation are audited and template-policy gated.

## Terminal and memory model

The live screen is a dense grid. Scrollback uses packed text and style runs with a byte budget,
oldest-first eviction, cold-pane decoding, and optional memory-mapped spill. Spill files are
private and template redaction runs before persistence.

The terminal path supports child-negotiated mouse reporting, bracketed paste, focus events, OSC
52 forwarding, extended keys, and usable scrollback navigation. Output queues and parsing work are
bounded under a noisy child.

A pane can be toggled fullscreen, hiding the navigation columns and giving the terminal the full
client window. The navigation columns remain accessible via a keybinding or mouse gesture.

With several clients, PTY size has deterministic ownership or arbitration. A smaller background
view cannot continuously resize a pane away from the active owner.

## Protocol and safety

The protocol remains host-agnostic and gains version/capability negotiation before remote use.
Socket or pipe access is restricted to the user. Slow or lagged clients recover with a full
snapshot rather than retaining silently corrupted grids.

Argus never auto-pushes, auto-merges, or silently deletes work. Destructive actions name the
affected checkout, branch, panes, and dirty files before confirmation. Optional per-template
sandboxing limits writes outside the checkout.
