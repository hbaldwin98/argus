# Argus Desktop on GPUI: Scope

The plan for moving `argus-desktop` from a Tauri webview to native GPUI
through `gpui-kit`. The Argus feature "desktop on GPUI" holds the decisions
and the phase tasks; this file holds what the phases learn. It replaces
DESKTOP.md's page sections once the port lands, and is deleted then.

## The stack

`gpui-kit` 0.8.0 pinned exactly, which pins `gpui-pre` 0.3.9 (a weekly
snapshot of Zed's `main`, published by one owner). The official `gpui`
crate on crates.io is 0.2.2 from October 2025 and has not moved since.
Zed's `editor` crate is GPL-3.0-or-later and unpublished; Argus is MIT OR
Apache-2.0, so every text control is `gpui-kit`'s `Input`, `Textarea` or
`Editor`.

Checked in the `gpui-pre` 0.3.9 source:

- `WindowBackgroundAppearance` is `Opaque`, `Transparent`, `Blurred`,
  `MicaBackdrop`, `MicaAltBackdrop`, and `Window::set_background_appearance`
  changes it on a live window. There is no Acrylic variant; whether
  `Blurred` is Acrylic on Windows is the spike's to find out.
- `VisualTestContext` and the headless app context have `render_to_image`,
  which fails rather than fakes when no headless renderer is configured.

## The seam

`session.rs` is plain Rust and stays the app's logic. Its inputs are
`Session::receive(ServerMsg)` and `Session::call(Call)`; its outputs are
`events(now)`, `everything(now)` (the replay a fresh page gets) and
`take_frames()` (terminal rows, polled every 16 ms). Every surface but the
terminal is sent as a whole snapshot, deduplicated by `connection::sent::Sent`;
`Frame` is the only diff.

So the port is in two steps:

1. **The GPUI front end speaks the contract.** `Shared::publish` sends
   `Event`s to a channel a `cx.spawn` loop drains into a root model entity;
   views render the latest snapshot per surface; handlers build `Call`s and
   go through `Shared::act`. Session, screens and their tests are untouched,
   and both front ends can run during the port.
2. **The contract collapses.** `Session` lives in an `Entity<Session>`,
   views read it, and `Sent<T>`, `everything()`, serde on the page types,
   ts-rs and the bindings go. The `*Page` structs are today's view models,
   so this is a refactor of its own, after the port.

Threading: daemon I/O is tokio (`connection/daemon.rs`), GPUI's executors
are not. The tokio runtime starts in `main` before the GPUI app; daemon
tasks hand messages to the UI thread over a channel rather than locking the
session from tokio threads.

## Surfaces

Sizes: S under a day, M 1-3 days, L 1-2 weeks, XL more. Mappings are from
the `gpui-kit` catalog and still to be confirmed per component in the parity
spec phase.

### Shell, panes, keys, settings

| Surface | Page | gpui-kit | Hard parts | Size |
|---|---|---|---|---|
| Terminal screen | panes/screen.ts 530 | custom Element, Zed's terminal_element as the model | cell metrics, wide cells, cursor, selection, contrast fix, theme repaint, resize report, large frames | XL |
| Terminal input | panes/input.ts 171 | custom `EntityInputHandler` | IME composition, keystroke to `PageKey`, mouse reports, wheel notches | L |
| Keymap and focus layers | keys/keymap.ts 432, focus.ts 51 | root key interceptor over Rust's keymap; layers as key contexts | leader and chords do not fit `bind_keys`; imports ~15 other surfaces | L |
| Project rail | chrome.ts 1088, chrome/projects.ts 639 | `Sidebar`, `Tree`, `PopupMenu` | picked-row keyboard model, per-row menus, middle-click close, width drag | L |
| Split grid | panes/split.ts 420, split_math.ts 109 | custom layout over `ResizeHandle` | arbitrary zones, header drag onto a pane, per-frame throttle | L |
| Docks | layout/docks.ts 269 | `Resizable`, `TabBar`, `PopupMenu` | container only; placement stays in Rust | M |
| Panel drag to dock | layout/drag.ts 165, edge.ts 57 | custom | drop zones, preview overlay | M |
| Floating saved-view windows | layout/floats.ts 141 | custom (`Dialog` neither moves nor resizes) | drag, resize, z-order | M |
| Console | panes/console.ts 311 | `TabBar`, `Dialog`, `Input` | reuses the terminal screen | M |
| Command palette | keys/palette.ts 345 | `Command` | `>` and `#` scoping | M |
| Shortcuts sheet | keys/shortcuts.ts 241, sections.ts 191 | `Sheet`, `Kbd` | — | S-M |
| Title strip | system/frame.ts 51 | `TitleBar` | Win11 snap-layout hover, DWM border colour via the raw HWND | M |
| Status bar | chrome.ts | `StatusBar` | — | S |
| Add-project dialog | chrome/projects.ts | `Dialog`, `Input`, `List` | path completion | M |
| Menus | kit/menu.ts 131 | `PopupMenu` | disabled reason and hint per item | S |
| Settings | settings/preferences.ts 656 | `Settings`, `Slider`, `Select`, `Switch` | retained state so a dragged slider keeps its hold | M |
| Font picker | settings/font_picker.ts 183 | `Combobox` | each row in its own face | S-M |
| Backdrop | (CSS alpha) | `set_background_appearance` | Acrylic, opacity as theme alpha | M |
| File tree | kit/file_tree.ts 101 | `Tree` | change marks, ignored-folder fold | S-M |
| Chime | system/chime.ts 43 | audio crate | WebAudio today | S |

### Board, review, agents

| Surface | Page | gpui-kit | Hard parts | Size |
|---|---|---|---|---|
| Feature stage | board/feature_stage.ts 1068 | `List`, `Resizable`, custom board columns | card drag with ghost, per-column accept | L |
| Work lens | board/lenses/work.ts 1132 | `Tree`, `Textarea`, `Input`, `Select` | keyboard outline with in-place forms | L |
| Sequence diagram | board/lenses/sequence.ts 702 | custom painting | dashes, arrowheads, frames; lifeline and arrow drag rewriting the flow; pan and zoom | L |
| Flows lens | board/lenses/flows.ts 325 | `List`, `Textarea` | step bindings, expand over the window | M |
| Record lens, lens header | record.ts 141, head.ts 145 | `Tree`, `TabBar`, `DropdownButton` | — | S |
| Tracker link, reconcile | tracker/link.ts 321, reconcile.ts 400 | `Dialog`, `Input`, `Checkbox` list | checkbox-list keyboard, editable line detail | M each |
| Capture note | board/capture.ts 297 | `Textarea` | line-number gutter in sync with the text | M |
| Review dialog | review/dialog.ts 391 | `Resizable`, `List`, custom diff rows | line-range pick, inline comment box, virtualised large diffs | L |
| Comment box | review/comment_box.ts 92 | `Textarea`, `Tag` | shared by review and the Code stage | S |
| History dialog | review/history.ts 260 | `Tree`, shared diff view | — | M |
| Compose box | agents/compose.ts 245 | `Dialog`, `Select`, `Textarea` | submit key, IME, image paste | S |
| Sessions dialog | agents/sessions_dialog.ts 391 | `List`, `Resizable`, `DropdownButton` | filter menus' Escape and focus | M |
| Transcript reader | agents/transcript.ts 287 | `VirtualList` or `MessageScroller`, `TextView` | find highlighting, jump to turn, open at the bottom only for a new session | M-L |

### Checkouts, code, views

| Surface | Page | gpui-kit | Hard parts | Size |
|---|---|---|---|---|
| Code stage | code/stage.ts 880 | `Tree`, `v_virtual_list` of Rust's highlighted rows | change wash and removed-line rules, change ruler, cursor, picked lines | XL |
| Checkouts stage | checkouts/stage.ts 762, files.ts 405 | `DataTable`, `Resizable`, `Tree` | custom cells, keyboard parity | M-L |
| Extension views, dialogs | extensions/views.ts 669, dialogs.ts 193 | `Sidebar`, `Dialog` | a dialog that leaves focus in the pane | M |
| Saved views, agent view | views/saved.ts 549, views/agent.ts 280 | `Tabs`, `Badge`, the block renderer | horizontal scrolling columns | M each |
| Markdown block | views/blocks/markdown.ts 457 | our parser, ported to Rust, rendered to elements | `TextView::markdown` documents none of our guarantees (HTML as text, http(s) links only, depth caps) | M |
| Table block | blocks/table.ts 269 + table/ 384 | `DataTable` | sort, row actions, rows expanding to blocks, TSV copy | L |
| Chart block | blocks/chart.ts 280 | `AreaChart` or `Plot` | per-series dash and fill by tone | M-L |
| Diagram, graph blocks | blocks/diagram.ts 267, graph_layout.ts 348 | custom painting; graph layout ports to Rust | arrows, bends, hit-testing, zoom | L each |
| Patch block | blocks/patch.ts 139 | the shared diff view | — | S-M |
| Form block | blocks/form.ts 152 | `Form` and its fields | — | M |
| Simple blocks | 16 files, ~700 lines | `Alert`, `Progress`, `Collapsible`, flex | matrix and timeline layouts | M together |

### Long poles

The terminal (screen and input), the keymap, the Code stage and the
diagram painting (sequence, diagram, graph) carry most of the risk. The
diff view is built once and shared by review, history, patch and the Code
stage. One drag helper serves board cards, sequence arrows, panel docking
and pane headers.

## What else changes

- **Theme.** About twelve palettes, light and dark, each a role set
  (`--bg-*`, `--fg-*`, `--accent`, `--line`, ...) with `color-mix` derivations
  and an opacity alpha. These become a Rust theme mapped onto `cx.theme()`,
  plus the type scale, the bundled fonts (loaded with `add_fonts`), the UI
  and mono family overrides, a zoom factor and the terminal palette. The
  10.5k lines of CSS are restyled surface by surface.
- **Shell.** Tauri, `tauri-plugin-notification`, `window-vibrancy` and
  `tauri-build` go. Notifications move to a crate such as `notify-rust`;
  clipboard to GPUI's `ClipboardItem` or stays on `arboard`; fonts to GPUI's
  font list or stay on `fontdb`. `windows_subsystem = "windows"` stays.
- **Tests.** `fixtures.rs` and the `screens/` scenes carry over: a scene
  becomes a window in `VisualTestContext`. `screens/live.rs`, the ts-rs
  bindings, `page/test` and `shoot.mjs`/`audit.mjs` go; screenshots move to
  `render_to_image`, and the audit is rebuilt on `simulate_*`.
- **Build and CI.** The `page` job, the bindings check and the WebKitGTK
  apt lines go (`ci.yml`, `release.yml`, `reliab.yml`); GPUI's Linux
  packages and a headless renderer come in. README, AGENTS.md and DESIGN.md
  rows change with them.

## Decided

The feature board holds each with its reasons; in short:

- **A new crate** beside `argus-desktop`, the two side by side until the
  new one replaces it.
- **The look is a hybrid**: `gpui-kit`'s flat surfaces and components,
  kept full-window as today (no title bar, no status bar, the stage keeps
  its header and the window controls, the sidebar foot keeps the counts).
  The canvas https://claude.ai/artifact/VeXPoEQLFzJWDLpR26qfcx shows it.
- **Git gets a sidenav entry** with Changes and History. Changes replaces
  the review dialog, History the history dialog; branch rows stay in
  Checkouts.
- **Windows first**, Linux and macOS after.
- **Both screenshots and UI integration tests** gate each surface, so a
  headless `render_to_image` has to work before the port starts.
- The seven cross-cutting behaviour questions: see
  `gpui/parity/cross-cutting.md` §Decided.

- **`Session` moves into a shared crate.** `argus-desktop` is a binary
  with no library target; its Tauri-free modules (`session.rs` and the
  per-surface state) move into a crate both front ends depend on, rather
  than `argus-desktop` growing a library target that would pull Tauri
  into the new crate's build.

## The Code editor

`main` now edits a working file inline in the Code stage: CodeMirror with
Vim mode, tree-sitter grammars installed on request, live with agents
(`page/src/code/editor.ts`, `sync.ts`, `grammar/`). The GPUI editor keeps
Vim mode, built on `gpui-kit`'s `Editor`, which has tree-sitter but no
Vim layer; the live sync with agents is built for GPUI too. It is its own
track, inventoried and specced after the spike, and does not hold the
spike up.

## Phases

Each phase is a workflow. The coordinator reads its result before starting
the next.

1. **Inventory**: the tables above.
2. **Parity spec**: per surface group, every behaviour the port must keep,
   drawn from DESIGN.md, the code and the scenes; each spec checked against
   the code by an independent agent; a completeness pass for what is missing.
3. **Spike**, on Windows, in the new crate, independent of `Session`: a
   frameless window in the Ledger palette with the window controls; the
   terminal element drawing rows shaped like `Frame`; IME in a `Textarea`
   and a screenshot pasted into it; the dock and resizable splits; Mica,
   `Blurred` and opacity switched on a live window; `render_to_image` of a
   view in a test; and the time from a one-line style change to seeing it.
4. **Architecture**: the open questions decided.
5. **Port**: per surface, a sonnet implementer in an isolated worktree, an
   independent verifier against the parity spec, the coordinator's read
   before commit.
6. **Removal**: the webview, `page/`, the npm build and the contract.
