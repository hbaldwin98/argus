# Argus: Target Design

This document defines the intended product. It does not claim these systems are implemented;
[`DESIGN.md`](DESIGN.md) is the current behavior and [`ROADMAP.md`](ROADMAP.md) orders the gap.

## Product boundary

Argus is a terminal workspace built around one navigation spine:

```text
Project -> Repository -> Checkout -> Agent, shell, or editor
```

A workspace scopes the visible projects without adding another column. Argus is not a general
tmux replacement, a GUI, an agent host, a Git porcelain, or a project tracker. Teams may already
plan work in Jira, Linear, GitHub Issues, or somewhere else; Argus runs existing command-line
harnesses and makes their checkouts, state, review work, and accumulated understanding visible.

The spine is the default *view*, not the only one. A view owns the whole content area and is
switched between as a tab. Review and durable work context need the full area rather than another
narrow navigation column beside a pane. Switching views never stops a pane, and the spine is always
one keystroke away — what a view replaces is the screen, never the running work.

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
Template identity and display title are separate persisted fields. The daemon
names a pane from the latest user prompt a harness reports; an agent may refine
that name with an explicit title command.

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
colour. The same diff reads unified or split side by side, chosen in the client over data the wire
already carries; a comment means the same thing in either, because it anchors by both line
numbers. Staging, unstaging, and reverting are deliberately not Argus's job.

## Agent context and memory

Projects and checkouts can hold plain Markdown notes. Checkbox lines provide open, done, and
pinned states whose counts roll up the tree. Forwarding to an agent is explicit except for a
template's opt-in pinned-note injection. Notes remain human-owned documents; they are one source an
operator can pull into an agent, not Argus's model of the work.

The daemon exposes the same scoped context through MCP, HTTP, and `argus ctx`. A per-checkout token
limits every agent to approved read and write calls. Write operations such as note changes,
review requests, and worktree creation are audited and template-policy gated.

Argus's durable context is written primarily by agents and corrected by humans. Its purpose is to
carry useful understanding between conversations, not to duplicate the team's work tracker. The
normal loop is:

1. A person sends an agent the sources and instructions relevant to the current work.
2. The agent uses them and records the durable results of its work.
3. Later agents receive a bounded, relevant subset of those results.
4. A person corrects, removes, or supersedes anything that should not guide future work.

A **work context** is a durable topic around which that understanding accumulates. It may have a
short brief and links to external work, but it is not a feature card, assignment, or lifecycle. A
checkout may suggest a context, but sharing a checkout or being its only context never proves that
the context applies to a new request. The pane's current work and the user's request determine what
is relevant.

Agents write typed **artifacts** into a work context: decisions, tasks, findings, assumptions, open
questions, and compact summaries. Each artifact carries its source, authoring session, checkout,
and time. A task may retain an opaque Jira, Linear, or GitHub key, but Argus neither synchronizes
nor replaces that tracker. Routine transcript and progress chatter are not durable artifacts.

Artifacts have correction semantics suited to their type. A decision records what was chosen, the
real alternative, and the reason; a later decision supersedes it without erasing the old reasoning.
A false finding or assumption can be corrected or withdrawn. Tasks can be completed or discarded.
Human corrections take precedence in future reads, but retain provenance so an agent can explain
where its context came from.

An agent starts from a **context packet**, not a whole project database. The packet combines what
the person explicitly forwarded with a bounded selection of applicable artifacts. It states why
each durable item was included and omits stale, superseded, completed, or unrelated material unless
the current request calls for history. Agents refresh the packet when its revision changes and
write newly learned artifacts back as part of finishing work.

The primary UI lets a person inspect and correct the context agents have built. A decision tree,
task list, timeline, or board may be a useful projection of artifacts, but no projection is the
source of truth and none requires the person to maintain a second planning system.

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
