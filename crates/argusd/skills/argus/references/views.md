<!-- argus:managed-skill -->

# Views

A view shows the person something you read with your own tools — a Jira or
Linear ticket, a CI run, a query's rows — in a side panel of the Argus desktop
window. Argus never calls a tracker: you read it, translate it into the blocks
below, and send them. Show a view only when the person asked to see something,
or when a table or a ticket says it better than a reply would. The terminal
client draws a text subset of them in its `views` stage; charts, graphs, trees,
boards, timelines, matrices, diffs, forms and diagrams there read "open in the
desktop".

```sh
"$ARGUS_HOOK" view show --stdin <<'EOF'
{ "id": "arg-142", "title": "Rank recipes by freshness", ... }
EOF
"$ARGUS_HOOK" view close arg-142
```

It prints `shown: <id>, <n> blocks`, or the refusal with the path of the block
at fault (`blocks[1].rows[3] has 5 cells, the table has 6 columns`): fix that
block and send the view again. Sending an `id` again replaces that view, so
reuse the id when you refresh it. When the person presses Refresh you are asked
to read the source again and show the view anew with the same id.

To move one part of a view — a progress bar, a log tail — without sending it
all again, patch that block by its path:

```sh
"$ARGUS_HOOK" view patch arg-142 'blocks[1].columns[0][2]' --stdin <<'EOF'
{ "kind": "progress", "label": "Migration", "value": 412, "max": 1000 }
EOF
```

It prints `patched: <id> <path>`. A path starts `blocks[i]` and continues with
`.blocks[i]` (section, card, details), `.columns[c][i]`, `.tabs[t].blocks[i]`,
`.items[i]` (grid) or `.details[r][i]` (a table row's details).

## The view

```json
{
  "id": "arg-142",
  "title": "Rank recipes by freshness",
  "status": { "label": "In progress", "tone": "active" },
  "subject": "https://acme.atlassian.net/browse/ARG-142",
  "feature": 12,
  "refresh_every": 300,
  "actions": [{ "id": "rerun", "label": "Rerun failed", "tone": "active" }],
  "blocks": [ ... ]
}
```

`id` is yours, up to 64 bytes. `subject` is the ticket's URL when the view is
of one; leave it out otherwise. Only `http` and `https` links are accepted.
`feature` is the number of the feature the view belongs with: its header offers
the view too. `refresh_every` (at least 60 seconds) has you asked to read the
source again that often while the view is up.

## Actions

An action is a button: `{ "id", "label", "tone", "confirm" }`, where `confirm`
is a question asked before it is sent. Put them on the view, a `card`, a `row`,
a `table` (on its selected rows) or as a `form`'s `submit`; at most 12 in one
place. Argus runs nothing for a button. When the person presses one you are told,
on one line:

```
The person pressed `rerun` ("Rerun failed") in the view `ci`. Selected rows: ["4812"]. Form `deploy` answers: {"branch":"main","dry_run":true}. Do what it asks with your own tools, then show the view again with `argus-hook view show --stdin`, same id `ci`, so the person sees it done.
```

The rows and form sentences appear only when there are some. Numbers arrive as
`3.0`. The button waits until you show the view again, so always do.

`tone` is the only colour you choose, by meaning: `neutral`, `good`, `warn`,
`bad`, `active` (in progress), `info`.

## Blocks

Each block has a `kind`:

- `markdown` `{ "text" }`: headings, lists, emphasis, code, links, quotes. No HTML, no images.
- `fields` `{ "fields": [{ "label", "value" }] }`: a ticket's details, in your order.
- `table` `{ "columns": [{ "title", "numeric" }], "rows": [[value, ...]], "sorted_by", "total" }`: at most 200 rows; send the first ones and set `total` to how many there were.
- `list` `{ "items": [{ "who", "at", "title", "body", "chip" }] }`: comments, events, commits; `body` is Markdown, `at` RFC 3339. An item without `who` reads as an event.
- `stats` `{ "stats": [{ "label", "value", "delta", "tone" }] }`: `value` already formatted (`4m 12s`, `70%`).
- `callout` `{ "tone", "text" }`: one line or a short paragraph that must not be missed.
- `code` `{ "lang", "text" }`: a log excerpt or a query.
- `diagram` `{ "source" }`: Mermaid sequence diagrams.
- `section` `{ "heading", "blocks" }`, `columns` `{ "columns": [[block, ...], ...] }`, `tabs` `{ "tabs": [{ "title", "blocks" }] }`: layout, nesting at most four deep.
- `card` `{ "title", "chip", "tone", "blocks", "actions" }`: a bordered box; a tone tints its border.
- `grid` `{ "min": "sm" | "md" | "lg", "items": [block, ...] }`: as many side by side as fit, wrapping. Cards in a grid make a dashboard.
- `details` `{ "summary", "chip", "open", "blocks" }`: a line that opens to its blocks.
- `row` `{ "align": "start" | "spread" | "end", "items": [{ "kind": "value", "value" } | { "kind": "action", "action" }] }`: values and buttons on one line.
- `divider` `{}`.
- `chart` `{ "shape": "line" | "bar" | "area" | "spark", "x": [label, ...], "series": [{ "name", "values": [n | null, ...], "tone" }], "unit" }`: one value per x, up to 500 points and 8 series. Argus draws the axes.
- `progress` `{ "label", "value", "max", "tone", "note" }`: `note` is already formatted, `412 / 1,000`.
- `timeline` `{ "events": [{ "at", "title", "tone", "body", "lane" }] }`: in order; events sharing a `lane` line up in a column of their own.
- `tree` `{ "nodes": [{ "label", "value", "chip", "open", "children" }] }`: files, dependencies; up to 1000 nodes, 12 deep.
- `graph` `{ "nodes": [{ "id", "label", "tone" }], "edges": [{ "from", "to", "label" }], "direction": "down" | "right" }`: boxes and arrows Argus lays out; up to 200 nodes.
- `diff` `{ "path", "patch" }`: a unified diff, drawn as Review draws one.
- `matrix` `{ "rows", "columns", "cells": [[{ "tone", "weight", "label" }]] }`: tinted cells, `weight` 0 to 1.
- `checklist` `{ "items": [{ "label", "done", "note" }] }`: read-only.
- `log` `{ "lines": [{ "at", "level": "debug" | "info" | "warn" | "error", "text" }] }`: a tail, up to 1000 lines.
- `board` `{ "columns": [{ "title", "chip", "cards": [{ "title", "body", "chip", "value" }] }] }`.
- `form` `{ "id", "fields": [{ "name", "label", "input" }], "submit": action }`: `input` is `{ "kind": "text", "default", "placeholder", "multiline" }`, `number` (`default`, `min`, `max`), `choice` (`options`, `default`), `toggle` (`default`) or `date` (`default`). The answers come back with the press.

A `table` also takes `row_keys` (one per row), `select` (`none`, `one`, `many`),
`actions` offered on the selected rows (these need `row_keys`), and `details`,
one entry per row: `null` or the blocks that row opens to. Tables of more than
eight rows get a filter; a `stat` takes a `trend`, recent values oldest first.

## Values

A field, a table cell: `{ "kind": ..., ... }`.

`text` `{ "text" }`, `number` `{ "number", "unit" }`, `bool` `{ "value" }`,
`date` `{ "date": "2026-10-24" }`, `date_time` `{ "at": RFC 3339 }`,
`person` `{ "name" }`, `people` `{ "names" }`, `chip` `{ "chip": { "label", "tone" } }`,
`chips` `{ "chips": [...] }`, `link` `{ "label", "url" }`,
`ticket` `{ "key", "title", "url", "status" }`, `progress` `{ "value", "max", "tone" }`,
`duration` `{ "ms" }`, `code` `{ "text" }`, `empty` `{}`.

These open things in Argus rather than a browser, read against your pane:
`path` `{ "path", "line" }` (relative to the checkout's root; opens in the
person's editor), `commit` `{ "sha", "title" }` (opens History),
`feature` `{ "number" }`, `pane` `{ "pane", "label" }` (focuses it).

## A ticket

Copy values verbatim; never paraphrase a ticket's text. Map its parts:

- Status → the view's `status`, toned by its category (to do `neutral`, in progress `active`, in review `warn`, done `good`, blocked `bad`).
- Description and acceptance criteria → `markdown` blocks. Jira's rich text (ADF) becomes Markdown.
- Assignee, reporter, priority, labels, sprint or cycle, estimate, due date → `fields`, each with its typed value.
- Custom fields → more `fields`, labelled by their names, not their ids (Jira: ask for the field names, or read them from the issue's `names`). The board's tracker map (`"$ARGUS_HOOK" tracker-map`) says which ones the team wants shown; leave out empty ones unless it asks for them.
- Comments → a `list`; status changes → `list` items without `who`.
- Blockers → a `warn` callout naming the blocking ticket.
- Linked tickets and subtasks → a `table` whose cells are `ticket` values.

Linear has no free-form custom fields: label groups, estimate, cycle, project
and milestone are its equivalents.
