<!-- argus:managed-skill -->

# Shared work in Argus

These commands use the helper established in the skill. The pane's checkout
selects the feature whose brief, tasks, and decisions you read and change.
Feature selection is shared by panes in that checkout, so inspect the current
feature before changing it. Preserve the user's chosen tracker and scope.

## Features

```sh
"$ARGUS_HOOK" feature
"$ARGUS_HOOK" feature list
"$ARGUS_HOOK" feature use <returned-slug>
"$ARGUS_HOOK" feature open "session restore"
"$ARGUS_HOOK" feature note "restore uses the recorded conversation ID"
```

For implementation work, use a matching existing feature or open one if needed.
`open` also selects the new feature for this checkout. Append findings a later
agent will need to the brief; avoid duplicating its decisions or a running log.

## Tasks

```sh
"$ARGUS_HOOK" task
"$ARGUS_HOOK" task doing <id>
"$ARGUS_HOOK" task done <id>
"$ARGUS_HOOK" task todo <id>
"$ARGUS_HOOK" task add "restore the recorded conversation"
"$ARGUS_HOOK" task add "test reconnect" --key PROJECT-412
"$ARGUS_HOOK" task retitle <id> "test reconnect after daemon restart"
"$ARGUS_HOOK" task drop <id>
```

Read the tasks before taking one up. Move the task you are working on to `doing`
and to `done` when complete. Task completion is separate from the human accepting
the feature as a whole. Add relevant discovered work or import tasks when asked;
`--key` retains an external tracker's identifier without synchronizing that tracker.
Retitle or drop tasks only when the requested work calls for correcting the board.

## Decisions

```sh
"$ARGUS_HOOK" decisions
"$ARGUS_HOOK" decide "resume by conversation ID" --over "most recent session" --because "panes share a checkout"
"$ARGUS_HOOK" decide "persist the ID with the pane" --under <id>
"$ARGUS_HOOK" decide "use the new identity source" --supersedes <id> --because "the previous source omits resumed sessions"
```

Read the board before planning. Record a decision when you choose one real option
over another, with the reason and the parent decision that constrained it. Routine
steps do not need decisions. Revisit an earlier decision when a new finding
invalidates it; supersede it so the earlier reasoning remains visible.

## Checkout notes

```sh
"$ARGUS_HOOK" context
"$ARGUS_HOOK" todo add "verified reconnect"
"$ARGUS_HOOK" todo done <line>
"$ARGUS_HOOK" todo open <line>
```

Write notes only where the project allows it. These writes are attributed to you;
the daemon refuses them when `agent_todos` is disabled. They change the checkout's
note, not the feature task list. Read current line numbers before updating a note.
Pinned standing instructions cannot be completed as todo items. Read refusal
messages and continue within the available permissions.
