<!-- argus:managed-skill -->

# Decisions

Decisions belong to the selected feature (see [features.md](features.md)) and
explain why the work looks the way it does.

```sh
"$ARGUS_HOOK" decisions
"$ARGUS_HOOK" decide "resume by conversation ID" --over "most recent session" --because "panes share a checkout"
"$ARGUS_HOOK" decide "persist the ID with the pane" --under <id>
"$ARGUS_HOOK" decide "use the new identity source" --supersedes <id> --because "the previous source omits resumed sessions"
```

Read the board before planning, and plan within it.

Record a decision when you choose one real option over another that a later
agent might reasonably have chosen instead. Name the rejected option with
`--over` and the reason with `--because`. Use `--under` when an earlier decision
constrained this one. Routine steps and forced choices do not need decisions.

When a new finding invalidates a decision, record the replacement with
`--supersedes` so the earlier reasoning stays visible, and rewrite whatever the
feature's brief and open task briefs say about the old choice. If the change
reverses something the human decided, confirm with them first.

A decision can say more than its name: `--context` for what made it necessary,
`--detail` for the decision in full, `--consequence` (repeatable; lead with `-`
for a cost, `+` or nothing for a gain) for what it costs and buys, and
`--task <id>` for the task it was raised under. `--rejected` records an answer
that was considered and turned down, so nobody proposes it again.

```sh
"$ARGUS_HOOK" decide "refuse when the queue is full" --task <task-id> --context "a dropped frame is a wrong screen" --consequence "-senders must retry"
```

## Open decisions

When the choice is the human's to make — it changes scope, cost, or behavior
they will live with, or they asked to be consulted — do not decide it alone.
Raise it as an open decision: the question, and the options being weighed.

```sh
"$ARGUS_HOOK" decide "which bound does the queue use?" --open --option "a count" --option "a byte size" --task <task-id>
"$ARGUS_HOOK" decisions resolve <id> "a byte size" --because "frames vary a hundredfold"
```

`decisions`, `feature`, and your pane's context mark open decisions as
questions. One is not decided: do not build on any of its options, and say in
your status that you are waiting on it. Resolve one only on the human's word,
with their answer as they gave it; the options not taken are recorded as what
it was chosen over. `--task` ties the question to the task it holds up.

## Correcting a decision

A decision recorded in error — wrong feature, never really decided — is
withdrawn, with the number `decisions` prints. It stays in the history for
people and stops guiding agents; `restore` takes that back. Wrong wording is
rewritten in place: `show` prints one decision as a document, and `rewrite`
reads that document back. Rewriting corrects what a decision says; a decision
that turned out wrong is still superseded.

```sh
"$ARGUS_HOOK" decisions withdraw <id>
"$ARGUS_HOOK" decisions restore <id>
"$ARGUS_HOOK" decisions show <id>
"$ARGUS_HOOK" decisions rewrite <id> --stdin
```
