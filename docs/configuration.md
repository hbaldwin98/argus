# Configuration

## Where it lives

`ARGUS_CONFIG_DIR` names the directory explicitly. Otherwise Argus uses the platform's:

- Linux: `$XDG_CONFIG_HOME/argus`, usually `~/.config/argus`.
- macOS: `~/Library/Application Support/argus`.
- Windows: `%APPDATA%\argus\config`.

| File | Purpose |
| --- | --- |
| `projects.toml` | Workspaces, projects, repositories, agent templates, harnesses and extensions. Yours to edit; Argus only reads it |
| `client.toml` | The terminal client's editor, theme, layout and notification settings |
| `desktop.json` | The desktop window's own settings and what it remembers (layouts, docks, zoom, dictation's choices) |
| `dictation/` | The speech models the GPUI window downloads for dictation, and the voice-activity gate they share |
| `runtime.db` | Everything Argus writes: panes to relaunch, projects and repositories added or removed from a client, runtime workspaces, review comments, and the feature board |
| `argusd.log` | The running daemon's log; `argusd.log.1` is the run before |

Anything you do in a client (add a project, add a repository by path, remove either, create a
workspace) is recorded in `runtime.db`, never written back into `projects.toml`. Removing a project
the file declares hides it; adding the same directory back is the undo.

## `projects.toml`

```toml
[[workspace]]
name = "work"

[[project]]
name = "src"
root = "~/src"

[[project]]
name = "argus"
repos = ["~/src/argus"]
workspace = "work"
worktree_root = "~/worktrees"
setup = ["pnpm install"]
exclusive = true
exclude = ["vendor"]
include = ["target/scratch"]

[[agent]]
name = "claude"
cmd = ["claude"]
env = { CLAUDE_PROJECT_DIR = "." }
restart = "on-failure"
```

The daemon watches the file and applies a save in place: ids stay valid and panes keep running.
Something the file no longer names is removed once nothing runs in it. A half-written file fails to
parse and is ignored. Running agents keep the command and harness they started with, and
`[[extension]]` blocks are read only at daemon start.

### Workspaces and projects

A workspace scopes which projects the clients show; switching it changes every attached client, and
panes elsewhere keep running. Projects without a `workspace` are in `default`.

A project finds its repositories under `root`, names them in `repos`, or both:

- `root` is scanned at startup and every ten seconds for Git repositories at or beneath it, so a
  repository cloned there turns up on its own, and one deleted leaves once nothing runs in it. The
  scan stops at each repository (a submodule stays part of its parent), skips `.git`, `.argus`,
  `node_modules` and `target`, follows no directory symlinks, and goes at most eight levels deep.
  Linked worktrees are checkouts of their repository, not repositories of their own.
- `exclude` adds directories to skip and `include` overrides both `exclude` and the built-in list.
  Each entry is a bare name, matched anywhere, or a root-relative path.
- `repos` is taken at its word: a path need not be a Git repository, and no scan removes it.

`~/` and `~\` are expanded.

- `worktree_root` puts Argus-made worktrees under one directory per repository there, instead of
  `<primary-checkout>/.argus/worktrees/<branch>`.
- `setup` runs in each worktree Argus makes, in order, without a shell. A failure is reported and the
  worktree is kept.
- `exclusive` refuses a second agent in a checkout. Without it, a shared checkout is only marked.

### Agent templates

With no `[[agent]]` entries, Argus supplies `claude`, `codex`, `opencode` (`opencode --standalone --auto`, with its session tabs off), `pi`,
`agy` and `agent` (Cursor's CLI). Any `[[agent]]` entry replaces that list, so list every template
you want.

- `cmd`: an argument array, the executable first. The CLI must be installed, authenticated and on the
  daemon's `PATH`.
- `env`: added to the pane's environment.
- `harness`: which harness integrates it; by default the one named like the template, else the
  generic environment-only one. See [agents](agents.md).
- `restart`: `never` (the default), `on-failure` or `always`. Three restarts in a minute stop the
  cycle.

Harness blocks are described in [agents](agents.md#custom-harnesses), and extension blocks in
[extensions](extensions.md).

## `client.toml`

```toml
editor = "overlay"
editor_cmd = ""
theme = "mocha"
notifications = "off"
```

- `editor`: `overlay` (a floating terminal), `column`, or `external`. Known GUI editors always open
  externally.
- `editor_cmd`: an editor command, or empty to try `$VISUAL`, `$EDITOR`, then an installed terminal
  editor. It is split on whitespace, so quoted arguments are not supported yet.
- `theme`: `mocha`, `macchiato`, `frappe` or `latte`.
- `notifications`: `off` or `bell`, which rings when a pane in the background starts waiting, needs
  review, or fails.

The settings panel (`S`) writes this file as you change it.

## Environment variables

| Variable | Effect |
| --- | --- |
| `ARGUS_CONFIG_DIR` | Overrides the configuration directory for client and daemon |
| `ARGUS_INSTANCE` | Names an instance: its own socket or pipe and its own `instances/<name>` config directory, so a second Argus runs beside the first |
| `ARGUS_NO_RESTORE` | Starts the daemon without relaunching recorded panes |
| `ARGUS_THEME` | Overrides the theme for one client process |
| `ARGUS_DESKTOP_HOST` | The host the desktop window connects to on a first run |
| `ARGUS_BROWSER` | The browser executable agents' browsers run, instead of Chrome, Chromium or Edge |
| `ARGUS_DICTATION_MODELS` | The directory the GPUI window keeps its dictation models in, instead of `dictation/` under the configuration directory |
| `ARGUS_ARTIFACT_SCOPE` | `workspace` files an agent's feature, task and decision writes on the workspace board instead of the repository's |
| `RUST_LOG` | The daemon's log level, `info` by default |

## Sessions

Argus records non-exited shell and agent panes, in worktrees as well as primary checkouts. After a
daemon restart it starts each again in its checkout; it does not reattach old processes. An agent
whose harness reported a session id resumes that exact conversation (see
[agents](agents.md#resume)). Editors and browsers are not restored. Set `ARGUS_NO_RESTORE=1` to start
clean.

## Windows

Panes use the ConPTY built into Windows. To use a newer one (the console host Windows Terminal ships,
which redraws less, most noticeably on Windows 10), put `conpty.dll` and `OpenConsole.exe` from
Microsoft's `Microsoft.Windows.Console.ConPTY` package beside `argusd.exe` and restart the daemon.
The daemon loads libraries only from its own directory and System32, so a `conpty.dll` anywhere else
is never picked up.
