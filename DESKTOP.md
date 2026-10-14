# Argus Desktop: Plan

The owner wants a native desktop app, built with Tauri 2, as an **alternative view beside the TUI**
over the same daemon. This slice is the foundation plus an MVP: it connects to argusd, shows the
rail (projects → repositories → checkouts → panes) and one live pane drawn on a canvas, takes
keyboard input, paste and resize, and can open a shell on a checkout. Parity with the TUI's stages
(Workspace / Feature / Checkouts), review, the feature board, history, pickers, mouse forwarding,
selection and scrollback come later and are listed on the roadmap, not built now.

This is a plan, not a description of the code: nothing here is built yet. [`DESIGN.md`](DESIGN.md)
is current behavior, [`TARGET.md`](TARGET.md) the intended product, and [`ROADMAP.md`](ROADMAP.md)
orders the gap. The findings this plan rests on are under "Findings" at the end.

## Principles that decide the design

1. **One way to start a daemon.** The desktop binary never connects to the socket itself and never
   spawns argusd. It runs `argus bridge` — the `argus` executable beside its own, falling back to
   `argus` on PATH — and speaks the protocol over the child's stdin/stdout, exactly as
   `argus --host` does over ssh (`crates/argus-client/src/remote.rs`). Connect-or-start stays only
   in `argus`'s `launch.rs`. Later, `ssh -T host argus bridge` gives the desktop remote hosts for free.
2. **The client never predicts.** The rail changes when the daemon's tree says so. A spawned shell
   is selected only once `Created` names it AND the tree holds it (see the TUI's `app/awaited.rs`).
   Choosing which pane to *show* is local view state, like the TUI's selection.
3. **Rust holds the model, the page renders it.** All state — the tree, the selection, the screen
   grid, connection status — lives in a plain Rust struct (`session`) that takes `ServerMsg`s and
   page gestures and returns the `ClientMsg`s to send and the page events to emit. It has no Tauri
   types in it, so it is unit-tested like the TUI's `app`. The page (plain JS, no build step, like
   the deleted phone client) draws what it is sent and reports gestures. It holds a mirror of the
   rows only to repaint.
4. **No contract written twice.** Keys are encoded by ONE encoder, moved from the TUI's
   `pty_input/keys.rs` into `argus-protocol` (see "Foundation" below). The page sends key *names*
   (`KeyboardEvent.key` + modifiers); Rust maps them to the protocol key and encodes. Screen
   decoding uses `argus-protocol`'s `grid_from_runs` / `Scroll::apply` / `CellRun::apply` (and the
   older `PaneSnapshot`/`Damage` forms via whatever the protocol offers for spans), never a JS copy.

## Foundation (separate commit, before the desktop crate)

- **`argus-protocol/src/keys.rs`** — "the bytes a key press is to a pane's child". A
  crossterm-free `Key { code: KeyCode, mods: Mods }` (`KeyCode`: `Char(char)`, `Enter`, `Tab`,
  `BackTab`, `Backspace`, `Esc`, `Left`, `Right`, `Up`, `Down`, `Home`, `End`, `PageUp`,
  `PageDown`, `Delete`, `Insert`, `F(u8)` — whatever the existing encoder covers, no more; `Mods`:
  shift/alt/ctrl) and `encode(&Key) -> Vec<u8>` with **byte-identical** behaviour to today's
  `pty_input::encode_key`. Move the existing encoder tests across (they must keep passing,
  unchanged in what they assert), and keep a test in the TUI proving the crossterm→Key mapping
  plus encode gives the same bytes as before for every key the old tests covered. The TUI's
  `pty_input/keys.rs` keeps only what is the TUI's: `is_leader` and the crossterm → `Key`
  translation. Update both DESIGN tables (`keys` row in protocol; `pty_input/keys` row reworded).
  Protocol is a lib crate: no new dependencies.
- **`argus bridge` must exit when the daemon hangs up.** `tokio::io::stdin()` is a blocking read
  that keeps the runtime alive (see Findings), so today a bridge whose daemon
  went away lingers until its parent writes. Fix in `bridge.rs`: when `pipe` returns, end the
  process at once (`std::process::exit` with 0 on a clean end, after printing an error otherwise),
  with a comment saying why. Keep the existing tests passing.
- **`argus desktop`** — a TUI subcommand (`crates/argus-client/src/desktop.rs`, "the `argus
  desktop` command: opening the desktop window beside this terminal client") that starts
  `argus-desktop` from beside the `argus` executable (falling back to PATH), detached so closing
  the terminal does not close the window (unix `setsid` via `pre_exec`, as `launch.rs`
  `spawn_daemon` does; Windows `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`), stdio null, and
  returns. A missing binary is an error that says what to do. Usage string, `main.rs` parsing
  tests, README.
- **CI**: in `.github/workflows/ci.yml` and `release.yml`, a step before building that runs only on
  Linux (`if: runner.os == 'Linux'`):
  `sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config`,
  with a comment saying why (the desktop crate links WebKitGTK). `release.yml` packages
  `argus-desktop` beside the other three binaries in both the Unix `cp` list and the Windows
  `Copy-Item` list.

## The crate: `crates/argus-desktop` (package and binary `argus-desktop`)

All Rust under `src/` (the design_table test walks `crates/<name>/src`). Non-Rust files:
`build.rs` (`tauri_build::build()`), `tauri.conf.json`, `icons/icon.png` (256×256 **RGBA**) and
`icons/icon.ico` (converted from the removed phone client's `icon-512.png` at `20dbb14`, which is RGB), `assets/{index.html,app.js,style.css}`,
and a `.gitignore` entry for `crates/argus-desktop/gen/` (tauri-build writes schemas there).
Capabilities: the page uses only app commands and one `Channel`, which need no capability; add a
`capabilities/` file only if the build or runtime demonstrably needs one (verify, and say).

`Cargo.toml`: the workspace `[package]` shape; `tauri = { version = "2.12.1", default-features =
false, features = ["wry", "compression", "x11", "dbus", "custom-protocol"] }` (no
`common-controls-v6` — see Findings for why on Windows); `tauri-build = "2.7.1"` as the
build dependency; keep `tauri-macros` and `tauri-codegen` at 2.7.1 in the lock (`cargo update -p
tauri-macros --precise 2.7.1`, same for `tauri-codegen`) — every Tauri crate in the lock must be at
least two weeks old (today is 2026-10-14). Workspace deps: argus-protocol, tokio, serde,
serde_json, anyhow, tracing, tracing-subscriber; `unicode-width = "0.2"` (the lock's 0.2.2 is the
one alacritty uses, so widths match the daemon exactly). `tauri.conf.json`: no `version` (it falls
back to Cargo's, so the release bump stays in Cargo.toml), `productName` "Argus", identifier
`com.github.hbaldwin98.argus`, `build.frontendDist` "assets", `app.withGlobalTauri` true,
`app.windows` empty (the window is built in `setup`, see below), and a strict CSP:
`default-src 'self'; script-src 'self'; style-src 'self'; connect-src ipc: http://ipc.localhost`
(drop `'unsafe-inline'` for styles unless something needs it).

### Modules (each answers one question — rows go in a fourth DESIGN table)

| module | answers |
| --- | --- |
| `main` | startup: the runtime, the window, and running until it closes |
| `daemon` | reaching this machine's daemon through `argus bridge`: starting the bridge, greeting, and connecting again when it goes |
| `session` | what this window holds of the daemon — the tree, the pane on screen, the connection's state — and what each message and gesture changes |
| `screen` | a pane's screen as the page draws it: the grid the daemon's damage lands on, and the rows changed since the last frame |
| `rail` | what the rail lists from the tree, and what a row says |
| `page` | what the page and the window's Rust say to each other |
| `commands` | the calls the page makes, handed to the session |
| `keys` | a key the page reports, as the protocol's key |

(Rename/merge only with a reason; the module rule is the repo's.)

### `daemon`

- Find `argus[.exe]` beside `std::env::current_exe()`, else bare `argus` (PATH). Same scheme as
  `launch.rs` `daemon_exe_path`.
- Spawn `argus bridge` with `tokio::process::Command`: stdin/stdout/stderr piped, `kill_on_drop`,
  environment inherited (so `ARGUS_INSTANCE` / `ARGUS_CONFIG_DIR` pass through), on Windows
  `creation_flags(0x0800_0000)` (CREATE_NO_WINDOW — argus.exe is a console binary and a GUI parent
  would otherwise flash a console window; see argusd's `command.rs` `quiet()` for the precedent).
  Drain stderr into a bounded buffer (16 KiB, last bytes kept) — it carries the only error text.
- Reader task: `read_known_msg` loop into a bounded mpsc (not cancellation-safe, so its own task);
  writer task: `write_msg` from an unbounded receiver. Make the transport generic over
  `AsyncRead + AsyncWrite` so tests drive it with `tokio::io::duplex` and a played daemon.
- Greet: send `ClientMsg::Hello(Hello::this_build())` first, then `Greeting::read(&mut rx,
  GREETING_WAIT)`; map outcomes as `launch.rs::greet` does:
  `Up` → connected (keep `opening` messages, apply them first); `Refused(Protocol{..})` → connected
  but status carries the alarm (another protocol number: say both versions and `argus server
  restart`); `Refused(Predates)` → start a fresh bridge and do NOT greet on it; `Closed` → the
  bridge's stderr (last line(s)) and exit code become the status text, retry after a gap.
- When the connection ends: `Stopping` seen → status "argusd was stopped" and **no reconnect**
  until the page asks (`reconnect` command) — DESIGN.md: clients must not restart a daemon the user
  stopped. Otherwise (`Restarting`, a crash, the bridge dying) reconnect with a 2 s gap between
  failed rounds, forever, keeping the old tree on screen meanwhile.
- A daemon from another build (`Hello.version` differs) is a note on the status bar naming both
  versions and `argus server restart`, as the TUI does (`app/server.rs::greeted`).

### `session` (pure; unit-tested without Tauri)

- Holds: `tree: Vec<ProjectInfo>` (via `HeldTree::apply_telemetry` for `PaneTelemetry`),
  `daemon: Option<Hello>`, connection status, `selected: Option<PaneId>`, the `Screen` of the
  selected pane, the page's last grid capacity `(rows, cols)`, per-pane last Resize sent, pending
  spawn `request_id`s, window focus, last `View` sent.
- `select(pane)`: if different, `Unsubscribe` the old, drop its screen, `Subscribe` the new, then
  `Resize` with the page's capacity (the daemon sizes a pane to the minimum over its viewers, so
  the grid may come back smaller: the page pads). Selecting a pane not in the tree does nothing.
- `resize(rows, cols)`: store; if a pane is selected and its last sent size differs, `Resize`.
- A tree that no longer holds the selected pane: drop the selection and its screen (and
  Unsubscribe is unnecessary — the daemon already dropped it; check what the TUI does).
- On reconnect: forget sent sizes and subscriptions, resubscribe the selected pane and resend its
  size after the new opening arrives.
- `key(PageKey)` → `keys::to_key` → `argus_protocol::keys::encode` → `Input{pane, bytes}` (nothing
  when no pane or empty bytes). `text(String)` (IME/composition commit) → `Input` with its UTF-8.
  `paste(String)` → `ClientMsg::Paste` (the daemon brackets it only if the child asked).
- `spawn_shell(checkout)` → `SpawnShell{checkout, request_id}`; on `Created{request_id, ..}` for a
  pending id, remember the created pane and select it as soon as a tree holds it.
- View reporting: only if `daemon.can(FOCUS)`: send `ClientMsg::View{project, checkout, pane,
  active}` whenever it changes, where `active` = window focused (page reports focus/blur) — a
  background desktop window must never take the focus away from a TUI the person is typing in.
- `PaneClosed`, `Clipboard` (OSC 52 → page event; the page writes it to the clipboard best effort),
  `Error{message}` → status note.
- Messages this client does not use are ignored explicitly (one arm, with a comment).
- Emits page events: `status`, `rail` (whole rail view, on any tree/selection/status change),
  `frame` (from `Screen`, at most once per frame tick), `clipboard`.

### `screen`

- State: rows, cols, `Vec<Vec<Cell>>`, cursor, mouse tracking, alternate screen, dirty rows, a
  `full` flag, and whether the cursor changed.
- `PaneRows` → `grid_from_runs`, full. `RowDamage` → `scroll.apply` first, mark `top..bottom`
  dirty, then each run's `apply`, mark its row (if in range); update cursor/mouse/alt (a
  cursor-only frame has empty runs). Support the pre-`cell-runs` `PaneSnapshot` / `Damage` forms
  too (an argusd that predates the greeting sends them).
- `take_frame() -> Option<Frame>`: `None` if nothing changed. Each dirty row becomes segments:
  maximal stretches of one style `[col, cells: [string...], fg, bg, flags]` — cells as an array
  because a cell may hold combining marks (the page must not split strings). A wide cell
  (`UnicodeWidthChar::width(first char) == Some(2)`) is its own segment with flag `16` (WIDE) and
  its spacer cell is omitted. Trailing default cells (`" "`, default colours, no flags) are trimmed;
  the page clears the row first. Colours stay packed as on the wire (`0` default, `1<<24|idx`,
  `2<<24|r<<16|g<<8|b`) so a theme switch is a local repaint. Flags 1 bold, 2 italic, 4 underline,
  8 reverse, 16 wide.
- The frame carries `pane, rows, cols, full, cursor{row,col,visible,shape}, alternateScreen,
  lines: [[row, segments]]`.

### `page` (serde types; field names are a contract with app.js — test their JSON)

One `Channel` from Rust to the page carries a tagged enum (`{"type": "status" | "rail" | "frame" |
"clipboard", ...}`, camelCase fields). Calls from the page: `connect(events: Channel)` (replaces any
previous channel; Rust sends status + rail at once, and a full frame if a pane is on screen),
`select{pane}`, `resize{rows, cols}`, `key{key}` (`PageKey { key, code, ctrl, alt, shift, meta }`
— `key` is `KeyboardEvent.key`), `text{text}`, `paste{text}`, `spawnShell{checkout}`,
`focus{focused}`, `reconnect`.

### `commands` / `main`

- Own multi-thread tokio runtime; `tauri::async_runtime::set(handle)` first in `main`; Tauri on the
  main thread; `.build(ctx)` then `run_return`, shut the runtime down, `process::exit(code)`.
- Managed state: the session behind a `Mutex`, the outgoing `ClientMsg` sender, the page Channel.
  Commands are `async` (sync commands run on the UI thread) and live in `commands` (crate-root
  commands cannot be `pub`, E0255).
- A frame tick task (16 ms) takes the screen's frame and sends it; nothing is sent when nothing
  changed. Coalescing happens in Rust: `Channel::send` never waits for the page.
- `on_page_load(Started)` drops the page channel: after a reload `Channel::send` keeps returning
  Ok into nothing (verified), so a send error is never how a stream ends.
- The window is built in `setup` with `WebviewWindowBuilder` (label `main`, title "Argus", ~1200×800,
  min size), with `data_directory` under `argus_protocol::config_dir()` (instance-scoped, so an
  `ARGUS_INSTANCE=dev` window never shares webview storage with the installed one; note macOS
  ignores it) and `disable_drag_drop_handler()` (Tauri's handler would swallow HTML drops).
- `#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]` with a `//!` paragraph
  saying why (argus-hook's main.rs has the precedent and wording).
- Diagnostics: tracing to a file `argus-desktop.log` in `config_dir()` (keep the previous run as
  `.1`, like argusd's `logging.rs`), level from `RUST_LOG`, default `info`; failing to open it is
  never a reason not to start. If this needs its own module, add it to the table (`logging`).
- Panics abort in release (`panic = "abort"`): no `unwrap` on anything the daemon or page controls.

### The page (`assets/`)

- `index.html`: rail on the left (fixed width, scrolls), stage on the right: a header line (the
  pane's title, status word, checkout/branch), the terminal area (a text canvas and a cursor
  overlay canvas stacked), and a one-line status bar at the bottom (connection state, notes,
  a Reconnect button when stopped). A hidden, focusable `<textarea>` takes keyboard input and IME.
- `style.css`: Catppuccin Mocha by default, Latte under `prefers-color-scheme: light`, as CSS
  custom properties; the ANSI-16 palette and screen bg/fg from the removed phone client's `style.css` (`git show 20dbb14:crates/argus-web/assets/style.css`). System
  monospace stack (`ui-monospace, "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace`).
  Status words get colours (`waiting`/`needs-review`/`failed` loud, `working` accent, `idle`/`done`
  quiet, `exited` dim).
- `app.js`: `const { invoke, Channel } = window.__TAURI__.core`. Wrap `onmessage` in try/catch
  (Tauri 2.12.1 stops delivering after a handler throws). Renderer: measure
  the cell from the font (`measureText('M'.repeat(100))`, `fontBoundingBoxAscent/Descent`), device
  pixels, `ResizeObserver` + devicePixelRatio change → capacity rows/cols → debounced (≈50 ms)
  `resize`; draw the grid's own rows×cols and pad the rest; per dirty row: clear, fill non-default
  bgs, `fillText` per cell at `col*cellW` (wide glyphs centred over two cells), underline as a
  rect, reverse swaps after resolving; colours: default → theme fg/bg, idx 0–15 → ANSI-16 CSS
  vars, 16–231 cube (`v ? 55+40v : 0`), 232–255 grey (`8+10n`), rgb. Cursor on the overlay: block/
  underline/bar per shape, hollow when the window is unfocused, hidden when not visible. Keep a
  mirror of rows for repaint on theme change / DPR change.
- Keys: on the textarea's `keydown` (skip while `isComposing`): every key goes to the pane —
  `preventDefault()` so WebView2/WebKit accelerators (F5, Ctrl+F, Ctrl+R…) never fire — except
  the paste chord (Cmd+V on macOS, Ctrl+Shift+V elsewhere), which is left to the browser so the
  `paste` event fires → `paste{text}`. `compositionend` → `text{text}`. Modifier-only keydowns are
  not sent. Click on the terminal focuses the textarea; window `focus`/`blur` → `focus`.
- Rail: project heading (name, count of panes that need you), repository, checkout row (name,
  branch or "detached", git summary like the TUI's `git_state`: `↑a ↓b +staged !unstaged`, or
  `clean`), and pane rows (`◆` agent / `›` shell, title, loudest status word; a working pane
  animates subtly). Clicking a pane row → `select`; a small "+ shell" control on a checkout row →
  `spawnShell`. The selected row is highlighted. Keyboard navigation of the rail is not in scope.
- Empty states: "Connecting to argusd…", "No projects — run `argus init` in a repository", "Pick a
  pane on the left".

### Tests (Rust, colocated; names state the rule)

- `screen`: a snapshot then damage produces the expected rows; scroll is applied before runs and
  marks its region; a wide char gets the WIDE flag and its spacer vanishes; trailing blanks are
  trimmed; a cursor-only damage yields a frame with no lines; no change yields `None`; a fresh
  `PaneRows` replaces dimensions; the old `PaneSnapshot`/`Damage` forms work.
- `session`: selecting subscribes then resizes, re-selecting another unsubscribes the first;
  resize is deduplicated per pane; a reconnect resubscribes and resends the size; a spawned shell is
  selected only once `Created` arrived and the tree holds it; `View` is sent only to a daemon that
  can take `focus`, and `active` follows window focus; `Stopping` stops reconnecting; a tree without
  the selected pane drops it; keys with no pane send nothing.
- `daemon`: against a played daemon over `tokio::io::duplex`: greets and keeps the opening;
  a daemon that predates the greeting is reached again without greeting; closing before a word
  reports the bridge's stderr. (Spawning a real `argus bridge` is not needed in unit tests.)
- `keys`: DOM key names map to the protocol keys (`ArrowUp` → Up, `Enter`, `Backspace`, `F5`,
  printable chars with shift, Ctrl+letter, Alt+letter, `Dead`/`Unidentified`/modifier-only → none).
- `page`: the JSON of each event and call matches what `app.js` reads (a test per shape).
- `rail`: the rows built from a fixture tree (order, git summary, needs-you counts, editors left out).
- An ignored `dump_rail` test that prints the rail for a busy fixture tree as text (the repo's
  dump-test convention, adapted: there is no TestBackend for a webview).

### Docs

- DESIGN.md: a process-model paragraph for the desktop client (what it is, how it reaches the
  daemon through `argus bridge` and why, that it shows one pane at a time and the daemon sizes a
  pane to its smallest viewer, View reporting and window focus, `Stopping`, the webview data
  directory per instance, the page as plain JS with no build step); the binary list at the top; the
  fourth responsibility table (prose paragraph then table, as the others), and `design_table.rs`
  `CRATES` gains `"argus-desktop"` in the same position as its table.
- README.md: what the desktop app is, `argus desktop`, building it (Linux needs
  `libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config` to build and `libwebkit2gtk-4.1-0` to run;
  macOS/Windows need nothing extra; Windows uses the WebView2 runtime), running a dev build beside
  an installed one (`ARGUS_INSTANCE=dev`), the binary list ("three executables" → four). Note that
  `scripts/reload.sh` installs only the daemon and terminal client.
- AGENTS.md: the crate list (four crates again) and `-p` names; that building the workspace on
  Linux needs the WebKitGTK dev packages.
- TARGET.md: rewrite "Argus is not a general tmux replacement, a GUI, …" — Argus is terminal-first;
  the desktop app is another renderer of the same daemon and never owns state the terminal client
  lacks; it is still not an agent host, Git porcelain or tracker. Note the client budgets apply to
  the terminal client.
- ROADMAP.md: a new item for the desktop client: what landed (this slice) and what remains (the
  three stages, review, feature board, history, pickers and modals, mouse forwarding and selection,
  scrollback, several panes, remote hosts through `ssh … argus bridge`, packaging as .app/.msi/
  AppImage with signing and updates).

## Findings

Checked on 2026-10-14 against this repository and a throwaway Tauri probe built and run under Xvfb
on Ubuntu 24.04 (rustc 1.97). Windows and macOS were not built.

- **Versions.** `tauri` 2.12.1 and `tauri-build` 2.7.1 are the newest releases at least two weeks
  old; their MSRV is 1.90. 2.12.1 stops delivering a `Channel`'s messages after its `onmessage`
  handler throws once (fixed in 2.12.3), hence the try/catch. The 3.0 alphas need Rust 1.95+ and are
  not used. The crate adds about 256 packages to `Cargo.lock`; a clean release build of the probe
  took 2.5 minutes without LTO.
- **No tauri-cli, no JS build.** `cargo build`/`test`/`clippy` work with `build.rs` calling
  `tauri_build::build()`. `frontendDist` is embedded in every build as long as `devUrl` is absent,
  and it and the icons must exist at compile time, including for `cargo test` and `clippy`.
  `icons/icon.png` must be 8-bit RGBA; Windows needs `icons/icon.ico` for its resource file.
  tauri-build writes `gen/schemas/` into the crate on every build.
- **Windows features.** The default `common-controls-v6` feature imports `TaskDialogIndirect`,
  whose manifest is linked only into `[[bin]]` targets, so the bin's test harness can fail to start
  on MSVC (`STATUS_ENTRYPOINT_NOT_FOUND`). Leaving the feature out avoids it. MSVC links the WebView2
  loader statically; the runtime is needed only to run the app.
- **IPC.** A `Channel<T: Serialize>` delivers JSON under 8 KiB by one eval and in order; it is the
  fastest path for many small messages (2000 × 0.5 KB in 56 ms). `Channel::send` never waits for the
  page, so coalescing belongs in Rust. Events are slower and need a capability. App commands from
  the local origin need no capability.
- **Lifecycle.** After `location.reload()` `Channel::send` keeps returning Ok into nothing, so
  streams end on page load, not on a send error. `App::run` ends in `process::exit`; use
  `run_return` to clean up. Synchronous commands run on the UI thread.
- **Linux CI.** `libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config` on ubuntu-22.04 and its arm image.
  macOS needs nothing.
- **`argus bridge` lingers.** `tokio::io::stdin()` is a blocking read that cannot be cancelled, so
  when the daemon hangs up the bridge does not exit until its parent writes or closes stdin. The ssh
  path has the same problem today. Hence the fix under "Foundation".
- **The screen on the wire.** Cells carry only bold, italic, underline and reverse; dim, strike,
  hidden and hyperlinks are dropped by the daemon. There is no width flag, but the lock has one
  `unicode-width`, shared with alacritty, so a width computed in Rust matches the daemon. Scroll is
  inferred and only ever upward, and is applied before the runs. A pane is sized to the minimum over
  its viewers, so a desktop window and a TUI on one pane share the smaller size.
- **Keys.** Only `MouseTracking` and the alternate screen reach clients; application cursor mode,
  bracketed paste and the kitty keyboard stay in the daemon, which brackets pastes itself.
