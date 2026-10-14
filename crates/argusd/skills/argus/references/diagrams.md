<!-- argus:managed-skill -->

# Sequence diagrams

Sequence diagrams belong to the selected feature (see [features.md](features.md)).
They record how parts of the system talk during one flow — client and daemon,
hook and store, user and TUI — without replacing decisions or tasks.

Argus stores **Mermaid** `sequenceDiagram` source on the feature. The TUI's
flows lens lists each diagram, reads its messages as numbered steps, and draws
it beside them.

## When to add one

Add a diagram when a later reader needs to see **order and direction**: who
calls whom, what gets pushed vs polled, and where a boundary sits. Prefer a
**decision** for why a shape was chosen, and a **task** for work left to do.
Do not diagram every function call or paste command output.

Good fits:

- A new protocol message or hook path and who sends it first
- A user gesture that opens an overlay and what the daemon returns
- A failure or retry path across two processes

Skip when a short decision `--because` or a task brief is enough.

## Writing the source

Use standard Mermaid sequence syntax. Start with `sequenceDiagram`, name
participants on their own lines, then messages:

```mermaid
sequenceDiagram
    participant User
    participant Client as argus TUI
    participant Daemon as argusd

    User->>Client: open Feature view
    Client->>Daemon: list diagrams for feature
    Daemon-->>Client: DiagramList
```

Keep participants few and messages readable. Alt/opt blocks are fine when a
branch matters; avoid styling directives the TUI renderer may ignore.

## Binding a step

A step a decision settles, or a task carries out, names it in a Mermaid comment
on the message's line or the line right after it. Use the ids `"$ARGUS_HOOK"
feature` and `task` print:

```mermaid
sequenceDiagram
    hook->>argusd: brief request %% decision #4
    argusd-->>hook: refused
    %% task #9
```

The TUI shows the decision beside the step and, under it, the tasks that carry
it out. Bind only where the decision or task is about that message; a comment
that names neither is an ordinary comment.

## Helper commands

```sh
"$ARGUS_HOOK" diagram
"$ARGUS_HOOK" diagram add "open overlay from Feature view" --stdin
"$ARGUS_HOOK" diagram drop <id>
```

For `add`, put the full Mermaid source on standard input (heredoc or pipe).
Read the list first so you know existing ids before dropping one.

Do **not** append diagram source to the feature brief with `feature note`;
briefs stay prose.
