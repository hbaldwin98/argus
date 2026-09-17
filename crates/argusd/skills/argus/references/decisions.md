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
`--supersedes` so the earlier reasoning stays visible. If the change reverses
something the human decided, confirm with them first.
