# Extensions

An extension is a program the daemon runs beside itself that adds to the interface: views of rows
(dev servers, containers), keys and palette commands, dialogs, status-bar notices, and panes it
opens. The daemon owns it, so it works the same under `argus --host`.

```toml
[[extension]]
name = "docker"                       # lowercase letters, digits and dashes
command = ["python3", "docker-extension.py"]
dir = "~/argus-extensions"            # where it runs; the config directory by default
env = { DOCKER_HOST = "unix:///var/run/docker.sock" }
```

The blocks are read when the daemon starts, so restart it after changing one. An extension that
exits is started again after a backoff of one second doubling to one minute. Its output goes to
`extension-<name>.log` in the config directory.

The guide to writing one, from an empty file to every endpoint, field, event and limit, is printed by
the `argus` binary itself, so it matches the build you run:

```sh
argus extension guide         # the walkthrough, then the reference
argus extension list          # the extensions running, and what each publishes
argus extension show docker   # one of them: its guide, its commands and its views
```

`list` and `show` ask the daemon, and take `--host HOST` before `extension` for another machine's.
`scripts/docker-extension.py` is a complete example.

## In the desktop window

Each view an extension publishes is listed under **Extensions** in the sidebar while its scope
matches. It opens docked on the left, right or bottom of the stage, or as a page of its own, where
its `place` says until you move it: by its header's dock menu, **Open as page** or **Dock** (`>` and
`<`), or by dragging its tab or header to an edge of the stage. The window remembers where each view
was put. A dock holding several panels shows a tab for each.

The terminal client does not draw extensions yet.
