# Agents

Argus runs agent CLIs you already have and integrates each through a **harness**: a description of
how that CLI can report its state. Built in are Claude Code (`claude`), Codex (`codex`), OpenCode
(`opencode`), pi (`pi`), Google Antigravity (`agy`), Cursor Agent (`agent`), and `generic`.

## Reporting

Every agent pane receives `ARGUS_HOOK`, `ARGUS_HOOK_URL`, `ARGUS_HOOK_TOKEN`, `ARGUS_PANE` and
`ARGUS_INSTRUCTIONS`, so any CLI or wrapper that can run a command can report without a hook file:

```sh
"$ARGUS_HOOK" status working
"$ARGUS_HOOK" status waiting "needs database access"
"$ARGUS_HOOK" status needs-review "ready for review"
"$ARGUS_HOOK" status done "reviewed and complete"
"$ARGUS_HOOK" status failed "tests failed"
"$ARGUS_HOOK" title "repairing session restore"
"$ARGUS_HOOK" checkout                       # after changing into another checkout
"$ARGUS_HOOK" session "harness-session-id"
"$ARGUS_HOOK" comments                       # the checkout's newest 100 review comments
"$ARGUS_HOOK" telemetry --model M --context N --window N --cost USD --tool NAME
```

The states are `idle`, `working`, `waiting` (needs a person), `needs-review`, `done` (reviewed and
complete) and `failed`. The note after a status is shown under the pane's row and cleared by the next
report without one. A later `working` resumes any state. Every form exits successfully, so a stopped
daemon never breaks an agent's turn.

Argus names a pane from the prompt you last sent; `title` refines it. The rest of `argus-hook` is
for agents, and the argus skill below teaches it: features, tasks, decisions and diagrams (`feature`, `task`, `decisions`, `decide`, `diagram`),
views (`view`), other agents (`agents`, `agent` to start one in a pane of its own), browsers
(`browser`), and `instructions` and `say` for harnesses that inject command output into context.

## What each built-in harness installs

| Harness | Where | Notes |
| --- | --- | --- |
| Claude Code | hooks in `.claude/settings.local.json`; a mod under Argus's config directory (2.1.287+) | The mod delivers messages and reports context window and cost |
| Codex | `.codex/hooks.json` | Codex asks you to trust project hooks once; the command stays identical across starts |
| OpenCode | `.opencode/plugin/argus-status.js` | Runs with `--standalone`: a shared opencode server would report every pane to whichever started it. Its session tab strip is off (`OPENCODE_CLI_CONFIG_CONTENT`), so a pane stays on the session its row shows |
| pi | `.pi/extensions/argus-status.ts` | pi asks you to trust the project once |
| AGY | `.agents/hooks.json` under the `argus` key | |
| Cursor Agent | `.cursor/hooks.json` and `.cursor/rules/argus.mdc` | |

Each also gets the **argus skill**, embedded in the daemon: `.claude/skills/argus`,
`.pi/skills/argus`, or `.agents/skills/argus`. It tells the agent how to report, move checkouts and
use the board. Where the harness has a context hook, the agent starts with `argus-hook context`
already printed: review comments, the checkout's feature, and open decisions waiting on a person.

Managed files are swept from every configured checkout at daemon start and removed when the last
agent leaves a checkout. Their paths go into the repository's `.git/info/exclude`, never a tracked
file. A file you own at one of those paths is never overwritten; removing the `argus:managed-skill`
marker from a generated file makes it yours.

What Argus says to an agent, such as a review comment, goes in through the harness where a plugin
runs inside the session (OpenCode, pi, the Claude Code mod), so it can never land in a dialog. Other
harnesses are typed into.

Telemetry (the tool running now, context used, cost, model) comes from whatever each harness
exposes. It is live state and not stored.

## Resume

When a harness reports a session id, a restored pane reopens that exact conversation:

| Harness | Resume by id | Legacy fallback | First message |
| --- | --- | --- | --- |
| Claude Code | `--resume {session_id}` | `--continue` | prompt as last argument |
| Codex | `resume {session_id}` | `resume --last` | prompt as last argument |
| OpenCode | `--session {session_id}` | `--continue` | `--prompt {prompt}` |
| pi | `--session {session_id}` | `--continue` | prompt as last argument |
| AGY | `--conversation {session_id}` | `--continue` | none |
| Cursor Agent | `--resume {session_id}` | `--continue` | prompt as last argument |

An agent resumed this way that exits non-zero within five seconds is replaced by a fresh one. The
legacy fallback means "the last conversation here", so only one pane per checkout and harness may
claim it.

## Custom harnesses

```toml
[[harness]]
name = "herdr"
settings = ".herdr/hooks.json"   # checkout-relative; omit for an environment-only harness
hooks_key = "hooks"
shape = "flat"                   # "matcher" for Claude Code-style nesting
context_event = "session_start"
skill_dir = ".agents/skills/argus"
resume = ["--continue"]
resume_id = ["--resume", "{session_id}"]
first_message = ["{prompt}"]
transcript = { format = "neutral", path = "~/.herdr/sessions/{session_id}.jsonl" }

[harness.events]
turn_start = "working"
turn_end = "idle"
ask = { reports = "waiting", note = true }
prompt = { reports = "working", title = true }
start = { reports = "idle", session_id = "session_id" }

[[agent]]
name = "herdr"
cmd = ["herdr"]
harness = "herdr"
```

- Event values are the states above. `note = true` takes the hook's stdin as the note; `title = true`
  takes it as the user's prompt; `session_id` names the top-level stdin JSON key holding the
  conversation's identity.
- `resume_id` and `first_message` are argv templates; placeholders are filled without a shell.
- `skill_dir` opts into the argus skill; without it the agent gets compact instructions in
  `ARGUS_INSTRUCTIONS`.
- A block reusing a built-in's name replaces it outright, plugin and resume arguments included.

### Transcripts

The desktop window's recent-sessions dialog reads transcripts from where each harness keeps them;
Argus never stores one. `transcript` names a format (`claude`, `codex`, `cursor`, `pi` or `neutral`)
and a path template using `~`, `{session_id}`, `{checkout}`, `{project}` (a directory named after the
checkout's path) and `*`. `transcript = "claude"` alone reads Claude Code's format where Claude Code
keeps it.

The neutral format is one JSON object per line:

```jsonl
{"kind":"user","text":"Tidy the release notes","at":"2026-10-14T09:00:00Z"}
{"kind":"tool","tool":"shell","summary":"npm run lint","at":"…","failed":true}
{"kind":"agent","text":"The notes are tidied.","at":"…"}
{"kind":"title","text":"Release notes"}
```

A CLI that keeps no transcript can have its hooks write one:
`argus-hook transcript FILE user|agent|title TEXT` or
`argus-hook transcript FILE tool NAME SUMMARY [--failed]` (`--stdin` reads the text from stdin).
A harness with no transcript still lists its sessions, from what Argus saw of their panes.
