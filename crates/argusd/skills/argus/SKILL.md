---
name: argus
description: Keeps an Argus pane's status and shared work context current. Use when running inside Argus (ARGUS_PANE and ARGUS_HOOK are set), or when asked to use Argus features, tasks, decisions, notes, or review feedback.
---

<!-- argus:managed-skill -->

# Argus

Argus shows this conversation as a pane alongside other agents. Use its helper
to report what the work means and read the context the human has left for you.
User instructions take precedence over this skill; it does not authorize work
beyond the user's request.

## Establish context

Check that `ARGUS_PANE` and `ARGUS_HOOK` are set before calling the helper.
If they are absent, continue the user's task without Argus reporting. Do not
guess a pane ID, use another pane's credentials, or start a daemon to report.

Use the executable in `ARGUS_HOOK`, which may not be on `PATH`. In a POSIX shell,
invoke it as `"$ARGUS_HOOK"`; in PowerShell, use `& $env:ARGUS_HOOK`.
The examples below use POSIX syntax. Routing comes from the inherited environment.

At the start of work, and when the task or checkout changes, read:

```sh
"$ARGUS_HOOK" context
"$ARGUS_HOOK" comments
"$ARGUS_HOOK" feature
"$ARGUS_HOOK" task
```

`context` includes project and checkout notes. Lines marked `- [!]` are standing
instructions. Review comments are durable feedback for this checkout. `feature`
includes the current feature's brief and decision board; `task` shows its tasks.
Use returned IDs, slugs, and line numbers for subsequent commands.

## Keep the pane informative

Name the pane after the task once you understand it, and rename it when the task
changes. Report `working` when starting or resuming work; some harnesses have no
turn-start event. Existing lifecycle hooks still report the events they support.

```sh
"$ARGUS_HOOK" title "repairing session restore"
"$ARGUS_HOOK" status working
"$ARGUS_HOOK" status waiting "needs database access"
"$ARGUS_HOOK" status failed "blocked by an unavailable dependency"
"$ARGUS_HOOK" status needs-review "ready for review"
"$ARGUS_HOOK" status done "reviewed and complete"
```

Use `waiting` when you need a human, with a brief reason that contains no secrets.
Use `failed` for work you cannot complete because of a failure. Use `needs-review`
when changes are ready to inspect, and `done` only after review and completion.
A turn stopping or a process exiting does not establish that the task is done.
Report meaningful transitions, not every command. Reporting failures should not
prevent progress on the user's task. The helper always exits successfully, so
read command output for refused writes rather than treating exit code 0 as proof.

## Work in the right checkout

Other agents may share the checkout. Avoid switching its branch in place;
when another branch is needed, create a linked worktree and continue there.
After moving, run `"$ARGUS_HOOK" checkout` from the new directory so Argus can
move this pane under a known checkout in the same project. This reports a move;
it does not create a worktree or change your working directory. Resolve this
skill and its references from the new checkout after moving; Argus may remove
the old checkout's managed skill when its last agent leaves.

## Maintain shared work

When implementing a feature or updating the board, read
[references/work.md](references/work.md) for feature selection, task updates,
decisions, and note writes. Keep those records relevant to the requested work;
an informational question alone does not require creating a feature or tasks.
