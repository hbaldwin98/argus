# Building an Argus extension

This guide starts from nothing and ends with a working extension. Follow the steps in order; the
reference after them covers every field, event, limit and refusal.

## What an extension is

Argus runs coding agents and shells in panes. One daemon, `argusd`, holds them, and two clients
draw them: `argus`, a terminal interface, and `argus desktop`, a window.

An extension is a program of yours that the daemon starts and keeps running beside itself. Over
HTTP on the loopback interface it publishes JSON to the daemon:

- views: tables listed in the desktop window's sidebar under Extensions;
- commands for the palette and the keyboard;
- dialogs that ask the person something;
- notices for the status bar;
- panes that run a command.

It hears what the person does (an action on a row, a command run, a dialog answered) on an event
stream it holds open: server-sent events, one JSON object each.

Only the desktop window draws extensions; the terminal client shows none of it. An extension sends
data, never HTML, script or images: Argus draws everything from the JSON and shows every string as
text.

## Step 1: write the program

Any language that can make HTTP requests works. This guide uses Python with only its standard
library. Put the file anywhere; the `dir` you give in step 2 is the directory it runs in.

Save this as `hello-extension.py`. It publishes one view and prints every event it hears:

```python
import json, os, urllib.request

BASE = os.environ["ARGUS_HOOK_URL"]
TOKEN = os.environ["ARGUS_HOOK_TOKEN"]

VIEW = {
    "name": "services",
    "title": "Services",
    "columns": [{"title": "Name"}, {"title": "Port", "numeric": True}],
    "rows": [
        {"key": "web", "cells": [{"kind": "text", "text": "web"}, {"kind": "number", "number": 8080}],
         "status": {"label": "up", "tone": "good"}},
        {"key": "db", "cells": [{"kind": "text", "text": "db"}, {"kind": "number", "number": 5432}],
         "status": {"label": "down", "tone": "bad"}},
    ],
}


def send(method, endpoint, body):
    """One request to this extension's endpoint; the answer's text."""
    data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
    request = urllib.request.Request(f"{BASE}/{endpoint}", data=data, method=method)
    request.add_header("Authorization", f"Bearer {TOKEN}")
    with urllib.request.urlopen(request, timeout=10) as answer:
        return answer.read().decode()


def handle(event):
    print(json.dumps(event), flush=True)


print("view:", send("PUT", "view", VIEW), flush=True)
events = urllib.request.Request(f"{BASE}/events", headers={"Authorization": f"Bearer {TOKEN}"})
with urllib.request.urlopen(events) as stream:
    for raw in stream:
        line = raw.decode().strip()
        if line.startswith("data: "):
            handle(json.loads(line[len("data: "):]))
```

- The daemon hands the program `ARGUS_HOOK_URL`, its own base URL, and `ARGUS_HOOK_TOKEN`, sent
  on every request as `Authorization: Bearer TOKEN`. Both change every time the daemon starts it,
  so the program cannot be run by hand: started outside Argus it fails at once on the missing
  variable.
- `PUT view` publishes the table. The daemon answers `{"refused":[]}`: no key was refused.
- `GET events` stays open. When the daemon goes away it ends, the program exits, and the next
  daemon starts it again.

## Step 2: register it

The daemon reads `[[extension]]` blocks from `projects.toml` in Argus's config directory:

| Platform | Config directory |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/argus`, usually `~/.config/argus` |
| macOS | `~/Library/Application Support/argus` |
| Windows | `%APPDATA%\argus\config` |

With `ARGUS_INSTANCE=NAME` set, the directory is `instances/NAME` inside that one; with
`ARGUS_CONFIG_DIR` set, it is that directory instead. The daemon writes a `projects.toml` there when
it first starts. Add a block at its end:

```toml
[[extension]]
name = "hello"
command = ["python3", "hello-extension.py"]
dir = "~/argus-extensions"
```

On Windows the interpreter is usually `python`: `command = ["python", "hello-extension.py"]`.

- `name` is lowercase ASCII letters, digits and dashes, starting with a letter or digit, at most
  {{MAX_EXTENSION_NAME}} bytes. It is in the extension's URL. A block with a bad name, a name
  already used, or an empty `command` is skipped, and `argusd.log` in the config directory says why.
- `command` is the program and its arguments, run without a shell: no pipes, no globbing, no `~`.
  A relative script path is relative to `dir`.
- `dir` is the directory it runs in, `~` expanded. Left out, it is the config directory, so a
  script kept beside `projects.toml` can be named by its file name alone.
- `env`, optional, adds variables: `env = { DOCKER_HOST = "unix:///var/run/docker.sock" }`.

## Step 3: restart the daemon

The daemon reads the blocks only when it starts:

```sh
argus server restart
```

This restarts the daemon, and with it every pane: each shell and agent is ended and started again.
If you are an agent running inside Argus (`ARGUS_PANE` is set), that includes the shell you run
commands in, which dies before the restart finishes. So:

- In a checkout of Argus itself, `scripts/reload.sh` detaches the restart and survives it.
- Otherwise ask the person to restart Argus, or run `argus server restart` themselves.
- Better still while the extension is new, try it on a daemon of its own, which leaves the
  installed Argus and every pane in it alone. `ARGUS_INSTANCE` names that daemon; every `argus`
  command run with it set talks to that daemon and its own config directory:

```sh
ARGUS_INSTANCE=dev argusd &                       # starts it, writing instances/dev/projects.toml
# add the [[extension]] block to that projects.toml, then:
ARGUS_INSTANCE=dev argus server restart           # no panes of yours run there
ARGUS_INSTANCE=dev argus desktop                  # a window on that daemon
ARGUS_INSTANCE=dev argus server stop              # when done: ends it and its extensions
```

In PowerShell, run `$env:ARGUS_INSTANCE = "dev"` first and leave the prefix off.

## Step 4: check that it runs

```sh
argus extension list
argus extension show hello
```

`list` names each running extension with its counts of views and commands. `show` prints what one
published: its commands, its views with their columns and actions, and its own guide. In the desktop
window, **Services** is now in the sidebar under Extensions; select a row, and run something from
the palette, to see events.

The extension's output goes to `extension-hello.log` in the config directory, replaced on each
start. After step 1 it holds the answer to the put, then one line per event:

```text
view: {"refused":[]}
```

```json event
{"event": "action", "view": "services", "action": "restart", "row": "web"}
```

When it does not run:

| What you see | Why, and what to do |
| --- | --- |
| Not in `argus extension list`; its log says the interpreter was not found, or is empty | The program could not start: often `python3` where Windows has `python` (Windows' stand-in prints `Python was not found`). `argusd.log` says `extension hello exited`, or `could not start extension hello`, again and again. Fix `command`, restart. |
| Not in the list, and no log at all | The block was skipped (bad `name`, a name used twice, no `command`), or the daemon was not restarted. `argusd.log` says which. |
| In the list now and then, with a traceback in its log | It keeps exiting, and the daemon keeps restarting it, waiting longer each time up to a minute. Read the traceback. |
| `HTTP Error 409: Conflict` in the log | A put broke a rule. The answer's body names the field, such as `views[0].rows[1].key: each row needs a key of its own, not "web"`; print `error.read()` to see it. Nothing of that put was taken. |
| `HTTP Error 400: Bad Request` | The body is not the JSON the endpoint takes: `not a view`. |
| `HTTP Error 401: Unauthorized` | The token is not this run's: the program was run by hand with copied variables, or kept them from before a restart. Let the daemon start it. |
| `argus extension list`: `argusd is not running` | No daemon under this `ARGUS_INSTANCE`. Start one. |

## Step 5: act on a row

Give the view actions. A row action names the selected row; a view-wide one acts on the whole
view. `confirm` asks the person first; `key` is a single character heard while the view has the
keys. Add, after `VIEW`:

```python
VIEW["actions"] = [
    {"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart this service?",
     "help": "Restarts the selected service."},
    {"id": "logs", "label": "Logs", "key": "o", "help": "Opens a console following its log."},
    {"id": "refresh", "label": "Refresh", "view_wide": True, "help": "Reads the services again."},
]
```

and answer the action in `handle`, here with a notice: one line of plain text on the status bar.

```python
def handle(event):
    print(json.dumps(event), flush=True)
    if event["event"] == "action" and event["action"] == "restart":
        send("POST", "notice", "restarting " + event["row"])
```

The view does not change on its own: the window shows exactly what the last `PUT view` said. To show
a new state, change `VIEW` and put it again.

## Step 6: choose where it opens

A view opens docked on the right, where the agents' views open, unless `place` says `left` or
`bottom`, which dock it against that edge of the stage, or `stage`, which opens it as a page; the
person can move it to any of them. `icon` names its icon in the sidebar:

```python
VIEW["place"] = "stage"
VIEW["icon"] = "server"
```

## Step 7: add a command with a key

Commands go in the palette, and on a key when they ask for one. Put them before opening the event
stream:

```python
COMMANDS = [
    {"name": "add", "title": "Add a service", "key": {"key": "w", "alt": True},
     "help": "Asks for a name and adds a row for it."},
]
print("commands:", send("PUT", "commands", COMMANDS), flush=True)
```

The answer names any key refused, and the command stays without it:

```json refusals
{"refused": [{"name": "add", "reason": "Alt W is k8s's Apply"}]}
```

A key is refused when Argus binds it already, when it is a digit or the leader (Ctrl Space), or when
another extension registered it first. "Keys" in the reference lists them. Pick another and put the
commands again.

## Step 8: ask with a dialog

Answer the command with a dialog. Naming the command's `id` in `answers` opens it in the window
that ran the command, with the keys. The person's answer arrives as an event naming the dialog's id:

```python
DIALOG = {"title": "Add a service", "kind": {"prompt": {"message": "Its name"}}}
asked = {}
```

and, in `handle`, after the `if` of step 5:

```python
    elif event["event"] == "command" and event["command"] == "add":
        raised = json.loads(send("POST", "dialog", dict(DIALOG, answers=event["id"])))
        asked[raised["dialog"]] = "add"
    elif event["event"] == "answer" and asked.pop(event["dialog"], None) == "add":
        name = event["answer"]["text"]
        VIEW["rows"].append({"key": name, "cells": [{"kind": "text", "text": name}, {"kind": "empty"}]})
        send("PUT", "view", VIEW)
```

These events reach `handle` on the way:

```json event
{"event": "command", "command": "add", "id": 12,
 "context": {"workspace": "default", "project": null, "repository": null, "checkout": null, "pane": null}}
```

```json event
{"event": "answer", "dialog": 13, "answer": {"text": "cache"}}
```

A dialog the person cancels sends `{"event": "cancel", "dialog": 13}` instead. Besides a prompt
there are a yes-or-no confirm, a pick from a list and a small form; the reference has all four.

## Step 9: open a pane

An extension can open a shell, on a checkout or as a console in the home directory, with a command
typed into it as though by the person:

```python
PANE = {"run": "ping localhost"}
```

and, in `handle`:

```python
    elif event["event"] == "action" and event["action"] == "logs":
        send("POST", "pane", PANE)
```

Add `"checkout": ID` for a shell on a checkout; a command event's `context` carries the ID of the
one the person is at.

## Step 10: say what it is

`argus extension list` and `argus extension show` print what an extension says of itself, so an
agent can learn what it does without reading its source. Say it once at start-up:

```python
ABOUT = {
    "summary": "Example services, with restart, logs and an add command.",
    "guide": "Open **Services** under Extensions. `r` restarts the selected service; Alt W adds one.",
}
send("PUT", "about", ABOUT)
```

Each command and action's `help`, added in steps 5 and 7, is printed beside it there.

For a fuller example, `scripts/docker-extension.py` in Argus's repository
(https://github.com/hbaldwin98/argus) shows Docker containers: refreshing them as they change,
actions that ask first, a command answered with a pick dialog, a pane following a container's logs,
and an about.

# Reference

## How it runs

| Variable | What it holds |
| --- | --- |
| `ARGUS_HOOK_URL` | The extension's own base URL, `http://127.0.0.1:PORT/extension/NAME`. Every endpoint below is under it. |
| `ARGUS_HOOK_TOKEN` | A token minted for this run of this extension. Send it on every request as `Authorization: Bearer TOKEN`. |
| `ARGUS_EXTENSION` | Its name, for a program that would rather not parse the URL. |

- Its stdout and stderr go to `extension-NAME.log` in the config directory, replaced on each start.
- When it exits, for any reason, the daemon starts it again after a wait that doubles from a second
  up to a minute, and goes back to a second once a run has lasted a minute.
- The moment it exits, everything it published is taken off every window: its views, commands,
  keys, about, and open dialogs. A new run starts from nothing with a new token, so publish
  everything again at start-up.
- When the daemon stops it ends every extension. On Windows that happens even if the daemon is
  killed outright. Elsewhere, notice the event stream closing and exit.
- The blocks in `projects.toml` are read only when the daemon starts.

## The endpoints

Every request carries `Authorization: Bearer $ARGUS_HOOK_TOKEN`. Bodies are JSON unless said
otherwise, at most {{MAX_EXTENSION_BODY}} bytes.

| Request | Body | Answer |
| --- | --- | --- |
| `PUT view` | one view, or a list of them | `200` with the action keys refused |
| `PUT commands` | a list of commands | `200` with the command keys refused |
| `PUT about` | a summary and a guide | `200`, empty |
| `POST dialog` | a dialog | `200` with the dialog's id |
| `POST notice` | one line of plain text, not JSON | `200`, empty |
| `POST pane` | a checkout and a command | `200` with the pane's id |
| `GET events` | none | held open: server-sent events |

A refusal is one of these, its body plain text:

| Status | When | Body |
| --- | --- | --- |
| `401` | No token, a wrong one, or an extension the daemon is not running | empty |
| `405` | The wrong method for the endpoint | `view takes PUT` |
| `413` | A body over {{MAX_EXTENSION_BODY}} bytes | `request body is too large` |
| `400` | A body that does not parse as what the endpoint takes | `not a view`, `not a list of commands`, `not a summary and guide`, `not a dialog`, `not a pane` |
| `409` | It parses but breaks a rule; nothing of it was taken | the rule, by the path to what broke it, such as `views[0].rows[3][1]: a link is an http or https URL` |

A `409` names the field: `views[1].name: "web" is another view's`,
`commands[0].help: one line of at most {{MAX_HELP_BYTES}} bytes`, `views: at most {{MAX_VIEWS}}`.
Fix that field and put the whole thing again.

### `PUT view`

Replaces every view the extension publishes: send the whole list each time, or one view on its
own when it has one. The person sees a view in the desktop window's sidebar under Extensions while
its scope holds, and opens it docked in the side panel or as a page.

```json view
{
  "name": "containers",
  "title": "Containers",
  "scope": "global",
  "place": "stage",
  "icon": "box",
  "columns": [{"title": "Name"}, {"title": "CPU", "numeric": true}, {"title": "Started"}],
  "rows": [
    {
      "key": "web",
      "cells": [
        {"kind": "text", "text": "web"},
        {"kind": "number", "number": 3.5, "unit": "%"},
        {"kind": "date_time", "at": "2026-10-15T09:30:00Z"}
      ],
      "status": {"label": "running", "tone": "good"}
    }
  ],
  "actions": [
    {"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart this container?",
     "help": "Restarts the selected container with docker restart."},
    {"id": "refresh", "label": "Refresh", "view_wide": true, "help": "Reads docker ps again."}
  ]
}
```

- `name` names the view in what comes back. At most {{MAX_VIEW_ID_BYTES}} bytes, unique among the
  extension's views.
- `title` is what the sidebar and its header say, at most {{MAX_VIEW_TITLE_BYTES}} bytes.
- `scope` is where it is listed:
  - `"global"`, the default: always.
  - `{"repository": ID}`: while that repository is the one the person is in.
  - `{"checkout": ID}`: while that checkout is on screen.

  IDs are the daemon's; a command event's `context` carries them.
- `place` is where it first opens: `"right"` (or `"panel"`, the default, the same) docks it on the
  right, where the agents' views open; `"left"` and `"bottom"` dock it against that edge of the
  stage; `"stage"` opens it as a page. The person can move it, and the window remembers their
  choice. A daemon older than the docks refuses a view naming `left`, `right` or `bottom`.
- `icon` is one of {{EXT_ICONS}}. Any other name of lowercase letters, digits and dashes, at most
  {{MAX_ICON_NAME}} bytes, is drawn as a generic icon.
- `columns` are `{"title", "numeric", "optional"}`; a numeric column is aligned right, and an
  optional one is dropped where the table is too narrow for every column, before it scrolls
  sideways. A view needs at least one.
- `rows` are at most {{MAX_TABLE_ROWS}}. Each row has:
  - `key`, unique in the view, which an action on the row names.
  - `cells`, one value per column in order.
  - `status`, an optional chip that leads the row, with the extension's own word and a tone.
- `actions` are at most {{MAX_ACTIONS}}. Each has:
  - `id`, unique in the view, and a `label`.
  - `key`, one character heard while the view has the keys (see "Keys").
  - `view_wide: true` to act on the view rather than the selected row.
  - `confirm`, a question asked before the action is sent.
  - `help`, one line of at most {{MAX_HELP_BYTES}} bytes saying what it does, for
    `argus extension show`.
- `default` is the `id` of a row action that a double-click on a row, or Enter on the selected one,
  sends for that row, as its key would. What it means for the row's state is the extension's to
  decide: a stopped service's start, a running one's logs.

The answer lists the action keys refused, by `VIEW/ACTION`. The action stays, without its key:

```json refusals
{"refused": [{"name": "containers/stop", "reason": "s is another action's in containers"}]}
```

### Values

A cell is one of these, chosen by `kind`. Values do not nest, apart from the chips inside `chip`,
`chips` and `ticket`.

```json values
[
  {"kind": "text", "text": "web"},
  {"kind": "number", "number": 3.5, "unit": "%"},
  {"kind": "bool", "value": true},
  {"kind": "date", "date": "2026-10-15"},
  {"kind": "date_time", "at": "2026-10-15T09:30:00Z"},
  {"kind": "person", "name": "Ada"},
  {"kind": "people", "names": ["Ada", "Grace"]},
  {"kind": "chip", "chip": {"label": "healthy", "tone": "good"}},
  {"kind": "chips", "chips": [{"label": "web"}, {"label": "db", "tone": "info"}]},
  {"kind": "link", "label": "dashboard", "url": "https://example.com/d/1"},
  {"kind": "ticket", "key": "OPS-12", "title": "Disk full", "url": "https://example.com/OPS-12",
   "status": {"label": "open", "tone": "warn"}},
  {"kind": "progress", "value": 41, "max": 100, "tone": "active"},
  {"kind": "duration", "ms": 252000},
  {"kind": "code", "text": "ranking.fresh"},
  {"kind": "path", "path": "src/main.rs", "line": 12},
  {"kind": "commit", "sha": "a41c9e2", "title": "Keep units on import"},
  {"kind": "feature", "number": 12},
  {"kind": "pane", "pane": 3, "label": "CI triage"},
  {"kind": "empty"}
]
```

- `number` is a JSON number, with an optional `unit` drawn after it. A number, here or in a
  `progress`, is at most 1e15 in magnitude; a larger one is refused with a `409`.
- `date` is `YYYY-MM-DD`; `date_time` is RFC 3339.
- `progress` is a meter, `value` out of a `max` above 0; `duration` is milliseconds, drawn as `4m 12s`.
- `path`, `commit`, `feature` and `pane` point into Argus rather than out of it: a click opens the
  file in the person's editor, the commit in History, the feature, or the pane. A path is relative
  to the root of the checkout the person is at, and one that is absolute, has a drive letter or climbs
  out with `..` is refused with a `409`.
- A `link`'s `url`, and a `ticket`'s when it has one, must be `http` or `https`. Anything else is
  refused with a `409`.
- A `kind` that is not in the list above is refused with a `409` too.
- A chip is `{"label", "tone"}`. The tone is the only colour an extension picks, and it picks a
  meaning: {{TONES}}. `neutral` is the default.

### `PUT commands`

Replaces every command the extension adds. A command is listed in the desktop window's palette, on
its shortcuts sheet under the extension's name, and on its key when it has one. Running it sends a
`command` event.

```json commands
[
  {"name": "up", "title": "Start a compose service", "key": {"key": "s", "alt": true},
   "scope": "checkout", "help": "Asks which service of the checkout's compose file to start."},
  {"name": "deploy", "title": "Deploy", "key": {"key": "F5", "shift": true}},
  {"name": "tail", "title": "Follow the logs", "key": {"key": "W"}, "scope": {"view": "containers"}},
  {"name": "refresh", "title": "Refresh the containers", "scope": "anywhere"}
]
```

- `name` is unique among its commands; `title` is what the palette says. At most
  {{MAX_COMMANDS}} commands.
- `key` is optional:
  - `key` is spelled as the browser's `KeyboardEvent.key`: `d`, `D`, `F5`, `Enter`.
  - `ctrl`, `alt` and `shift` are each `false` unless given.
  - A character carries its own shift (`D` is Shift D), so `shift` matters only for a named key
    such as `F5`.
- `scope` is where its key is heard, and what its `context` is about:
  - `"anywhere"`, the default: wherever the window's own keys are, and through the leader
    (Ctrl Space) from inside a pane.
  - `{"view": NAME}`: while that view of the extension's has the keys.
  - `"checkout"`: on a checkout picked on the rail, or a row of the Checkouts stage.
  - `"pane"`: on a pane picked on the rail, and through the leader from a pane.
- `help` is one line of at most {{MAX_HELP_BYTES}} bytes, for `argus extension show`.

The answer lists the keys refused; the command stays, in the palette without its key:

```json refusals
{"refused": [{"name": "deploy", "reason": "Shift F5 is k8s's Deploy everything"}]}
```

### Keys

An extension's key never shadows a key Argus binds. A refused key comes back in the answer, as
above, and on every window's status bar; the command or action keeps working without it.

A command's key is refused when it is:

- Ctrl Space, the leader.
- A digit without Ctrl or Alt, which opens a stage.
- One of Argus's own: {{KEPT_KEYS}}.
- Another extension's: the one that registered first keeps it.
- The key of one of its own view's actions, for a command scoped to that view.

An action's key is one character. It is refused when it is:

- A digit.
- One the view keeps for moving, filtering, docking and opening: {{STAGE_KEYS}}.
- Another action's in the same view, or a command's scoped to that view.

What is free depends on what other extensions registered first, so read the answer.

### `PUT about`

Says what the extension is, for `argus extension list` and `argus extension show`. No window draws
it.

```json about
{"summary": "Docker containers: restart, stop and follow their logs.",
 "guide": "Open **Containers** in the sidebar. `r` restarts the selected container, `s` stops it."}
```

- `summary` is one line of at most {{MAX_SUMMARY_BYTES}} bytes.
- `guide` is optional Markdown of at most {{MAX_ABOUT_GUIDE_BYTES}} bytes.

### `POST dialog`

Asks the person something in the desktop window and answers with the dialog's id. Their answer, or
their cancelling it, arrives later as an event naming that id.

```json dialog
{"title": "Restart web", "kind": {"confirm": {"message": "Restart the web container?", "yes": "Restart"}}}
```

```json dialog
{"title": "Rename", "kind": {"prompt": {"message": "New name", "initial": "web"}}}
```

```json dialog
{"title": "Start a compose service", "answers": 12,
 "kind": {"pick": {"message": "In ~/src/shop", "items": [{"key": "web", "label": "web"}, {"key": "db", "label": "db"}]}}}
```

```json dialog
{"title": "Deploy", "kind": {"form": {"fields": [{"name": "env", "label": "Environment", "initial": "staging"},
                                                  {"name": "tag", "label": "Image tag"}]}}}
```

```json raised
{"dialog": 7}
```

- `confirm` is yes or no; `yes` is what its button says.
- `prompt` takes one line of text, starting from `initial`.
- `pick` is one of at most {{MAX_ITEMS}} `items`, each a `key` that comes back and a `label`.
- `form` is named lines of text, at most {{MAX_ITEMS}} `fields`, answered together.

Only one window asks. A dialog with `answers`, the `id` of a command event, goes to the window that
ran the command, which gives it the keys. Answer a command's event promptly: the daemon remembers
only a command's recent runs. Any other dialog goes to the window the person was at last, and
waits there to be clicked rather than taking the keys from a pane being typed in. With no desktop
window open the dialog is refused with a `409`: `no window is open to ask`.

### `POST notice`

The body is plain text, not JSON. Its first line goes on the status bar, cut at
{{MAX_NOTICE_CHARS}} characters.

```text
web restarted
```

### `POST pane`

Opens a shell on a checkout, or a console in the home directory when `checkout` is left out, and
types `run` into it as though the person had. Answered with the pane's id.

```json pane
{"checkout": 9, "run": "docker logs -f web"}
```

```json opened
{"pane": 31}
```

### `GET events`

Held open as server-sent events: one `data: JSON` line per event, then a blank line. A comment line,
`: still here`, arrives every 15 seconds while nothing happens. Open it again whenever it closes; a
newer connection replaces an older one. An action or command taken while no stream is open is
refused to the person, who is told the extension is not listening.

An action in a view; `row` is `null` for a view-wide action:

```json event
{"event": "action", "view": "containers", "action": "refresh", "row": null}
```

A command run. `id` is what a dialog answering it names in `answers`. `context` is where the person
was when they ran it: the checkout picked on the rail, else the Checkouts stage's row, else the pane
on screen, read against the daemon's tree. `argus focus` prints the same shape. Any part may be
`null`. `since` is when the pane's status last changed, in Unix seconds:

```json event
{"event": "command", "command": "up", "id": 12,
 "context": {"workspace": "default",
             "project": {"id": 1, "name": "shop"},
             "repository": {"id": 4, "name": "shop"},
             "checkout": {"id": 9, "path": "/home/me/src/shop", "branch": "main", "primary": true, "dirty": false},
             "pane": {"id": 31, "kind": "agent", "title": "fix the cart", "agent": "claude", "status": "working",
                      "note": null, "since": 1760520600, "model": "opus"}}}
```

A pane's `kind` is `shell`, `agent` or `editor`; its `status` is `idle`, `working`, `waiting`,
`needs-review`, `done`, `failed` or `exited`.

A dialog answered; `answer` fits its kind:

```json event
{"event": "answer", "dialog": 7, "answer": "yes"}
```

```json event
{"event": "answer", "dialog": 8, "answer": {"text": "web-2"}}
```

```json event
{"event": "answer", "dialog": 9, "answer": {"picked": "db"}}
```

```json event
{"event": "answer", "dialog": 10, "answer": {"form": {"env": "staging", "tag": "v1.4"}}}
```

A dialog closed without an answer: the person cancelled it, or the window asking went away:

```json event
{"event": "cancel", "dialog": 7}
```

## Limits

| What | At most |
| --- | --- |
| An extension's name | {{MAX_EXTENSION_NAME}} bytes |
| A request's body | {{MAX_EXTENSION_BODY}} bytes |
| Views | {{MAX_VIEWS}} |
| A view's name | {{MAX_VIEW_ID_BYTES}} bytes |
| A view's title | {{MAX_VIEW_TITLE_BYTES}} bytes |
| An icon's name | {{MAX_ICON_NAME}} bytes |
| Rows in a view | {{MAX_TABLE_ROWS}} |
| Actions in a view | {{MAX_ACTIONS}} |
| Commands | {{MAX_COMMANDS}} |
| A command's or action's help | {{MAX_HELP_BYTES}} bytes, one line |
| The about summary | {{MAX_SUMMARY_BYTES}} bytes, one line |
| The about guide | {{MAX_ABOUT_GUIDE_BYTES}} bytes |
| A pick's items, or a form's fields | {{MAX_ITEMS}} |
| A notice | {{MAX_NOTICE_CHARS}} characters, one line; the rest is cut, not refused |

Past any limit but the notice's, the whole put is refused with a `409` naming the path, and what
was published before stays.
