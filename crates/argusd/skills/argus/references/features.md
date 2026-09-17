<!-- argus:managed-skill -->

# Features and briefs

A feature is the shared record of one piece of work: a brief, its tasks, and its
decisions. The pane's checkout selects the feature, and every pane in that
checkout shares the selection, so inspect it before changing it.

```sh
"$ARGUS_HOOK" feature                      # the selected feature, brief, and decisions
"$ARGUS_HOOK" feature list
"$ARGUS_HOOK" feature use <returned-slug>
"$ARGUS_HOOK" feature open "session restore" --body "<brief>"
"$ARGUS_HOOK" feature note "restore uses the recorded conversation ID"
"$ARGUS_HOOK" feature export
```

## Choosing a feature

For implementation work, run `feature`. If the selected feature matches the
request, keep it. Otherwise check `feature list` and `use` a matching one. Only
`open` a new feature when none fits; `open` also selects it for this checkout.
Do not switch features just to answer a question.

## Writing the brief

Open a feature with a brief, not a bare title. A good brief is a few short
paragraphs a later agent could start from without this conversation:

- **Outcome**: what should be true when the feature is done, in user terms.
- **Scope**: what is in, and what is explicitly out.
- **Constraints**: contracts, files, or behavior that must not change.
- **Verification**: how someone will know it works (tests, a manual check).

Leave choices between options to the decision board rather than the brief.

## Keeping the brief current

`note` appends to the brief. Add findings a later agent would otherwise have to
rediscover: a non-obvious cause, a gotcha, where the real entry point lives, or
a change of scope the human agreed to. Do not append progress logs, command
output, or anything already recorded as a task or decision.

When the human asks for the reasoning as a document, `export` prints the feature
and its decision board as source material; write the document from it.
