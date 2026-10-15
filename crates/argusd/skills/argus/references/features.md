<!-- argus:managed-skill -->

# Features and briefs

A feature is the shared record of one piece of work: a brief, its tasks, and its
decisions. The pane's checkout selects the feature, and every pane in that
checkout shares the selection, so inspect it before changing it. A human names
a feature by its number (`#7`), which `feature` and `feature list` print;
commands take its slug.

```sh
"$ARGUS_HOOK" feature                      # the selected feature, brief, and decisions
"$ARGUS_HOOK" feature <slug>               # another feature, without selecting it
"$ARGUS_HOOK" feature list
"$ARGUS_HOOK" feature use <returned-slug>
"$ARGUS_HOOK" feature open "session restore" --body "<brief>"
"$ARGUS_HOOK" feature note "restore uses the recorded conversation ID"
"$ARGUS_HOOK" feature export
```

## Choosing a feature

For implementation work, start from the feature in your pane's context (or
`context`). If the selected feature matches the request, keep it. Otherwise
check `feature list` and `use` a matching one. Only `open` a new feature when
none fits; `open` also selects it for this checkout.
Do not switch features just to answer a question.

To read or write a feature other than the selected one — another agent in this
checkout is working on the selected one — add `--feature <slug>` to any
`feature`, `task`, `decisions`, `decide` or `diagram` command instead of `use`.
It leaves the checkout's selection alone.

## Writing the brief

Open a feature with a brief, not a bare title. A good brief is a few short
paragraphs a later agent could start from without this conversation:

- **Outcome**: what should be true when the feature is done, in user terms.
- **Scope**: what is in, and what is explicitly out.
- **Constraints**: contracts, files, or behavior that must not change.
- **Verification**: how someone will know it works (tests, a manual check).

Leave choices between options to the decision board rather than the brief, and
a choice that is the human's to make as an open decision there.

## Keeping the brief current

Every agent in the checkout is shown the whole brief at the start of each
session, so it has to be true and short. It describes the feature as it stands,
not how it got there.

`note` appends to the brief. Add findings a later agent would otherwise have to
rediscover: a non-obvious cause, a gotcha, where the real entry point lives, or
a change of scope the human agreed to. Do not append progress logs, command
output, or anything already recorded as a task or decision. What the human
still has to check or choose goes on the board as tasks (see
[tasks.md](tasks.md)), not in the brief.

When the brief states something that is no longer true — a superseded
decision, a scope item now built, a note later work disproved — rewrite that
paragraph with `brief` rather than appending a correction beside it.

## Correcting a feature

```sh
"$ARGUS_HOOK" feature brief <slug> "<the whole brief>"   # replaces it
"$ARGUS_HOOK" feature retitle <slug> "<title>"           # keeps the slug
"$ARGUS_HOOK" feature drop <slug>                        # only with nothing under it
```

`brief` replaces the brief outright, which is how a paragraph appended in error
is taken back; read it with `feature <slug>` first and write it back whole.
`drop` removes a feature opened by mistake, or one whose tasks you have dropped.

When the human asks for the reasoning as a document, `export` prints the feature
and its decision board as source material; write the document from it.

## Holding a feature, and ordering it after another

```sh
"$ARGUS_HOOK" feature hold <slug> "<why>"      # e.g. "until the user answers the install questions"
"$ARGUS_HOOK" feature unhold <slug>
"$ARGUS_HOOK" feature wait <slug> <other-slug> # <slug> comes after <other-slug>
"$ARGUS_HOOK" feature unwait <slug> <other-slug>
```

`feature list` shows a held feature's reason and the features it still waits
on; `feature` and your pane's context lead with them. Do not start work on a
held or waiting feature unless the human asks you to. Hold a feature when work
on it has to stop for a reason no task shows, and say the reason so a later
reader can tell when it no longer stands. Use `wait` rather than a hold when
the reason is another feature: the wait ends by itself once that feature is
accepted. Lift a hold when its reason has gone.

## Linking a feature to its tickets

```sh
"$ARGUS_HOOK" feature link <slug> <ticket-url>                # tracker and key read from the URL
"$ARGUS_HOOK" feature link <slug> <url> --tracker Shortcut --key SC-12
"$ARGUS_HOOK" feature link <slug> <url> --role "the PM's ticket"   # why it is linked
"$ARGUS_HOOK" feature unlink <slug> <key>
```

A link records which ticket in the team's tracker (Linear, Jira, GitHub
Issues, anything with a URL) a feature is about, so a person can open it from
Argus and you know which ticket to read. `feature` lists the links with their
URLs. Argus reads tracker and key out of Linear, Jira and GitHub URLs; for
another tracker give `--key`, and `--tracker` if the host is not its name.
Linking the same key again corrects its URL.

Argus does not talk to trackers; you do, with whatever tools your harness
has. Change nothing in a tracker, and bring nothing from one into Argus, that
the human has not asked for and approved line by line: say what you would
pull or send, wait for their answer, then do only that and report each change.

```sh
"$ARGUS_HOOK" tracker-map                        # the repo's map, and any draft waiting
"$ARGUS_HOOK" tracker-map draft --stdin          # propose one; plain words on stdin
```

The tracker map says, in plain words, what a feature, a task and a decision
answer to in each tracker, and what never leaves Argus. Read it before you link
or sync anything, and follow it. When asked to write one, draft it from what
you can read of the trackers: the draft waits beside the map until the human
approves it with `M` on the feature index, and until then the map that stands
is the one to follow. Draft only when asked.

### Reconciling a feature with its tickets

```sh
"$ARGUS_HOOK" reconcile --feature <slug>                  # what was asked, and where it stands
"$ARGUS_HOOK" reconcile propose --feature <slug> --stdin  # your proposal, as JSON
"$ARGUS_HOOK" reconcile report --feature <slug> --failed 1 "403 from Jira"
```

The person asks for a reconcile in the desktop window, and Argus tells you
what they asked. Read the map, the feature and the tickets with your own
tools, write nothing anywhere, then propose. A proposal is one line per
change, each a single direction:

```json
{"asked": "put D2 on the Jira ticket",
 "lines": [{"to": "Jira", "key": "ARG-142", "does": "comment with decision D2",
            "why": "D2 is accepted", "text": "Decision D2 (accepted): ...",
            "call": "Jira add comment\n  issue ARG-142\n  body the text above"}],
 "noticed": [{"key": "ENG-93", "text": "title differs in Linear"}]}
```

`to` is `Argus` for a change brought into Argus, else the tracker written to.
`text` is the exact text the line writes and `call` the call you will make.
Propose only within the ask; list anything else you saw under `noticed`, and
never act on it. Every line starts unticked. When the person ticks lines Argus
tells you; `reconcile` then lists exactly those, in the words the person may
have edited. Carry out those and nothing else, then `report`, naming each
line that failed with what the tracker said. Asked without a desktop, propose
the same way: a person can tick only what was proposed.

## Accepting a feature

```sh
"$ARGUS_HOOK" feature done <slug>
"$ARGUS_HOOK" feature reopen <slug>
```

`done` archives a feature as accepted. Accepting work is the human's call: run
it only when the human tells you to, never because your tasks are finished. It
is recorded as your move. `reopen` takes it back.
