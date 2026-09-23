---
name: argus
description: Keeps an Argus pane's status and shared work context current. Use when running inside Argus (ARGUS_PANE and ARGUS_HOOK are set) and you need to report status, or when asked to use Argus features, tasks, decisions, sequence diagrams, or review feedback.
---

<!-- argus:managed-skill -->

# Argus

Argus shows this conversation as a pane alongside other agents. User
instructions take precedence over this skill; it does not authorize work beyond
the user's request. If `ARGUS_PANE` and `ARGUS_HOOK` are not set, continue
without Argus; never guess a pane ID or use another pane's credentials.

Invoke the executable in `ARGUS_HOOK`, which may not be on `PATH`: `"$ARGUS_HOOK"`
in a POSIX shell, `& $env:ARGUS_HOOK` in PowerShell. It always exits 0, so read
its output to tell a refused write from a successful one.

## Context

Your harness usually shows the pane's context at session start: review comments,
the checkout's feature brief, and its open tasks. If it did not, or the checkout
changed, or you are about to write to the board, run it once:

```sh
"$ARGUS_HOOK" context
```

`feature` and `task` show the full brief, decisions, and every task with its brief
when the summary is not enough.

## Title and status

Set a short title (a few words, like a commit subject) once you understand the
task, and rename it when the task changes. Report only what hooks cannot know:

```sh
"$ARGUS_HOOK" title "repairing session restore"
"$ARGUS_HOOK" status waiting "needs database access"
"$ARGUS_HOOK" status failed "blocked by an unavailable dependency"
"$ARGUS_HOOK" status needs-review "ready for review"
"$ARGUS_HOOK" status done "reviewed and complete"
```

Hooks already report `working` and `idle` as turns start and stop, and read model
and context telemetry. Only if the pane never turns working on its own, report
`status working` yourself when starting or resuming.

Use `waiting` when you need a human, with a brief reason that contains no secrets;
`failed` for work you cannot complete; `needs-review` when changes are ready to
inspect; `done` only after review and completion. A turn stopping does not make a
task done. Reporting failures must not block the user's task.

## Checkout

Other agents may share the checkout, so do not switch its branch in place. For
another branch, create a linked worktree, continue there, and run
`"$ARGUS_HOOK" checkout` from the new directory. Resolve this skill from the new
checkout afterwards.

## Shared work

Each feature has a brief, tasks with subtasks, decisions, and optional sequence
diagrams. Only when you are about to change one of them, read its reference:

- [references/features.md](references/features.md): choosing, opening, and noting on a feature.
- [references/tasks.md](references/tasks.md): shaping tasks and moving them through `todo`, `doing`, `done`.
- [references/decisions.md](references/decisions.md): recording choices and superseding them.
- [references/diagrams.md](references/diagrams.md): recording an interaction flow as Mermaid.

Keep those records relevant to the requested work. Answering a question does not
require creating a feature or tasks.
