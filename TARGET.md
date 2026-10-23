# Argus: Target

What Argus is for and the lines it holds. [`ARCHITECTURE.md`](ARCHITECTURE.md) maps how it is
built; [`ROADMAP.md`](ROADMAP.md) is what is left.

## What Argus is

A workspace for running, reviewing and remembering the work of command-line coding agents. It runs
the harnesses a person already uses across their Git checkouts, shows what each is doing, lets them
review the result, and carries what agents learned into the next conversation.

Its spine is `Project → Repository → Checkout → Agent, shell or editor`. A workspace scopes which
projects are visible. Views (features, checkouts, review, code) replace the screen, never the running
work, and the spine is always one key away.

## What it is not

- **Not an agent.** It runs existing CLIs and never chooses how an agent works or browses.
- **Not a Git client.** It shows Git state, makes worktrees and switches branches. Staging,
  reverting, and comparing against a fork point belong to a Git client. It never pushes or merges.
- **Not a tracker.** It links to the team's tracker and may help reconcile with it, but never
  synchronizes or replaces it, and never asks a person to maintain a second board.
- **Not a general multiplexer.** Panes exist to hold agents and the shells beside them.

## Principles

1. **The daemon owns the work; clients are renderers.** Closing a client never stops a pane. The
   terminal client and the desktop window show the same daemon, and neither holds state the daemon
   lacks.
2. **The client never predicts.** A screen changes when the daemon says it did, so a refusal leaves
   the truth on screen.
3. **State comes from what is observed, never asserted.** An agent's status comes from its harness's
   events, a feature's standing from the agents and tasks working on it. The only thing a person sets
   by hand is accepting the work.
4. **Nothing is silently lost.** No auto-push, no auto-merge, no silent deletion. Destructive actions
   name what they affect first. A reversed decision is superseded, not erased.
5. **Agents write the durable context; people correct it.** Corrections take precedence and keep
   their provenance.
6. **Remote is the same product.** Everything happens beside the code, on the daemon's machine, and
   connecting to another machine never installs software there.

## Agent context and memory

This is the part not yet reached. Today context hangs off whichever feature a checkout is on. The
target is:

- A **work context** is a durable topic understanding accumulates around. A checkout may suggest one
  but never silently assigns it; the agent's current request decides what is relevant.
- Agents write typed **artifacts** into it: decisions, tasks, findings, assumptions, open questions
  and summaries, each with its source, session, checkout and time. Routine progress is not memory.
- An agent starts from a bounded **context packet**: what the person forwarded plus the applicable
  artifacts, each with why it was included, and a revision the agent refreshes on. Stale, superseded
  and unrelated material is left out unless asked for.
- The same context is readable through `argus-hook`, `argus ctx` and MCP, limited per checkout by
  token.
- The interface is where a person inspects and corrects that context. A tree, list or timeline is a
  view of it, never its source of truth.

## Bounds

- Terminal client: frames under 16 ms, under 50 MiB resident with twelve active panes, about
  1.5 MiB per idle pane, first paint under 30 ms against a warm daemon.
- Scrollback is byte-bounded, evicts oldest first, and spills privately with redaction applied
  before anything is persisted.
- The endpoint is restricted to its user; the protocol negotiates capabilities and authenticates
  before it is exposed beyond the machine; templates can sandbox writes outside their checkout.
