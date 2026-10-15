<!-- argus:managed-skill -->

# Views

A view shows the person something you read with your own tools — a Jira or
Linear ticket, a CI run, a query's rows — in a side panel of the Argus desktop
window. Argus never calls a tracker: you read it, translate it into the blocks
below, and send them. Show a view only when the person asked to see something,
or when a table or a ticket says it better than a reply would. The terminal
client does not draw views.

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

## The view

```json
{
  "id": "arg-142",
  "title": "Rank recipes by freshness",
  "status": { "label": "In progress", "tone": "active" },
  "subject": "https://acme.atlassian.net/browse/ARG-142",
  "blocks": [ ... ]
}
```

`id` is yours, up to 64 bytes. `subject` is the ticket's URL when the view is
of one; leave it out otherwise. Only `http` and `https` links are accepted.

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
- `diagram` `{ "source" }`: Mermaid.
- `section` `{ "heading", "blocks" }`, `columns` `{ "columns": [[block, ...], ...] }`, `tabs` `{ "tabs": [{ "title", "blocks" }] }`: layout, nesting at most four deep.

## Values

A field, a table cell: `{ "kind": ..., ... }`.

`text` `{ "text" }`, `number` `{ "number", "unit" }`, `bool` `{ "value" }`,
`date` `{ "date": "2026-10-24" }`, `date_time` `{ "at": RFC 3339 }`,
`person` `{ "name" }`, `people` `{ "names" }`, `chip` `{ "chip": { "label", "tone" } }`,
`chips` `{ "chips": [...] }`, `link` `{ "label", "url" }`,
`ticket` `{ "key", "title", "url", "status" }`, `empty` `{}`.

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
