# Argus: Architecture

A map, not a manual. The code says how each part works; every module opens with a `//!` header
naming the question it answers.

## Processes

```text
 argus (terminal) ──┐                       ┌─ PTYs: shells, agents, editors
 argus --host ─ssh──┤                       ├─ Git: status, worktrees, review
 desktop window ────┼── protocol ──▶ argusd ┼─ runtime.db (SQLite)
  (via argus bridge)                        ├─ pane API (loopback HTTP) ◀─ argus-hook, harness plugins
                                            └─ extensions, agents' browsers
```

- **`argusd`** owns everything that outlives a window: the terminals, Git observation, agent
  integration, extensions and the store. Clients start it when they cannot reach it, and closing a
  client stops nothing.
- **Clients** (`argus`, the desktop window) draw what the daemon sends. The desktop window and remote
  hosts reach the daemon through `argus bridge`, so starting a daemon lives in one place.
- **Agents** report to the daemon over a loopback HTTP API, through `argus-hook` or a plugin Argus
  installs into their harness.

## Crates

| Crate | Holds |
| --- | --- |
| `argus-protocol` | Everything two binaries must agree on: messages, the tree, the pane API, environment and flags |
| `argusd` | The daemon and `argus-hook` |
| `argus-client` | The terminal client and the `argus` command line |
| `argus-desktop-core` | What the desktop window holds and says, with no window in it |
| `argus-gpui` | The desktop window (`argus-desktop-next`) |
| `argus-desktop` | The Tauri window it replaces |

## Rules that hold everywhere

- **The daemon owns state; clients never predict.** A screen changes when the daemon says it did, so
  a refusal leaves the truth showing.
- **Contracts live in `argus-protocol`.** Binaries share no other types, and a contract written twice
  drifts.
- **Never send an older peer what it cannot read.** New messages are negotiated in the `Hello`
  greeting; a client that falls behind is sent current state, never a backlog.
- **The config is the user's; the store is Argus's.** `projects.toml` is only read. Everything Argus
  writes goes to `runtime.db`, one transaction per write.
- **Status is reported, never guessed.** An agent's state comes only from its harness's events.
- **Git is read with `git2` and changed with `git`.** Argus never pushes, merges, or deletes work
  silently.
- **Restore is relaunch.** After a restart, panes start fresh and agents resume their conversation
  by id; processes are never reattached.

[`TARGET.md`](TARGET.md) says what Argus is for and [`ROADMAP.md`](ROADMAP.md) what is left.
