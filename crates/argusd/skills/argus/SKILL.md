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
and which feature the checkout is on. If it did not, or the checkout changed, run
it once:

```sh
"$ARGUS_HOOK" context
```

The feature's brief is not in it. When the work touches the feature, or you are
about to write to the board, `feature` shows the brief and decisions and `task`
every task with its brief.

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
another branch, create a linked worktree and continue there. When a turn ends
with your working directory inside it, the pane follows you. To move it sooner,
or if your harness does not report its directory, run `"$ARGUS_HOOK" checkout`
from the new directory; it says whether the pane moved. Resolve this skill from
the new checkout afterwards.

## Other agents

Only when the user asks for a task to persist in a pane of its own, one they
can go back to, start it as another agent. Working in a worktree yourself, or
delegating to a subagent, needs none of this:

```sh
"$ARGUS_HOOK" agent --worktree fix-resize "fix the resize race; see task 12"
"$ARGUS_HOOK" agent --worktree port-x --from ~/src/other --stdin < brief.md
"$ARGUS_HOOK" agent --in ~/src/other "update the client for the new field"
"$ARGUS_HOOK" agent "review the diff in this checkout"
```

`--worktree` makes a new worktree, in your repository or the one `--from`
names; `--in` uses a checkout Argus already shows; with neither the agent shares
this checkout. It is your own kind of agent unless `--template` names another.
The prompt is all it knows, so make it a complete brief: the goal, the files,
how to verify, and what to report. A brief over 16 KB goes in a file the prompt
names. It prints the new pane's number, or why it was not started.

## Shared work

Each feature has a brief, tasks with subtasks, decisions, and optional sequence
diagrams. Only when you are about to change one of them, read its reference:

- [references/features.md](references/features.md): choosing, opening, and noting on a feature.
- [references/tasks.md](references/tasks.md): shaping tasks, moving them through `todo`, `doing`, `done`, and their criteria and notes.
- [references/decisions.md](references/decisions.md): recording choices, superseding them, and raising one that is the human's as an open question.
- [references/diagrams.md](references/diagrams.md): recording an interaction flow as Mermaid.

To show the person a ticket, a CI run or a query's rows in the desktop window,
or to keep a view they can come back to (saved views, with links between views
and embeds), read [references/views.md](references/views.md) first.

Keep those records relevant to the requested work. Answering a question does not
require creating a feature or tasks.

## Browser

When the work needs a web page — checking a UI you changed, reading a page
behind a dev server — use the pane's own browser, which the person watches in
the window's Browsers stage and can take over:

```bash
"$ARGUS_HOOK" browser          # prints its DevTools URL, starting it if needed
agent-browser --cdp "<url>" open http://localhost:5173
"$ARGUS_HOOK" browser close    # when you are done with it
```

Any tool that attaches over the Chrome DevTools Protocol works in place of
`agent-browser`. The person sees the tab you last acted in. If they take
control, your clicks, typing and navigation fail with an error saying so:
stop driving and wait, or ask them; reading the page still works. It is
headless with a fresh profile, and goes when the pane does.

## Extensions

To build an Argus extension, or find out what a running one does, ask the
`argus` CLI rather than guessing: `argus extension guide` walks through
building one step by step and then gives the full reference;
`argus extension list` and `argus extension show NAME` describe the ones
running now.
