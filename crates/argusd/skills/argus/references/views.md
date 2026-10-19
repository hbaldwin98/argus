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
`.items[i]` (grid) or `.details[r][i]` (a table row's details). An index is
digits only. A patch leaves the view's refresh timer running.

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

`id` is yours, up to 64 bytes, with no control characters (nor in any action id
or label, form id, field name or row key, which are typed back to you). Two
actions in one view cannot share an id. A block or value `kind` Argus does not
know is refused, so a misspelt one comes back with its path. Numbers are at most
1e15 in magnitude, a `trend` has at most 500 values and a `graph` 1000 edges. `subject` is the ticket's URL when the view is
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
`3.0`. Argus drops from a press whatever your view did not offer: a row key the
table does not have, an answer of the wrong kind, a choice that is not an
option, a number outside `min` and `max`, and text past 4096 bytes is cut. The
button waits until you show the view again, so always do. A press that could
not reach you (the view was gone, or you were not running) is the person's to
hear of: they are told, and you are not.

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
`path` `{ "path", "line" }` (relative to the checkout's root, with no `..`;
opens in the person's editor), `commit` `{ "sha", "title" }` (opens History),
`feature` `{ "number" }`, `pane` `{ "pane", "label" }` (focuses it, in any project; `argus-hook agents` lists the
other agents with their pane ids), and
`view` `{ "id", "label" }`, which opens the saved view `id` beside this one (see
"Saved views, links and embeds" below).

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

## Saved views, links and embeds

A view you `show` goes when you or the daemon do. One you want kept — a summary
of the repository's dependencies, an architecture map, a view of a ticket that
others will open — is **saved**: it is a record in Argus's store with a scope, so
the person finds it in the Views section whenever they come back, and any agent
can rebuild it.

```sh
"$ARGUS_HOOK" view save --stdin <<'EOF'
{
  "scope": { "kind": "repository" },
  "features": [12],
  "recipe": "List the workspace crates from Cargo.toml, then each one's direct dependencies from `cargo tree --depth 1`.",
  "sources": [
    { "kind": "file", "path": "Cargo.toml" },
    { "kind": "commit", "sha": "a41c9e2" },
    { "kind": "url", "url": "https://crates.io/crates/serde" },
    { "kind": "note", "text": "ignore dev-dependencies" }
  ],
  "view": { "id": "deps", "title": "Dependencies", "blocks": [ ... ] }
}
EOF
"$ARGUS_HOOK" view saved           # id, scope, current or stale, title
"$ARGUS_HOOK" view get deps        # the stored JSON, to refresh from
"$ARGUS_HOOK" view stale deps      # say it is out of date
"$ARGUS_HOOK" view delete deps
```

`save` prints `saved: <id>, <n> blocks`, or the refusal with the path of the
part at fault. The view inside is checked exactly as `show` checks it, and its
`id` is the saved view's name: unique across the whole store, up to 64 bytes,
with no control characters and no space around it. **Saving the same id again
replaces the view in place; there are no versions.** A refresh is that: read
what the view was built from, then `save` it again with the same id.

- `scope` is where the person looks for it: `{"kind": "repository"}` (the default;
  your pane's repository), `{"kind": "feature", "number": 12}` (one of its
  features) or `{"kind": "global"}` (belongs to no repository). An id already
  used by a view in another scope is refused; choose another or delete it.
- `features` are numbers of features of your repository to attach it to too. A
  save replaces the attachments for this repository only. Naming a feature the
  repository does not have is refused.
- `recipe` (at most 4096 bytes) is how to build it again, in your words, for the
  agent that is asked to refresh it, which may not be you. `sources` (at most 32)
  say what it was built from: `file` (relative to the checkout, no `..`),
  `commit` (a hex id), `url` (`http` or `https`; Argus does not fetch it, you
  read it) and `note`. Keep both current when you change how you build it.
- Argus records when it was saved and the commit your checkout was on. A view is
  marked **stale** when the person or you say so (`view stale`), or when that
  checkout has since moved to another commit. Argus does not diff the files in
  `sources`; judge for yourself when you read `view saved`, and refresh what is
  stale when you are asked to, or when you are working on what it shows. A save
  clears the mark.
- At most 500 views are kept; past that a save is refused until one is deleted.
- You can list, read, mark and delete the views of your own repository and the
  global ones, not another repository's.

`get` prints the record you can edit and `save` again: `id`, `scope`, `features`,
`recipe`, `sources`, `built_at`, `built_commit`, `stale` and the `view`. The
fields Argus fills in are ignored on the way back in.

When the person presses Refresh on a saved view and you are the agent that last
saved it, you are told on one line, with its recipe and sources:

```
Refresh the saved view `deps` ("Dependencies"). Recipe: List the workspace crates ... Sources: file Cargo.toml; url https://crates.io/crates/serde. Read them again with your own tools, then save it anew with `argus-hook view save --stdin`, same id `deps`; `argus-hook view get deps` prints it as it is.
```

Buttons in a saved view work as they do in a shown one, and are told to the agent
that last saved it. After a daemon restart nobody is remembered as that agent
until one saves it again.

### Links

Any element can open another saved view, which the person follows into a new
column beside this one — so you can build a trail of views, an overview that
links to a view per component. Link to a view by its id; it need not be saved
yet (it is drawn as missing until it is).

- As a value (a field, a table cell, a row item, a list of values): `{ "kind": "view", "id": "deps", "label": "Dependencies" }`.
- On an element drawn from plain text, add `"opens": "deps"` to: `section`, `card`, `details`, a `list` item, a `stats` entry, a `tree` node, a `graph` node, a `timeline` event, a `checklist` item, a `board` card.

### Embeds

`{ "kind": "embed", "view": "deps" }` draws that saved view's blocks right there,
in a bordered frame with its title. Only the id is stored, so the embedded view
stays as current as the last save of it, and a view cannot embed itself. The
reader follows an embed three views deep and draws a line instead past that, or
where the view is not saved.
