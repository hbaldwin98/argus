<!-- argus:managed-skill -->

# Tasks and subtasks

Tasks belong to the selected feature (see [features.md](features.md)).

```sh
"$ARGUS_HOOK" task                                   # the tree, with IDs and states
"$ARGUS_HOOK" task add "restore the recorded conversation"
"$ARGUS_HOOK" task add "bound the queue" --under <parent-id>
"$ARGUS_HOOK" task add "test reconnect" --key PROJECT-412
"$ARGUS_HOOK" task doing <id>
"$ARGUS_HOOK" task done <id>
"$ARGUS_HOOK" task todo <id>
"$ARGUS_HOOK" task brief <id> "outcome, boundaries, and verification"
"$ARGUS_HOOK" task retitle <id> "test reconnect after daemon restart"
"$ARGUS_HOOK" task drop <id>
```

## Shaping the board

A top-level task is one observable result a human could review on its own. Title
it as the outcome ("restore resumes the recorded conversation"), not an activity
("look into restore"). Read the existing tasks before adding any, and extend the
board rather than duplicating it.

Add subtasks with `--under` when a task needs several distinct steps, when you
discover a smaller piece of work inside it, or when one step needs its own
decision or verification. Do not create subtasks for single commands or for work
you will finish in the same few minutes. Keep nesting shallow; a third level is
rarely needed.

`--key` records an external tracker's identifier without synchronizing that tracker.

## Moving work

- Mark the task you are actively working on `doing` before starting it; keep
  only what you are actually working on in `doing`.
- When working a subtask, mark it `doing`, and mark its parent `doing` too if it
  is not already.
- Mark a task `done` when its own result is complete and verified. Mark a parent
  `done` only after every remaining subtask is `done` or dropped.
- Return a task to `todo` if you stop without finishing it.
- Task completion is separate from the human accepting the feature as a whole.

## Correcting the board

Use `brief` when the title cannot carry context, boundaries, or verification;
running it without text clears the brief. Retitle a task when its outcome was
misdescribed, and drop it when the work is no longer wanted. Do not reshape tasks
the requested work does not touch.
