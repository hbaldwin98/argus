# Argus

[![CI](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml/badge.svg)](https://github.com/hbaldwin98/argus/actions/workflows/ci.yml)

Argus is a workspace for running, reviewing and remembering the work of command-line coding agents.
It runs the agents you already use (Claude Code, Codex, OpenCode, pi, Antigravity, Cursor Agent, or
your own) across your Git checkouts, shows what each one is doing, lets you review what they
changed, and keeps the decisions they made. One daemon owns everything, so closing a window stops
nothing, and a terminal client and a desktop window show the same state.

## What it does

- **Keeps work running.** Shells and agents live in a daemon that outlives every client and restores
  them after a restart, reopening each agent's conversation where its CLI allows.
- **Organizes by Git.** Projects, repositories, checkouts and worktrees are how you navigate. Each
  checkout shows its Git state: `↑` ahead, `↓` behind, `+` staged, `!` unstaged. You create
  worktrees and switch branches in place.
- **Knows what agents are doing.** Each harness reports through hooks, so a row says when an agent is
  working, waiting on you, failed, or ready for review, and is named from your latest prompt.
- **Reviews changes.** Staged and unstaged work are separate diffs, unified or side by side, with
  syntax highlighting. Comments go straight to the agent in that checkout.
- **Remembers why.** Agents record features, tasks, and decisions with what they were chosen over.
  Later agents read that context, and a reversed decision is superseded, not erased. What a feature
  is doing is read off the agents and tasks working on it, with no board to maintain.
- **Gives agents tools.** Each agent can open its own headless browser, start other agents in panes
  you can watch, and show you views of what it found.
- **Extends.** Outside programs add views, keys, commands and dialogs (dev servers, containers, a
  deploy step).
- **Works remotely.** `argus --host <machine>` attaches over ssh to the daemon beside the code.

Argus does not replace your agents, Git client or issue tracker. It runs the first, defers to the
second, and links to the third.

## Install

Download the archive for your platform from
[GitHub Releases](https://github.com/hbaldwin98/argus/releases) (Linux and macOS on x86_64 and
arm64, Windows on x86_64), extract it, and add the directory to `PATH`. Keep the executables
together: `argus` starts `argusd` from beside itself.

```sh
argus update          # replace the installation with the newest release
argus update --check  # only ask whether there is one
```

The binaries are not code-signed: macOS may ask for approval under Privacy & Security, and Windows may
show a SmartScreen warning. To build from source, see [CONTRIBUTING.md](CONTRIBUTING.md).

Install and sign in to each agent CLI you want to use separately; Argus finds them on the daemon's
`PATH`.

## Quick start

```sh
argus init ~/src      # add a directory of repositories as a project
argus                 # open the terminal client
```

You can also add a project from inside the client: `n` on the project heading browses to a folder.
Every Git repository at or beneath it becomes one of the project's repositories. `i` on a repository
row makes a new one with `git init`.

On the rail, `a` starts an agent and `s` a shell in the selected checkout, `l` or Enter types into a
pane, and `Ctrl-Space` is the leader key for leaving it. `R` reviews a checkout, `2` opens its
features, and `?` lists every key for where you are. `q` detaches; everything keeps running.

## Commands

| Command | Does |
| --- | --- |
| `argus` | Open the terminal client, starting the daemon if needed |
| `argus desktop` | Open the desktop window on the same daemon |
| `argus --host <host>` | Attach to the daemon on another machine over ssh (keys or ssh-agent; argus installed there) |
| `argus init [dir]` | Add a directory as a project without opening a client |
| `argus focus [--watch]` | Print, as JSON, what the client you were last at is showing, for scripts and status lines |
| `argus server restart` / `stop` | Replace or stop the daemon; panes are restored on the next start |
| `argus extension guide` | How to write an extension |
| `argus update` | Update to the newest release |

## Documentation

- [Controls](docs/controls.md): every key, in the terminal client and the desktop window.
- [Configuration](docs/configuration.md): `projects.toml`, `client.toml`, environment variables.
- [Agents](docs/agents.md): harnesses, `argus-hook`, resuming sessions, custom harnesses.
- [Extensions](docs/extensions.md): adding views and commands from your own programs.
- [Architecture](ARCHITECTURE.md): the rules the code is built to, and why.
- [Target](TARGET.md) and [Roadmap](ROADMAP.md): what Argus is for and what is left to build.
- [Contributing](CONTRIBUTING.md): building, testing and releasing.

## License

Argus is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT License](LICENSE-MIT), at your option.
