# Parity spec: shell, panes, keys, settings

The requirements a GPUI port of the Argus desktop's shell must meet, written so a later agent can port and test a surface without reading the TypeScript. Group: **Shell, panes, keys, settings**. Companion to `GPUI.md` (scope, inventory) and `.agents/skills/gpui-kit/SKILL.md` (component catalog).

## How to read this

- **Paths.** `ts:X` is `crates/argus-desktop/page/src/X`; `rs:X` is `crates/argus-desktop/src/X`; `css:N` is `crates/argus-desktop/assets/style.css` line N; `scene:name` is a scene of `rs:screens/mod.rs`; `DESIGN.md §` and `DESKTOP.md §` name a heading of those files. A bare test name is a Rust `#[test]` in the module the row names, or a node test of `crates/argus-desktop/page/test/`.
- **The seam.** The page draws what Rust sends (`Event`, `rs:connection/page.rs:448`) and reports gestures as one `Call` (`rs:connection/page.rs:42`). Calls are written `name{field}` in snake_case, as `ts:connection/tauri.ts` sends them. A call answers nothing; what it changed comes back as an event. The port keeps `Session` (`rs:session.rs`) as the logic and may call `Session::call` directly (`GPUI.md` §"The seam").
- **No prediction.** `AGENTS.md`: the client never predicts the result of a request. Where the page does hold state of its own it is listed under **Page-held state** in the surface, because the port must hold it too (in a view entity) and nothing else will.
- **Ids.** `SHELL-<surface>-<n>`, stable. A behaviour is observable: a person, a test or the daemon can tell. Where today's page is inconsistent with its own docs, the behaviour says what the page does and a **Gap** line says what the docs claim; the port decides, the spec does not.
- **gpui-kit.** Each mapping was checked against `https://gpui-kit.com/component/{name}.md` (fetched for this spec) or the skill's `references/`. "Seen:" lists the API the page showed. "Not documented:" means the page was silent, so the port must spike it rather than assume. "custom" says why none fits. No API is invented.
- **Test tools the port has** (skill `references/gpui/test.md`): `window.find/click/right_click/double_click/hover/scroll/drag_to/press/input`, `click_with_options(.. ClickOptions::new().with_button(MouseButton::Middle) ..)` (unconfirmed), `pointer_down/move/up`, `key_down/key_up`, `TestInput::{commit_text, compose_text}`, `simulate_window_resize`, `simulate_window_scale_factor_change`, `cx.read_from_clipboard()`, `cx.opened_url()`, and `render_to_image` (`GPUI.md` §The stack). There is **no rendered-text assertion**: text is asserted through labels, `value()` and the model, not by reading pixels.

### Surfaces covered

| Id | Surface | Page module | Rust |
|---|---|---|---|
| SEAM | Event intake and focus hand-offs | `main.ts`, `state.ts` | `connection/` |
| KEYS | Keymap, layers, leader, Escape ladder | `keys/keymap.ts`, `focus.ts`, `chords.ts` | `keys/keymap.rs`, `page_key.rs` |
| PALETTE | Command palette | `keys/palette.ts` | `keys/palette.rs` |
| SHEET | Shortcuts sheet | `keys/shortcuts.ts`, `sections.ts` | `keys/keymap.rs` |
| SCREEN | Terminal screen | `panes/screen.ts`, `contrast.ts`, `size_label.ts` | `panes/screen.rs` |
| INPUT | Terminal input and mouse | `panes/input.ts` | `keys/page_key.rs`, `session.rs` |
| GRID | Split grid | `panes/split.ts`, `split_math.ts` | `panes/shown/` |
| CONSOLE | Console | `panes/console.ts`, `console_strip.ts` | `panes/console.rs` |
| DOCK | Docks | `layout/docks.ts`, `panels.ts` | `layout/docks.rs` |
| PDRAG | Panel drag to a dock | `layout/drag.ts` | `layout/docks.rs` |
| FLOAT | Floating saved-view windows | `layout/floats.ts` | `layout/docks.rs` |
| EDGE | Edge drag (sashes) | `layout/edge.ts` | `layout/dragged_width.rs` |
| STAGE | Stage hosting, strip words, empty stages | `chrome.ts` | `layout/stage.rs` |
| RAIL | Project rail, picked row, + menu | `chrome.ts` | `chrome/rail.rs` |
| SWITCH | Project switcher and removal question | `chrome/projects.ts` | `chrome/switcher.rs` |
| ADD | Add-project dialog | `chrome/projects.ts` | `chrome/add_project.rs` |
| MENU | Menus and right-click | `kit/menu.ts`, `middle.ts` | none |
| RENAME | Rename dialog | `panes/rename.ts` | `session.rs` |
| FOOT | Sidebar foot, status, tallies | `chrome.ts` | `chrome/status.rs` |
| NOTICE | Notices that pass | `chrome.ts` | `session.rs` |
| TITLE | Title strip and window buttons | `system/frame.ts` | `system/window_frame.rs` |
| SETTINGS | Settings dialog | `settings/preferences.ts` | `settings/preferences.rs` |
| FONT | Font picker | `settings/font_picker.ts` | `settings/fonts.rs` |
| LOOK | Theme, opacity, backdrop, zoom | `settings/preferences.ts` | `settings/backdrop.rs` |
| TREE | File tree | `kit/file_tree.ts`, `ignored_fold.ts` | none |
| SYSTEM | Chime, notifications, clipboard, opener | `system/` | `system/` |
| TIME | Local-time formatting | `kit/clock.ts` | none |

---

## SEAM: event intake and focus hand-offs

Source: `ts:main.ts`, `ts:state.ts`, `ts:connection/tauri.ts`, `rs:connection/{commands,shared,sent}.rs`, `rs:main.rs`. `DESKTOP.md §connection/commands / main`.

### Behaviours

- SHELL-SEAM-1. A window that loads asks once to `connect`; Rust answers by replaying everything the page needs: `Keymap` first, then every surface (`Session::everything`, `rs:session.rs:3186`). A reloaded or rebuilt view therefore holds nothing it has to remember. (`ts:main.ts:245`, `rs:connection/shared.rs` `attach_page`)
- SHELL-SEAM-2. Each event replaces what the surface held; the page keeps no history of events. (`ts:main.ts:1-3`, `ts:main.ts:61-207`)
- SHELL-SEAM-3. A surface drawn only while open (dialogs, palette) is sent whole whenever it changes, sent once as its `Default` (`open: false`) when it closes, and not at all after that. (`rs:connection/sent.rs`; tests `a_closing_is_sent_once`, `an_unchanged_thing_is_not_sent_again`)
- SHELL-SEAM-4. A handler that throws on one event is logged and the stream goes on. (`ts:connection/tauri.ts:48-53`)
- SHELL-SEAM-5. Terminal rows are the one diff: `Frame` events for the shown sides are taken every 16 ms and only when something changed. A frame names its side and is dropped when the page has no screen for that side. (`rs:main.rs:44-46,139-146`, `ts:panes/screen.ts:526-530`)
- SHELL-SEAM-6. `status` event: the sidebar foot, heading dimming and the stopped card are redrawn. (`ts:main.ts:63-66`)
- SHELL-SEAM-7. `docks` event: docks, the views panel and the saved-view nav entries are redrawn. (`ts:main.ts:67-73`)
- SHELL-SEAM-8. `console` event: the chrome is redrawn; when `typing` becomes a different non-null pane than before, the pane text area takes the keys (a console newly on screen, or a split side given the keys by a press on its header). (`ts:main.ts:74-82`)
- SHELL-SEAM-9. `rail` event hand-offs, in this order: (a) the screen expects the shown pane (skeleton rule SHELL-SCREEN-12); (b) the chrome is redrawn and the fullscreen-pane state follows `filled`; (c) when the stage goes from `new` to `workspace` and a pane is now shown, the composer draft is cleared; (d) when the stage becomes `new`, the composer takes focus and nothing below runs; (e) when the stage becomes `feature`, the pane text area is blurred; otherwise (f) while a rail row is picked the keys stay on the rail; otherwise (g) a pane newly shown (different id than before) or any shown pane after a stage change takes the keys. (`ts:main.ts:83-105`; test: none)
- SHELL-SEAM-10. A dialog that takes the keys from a pane (add-project, link, reconcile, review, history, transcript, sessions; `takesFromPane` in `rs:keys/keymap.rs:853-886`) blurs the pane text area when it opens over a pane that had the keys and remembers that; when no such dialog is left open and the pane had the keys, the pane takes them back, but only if a pane is shown. (`ts:main.ts:43-59`)
- SHELL-SEAM-11. `clipboard` event (a pane's child set the clipboard with OSC 52) writes the text to the system clipboard, best effort, failure silent. (`ts:main.ts:109-111`)
- SHELL-SEAM-12. `notice` event shows the SHELL-NOTICE line; `chime` event plays SHELL-SYSTEM's chime; `run{action}` opens the palette, sheet or settings (SHELL-KEYS-13); `keymap` stores groups, commands and layers and redraws the nav key hints. (`ts:main.ts:112-114,186-205`)
- SHELL-SEAM-13. A click anywhere closes a checkout's + menu; it also closes the project switcher unless the click's path went through the project heading (the heading redraws under its own clicks, so the target alone is unreliable). (`ts:main.ts:218-229`)
- SHELL-SEAM-14. A press anywhere outside the rail drops the rail's picked row. (`ts:main.ts:231-233`)
- SHELL-SEAM-15. Nav and strip buttons send: Search opens the palette locally; New `show_composer{open:true}`; Workspace `show_features{open:false}`; Features `show_features{open:true}`; Checkouts `show_checkouts{open:true}`; Code `show_code{open:true}`; Console `console{gesture:{on:"tab"}}`; Reconnect `reconnect{}`. (`ts:main.ts:234-242`)
- SHELL-SEAM-16. A `call` made while no connection is up goes nowhere and is not queued. (`rs:connection/shared.rs` `send`: `out` is `None` between connections)
- SHELL-SEAM-17. Threads: daemon I/O is tokio; the session is touched under one lock so calls, daemon messages and frame ticks reach the daemon and the view in the order they were made. (`rs:connection/shared.rs`; `GPUI.md` §Threading)
- SHELL-SEAM-18. The `views-tab` button's click opens the newest agent view (`openNewestView`). The nav table of SHELL-SEAM-15 leaves it out. (`ts:main.ts:235`)
- SHELL-SEAM-19. Startup: `ARGUS_DESKTOP_HOST` is used only when `desktop.json` holds no host. The webview data directory is per instance. Rust drops the page channel when a reload starts. The installed fonts and the ssh hosts are read on blocking threads after the window opens, so the first frames may not have them yet. (`rs:main.rs:58-63,69-73,79-90,117`)
- SHELL-SEAM-20. `Call::PasteClipboard` exists and is answered in Rust (`rs:connection/page.rs:82`, `rs:connection/commands.rs:39`), but no page code sends it (no `paste_clipboard` anywhere in `ts:` page code): a dead call. The paste chord itself is read by Rust (SHELL-KEYS-8).

### Page-held state (`ts:state.ts`)

`menuOpen`, `workspacesOpen`, `hostsOpen` (switcher); `start` (open + menu); `railPick`; `compose` draft; `editing`, `chosen`, `expanded` (other groups). Everything else is the last event of its kind.

### Page-side logic that must move to Rust

- The focus hand-off table SHELL-SEAM-8..10 is page logic (`ts:main.ts:43-105`). It decides who has keyboard focus from event diffs. In GPUI it becomes the root view's `Entity` update handler; keep it in one function.

### gpui-kit mapping

- **Custom (GPUI core).** Channel loop draining `Event`s into a root entity; focus moves are `FocusHandle::focus` (skill `references/gpui/focus-handle.md`). Focus lost after a surface vanishes: `cx.on_focus_lost` + `window.focus_lost_restore_target` (unconfirmed) (seen in `focus-handle.md` §4 "Restore Lost Focus"), which is exactly SHELL-SEAM-10's failure mode.
- `Root` mounts dialogs, sheets and notifications (`root.md`, `dialog.md` "Where dialogs render"): open the window with `gpui_kit::open_window`.

### Scenes and coverage

- Every scene replays `Session::everything` (`scene:*` via `write_screens`). Covered: SEAM-1 by `write_screens`. **Not covered by a scene or test:** SEAM-8, SEAM-9 (focus), SEAM-10, SEAM-13, SEAM-14 (all focus/pointer behaviours; `audit.mjs` clicks controls but asserts no focus rule). Rust tests only cover that the right events are sent, e.g. `a_command_goes_with_where_the_person_is_and_after_the_leader_from_a_pane`.

---

## KEYS: keymap, layers, leader, Escape ladder

Source: `ts:keys/{keymap,focus,chords}.ts`, `rs:keys/{keymap,page_key}.rs`, `rs:session.rs:1082-1190`. `DESIGN.md §Process model` (the keys paragraph), `DESKTOP.md §Panes side by side`. Tests: `rs:keys/keymap.rs` (`every_binding_has_a_label_and_a_key`, `every_place_has_a_group_and_every_binding_is_in_one`, `no_key_is_bound_twice_where_it_is_heard`, `every_layer_is_ordered_once_and_the_stage_comes_last`, `only_the_dialogs_take_the_keys_from_a_pane`, `the_zoom_keys_are_the_windows_wherever_the_keys_are`, `a_pane_keeps_its_keys_until_the_leader`, `only_a_pane_has_a_leader`, `the_leader_is_ctrl_space_with_or_without_shift`, `a_named_key_asks_for_its_shift_and_a_character_does_not`, `the_paste_chords_are_a_terminals`, `the_page_reads_keys_only_through_the_keymap`, `every_key_the_window_keeps_is_one_the_daemon_refuses_an_extension`), `rs:keys/page_key.rs` tests, node `keys/focus.test.mjs`.

### The table (Rust, stays)

`BINDINGS` (`rs:keys/keymap.rs:521-758`) is the one table: each binding has chords, place, action, label. `groups()` orders places for the sheet (`ORDER`, `:813-839`), `commands()` is the palette's list (`COMMANDS`, `:762-811`), `LAYERS` (`:853-886`) orders who has a key. The port reads these statics directly; the crossing as `Event::Keymap` goes away.

### Behaviours: matching and the Window place

- SHELL-KEYS-1. A chord matches a key when `key` is equal, ctrl/alt/meta are equal, and `shift` is equal **only for a named key** (more than one char): for a single character the character carries its own shift (`A` is shift-a). (`rs:keys/keymap.rs:441-448`; `ts:keys/keymap.ts:44-52`; tests `a_named_key_asks_for_its_shift_and_a_character_does_not`)
- SHELL-KEYS-2. `actionFor(places, key)` asks the places in the order given, the first binding with a non-leader chord that matches wins; one place may take a key from a later one (an open feature's digits are lenses before they are stages). (`ts:keys/keymap.ts:54-65`)
- SHELL-KEYS-3. Window-place keys (zoom in `Ctrl +`/`Ctrl =`, zoom out `Ctrl -`, reset `Ctrl 0`; `Cmd` instead of `Ctrl` on macOS) are heard **first, before any field, dialog or pane**, stop there, and send `command{action}`. No other place may bind them. (`ts:keys/keymap.ts:413-426`, `rs:keys/keymap.rs:488-511,563-565`; test `the_zoom_keys_are_the_windows_wherever_the_keys_are`)
- SHELL-KEYS-4. Zoom steps `50 67 75 80 90 100 110 125 150 175 200` percent; in/out step to the next step above/below the current value (a hand-edited between-steps value goes to the next one up/down); stops at either end; reset goes to 100; the zoom is kept in `desktop.json` and the whole window, panes' cells included, follows it. (`rs:settings/preferences.rs` tests `the_zoom_steps_as_a_browser_does_and_stops_at_either_end`, `a_hand_edited_zoom_is_held_to_the_steps_ends_and_steps_onto_them`; `rs:connection/commands.rs:72-77`)

### Behaviours: keys in a pane

- SHELL-KEYS-5. Inside a pane every key is the program's except the leader (`Ctrl Space`, `LEADER`, `rs:keys/keymap.rs:483`). The key after the leader is the window's; one nothing binds is dropped (not typed), except an extension command bound after the leader. A second leader sends `Ctrl Space` to the program. (`rs:session.rs:1078-1101`; tests `leader_twice_sends_leader_to_pane`, `a_key_after_the_leader_is_the_windows_not_the_programs`)
- SHELL-KEYS-6. Keys after the leader (Pane place): `f` fill/unfill; `1`..`5` Workspace/Feature/Checkouts/Console/Code; `` ` `` show or hide the console; `z` console fullscreen; `\` split the console; `o` next / `O` previous pane on screen; `Arrow` the keys to the pane that way; `Shift Arrow` move the pane that way (trade places); `n` composer; `Tab` review the pane's checkout; `Ctrl K` or `k` palette; `?` shortcuts; `,` settings; `v` views; `x` close the pane (ends its program); `Ctrl Space` send through. (`rs:keys/keymap.rs:534-566`)
- SHELL-KEYS-7. Pane keys without the leader: the paste chord (`Ctrl V`, `Ctrl Shift V`, `Alt V`; `Cmd V` on macOS), `Shift PageUp` and `Shift PageDown` (move the parked view back / forward). Everything else is encoded for the child (`PageKey` to `Key` to bytes with the child's keyboard mode) and sent as `Input{pane, bytes}`; typing returns a parked view to live. (`rs:keys/keymap.rs:557-559`, `rs:session.rs:1082-1101,1219-1234`; `rs:keys/page_key.rs:34-98`; tests `named_dom_keys_map_to_the_protocols_keys`, `printable_characters_type_themselves_shifted_or_not`, `ctrl_and_alt_letters_reach_the_child_as_a_terminal_sends_them`, `alt_on_a_mac_types_the_letter_not_the_option_layer`, `shift_tab_is_a_back_tab`, `keys_with_nothing_to_type_send_nothing`)
- SHELL-KEYS-8. The paste chord is answered in Rust, not the page (unless the leader is waiting): a clipboard **picture** goes first as `paste_file "clipboard.png"`, else **files** a file manager copied (each read whole, named by file name; an unreadable one says `could not read <name>: <err>`), else **text** (line endings normalised) as `paste`; with nothing: notice `nothing on the clipboard to paste`. (`rs:connection/commands.rs:35-107`; tests `the_paste_chord_reads_the_clipboard_unless_the_leader_is_waiting`, `a_clipboard_picture_is_encoded_as_a_png`)
- SHELL-KEYS-9. A key in a pane that opens the palette, sheet or settings reaches the view as `run{action}` after Rust dispatched it; the view then opens it with the pane's text area still focused. (`rs:session.rs:1170`; `ts:main.ts:203-205`)

### Behaviours: layers outside a pane

- SHELL-KEYS-10. Who has a key outside a pane is the first **layer up** in `LAYERS` order, for the key's focus target. The 24 layers, in order, with the places each hears and whether it takes the keys from a pane (T): `field` (none), `extensionDialog` (none), `settings` (none), `sheet` [anywhere, sheet], `addProject` (none, T), `link` [anywhere, link] T, `reconcile` [anywhere, reconcile] T, `review` [anywhere, review] T, `history` [anywhere, history] T, `transcript` [anywhere, transcript] T, `sessions` [anywhere, sessions] T, `start` [anywhere, start], `extension` [extension], `rail` [anywhere, rail], `menu` [anywhere, menu], `capture` [anywhere], `diagram` [diagram, outline, anywhere], `outline` [outline, anywhere], `features` [anywhere, feature, card], `files` [files], `checkouts` [anywhere, checkouts], `code` [anywhere, code], `extensionPage` [anywhere, extension], `stage` [anywhere] (always up, last). (`rs:keys/keymap.rs:853-886`; `ts:keys/focus.ts:31-35`; tests `every_layer_is_ordered_once_and_the_stage_comes_last`, node `the first layer up in the order has the keys`, `the layers are asked only as far as the walk goes`)
- SHELL-KEYS-11. "Up" predicates: `field` target inside `input`/`textarea`/`[contenteditable]`; `extensionDialog` a dialog control focused; `settings`/`sheet` open; `addProject`/`link`/`reconcile`/`review`/`history`/`sessions` open, `transcript` the reader open; `start` a + menu open; `extension` an extension view has the keys and no menu/dialog is up; `rail` a row is picked; `menu` the overlay layer has children; `capture` Feature stage with the note open; `diagram` an open feature with its diagram expanded; `outline` an open feature; `features`/`checkouts`/`code`/`extensionPage` that stage up; `files` Checkouts stage with the files panel holding the keys; `stage` always. (`ts:keys/keymap.ts:280-381`)
- SHELL-KEYS-12. A layer with no `places` (`field`, `extensionDialog`, `settings`, `addProject`'s own handler) answers its own keys; the dispatcher holds the key and does nothing, so typing into a field never fires an action. (`ts:keys/keymap.ts:386-388`; node `a layer that answers its own keys is up like any other`)
- SHELL-KEYS-13. Opening the palette, the shortcuts sheet or the settings closes the other two first: only one of the three is ever open. `?` while the sheet is open closes it (toggle); the palette and settings open idempotently. (`ts:keys/keymap.ts:168-182`)
- SHELL-KEYS-14. A dialog layer holds **every** key while it is open: it runs the binding it answers (via `sink`), opens the sheet for `?`, gives its filter field the keys for `filter`, and ignores the rest, but it never lets a key reach the stage under it. The window-level actions `NOT_THEIRS` (`workspace`, `feature`, `checkouts`, `compose`, `console`, `toggleConsole`, `views`, `palette`, `settings`) are not a dialog's: they are swallowed, not run. (`ts:keys/keymap.ts:208-219,386-406`)
- SHELL-KEYS-15. A menu opened from a dialog (the sessions filter) is drawn over it. While a `takesFromPane` dialog is up, every key goes to the menu's handler, which acts only on closeLayer and swallows the rest; the menu's own capturing handler also takes Up/Down/j/k. (`ts:keys/keymap.ts:392-394,322-327`; `ts:kit/menu.ts:115-130`)
- SHELL-KEYS-16. `rail` layer (a row picked): the key is looked up in [anywhere, rail]; the rail's action runs (SHELL-RAIL-20..30) or else the stage's `run`; if neither, no action: an extension command heard in `anywhere`/`checkout` (and `pane` when a pane row is picked). The layer always holds the key. (`ts:keys/keymap.ts:242-250`)
- SHELL-KEYS-17. `stage` layers (`features`, `checkouts`, `code`, `extensionPage`, `outline`, `diagram`, `capture`, `stage`): the action runs through `run` and is `preventDefault`ed when taken; with no action, an extension page's own one-character action key, else an extension command heard in `view`/`anywhere` (or `checkout` on the Checkouts stage). The layer always holds the key. (`ts:keys/keymap.ts:253-272`)
- SHELL-KEYS-18. `run(action)` does nothing (returns false) for any action but `closeLayer` while the overlay layer holds a menu or question. Otherwise: `palette`/`shortcuts`/`settings` open (SHELL-KEYS-13); `workspace`/`feature` `show_features{open:false|true}`; `checkouts` `show_checkouts{open:true}`; `code` `show_code{open:true}`; `compose` `show_composer{open:true}`; `console` `console{on:"tab"}`; `toggleConsole` `console{on:"toggle"}`; `consoleFullscreen` `console{on:"fullscreen"}`; `newConsole` `console{on:"spawn"}`; `extensionPage`/`extensionDock` `extension_mode{panel,page}` for the focused or paged extension view, else `command{action}`; `views` `toggle_views{}`; anything else goes to the stage's own action table (Checkouts, Code, extension page's `next/previous/first/last/choose`, the open feature's, else the Feature index's). (`ts:keys/keymap.ts:100-163`)
- SHELL-KEYS-19. **Escape ladder.** `closeLayer` (Escape in `anywhere`) asks each layer in `LAYERS` order for an `escape` hook, whether or not that layer is up, and stops at the first that returns true. The hooks that exist: `menu` (answers a Checkouts question no; answers the project-removal question no; answers an extension question no; closes the project switcher; else clears the overlay layer), `capture` (discard the note), `outline` (back to the index), `files` (put the files panel away if it has the keys), `code` (back to the review/history it came from or the stage before). A question outlives the menu drawn over it and is still answered no. (`ts:keys/keymap.ts:322-379`, `ts:keys/focus.ts:49-51`; node `escape puts away the first thing in the order, up or not`)
- SHELL-KEYS-20. A text field answers Enter/Ctrl-Enter/Escape itself through the `writing` place (`Enter` choose, `Ctrl Enter` or `Meta Enter` save, `Escape` closeLayer) and the font picker and palette field through the `palette` place (`Down`, `Up`, `Enter`, `Escape`, no `j`/`k`). (`rs:keys/keymap.rs:626-628,732-735`; `ts:chrome/projects.ts:417`, `ts:panes/rename.ts:98`, `ts:settings/font_picker.ts:142`)
- SHELL-KEYS-21. Extension commands: a chord in an extension's `commands` runs `extension_command{extension, command, checkout, pane}` when the key is heard in one of its scopes (`anywhere`, `view`, `checkout`, `pane`); a `view` scope also needs its view to be the focused or paged one; after the leader in a pane they are reached through `session.extension_keys`. The daemon refuses any extension key the keymap holds, so none shadows the window's. (`ts:keys/keymap.ts:67-92`; test `every_key_the_window_keeps_is_one_the_daemon_refuses_an_extension`)

### Behaviours: how a key is written

- SHELL-KEYS-22. A press is written `Ctrl Alt Shift Meta key` in that order, dropping absent modifiers; `Alt` is `⌥` and `Meta` is `⌘` on macOS; a single character with a modifier is upper-cased (`Ctrl K`), alone it is as typed (`j`, `A`); names: `" "` `Space`, arrows `↑ ↓ ← →`, `Escape` `Esc`, `PageUp` `PgUp`, `PageDown` `PgDn`. A chord after the leader is two presses, the leader first; the leader is written from the pane group's `leader` binding, `Ctrl Space` if it has none. A binding's alternatives are caps joined by `/`. (`ts:keys/chords.ts:24-60`)
- SHELL-KEYS-23. A layer holds only the keys it binds; a key it does not bind goes on to the next layer. With the files panel holding the keys (`files` layer), a key with no Files binding goes on to the `checkouts` layer. An extension view that holds the keys passes on a key it does not use, to the next layer up (rail, menu, stage and so on). So SHELL-KEYS-16 and -17's "always holds the key" means the layer's own bindings, not every key: a porter must not assume every layer holds every key. (`ts:keys/keymap.ts:359-369` (`files.keys` returns false when there is no action), `ts:keys/keymap.ts:227-240` (`extensionKeys` returns false and falls through), `ts:keys/focus.ts:20-22`; `rs:keys/keymap.rs:851-852,879-881`)

### Page-held state

None beyond the focused target. (`railPick`, `start`, `menuOpen` belong to SHELL-RAIL/SWITCH.)

### Page-side logic that must move to Rust

- `matches` and `actionFor` (`ts:keys/keymap.ts:44-65`): the third copy of `Chord::matches` (`rs:keys/keymap.rs:441`); delete the page copy, call the Rust one.
- The whole dispatcher: `handlers` and `hear` (`ts:keys/keymap.ts:280-406`), `run` routing by stage (`:100-163`), `NOT_THEIRS`, `railKeys`/`stageKeys`/`extensionKeys` (`:227-272`), `extensionActionFor`/`extensionCommand` scope choice (`:67-92`). The order is Rust's; the **open predicates and the action targets** are page logic and must become a Rust trait/table the root view implements.
- Key naming (`ts:keys/chords.ts:24-66`): chord to display string, including the leader lookup and macOS glyphs. Tested nowhere; add Rust tests.
- Which section the sheet opens on (`whereNow`, `ts:keys/keymap.ts:191-206`), see SHELL-SHEET.

### gpui-kit mapping

- **Display: `Kbd`** (`kbd.md`). Seen: `Kbd::new(Keystroke::parse("cmd-shift-p").unwrap())`, `.appearance(false)`, `Kbd::binding_for_action(&Action, None, window)`. It formats per platform (macOS symbols), so its text will differ from today's `Ctrl K`; to keep today's text, render the string from SHELL-KEYS-22 in a styled `div`. Our `Chord.key` uses DOM names (`ArrowLeft`, `" "`), `Keystroke::parse` takes `left`, `escape`: a mapping is needed either way.
- **Dispatch: custom.** Skill `references/gpui/action.md` shows `cx.bind_keys([KeyBinding::new("cmd-s", Save, Some(CONTEXT))])`, `.key_context()`, `.on_action()`. `GPUI.md` already says a leader (Ctrl-Space then a key) and the layered "first layer up has it" order do not fit `bind_keys`; the port is a root key interceptor over Rust's `keys::keymap`, with layers as key contexts only where one layer = one focused region (dialogs). Not documented: how to intercept a key before focused children (needed for Window-place keys, SHELL-KEYS-3); spike it (`on_key_down` is shown only inside a modal example in `focus-handle.md`, with no capture-phase variant).
- **Escape ladder: custom.** `Dialog` closes on Escape itself (`dialog.md` `.keyboard(true)`), which must be disabled and replaced by the ladder (SHELL-KEYS-19) or each dialog must route its Escape through it.

### Scenes and coverage

- Scenes: `palette` and `shortcuts` (both reached by `key(" ")` then `key("k")` / `key("?")`), `workspace-*` (a pane with the keys). **Not covered:** every layer's handler (no node test of `hear`, `run`, `railKeys`), the Escape ladder beyond `escape puts away…` (which tests only the walk), SHELL-KEYS-3 (window keys before fields), SHELL-KEYS-14/15, SHELL-KEYS-22 (formatting). Rust covers SHELL-KEYS-1, 5, 6, 7, 8, 10, 21.

---

## PALETTE: command palette

Source: `ts:keys/palette.ts`, `rs:keys/palette.rs`, `rs:keys/keymap.rs:762-811` (`COMMANDS`), `rs:session.rs:2789-2830`. `DESIGN.md §Process model` (palette paragraph).

### Behaviours

- SHELL-PALETTE-1. Opens from `Ctrl K` anywhere, leader `k` or leader `Ctrl K` in a pane, the nav's Search button, or the `palette` command. Opening an already-open palette does nothing. It remembers what had the keys, resets the query to empty, highlights the first result, sends `palette{open:true}`, and focuses the search field. (`ts:keys/palette.ts:48-116`)
- SHELL-PALETTE-2. The field says `Search, or > for commands, # for features`; Escape (palette place `closeLayer`), a press on the backdrop, or choosing a result closes it, sends `palette{open:false}`, and returns the keys to what had them. (`ts:keys/palette.ts:53-79,128-135`)
- SHELL-PALETTE-3. While open, Rust sends what it can go to and re-sends when it changes: every pane of the project (including closed repositories' and folded checkouts'), every checkout of the project, and the features of the repository the Feature stage reads (`featuresLoaded` false until that board has arrived). (`rs:keys/palette.rs:16-86`; tests `every_checkout_and_pane_is_listed_with_where_it_is`, `the_features_are_the_boards_with_how_each_stands`)
- SHELL-PALETTE-4. Results are four groups in this order: Commands, Panes, Features, Checkouts (extension results join Commands). A group's heading appears only if it has rows. (`ts:keys/palette.ts:31,204-239,293-326`)
- SHELL-PALETTE-5. A query is trimmed. A leading `>` limits the search to Commands, a leading `#` to Features; the character is stripped and the rest trimmed. With no needle and no scope each group shows only its first 6 rows in its own order; with a scope and no needle that group shows all; with a needle every matching row shows. (`ts:keys/palette.ts:33,277-298`)
- SHELL-PALETTE-6. Matching is a case-insensitive subsequence of the needle (whitespace removed) in the row text, left to right. Score: +5 for each hit directly after the previous, +3 for a hit at the start or after whitespace or any of `/ # - _ .`, minus 0.1 per index of the first hit, minus 0.01 per character of the text. A row with no match is hidden. With a needle each group is sorted by score descending (ties keep order). A feature row's text is `#<number> <title>`, matched without its `#` and its marked positions shifted back by one only when the query is scoped with a leading `#`; unscoped, it is matched with its `#` included. (`ts:keys/palette.ts:288`, `:303`) (`ts:keys/palette.ts:243-260,277-326`)
- SHELL-PALETTE-7. A row shows: an icon for its kind (per-command icon, default a chevron; panes a terminal; features a list; checkouts a branch; extension results a puzzle piece), its text with matched characters marked, a mono detail (a pane's `repository / checkout`; a checkout's branch), and an aside: for a command, its place's group title (only when the command's place is not Anywhere) then its key caps; for a pane its status word; for a feature its standing word; for a checkout its git summary when it is not clean. Command labels are sentence-cased. (`ts:keys/palette.ts:159-239,304-325`)
- SHELL-PALETTE-8. The highlighted row is the one at index `at`, tracked by its stable key (`command <action>`, `pane <id>`, `feature <slug>`, `checkout <id>`): when Rust re-sends the lists while open the highlight stays on the same key if still present, else clamps to the last row. Typing resets the highlight to the first row. Moving the pointer over a row highlights it. (`ts:keys/palette.ts:37-40,142-152,312-343`)
- SHELL-PALETTE-9. Down/Up (palette place) move the highlight and wrap around; Enter runs the highlighted result: a command `command{action}`; a pane `select{pane}` then the pane text area takes the keys; a feature `go_to_feature{slug}`; a checkout `go_to_checkout{checkout}`. A click on a row does the same (and keeps the keys in the field). (`ts:keys/palette.ts:65-79,154-157,178-238`)
- SHELL-PALETTE-10. The highlighted row also says what Enter does (`Open ↵`) before its aside, is scrolled into view, and is `aria-selected`. (`ts:keys/palette.ts:333-344`)
- SHELL-PALETTE-11. No rows: `No match.`; under `#` before the board arrives: `Reading the features…`. (`ts:keys/palette.ts:327-332`)
- SHELL-PALETTE-12. A command Rust cannot run says so in a notice (`the palette has no command <action>`); `Fill`/`Close` with no pane on screen says `no pane is on screen`; commands for a checkout act on the Checkouts stage's row or else the shown pane's checkout. (`rs:session.rs:2806-2830,2870-2930`; test `a_command_goes_with_where_the_person_is_and_after_the_leader_from_a_pane`)
- **Gap.** The footer advertises `Tab  next group` (`ts:keys/palette.ts:105`) but no handler exists: the key handler answers only `next`, `previous`, `choose`, `closeLayer`, so Tab moves browser focus into the rows. The port must decide: implement it, or drop the hint.
- SHELL-PALETTE-13. A checkout row's text is `<repository> / <name>`, and that is what the search matches. A pane row's detail is the `place` string Rust sends. (`ts:keys/palette.ts:229-236,209-215`)

### Page-held state

`query`, highlighted index and key, the current result list, the previously focused element. (`ts:keys/palette.ts:35-42`)

### Page-side logic that must move to Rust

- Everything in `fuzzy` (`:243-260`), `marked` (`:263-275`), the group ordering, `SHOWN = 6`, `>`/`#` scoping and the sorting (`:277-326`), `COMMAND_ICON` (`:159-176`), and the highlight-by-key rule (`:142-152`). Port as a pure function `rank(query, rows) -> Vec<Ranked{group, key, hits}>` with tests (none today).

### gpui-kit mapping

- **`Command`** (`command.md`). Seen: `Command::new(&state).group(CommandGroup::new().label(..).item(CommandItem::new().label(..).icon(..).action(..).keywords([..])))`, `.on_query`, `.on_select`, `.on_confirm`, `.on_cancel`, `.placeholder`, `.empty(..)`, `.header`, `.footer`, `.searchable(bool)`, `CommandItem::child(..)` for a custom row, `CommandState::{query,set_query,selected_index,matched_count,focus,set_loading}`; ↑/↓ move and wrap and skip disabled; Enter confirms; hosted in `window.open_dialog(.. .bordered(false) ..)`. **Does not fit as is:** local filtering is "a case-insensitive **substring** match against each item's label and keywords" (not our subsequence-with-score), the order is not rankable, matched-character marks are not offered, and Escape **clears a non-empty query first** instead of closing (SHELL-PALETTE-2). `>`/`#` scoping and the 6-row cap are ours. Option: `Command::new(..).searchable(false)` (which "shows or hides the search field and local filtering") with an `Input` in `header(..)` and Rust-ranked `items(..)`; `on_confirm(IndexPath)` indexes the supplied model, not the filtered view. Not documented: whether `header` can hold a focused `Input` that keeps the arrow keys; spike it.
- **Alternative: `List`** (`list.md`): `ListDelegate::perform_search(query, ..) -> Task<()>` lets Rust rank, `render_item` can draw marks, `ListState::searchable(true)`; sections with zero items are hidden; `render_section_header` for group headings.

### Scenes and coverage

- Scene `palette` (opens it empty; the scene's `palette{open:true}` makes Rust send the lists). **Not covered:** scoped queries, ranking, highlight tracking, wrap, Enter targets (no test of `fuzzy` or `drawResults`; only `rs:keys/palette.rs` tests data). Rust test of the data only.

---

## SHEET: shortcuts sheet

Source: `ts:keys/shortcuts.ts`, `ts:keys/sections.ts`, `ts:keys/keymap.ts:168-206`. Node tests `keys/sections.test.mjs`: `every binding of the keymap is on a row of its group, merged pairs apart`, `the pane section says the leader once, in no row that follows it`, `next and previous, and the arrows, are a row each in the pane`, `a pair of keys on one binding stays one row`, `the filter finds a row by its action or its key, grouped by section`, `the sheet opens on the section for where the person is`. Rust: `every_bound_key_appears_in_the_sheet`, `a_group_crosses_to_the_page_with_its_chords_and_labels`, `the_leader_and_question_mark_ask_the_page_for_the_sheet` (`rs:session.rs:5626`).

### Behaviours

- SHELL-SHEET-1. Opens from `?` outside a field, from a dialog (`?` is the dialog's `shortcuts`), and from leader `?` in a pane; `?` again closes it. It takes the keys: the pane text area is blurred and the close button is focused without a focus ring; closing returns focus to what had it. (`ts:keys/shortcuts.ts:93-232,236-241`, `ts:keys/keymap.ts:168-182`)
- SHELL-SHEET-2. Sections: one per keymap group in `ORDER` (25) then one per extension keymap group (`ext/<title>`). Left: a find field and a list of section buttons (the current one marked, the one for where the person is tagged `here`); right: the current section. (`ts:keys/shortcuts.ts:24-34,118-151`)
- SHELL-SHEET-3. It opens on the first existing section among: the open dialog (`review`/`history`/`sessions`), `pane` if the keys are in the pane's text area, `rail` if a row is picked, `diagram`, `outline`, the stage (`feature`/`checkouts`/`code`), else `anywhere`. (`ts:keys/sections.ts:176-191`, `ts:keys/keymap.ts:191-206`; node `the sheet opens on the section for where the person is`)
- SHELL-SHEET-4. A section lists its bindings, each a row: label sentence-cased and its caps; with more than two alternative chords only the first cap shows and the whole list is the tooltip. (`ts:keys/shortcuts.ts:38-59`)
- SHELL-SHEET-5. The pane section says once `Keys go to the program. For Argus, press <leader> then one key:`, lists the leader-chorded bindings without their leader in two columns, then a `Without <leader>` block (paste, paging, the leader sent on) whose caps keep their presses. The leader's own binding keeps both presses. (`ts:keys/sections.ts:113-141`, `ts:keys/shortcuts.ts:62-87`; node `the pane section says the leader once…`)
- SHELL-SHEET-6. Merged rows: `otherSide`+`previousSide` is `the keys to the next / previous pane on screen` (their chords); `focusLeft/Right/Up/Down` is `the keys to the pane that way` and `movePaneLeft/Right/Up/Down` is `move the pane that way, trading places`, each with one cap `← → ↑ ↓` or `<modifier> + arrow` when all four share a modifier prefix; if they do not share one the merge is skipped and the rows stay separate. (`ts:keys/sections.ts:44-111`; node `next and previous, and the arrows, are a row each in the pane`)
- SHELL-SHEET-7. Typing in the find field filters: split on whitespace, a row stays when **every** word is in its label or key text (lower-case); sections with no row left disappear; while filtering the right side shows every matching section with its title and the left list marks none as current; none matching says `No key or action matches.`. (`ts:keys/sections.ts:143-157`, `ts:keys/shortcuts.ts:118-151`; node `the filter finds a row by its action or its key…`)
- SHELL-SHEET-8. Clicking a section clears the filter, makes it current, redraws, and gives the keys back to the current button without a ring. Every redraw scrolls the right side to the top and the current button into view. (`ts:keys/shortcuts.ts:118-151`)
- SHELL-SHEET-9. With an empty find field, Down/Up (the `sheet` place and `anywhere`) change section, clamped at the ends (no wrap), from the field or the list; Escape closes and the key goes no further. The sheet asks for its own keys because its field swallows keys before the window hears them. (`ts:keys/shortcuts.ts:156-172`, `ts:keys/keymap.ts:186`)
- SHELL-SHEET-10. A press on the backdrop closes it; the footer reads `? opens on the section for where you are · ↑ ↓ change section · Esc close`. (`ts:keys/shortcuts.ts:200-232`)
- SHELL-SHEET-11. The sheet is generated from the keymap: every binding appears under its place's heading. (`rs:keys/keymap.rs` test `every_bound_key_appears_in_the_sheet`; node `every binding of the keymap is on a row of its group…`)
- SHELL-SHEET-12. The sheet's title is `Keyboard shortcuts`; its find field's placeholder is `Find a key or an action`. (`ts:keys/shortcuts.ts:108,200`)
- SHELL-SHEET-13. `?` typed while the find field has focus is typed into the field: the field layer holds it (field layer first, `ts:keys/keymap.ts:282-285`). It does not close the sheet. "Toggle" in SHELL-SHEET-1 applies only while focus is outside the field, for example on the close button where the sheet opens. (`ts:keys/shortcuts.ts:156-172` handles only closeLayer, next and previous)

### Page-held state

The open sheet's current section, query, the previously focused element. (`ts:keys/shortcuts.ts:14,93-232`)

### Page-side logic that must move to Rust

- `sections.ts` whole (`MERGES`, `arrowCap`, `keysOf`, `rowsOf`, `sections`, `matching`, `sectionFor`; 191 lines) is pure and tested under node; port it to Rust unchanged with the six node tests converted. Chord naming from SHELL-KEYS-22 is its dependency.

### gpui-kit mapping

- A **centered** dialog today, not an edge panel: `Dialog` (`dialog.md`: `window.open_dialog(cx, |dialog,_,_| dialog.title(..).child(..))`, `.keyboard(bool)`, `.overlay_closable(bool)`, `close_dialog`) is the closer fit; `GPUI.md` lists `Sheet` (`sheet.md`: `window.open_sheet_at(Placement::Right, cx, ..)` (unconfirmed), `.size`, `.resizable`, `.overlay`), which slides from an edge. Choose in the architecture phase; behaviours are identical.
- Section list: `List` (`list.md`) or `Sidebar` (`sidebar.md`: `SidebarMenuItem::new(..).active(bool).on_click`); find field: `Input` (`input.md`: `InputState`, `InputEvent::Change`); caps: `Kbd` or SHELL-KEYS-22's string; "here" tag: `Badge`/`Tag` (`tag.md`).

### Scenes and coverage

- Scene `shortcuts` (leader `?`). Node tests cover the generated content. **Not covered:** find/section interaction, focus return, SHELL-SHEET-9, SHELL-SHEET-1's focus.

---

## SCREEN: terminal screen

Source: `ts:panes/screen.ts` (530 lines), `ts:panes/contrast.ts`, `ts:panes/size_label.ts`, `rs:panes/screen.rs`, `rs:panes/shown/mod.rs`. `DESKTOP.md §panes/screen`, `DESIGN.md §Process model` (pane paragraph). Tests: `rs:panes/screen.rs` (`a_snapshot_sends_every_row_then_damage_only_the_rows_it_touched`, `a_scroll_redraws_its_region`, `a_wide_character_is_flagged_and_its_spacer_is_not_sent`, `trailing_blanks_are_trimmed_but_coloured_ones_are_kept`, `a_cursor_that_only_moved_sends_a_frame_with_no_lines`, `nothing_changed_is_no_frame`, `a_fresh_grid_replaces_the_dimensions`, `a_refresh_sends_every_row_again`, `a_backward_drag_copies_in_reading_order_without_trailing_blanks`, `a_wide_character_is_copied_once`, `a_parked_view_draws_its_history_and_holds_still_under_live_output`, `a_parked_view_survives_a_screen_that_grew`), node `panes/size_label.test.mjs`.

### Behaviours

- SHELL-SCREEN-1. There is one screen per **side** Rust draws on: the Workspace, the console's `first` and `second`, each `beside(n)` split, and each extension console `view(n)`. A frame names its side; one for a side the view has no screen for is dropped and Rust sends the screen whole when the side is bound. (`ts:panes/screen.ts:449-530`, `rs:connection/page.rs` `Side`)
- SHELL-SCREEN-2. A cell is `13px` mono wide by a fixed `20px` high; the width is measured from 100 `M`s in the chosen mono family; the glyph baseline is centred in the 20 px line. The canvas is exactly `cols x cell.w` by `rows x cell.h`, device-pixel sharp, and draws only the grid Rust sent (a pane smaller than its room is not padded with fake rows). (`ts:panes/screen.ts:21-35,95-107,131-143`)
- SHELL-SCREEN-3. A frame carries `{pane, side, rows, cols, full, cursor, alternateScreen, mouse, lines, parked}`; `lines` are `[row, segments]`, each row cleared then redrawn; a segment is `[col, cells[], fg, bg, flags]` where `cells` is an array because a cell may hold combining marks, and trailing default-style blanks are not sent. (`rs:connection/page.rs:1442-1460`, `rs:panes/screen.rs:take_frame`; test `trailing_blanks_are_trimmed_but_coloured_ones_are_kept`)
- SHELL-SCREEN-4. A frame for a different pane than the screen holds, or with `full`, resets the held rows; a different pane also clears the selection. A changed `rows`/`cols` redraws every row; otherwise only the rows listed. The cursor is redrawn every frame. (`ts:panes/screen.ts:241-280`)
- SHELL-SCREEN-5. Colours arrive packed: `0` default, `1<<24 | n` indexed, `2<<24 | rgb`. Indexed `0..15` come from the theme's ANSI palette (`--ansi-0..15`), `16..231` are the 6x6x6 cube (`0` or `55 + 40c` per channel), `232..255` greys `8 + 10(n-232)`. Default foreground is the theme foreground; default background is not painted. `REVERSE` swaps them (default background is the theme's sunken colour). (`ts:panes/screen.ts:45-48,109-129,148-153`)
- SHELL-SCREEN-6. When a cell has an explicit background, the foreground is moved toward black or white (whichever contrasts the background more) in 10 % steps until WCAG contrast reaches **4.5**, so a diff's coloured background keeps readable text; colours it cannot parse are left; results are cached (cleared past 2000). (`ts:panes/contrast.ts:28-80`)
- SHELL-SCREEN-7. Flags: bold draws weight 700, italic italic, underline a 1 px rule 3 px above the cell's bottom across the segment, **wide** (flag 16) draws one glyph centred over two cells with the background two cells wide, reverse as above. Blank characters are not drawn. (`ts:panes/screen.ts:30-35,145-174`)
- SHELL-SCREEN-8. Cursor shapes: `bar` 2 px wide, `underline` 2 px tall at the cell's foot, `block` filled with the glyph under it redrawn in the background colour when this side has the keys **and** the window has focus **and** the pane text area is focused, else a 1 px **hollow** outline. Hidden when `visible` is false or off the grid. The side without the keys draws hollow (`keyed`). A parked view draws no cursor (Rust sends it invisible). (`ts:panes/screen.ts:204-225,419-424`, `rs:panes/screen.rs:take_frame`)
- SHELL-SCREEN-9. A selection is drawn as the foreground colour at 30 % alpha: start/end normalised to reading order, the first row from its start column, the last to its end column inclusive, whole rows between; it is drawn only, the copied text is read from Rust's grid. (`ts:panes/screen.ts:186-202`; SHELL-INPUT-8)
- SHELL-SCREEN-10. The Workspace header names the grid: `cols x rows` shown for 1000 ms after a size change that was not a pane switch and not the first grid, then at rest empty; a parked view leads with `↑ offset/depth` and keeps it at rest; no grid yet says nothing. The label is recomputed 20 ms after the window ends. (`ts:panes/screen.ts:296-307`, `ts:panes/size_label.ts:13-20`; node `the size is silent at rest and shows just after a resize`, `a parked view keeps its place at rest and leads the size while it shows`, `a pane with no grid yet says nothing`)
- SHELL-SCREEN-11. A side reports its room to Rust: 50 ms after any change of size, visibility, scale, font or binding, as `resize{side, rows, cols}` with `rows = max(1, floor(height / cell.h))`, `cols = max(1, floor(width / cell.w))` of the area inside its padding, only when different from the last one sent. It reports nothing while hidden or zero-sized (measuring would size the pane to one cell), and **retries every 50 ms while an edge or gutter is being dragged**, so a pane is resized once per drag. (`ts:panes/screen.ts:309-342`; test `rs:panes/shown/mod.rs` `a_resize_is_sent_only_when_the_size_changes`)
- SHELL-SCREEN-12. When a different pane is about to be shown (`Rail.shown` changed, or a console tab chosen) and no frame arrives within **250 ms**, bars stand in for it (7 bars, widths 55, 80, 70, 85, 40, 75, 60 % of the row); the first frame or a null expectation removes them. A live pane's first screen comes well inside the delay so it never flashes. (`ts:panes/screen.ts:24-26,233-239`)
- SHELL-SCREEN-13. Metrics are redone and the grid re-reported when: the window loads, the mono font family finishes loading (four faces: regular, bold, italic, bold italic, awaited), the device pixel ratio changes (another monitor, a zoom), or the settings change the family. A theme change (or the system's light/dark when following it) re-reads the palette and repaints without re-reporting unless the family changed. (`ts:panes/screen.ts:344-402`)
- SHELL-SCREEN-14. Rebinding a side to another `Side` forgets the last-sent size and reports again; binding to none drops its pane, rows and selection. A split cell that returns to screen is bound again (null then its side) because Rust forgot its size when the pane left. (`ts:panes/screen.ts:404-417`, `ts:panes/split.ts:155-161`)
- SHELL-SCREEN-15. The daemon sizes a pane to the client the person was last at, so the grid that comes back may differ from the room reported; the screen draws the grid it is sent and the room around it stays empty. (`DESKTOP.md §session` `select`; `rs:panes/shown/mod.rs` `resize`)
- SHELL-SCREEN-16. A mouse-aware child: `mouse` in the frame says the child asked for the mouse reports; the screen then treats a press as the child's and only Shift-drag selects. A parked view never takes the mouse. (`ts:panes/screen.ts:389-393`)
- SHELL-SCREEN-17. Row model on the Rust side: a view that is parked in history sends only its own rows and only when they change; a `refresh` sends every row again; the view is the same `argus_protocol::panes::view::PaneView` the terminal client keeps. (`rs:panes/screen.rs:24-170`)

### Page-held state

The mirror of the rows (so a theme or scale change can repaint), cell metrics, palette, selection, parked, size-visible deadline, last-sent room, skeleton timer. (`ts:panes/screen.ts:77-93`)

### Page-side logic that must move to Rust

- Colour resolution (`colour`, `step`, the packed format; `:45-129`), the contrast fix (`contrast.ts` whole, 60 lines), flags decoding, `drawRow`/`glyphAt` layout rules, selection normalisation, cursor geometry, `measure()` and the rows/cols from pixels formula (`:95-107,313-342`), the 50 ms / 250 ms / 1000 ms timers, `size_label`. In GPUI the terminal element owns rendering; **Rust keeps the segment-to-glyph-run translation** and can drop the packed-colour wire format once the session feeds the element directly (`GPUI.md` step 2).
- A new Rust unit test target: colour index to RGB and `readable()` have no Rust copy today; port `contrast.ts` with property tests.

### gpui-kit mapping

- **Custom Element, no kit component.** `GPUI.md`: custom `Element` with Zed's `terminal_element` as the model (cell metrics, wide cells, cursor, selection, contrast fix, theme repaint, resize report, large frames). Skill `references/gpui/element.md` + `element-api.md` (`request_layout`, `prepaint`, `paint`). `Skeleton` for SHELL-SCREEN-12 (`skeleton.md`: `Skeleton::new().w(px(250.)).h_4()`). Test: `render_to_image` for painting; geometry through `window.find(id).bounds()`; resize through `simulate_window_resize` / `simulate_window_scale_factor_change` (`test.md`).

### Scenes and coverage

- Scenes: `workspace-agent`, `workspace-shell` (wide/colour content), `workspace-waiting`, `workspace-failed`, `workspace-exited`, `workspace-editor`, `workspace-split`, `workspace-console`. **Not covered by a scene or test:** contrast fix (SHELL-SCREEN-6, no test), cursor states (hollow vs filled), selection draw, skeleton (SHELL-SCREEN-12), DPR/font reflow (SHELL-SCREEN-13), parked-view header (only the label function is tested).

---

## INPUT: terminal input and mouse

Source: `ts:panes/input.ts` (171 lines), `rs:keys/page_key.rs`, `rs:session.rs:1082-1236,1355`. `DESIGN.md §Process model` (wheel and mouse paragraph), `§Panes and terminal state` (clipboard). Tests: `rs:keys/page_key.rs` (see SHELL-KEYS-7), `rs:session.rs` `keys_text_and_pastes_go_to_the_shown_pane_and_nowhere_without_one`, `a_parked_view_*` in `rs:panes/screen.rs`.

### Behaviours

- SHELL-INPUT-1. The keys go to one hidden, focusable text area that sits inside whichever screen has them; moving the keys moves the text area into that screen's container without losing its focus. (`ts:panes/console.ts:260-277`, `index.html` `#input`)
- SHELL-INPUT-2. A key press is skipped while an input method is composing (`isComposing` or key code 229), skipped when it is a bare modifier (`Shift`, `Control`, `Alt`, `Meta`, `AltGraph`, `CapsLock`), skipped on macOS when `Meta` is held and the Pane place does not bind it (window chords stay the window's), and otherwise **always** prevented (no webview accelerator such as F5, Ctrl-F, Ctrl-R may fire) and sent as `key{key:{key, code, ctrl, alt, shift, meta}}`. Every key press also clears every screen's selection. (`ts:panes/input.ts:27-50`)
- SHELL-INPUT-3. Composition: when composition ends, the committed text is sent as `text{text}` and the area is emptied; any non-composing `input` event empties the area. There is no preedit display: while composing nothing is drawn. **Gap:** a GPUI `EntityInputHandler` should at least match this and may show the marked text. (`ts:panes/input.ts:52-59`)
- SHELL-INPUT-4. A `paste` event (not the paste chord, which Rust handles, SHELL-KEYS-8) is prevented and its plain text sent as `paste{text}`; no text sends nothing. (`ts:panes/input.ts:61-65`)
- SHELL-INPUT-5. A press with any mouse button on a screen focuses the text area (and prevents the browser default); if that screen does not have the keys and holds a pane, gives it them: the Workspace's screen `select{pane}`, a split or view screen `focus_view{pane}`, a console side `console{on:"select", pane}`; clears every other screen's selection. Only the start of a selection (SHELL-INPUT-7) is limited to the left button. (`ts:panes/input.ts:90-108`)
- SHELL-INPUT-6. If the child asked for the mouse (frame `mouse`, not parked) and Shift is not held, the press is the child's: `mouse{gesture:"down", button, at:[row,col]}` for any button; drags (button held, cell changed) send `mouse{gesture:"drag"}` and the release `mouse{gesture:"up"}` for the same button at the cell under the pointer. A move with no button sends `mouse{gesture:"move", button:0}` only on the screen with the keys, only when the cell changed. Rust encodes in the mode the child asked for. (`ts:panes/input.ts:74-161`, `rs:session.rs:1190-1217`)
- SHELL-INPUT-7. Otherwise a left press starts a selection at the cell under the pointer (a non-left press does nothing); dragging selects to the cell under the pointer, clamped inside the grid so a drag past an edge selects to the edge; the selection only shows once the pointer has left the anchor cell. (`ts:panes/input.ts:108-148`, `ts:panes/screen.ts:282-290`)
- SHELL-INPUT-8. Releasing after a real drag sends `copy{from:[row,col], to:[row,col]}`; Rust reads the text between the two cells (reading order, each row's trailing blanks dropped, rows joined by newlines, a wide character once) and writes it to the clipboard; an empty selection writes nothing; a failure says `could not copy: <err>`. A press and release without a drag copies nothing and leaves no selection. (`ts:panes/input.ts:150-161`, `rs:connection/commands.rs:115-125`; tests `a_backward_drag_copies_in_reading_order_without_trailing_blanks`, `a_wide_character_is_copied_once`)
- SHELL-INPUT-9. A right press over a screen opens no menu; it still does the first part of SHELL-INPUT-5, focusing the text area and handing over the keys. (`ts:panes/input.ts:90-113`)
- SHELL-INPUT-10. **Wheel**: the page prevents scrolling, accumulates `deltaY / 100` for pixel deltas (physical notch = 100 px) or `deltaY / 3` for line deltas, sends the whole notches only (`trunc`, the fraction carried), as `wheel{pane, notches, at:[row,col]}` for the pane **under the pointer**, whichever side has the keys, negative notches up. Rust decides what a notch is: a child that asked for the mouse gets wheel reports, the normal screen parks the view in history, a full-screen child that asked for nothing gets a cursor key per notch; typing or a press returns a parked view to live. (`ts:panes/input.ts:119-136`, `rs:session.rs:1182-1191`, `DESIGN.md §Process model`)
- SHELL-INPUT-11. The window's focus and blur are reported as `focus{focused}`; Rust reports its view to the daemon only while focused, so a window in the background never takes the focus from a terminal client being typed in. Focus or blur of the text area redraws every cursor (SHELL-SCREEN-8). (`ts:panes/input.ts:67-68,163-170`, `rs:session.rs:1355-1359`)
- SHELL-INPUT-12. The key table, leader and Rust dispatch are in SHELL-KEYS-5..9; typing returns a parked view to live. (`rs:session.rs:1219-1226`)
- SHELL-INPUT-13. A selection drawn by a drag stays on screen after its release copies it. It is cleared only by the next key press in the pane, a press on any screen, or a different pane's frame. The copy itself reads Rust's grid, so the drawn selection is not what gets copied. (`ts:panes/input.ts:150-161` (mouseup sends `copy` and clears only the anchor), `ts:panes/input.ts:34,100,111`; `ts:panes/screen.ts:250`)

### Page-held state

Active screen for the drag, selection anchor, whether it moved, held button, last reported cell, wheel remainder. (`ts:panes/input.ts:74-78,122`)

### Page-side logic that must move to Rust

- Notch accumulation and the 100 px / 3 line constants (`:129-132`), cell-under-pointer with clamping (`ts:panes/screen.ts:284-290`), the select-versus-report decision (`takesMouse && !shift`), de-duplicating moves by cell, the focus-to-side routing of a press (`:94-99`). A Rust `fn mouse_press(side, cell, shift) -> Report | Select` and a `WheelAccumulator` can be unit-tested.

### gpui-kit mapping

- **Custom `EntityInputHandler`** (`GPUI.md`; skill `test.md` §"Exercise text composition": `gpui_kit::ElementInputHandler::new(bounds, entity)`, `TestInput::commit_text(text, cx)`, `compose_text(replacement, text, selection, cx)`, `marked_text_range`, `selected_text_range`, `unmark_text`, ranges in UTF-16). Mouse and wheel: GPUI element mouse events (`on_scroll_wheel`, `MouseButton` as in `test.md`). Not documented in kit: Shift-held query on a press (core `MouseDownEvent.modifiers`, not in the kit docs); check in the spike. No kit component fits.
- Paste: `Input`/`Textarea::on_paste(|item,_,cx| ..)` shows `ClipboardEntry::Image` and `ClipboardEntry::ExternalPaths` (`input.md` "Paste Hook") for the composer; the terminal's paste is Rust's today (`arboard`, `GPUI.md` "clipboard to `ClipboardItem` or stays on `arboard`").

### Scenes and coverage

- Scenes: any `workspace-*`. **Not covered:** all pointer and wheel behaviours (SHELL-INPUT-5..10), IME (SHELL-INPUT-3), focus reporting; Rust covers key bytes, paste and copy text only.

---

## GRID: split grid

Source: `ts:panes/split.ts` (420 lines), `ts:panes/split_math.ts` (109 lines), `rs:panes/shown/workspace_grid/{mod.rs,split_grid.rs}`, `rs:panes/shown/mod.rs`. `DESKTOP.md §Panes side by side`. Tests: node `panes/split_math.test.mjs` (`rectangles tile the grid row by row from the heights and widths`, `dragging a gutter moves the edge between its two panes and leaves the rest`, `a drag stops at the minimum of the pane that is shrinking`, `a pair with no room for two minimums does not move`, `sizes become thousandths that sum to exactly one thousand`, `a header over the middle of a pane swaps; near an edge it goes there`, `the lit zone is the half it would take, or the whole pane to swap`, `the keys on a split name its screen, and on the Workspace's or off the grid none`); Rust `split_grid.rs` (`new_panes_fill_rows_of_ceil_sqrt_top_first`, `adding_evens_only_the_row_joined_or_the_heights_for_a_new_row`, `a_short_last_row_stretches_its_panes_across_the_width`, `closing_gives_the_width_to_the_left_neighbour_or_the_right_for_the_first`, `closing_the_last_of_a_row_drops_the_row_and_its_height_goes_to_a_neighbour_row`, `dropping_on_the_centre_swaps_and_keeps_the_sizes_of_the_places`, `dropping_on_a_side_joins_that_row_beside_the_target_and_evens_it`, `dropping_on_an_edge_makes_a_row_of_its_own_taking_half_the_target_rows_height`, `dropping_on_itself_or_a_pane_that_is_not_there_changes_nothing`, `neighbours_are_in_the_row_or_the_nearest_in_the_next`, `reading_order_is_left_to_right_then_down`, `dragged_sizes_must_keep_the_shape_and_are_normalised`, `evening_resets_a_row_or_the_rows_alone`, `a_pane_is_refused_when_it_would_leave_one_below_the_least`), `workspace_grid/mod.rs` (`a_pane_joins_by_the_grid_rule_at_the_room_of_the_cell_before_it`, `a_grid_refuses_a_pane_that_would_leave_one_smaller_than_the_least`, `the_page_sends_sizes_in_thousandths`, `dragged_sizes_that_do_not_fit_the_grid_change_nothing`, `an_emptied_cell_goes_and_the_cells_after_it_number_down_with_the_keys`), `shown/mod.rs` (`the_keys_step_along_the_row_and_wrap`, `the_focused_pane_trades_places_with_its_neighbour_and_the_keys_move_spatially`, `closing_a_split_keeps_the_others_shifts_them_down_and_hands_the_keys_to_its_neighbour`).

### Behaviours

- SHELL-GRID-1. The grid is what `Rail.row` carries: `beside[]` (panes split beside the first, `None` for one the tree no longer names, its cell keeping its place without a header), `grid` (cell numbers in each row; `0` is the Workspace's pane, `n+1` is `Beside(n)`), `heights` (each row, thousandths), `widths` (each pane in its row, thousandths), `keyed` (cell with the keys) and `minCols 20` / `minRows 6`. (`rs:panes/shown/workspace_grid/mod.rs:26-45`, `ts:panes/split.ts:37-62`)
- SHELL-GRID-2. A pane's rectangle is its row's top `y = sum of earlier heights`, its `x = sum of earlier widths in its row`, `h = height/1000`, `w = width/1000` as fractions of the grid. The grid fills the stage body. (`ts:panes/split_math.ts:19-103`)
- SHELL-GRID-3. With two or more panes every cell draws a header: title, a status dot, and a close `✕` titled `Take out of the grid` (cell 0 keeps that room empty); the cell with the keys has its header and cell marked. With one pane no cell has a header, and there is no gutter. (`ts:panes/split.ts:122-137,166-173`)
- SHELL-GRID-4. The Workspace is shown (`wanted = 1 + beside.len`) only while a pane is shown; with none the grid is hidden. Cells are made once per slot and kept, so a pane closed and another opened reuse a terminal; each non-first cell owns a `Beside(n)` screen. (`ts:panes/split.ts:51-79,143-180`; `ts:panes/screen.ts:478-498`)
- SHELL-GRID-5. A pane is added by the **grid rule**: it joins the first row with fewer than `ceil(sqrt(n))` panes else starts a row, so an untouched grid fills 2, 2 over 1, 2 over 2, 3 over 2, 3 over 3; only the joined row is made even (or the row heights when a row starts); every other size stays as dragged. (`rs:panes/shown/workspace_grid/split_grid.rs:102`; test `new_panes_fill_rows_of_ceil_sqrt_top_first`)
- SHELL-GRID-6. A pane is **refused** with a notice when: the Workspace has no pane of its own (`show a pane in the Workspace first, then open another beside it`), another pane would leave one below 20 columns by 6 rows by the sizes the view last reported (`no room for another pane: each needs 20 columns by 6 rows. Close one first`), the grid already holds 255 cells (`the grid holds as many panes as it can: close one first`), or the pane is on the console (`that pane is open in the console: close it there first`). A grid not yet measured has room. Nothing moves on a refusal. (`rs:panes/shown/workspace_grid/mod.rs:58-66,261`; tests `a_grid_refuses_a_pane_that_would_leave_one_smaller_than_the_least`, `a_pane_on_the_console_is_refused_and_nothing_is_subscribed`)
- SHELL-GRID-7. **Gutters.** A vertical gutter sits after every pane but the last in a row (at its right edge, spanning the row); a horizontal one sits under each row that has a row below it (at that row's `y`). They are separators (`Resize the panes`, `Drag to resize`). (`ts:panes/split.ts:182-219`)
- SHELL-GRID-8. A left press on a gutter starts a drag: sizes at the start are `share x length of the grid` in pixels; the least a pane may be is `minCols x cell.w + padding` across or `minRows x cell.h + padding + 28` down; only the **two panes either side** change, neither goes below its least, and a pair with room for less than two minimums does not move. Sizes are then turned into whole thousandths summing to exactly 1000, the rounding left going to the largest remainders. (`ts:panes/split.ts:221-305`, `ts:panes/split_math.ts:19-103`; node tests above)
- SHELL-GRID-9. While a gutter is held the cells move at most once a frame, the page does not redraw from Rust (`busy`), the body is marked `resizing` so panes wait to report their size, and on release the new sizes are sent **once** as `set_shares{heights, widths}`. The page then **holds** those sizes until Rust echoes the same sizes back or 600 ms pass (Rust declines a wrong-shaped set silently), after which it draws Rust's. (`ts:panes/split.ts:37-62,238-305`; tests `dragged_sizes_that_do_not_fit_the_grid_change_nothing`, `dragged_sizes_must_keep_the_shape_and_are_normalised`)
- SHELL-GRID-10. Double-clicking a vertical gutter sends `even_shares{row}` (that row's panes made the same width); a horizontal gutter sends `even_shares{row:null}` (the rows made the same height). (`ts:panes/split.ts:239`; test `evening_resets_a_row_or_the_rows_alone`)
- SHELL-GRID-11. **Header drag.** A left press on a header starts moving the pane after 4 px of travel (a shorter press is nothing). While held, the cell under the pointer other than its own lights the zone the drop would take; over the stage body but over no cell, the body is marked as a stage drop. Escape (captured, swallowed) cancels. Letting go over a cell sends `move_pane{pane, onto, zone}`; elsewhere nothing is sent and the grid is redrawn. The `✕` on a header does not start a move. (`ts:panes/split.ts:307-405`)
- SHELL-GRID-12. **Zones.** The zone is the nearest edge when the pointer is within a quarter of the cell of it (ties go to the first of left, right, top, bottom), else the centre. The lit area is: left half, right half, top half, bottom half, or the whole cell for centre. A drop on the **centre swaps** the two panes and each keeps the size of its place; on a **side** it joins that row beside the target and evens the row; on **top or bottom** it starts a row of its own taking half the target row's height; on itself or a pane that is not there nothing changes. (`ts:panes/split_math.ts:19-103`; tests `a header over the middle of a pane swaps; near an edge it goes there`, `dropping_on_*` in `split_grid.rs`)
- SHELL-GRID-13. **Drag from the rail** (SHELL-RAIL-17): the same drag with no source cell. Dropped on a cell's zone: `drop_pane{pane, onto, zone}`, unless dropped on the pane itself; dropped on the stage over no cell: `drop_pane{pane, onto:null, zone:"centre"}`. Rust shows it on the Workspace if that holds nothing, else joins the grid by the rule then places it where it was dropped; a refusal is a notice. A click, with no drag, stays the row's click. (`ts:panes/split.ts:407-420`, `rs:session.rs:1007-1028`)
- SHELL-GRID-14. Closing a split (`✕` on its header: `close_beside{side:{beside:n}}`, or the keyboard) takes it out of the grid, the pane keeps running and is on the rail; its width goes to its left neighbour (the right for the first in a row), an emptied row's height to the row above, else below; the panes after it number down onto the rooms of the sides they take and are redrawn whole; keys left on a vanished cell go to its left neighbour, else the Workspace. Only a split can be taken out, never the Workspace's own pane while it is the only one; when the Workspace's pane goes, the first split takes its place at its size with its keys. (`rs:panes/shown/mod.rs:272-340`; tests `closing_a_split_keeps_the_others_shifts_them_down_and_hands_the_keys_to_its_neighbour`, `the_workspace_going_moves_the_first_split_over_at_its_size_with_its_keys`, `only_a_split_is_taken_out_of_the_grid`, `an_emptied_cell_goes_and_the_cells_after_it_number_down_with_the_keys`)
- SHELL-GRID-15. **Keyboard** (after the leader in a pane, SHELL-KEYS-6): Arrow gives the keys to the pane in that direction (in its row for left/right; up/down the nearest in the next row by middle); `o`/`O` step the keys through the panes on screen in reading order, console sides included, wrapping; `Shift Arrow` trades places with the neighbour. `focus_towards`, `move_held` and `step_row` exist as `Call`s but the page never sends them. (`rs:panes/shown/mod.rs:327-345`, `rs:session.rs:2034-2045`; tests `the_keys_step_along_the_row_and_wrap`, `the_focused_pane_trades_places_with_its_neighbour_and_the_keys_move_spatially`, `neighbours_are_in_the_row_or_the_nearest_in_the_next`; session test `the leader and o: the next pane on screen`, `rs:session.rs:6459`)
- SHELL-GRID-16. The layout is the window's own and is not persisted across restarts (`DESKTOP.md §Panes side by side`: "Not yet"); the daemon learns each pane's rows and columns only. Sizes survive a visit to another stage because `shown` holds the grid. (`DESKTOP.md §Panes side by side`)
- SHELL-GRID-17. **Header and rail drag: busy and Escape.** A header drag (or a drag from the rail) sets `busy` once it passes 4 px, so Rust's rail redraws do not move the cells during the drag (SHELL-GRID-9 is the gutter's). Its Escape listener is installed on the press, so Escape is captured and swallowed, and cancels, even before the 4 px threshold. (`ts:panes/split.ts:362-396` (`busy = true` in move; keydown listener added at the press, line 395), `ts:panes/split.ts:150`)

### Page-held state

The sizes a gutter was let go at (`held`, 600 ms), `busy` (drag in progress), the cells (DOM per slot). (`ts:panes/split.ts:35-49`)

### Page-side logic that must move to Rust

- `split_math.ts` whole (`cellRects`, `dragGutter`, `toThousandths`, `dropZone`, `zoneRect`, `keyedSplit`; 109 lines, node-tested) and `minimum(axis)` (`ts:panes/split.ts:224-234`: pixel least from cell size + terminal padding + 28 px header). The thousandths wire format exists only for the page; with GPUI pass `f32` fractions and let `Layout::resized` normalise. `rs:.../split_grid.rs` already has the same layout in fractions: unify (`rects` there is `cells()`, `:210`).
- Drop-target hit-test over cell boxes (`ts:panes/split.ts:311-324`) and the stage-drop detection (`:340-348`).

### gpui-kit mapping

- **Custom layout over `ResizeHandle`** (`GPUI.md`). Seen in `resizable.md`: standalone `resize_handle(id, Axis::Horizontal).inside(HandleEdge::Trailing).on_drag(ResizeSidebar, |drag,_,_,cx| cx.new(|_| drag.clone())).on_double_click(..)` with `on_drag_move(.. &DragMoveEvent<..>)` on the container; "The handle owns the band, the cursor and the indicator; what a drag resizes is yours." That covers gutters and their double-click. `h_resizable`/`v_resizable`/`resizable_panel` give one axis of panels with `.size`, `.size_range`, `.fixed`, `.visible`, nested `.group(..)`, but not arbitrary rows-of-columns with per-pane drop zones; use them only if rows-of-columns can be expressed as `v_resizable` of `h_resizable`s (then double-click-to-even and the two-neighbour rule are theirs: verify "only the two neighbours change" in the spike, not documented).
- Drop zones and the lit preview: custom (`on_drag`, `on_drag_move`, `drag_over` appear in `list.md` §"Drag and Drop Reordering"). Test: `window.drag(from, to, cx)` and `pointer_down/move/up` (`test.md`).

### Scenes and coverage

- Scene `workspace-split` (two panes). **Not covered:** 3+ panes, every drag (gutter, header, rail), refusals, even-out, held-size echo (SHELL-GRID-9), keyboard moves in a scene (Rust only).

---

## CONSOLE: the console

Source: `ts:panes/console.ts` (311 lines), `ts:panes/console_strip.ts`, `rs:panes/console.rs`, `rs:session.rs:1795-1815,1920-2130`. `DESIGN.md §Panes and terminal state` (console paragraph) and `§Process model`. Tests: `rs:panes/console.rs` (`the_first_console_is_chosen_and_a_closed_one_falls_back_to_the_one_before`, `a_split_sides_console_ending_leaves_the_other_filling_the_console`, `a_tab_chosen_for_the_other_side_trades_places_and_unsplitting_keeps_the_one_asked`, `the_place_follows_the_stage_unless_fullscreen`, `the_page_marks_the_chosen_tab_and_only_a_console_as_showing`, `the_page_names_both_sides_of_a_split_and_the_one_with_the_keys`), node `panes/console_strip.test.mjs` (`the menu holds split, move and fill the window`, `the menu names the way back once split, undocked or full`, `an idle shell wears no dot, a busy or failed one does`), `rs:session.rs` tests around `leader_then(&mut s, "4")`, `"`"`, `"z"`.

### Behaviours

- SHELL-CONSOLE-1. **Where it is** (`ConsolePlace`): `hidden` (only the foot's chip says so), `docked` (in its dock), `tab` (the Console stage fills the stage), `fullscreen` (everything but the sidebar's projects and the strip's window buttons put away). Rule: fullscreen covers whichever stage is up; otherwise the Console stage is `tab`, the composer is `hidden`, any other stage is `docked` while its dock shows it, else `hidden`. (`rs:panes/console.rs:205-213`; test `the_place_follows_the_stage_unless_fullscreen`)
- SHELL-CONSOLE-2. Gestures (`console{gesture}`): `toggle` shows it docked or hides it wherever it is; `tab` opens the Console stage; `dock` puts it in its dock; `fullscreen` toggles fullscreen (back to where it was entered); `hide`; `select{pane}` a tab or split-side header (that console on screen, with the keys); `spawn` a new console, shown once the daemon makes it; `close{pane}` the daemon ends that console (`Kill`); `split` splits or puts the split away keeping the side with the keys; `closeSide{pane}` leaves the split, its console stays a tab. (`rs:panes/console.rs:21-45`, `rs:session.rs:1795-1812`)
- SHELL-CONSOLE-3. Tabs are never predicted: a tab appears when the daemon's list holds the console and goes when it says it ended. A tab that ended while chosen falls back to the one started before it (else after); a side's tab that ended takes the split with it, the other side filling the console. Fullscreen, the split and the chosen tabs last as long as the run (a restarted daemon's consoles are new shells under new ids). (`rs:panes/console.rs:127-148`; tests `the_first_console_is_chosen_and_a_closed_one_falls_back_to_the_one_before`, `a_split_sides_console_ending_leaves_the_other_filling_the_console`)
- SHELL-CONSOLE-4. Choosing the tab on the second side for the first side trades places; splitting puts the first tab on neither side on the second side, or a new console when every tab is shown. (`rs:panes/console.rs:162-200`; test `a_tab_chosen_for_the_other_side_trades_places_and_unsplitting_keeps_the_one_asked`)
- SHELL-CONSOLE-5. The console head: a tab strip, a `+` (`New console`, only when the daemon takes `SpawnConsole`: `canSpawn`), a `⋯` menu, and a hide `✕` (`Hide the console` with its key). The page marks `console-<place>` classes and hides the element when `hidden`. (`ts:panes/console.ts:99-203`)
- SHELL-CONSOLE-6. A **tab**: shows its title, a status dot unless the shell is `idle`, is marked chosen (`on`) and `live` while on screen on either side, has the tooltip `shell, <status>`; click `console{on:"select", pane}`; its `✕` `console{on:"close", pane}`; a **middle click** `console{on:"close", pane}`. (`ts:panes/console.ts:122-158`, `ts:panes/console_strip.ts:320-322`)
- SHELL-CONSOLE-7. A tab's **right-click menu** (heading = title): `Rename…` (SHELL-RENAME), on a split side's header `Take out of the split`, a rule, `Close` (danger), and when there are other tabs `Close the others` (danger), which sends one `console{on:"close"}` per other tab. (`ts:panes/console.ts:50-69`)
- SHELL-CONSOLE-8. The `⋯` menu (opened under the button, right-aligned): `Split` or `Put the split away`; if docked `Open in the Console stage` (`on:"tab"`) else `Dock at the bottom`-style (`on:"dock"`, with the dock's name); `Fullscreen` or `Leave fullscreen`; each with its key caps as a hint (split: leader `\`, stage: `4`, fullscreen: leader `z`); then a rule and the dock moves `Dock on the left / on the right / at the bottom` (the dock the console is in marked `here` and disabled while docked; no `Float`). Each item sends `console{on:"split"|"tab"|"dock"|"fullscreen"}` or `docks{on:"move", panel:"console", dock}`. (`ts:panes/console.ts:72-95`, `ts:panes/console_strip.ts:334-347`; node `the menu holds split, move and fill the window`, `the menu names the way back once split, undocked or full`)
- SHELL-CONSOLE-9. A **split side** has its own header (only while split): the console's title and status dot, the whole header a button that gives that side the keys (`console{on:"select"}`; tooltip `Type in <title>` when it lacks them), a right-click menu as in SHELL-CONSOLE-7 with `Take out of the split`, and a `✕` titled `Take out of the split` (`console{on:"closeSide", pane}`). The side with the keys is marked. (`ts:panes/console.ts:207-240`)
- SHELL-CONSOLE-10. With no console on screen the body says `No consoles yet: a shell in your home directory, in no checkout.` and (when `canSpawn`) a `Start a console` button (`on:"spawn"`); with tabs but none on screen, a `Type in the console` button selects the chosen (else first) tab. (`ts:panes/console.ts:279-310`)
- SHELL-CONSOLE-11. The keys text area goes into the screen that has the keys: an extension console view the session says has them, else a console side whose pane is `typing`, else the split cell the rail says is keyed, else the Workspace's. Every other screen draws its cursor hollow. The console's screens expect their pane (skeleton, SHELL-SCREEN-12) while on screen. (`ts:panes/console.ts:245-277`)
- SHELL-CONSOLE-12. The Workspace's pane and the console are on screen together; only the keys move between them: a console shown or picked takes them; the Workspace's pane takes them back when pressed, chosen on the rail, asked for by its key, or when the console hides and lets its panes go. When the console is hidden its panes go (`settle_console`). (`DESIGN.md §Panes and terminal state`; `rs:session.rs:2138-2145`)
- SHELL-CONSOLE-13. Persisted: its dock and whether the dock shows it and the dock size (in `desktop.json`, via DOCK); not persisted: fullscreen, the split, the tabs chosen. (`rs:panes/console.rs:1-15`)
- SHELL-CONSOLE-14. The console's `⋯` button is titled `Split, move, fill the window` (aria `Console menu`). The hide `✕` sends `console{on:"hide"}`, not `toggle` (SHELL-CONSOLE-2). (`ts:panes/console.ts:178-179,198`)

### Page-held state

None (all in `ConsolePage`).

### Page-side logic that must move to Rust

- `wearsDot` and `stripItems` (`ts:panes/console_strip.ts`, node-tested), `tabMenu` and `stripMenu` assembly, **`Close the others` as a page loop of N calls** (`ts:panes/console.ts:61-67`; Rust should own a `close_others{pane}` so it is one decision), `keyedScreen()` precedence (`ts:panes/console.ts:245-255`), `titled()` key-hint lookup. `keyedSplit` is SHELL-GRID's.

### gpui-kit mapping

- **`TabBar`/`Tab`** (`tabs.md`). Seen: `TabBar::new(id).selected_index(i).on_click(|ix,_,_| ..)`, `Tab::new(label).suffix(Button::new("close").ghost().xsmall().icon(IconName::Close))`, `.prefix(..)`, `.suffix(..)` on the bar for `+`/`⋯`/hide, `.folder()` variant, `with_menu(bool)` (the API table; one example shows `.menu(true)`, so it is unsettled in `tabs.md`), `.max_width(px)`, `.track_scroll(&ScrollHandle)`. **Not documented:** middle-click close, per-tab context menu (the sidebar docs show `.context_menu(..)` for `SidebarMenuItem`, not for `Tab`; use `div().context_menu(..)` from `menu.md` around the tab), tab reorder (the description mentions "reordering" but no API is shown), per-tab status dot (put it in `Tab::child`/`prefix`). `Dialog` (`dialog.md`) for nothing here. The screens are SCREEN's custom element; the `+`/`⋯`/hide use `Button` (`button.md`), the menu `PopupMenu` (MENU).
- Middle click: custom `on_mouse_up(MouseButton::Middle)`; test with `click_with_options(.. with_button(MouseButton::Middle) ..)` (unconfirmed).

### Scenes and coverage

- Scenes: `workspace-console` (docked under the pane), `docks` (console below). **Not covered by any scene:** Console stage, fullscreen, split, tab menus, empty body, spawn. Rust and node tests cover the rules; no test of `drawHead`, middle-click, or `Close the others`.

---

## DOCK: docks

Source: `ts:layout/docks.ts` (269 lines), `ts:layout/panels.ts`, `rs:layout/docks.rs` (1013 lines), `rs:layout/stage.rs`. `DESIGN.md §Process model` (docks paragraph), `DESKTOP.md` (none). Tests: `rs:layout/docks.rs` (`a_panel_starts_hidden_in_its_home_dock`, `hiding_a_panel_and_showing_it_again_finds_it_where_it_was_put`, `panels_in_one_dock_are_tabs_and_one_shows_at_a_time`, `the_dock_a_panel_leaves_shows_the_tab_before_it_or_closes_once_empty`, `an_extensions_view_is_placed_once_where_it_asks_and_then_where_the_person_puts_it`, `a_dock_shows_only_panels_that_exist_and_keeps_the_place_of_one_gone`, `a_layout_is_saved_once_per_change_and_sizes_are_held_to_their_bounds`, `a_hand_edited_layout_is_made_whole`, `every_panel_that_exists_is_told_with_what_it_can_do`, `every_panel_a_dock_or_dialog_shows_is_among_those_told`, `the_page_never_reads_what_a_panel_can_do_from_its_name`, `a_dock_gesture_stages_or_closes_a_panel_by_name`), `rs:layout/stage.rs` (`a_page_goes_back_to_the_stage_it_opened_over`, `going_to_another_stage_closes_the_page`, `the_workspace_is_drawn_only_while_its_stage_is_up_and_no_fullscreen_console_covers_it`).

### Behaviours

- SHELL-DOCK-1. Three docks: `left` (between the sidebar and the stage), `right`, and `bottom` (under the stage alone). A **panel** is the console (home: bottom), the views agents showed (home: right), an extension's view (where its extension says, the first time), a saved view (right, the first time). A panel the person placed stays placed whether it shows or even exists. (`rs:layout/docks.rs:1-15,64-110`; test `a_panel_starts_hidden_in_its_home_dock`)
- SHELL-DOCK-2. A dock shows **one** of its panels that exist now (the chosen one, else the first that exists), is open or closed, and a dock with none of them is closed on screen. Hidden and shown again a panel is where it was put; an extension's view gone with its extension finds its place again when it returns. (`rs:layout/docks.rs:508-530`; tests `panels_in_one_dock_are_tabs_and_one_shows_at_a_time`, `a_dock_shows_only_panels_that_exist_and_keeps_the_place_of_one_gone`)
- SHELL-DOCK-3. A dock draws a **tab strip only while it holds more than one panel that exists**, one tab per panel, the shown one marked; each tab's text is its panel's own title and `title` tooltip; it is a draggable handle (SHELL-PDRAG). With one panel the panel's own head names it. (`ts:layout/docks.ts:49-52,165-230`)
- SHELL-DOCK-4. Tab behaviour: click `docks{on:"show", panel}`; a middle click or `✕` closes **only** a panel Rust says `closes` (a saved view): `docks{on:"close", panel}`; a tab's right-click menu (heading = title): the move items, a rule, and `Close` for a closing panel, else `Hide` (`docks{on:"hide", panel}`). (`ts:layout/docks.ts:175-230,234-237`)
- SHELL-DOCK-5. The tab strip ends with the shown panel's extension tools (`extensionTools`) and its dock menu button: `Move to another dock`, drawn as the icon of the dock the panel sits in. A head that has a Dock button of its own passes `chevron` so one icon never means two buttons. (`ts:layout/docks.ts:86-108,225-229`)
- SHELL-DOCK-6. **Dock menu** (button under itself, or a tab's right-click): `Dock on the left`, `Dock on the right`, `Dock at the bottom` (the one it is in marked `here` and disabled, when docked), and, only when Rust says the panel `floats`, a rule and `Float as a dialog` (disabled `here` if already floating). Each sends `docks{on:"move", panel, dock}` or `docks{on:"float", panel}`. (`ts:layout/docks.ts:55-79`; test `a_dock_gesture_stages_or_closes_a_panel_by_name`)
- SHELL-DOCK-7. Rust semantics of the gestures: `show` opens the panel's dock on its tab (or raises its dialog); `hide` closes the dock only if the panel is the tab shown; `move` takes the panel to the end of that dock's tabs and shows it (out of fullscreen, the Console stage or the page for the console and extension views); `stage` makes an extension view a page of its own (the console's stage is the Console stage); `float` makes a saved view a dialog; `close` removes a closing panel for good; every other panel stays. The dock a panel leaves shows the tab before it, or closes once empty. (`rs:layout/docks.rs:532-621`, `rs:session.rs:1815-1860`; tests `the_dock_a_panel_leaves_shows_the_tab_before_it_or_closes_once_empty`, `a_dock_gesture_stages_or_closes_a_panel_by_name`)
- SHELL-DOCK-8. What a panel can do is told by Rust (`PresentPanel{panel, kind, floats, stage, closes}`) and the page never infers it from the panel's name; a panel Rust did not say exists draws nothing. (`ts:layout/panels.ts`; test `the_page_never_reads_what_a_panel_can_do_from_its_name`)
- SHELL-DOCK-9. Placement on screen: each dock puts its shown panel in its body and **parks** the others (kept alive, not destroyed, so state and ids survive a move); the console on the Console stage or fullscreen goes to the stage, an extension view that is a page draws on the stage, not in a dock; the pane text area keeps its focus across a move. (`ts:layout/docks.ts:113-162`)
- SHELL-DOCK-10. Size: the bottom dock's height and a side dock's width, in CSS pixels, set when its sash is let go (SHELL-EDGE), held by Rust to `bottom 120..1200` and `side 280..1600`, defaults `bottom 280`, `side 560`; kept in `desktop.json`. The bottom dock's sash is shown only while the bottom dock is. Under 1200 px window width the right dock opens **over** the stage as a drawer (up to the dragged width and leaving a sliver) rather than squeezing the pane. (`rs:layout/docks.rs:242-243,291-297,621-625`, `css:10440-10460`; test `a_layout_is_saved_once_per_change_and_sizes_are_held_to_their_bounds`)
- SHELL-DOCK-11. A layout is saved once per change; a hand-edited file is made whole (a panel placed twice keeps its first place, a panel with a home and no place goes home, sizes held to bounds, a dock with nothing in it closed); a file from before docks keeps the console docked at the bottom as tall as it was, the views panel's width as the right dock's, and each extension view where it was. (`rs:layout/docks.rs:428-470`, `rs:settings/remembered.rs:293-316`; tests `a_hand_edited_layout_is_made_whole`, `a_file_from_before_the_docks_keeps_its_console_views_panel_and_extension_views_where_they_were`)
- SHELL-DOCK-12. Pages: opening a view as a page over a stage returns to the stage under it when closed, docked, or its view goes (a notice says `<title> was closed` or `<extension>: <title> is no longer shown here`); going to any other stage closes the page; the Code stage goes back to the stage it was opened over. (`rs:layout/stage.rs`; tests listed above)
- SHELL-DOCK-13. A dock sash's release reports `docks{gesture:{on:"size", dock, size}}` (`DockGesture::Size`), sent only when the size is a number. A double-click on a dock sash only redraws the docks and sends nothing: there is no reset-to-default for docks, unlike the rail and the feature and code edges (SHELL-EDGE-2). (`ts:layout/docks.ts:261-266`; `rs:layout/docks.rs:38-40`)

### Page-held state

None (DOM parking only).

### Page-side logic that must move to Rust

- `dockOf`, `dockShowsTabs`, `moveItems` (menu assembly incl. `here` rules), `closeOf`, the `element(panel)` kind switch, parking and focus-keeping (`ts:layout/docks.ts:44-162`). The kind-to-view mapping becomes a Rust `match` on `PanelKind`.

### gpui-kit mapping

- **Candidate A (as `GPUI.md`): `Resizable` + `TabBar` + `PopupMenu`** (`resizable.md`, `tabs.md`, `menu.md`), container only; placement stays in Rust. `resizable_panel().size(px).size_range(min..max).fixed().visible(bool)`; `ResizableState::sizes()`, `.on_resize(|state,window,cx|..)` reports a drag (we send the size once on release, SHELL-EDGE).
- **Candidate B: `Dock`** (`dock.md`): `DockArea` via `DockSkin::dock_area("main-dock", Some(1), window, cx)` (unconfirmed), `DockLayout::h_split()/v_split()/tabs()`, `DockPlacement` left/right/bottom regions, `BasePanel`/`Panel` traits, `panel_handle`, `DockEvent::LayoutChanged`, `area.dump(cx)` / `area.load(state, ..)` as `DockAreaState`, panels "added, removed, activated, zoomed, and moved at runtime"; drag-and-drop is built in. **It owns its own layout state and persistence**, while Argus keeps one model in Rust (`layout/docks.rs`) with places the kit has no word for (a **stage** page, a **float** dialog), `present`/`closes`/`floats` abilities, and "dock shows one of its panels". Adopting it means mirroring Rust's model into `DockArea` on every `docks` event. Not documented: one-active-tab-per-edge-dock semantics, tab strip hidden for a single panel, a custom drop target for "stage". Decide in the architecture phase; the behaviours above are the contract either way.
- Menu button and tools: `Button` + `PopupMenu::dropdown_menu` (`menu.md`); `Button::dropdown_menu_with_anchor(Anchor::TopRight, ..)` positions it.

### Scenes and coverage

- Scenes: `docks` (console below, an extension on the right), `workspace-console`, `agent-views`, `saved-view`, `float`. **Not covered:** tab strip in a scene with 2+ panels in one dock, left dock, the dock menu, `closes` tabs, drawer mode under 1200 px, SHELL-DOCK-12 (Rust tests only).

---

## PDRAG: panel drag to a dock

Source: `ts:layout/drag.ts` (165 lines), `rs:layout/docks.rs` (`DockGesture::Move`/`Stage`). `DESIGN.md §Process model` (docks paragraph: drag). No tests of the drag; `targetAt` is exported but has no node test.

### Behaviours

- SHELL-PDRAG-1. A drag starts on a left press on a dock **tab**, a panel **header** (the console's head), or an extension view's **row** in the sidebar, and becomes a drag after the pointer travels **5 px**; a press that never travels stays the click it was (the click a release would produce is swallowed once after a real drag). A press on a button, input, textarea or link inside the handle stays theirs unless the handle is that element. A dialog (floating) header moves the dialog instead (SHELL-FLOAT). (`ts:layout/drag.ts:19,83-120`)
- SHELL-PDRAG-2. While dragging, a ghost with the panel's title follows the pointer (12 px right and below), the body is marked `panel-dragging`, and a preview rectangle with words shows where it would land. Mouse events, not the browser's drag-and-drop, so the preview follows over canvases. (`ts:layout/drag.ts:109-136`)
- SHELL-PDRAG-3. **Target at a point** over the work area (outside it: no target): within `min(160, 20 % of work width)` of the left edge `left`, of the right edge `right`; else the bottom `min(160, 30 % of the centre height)` of the centre `bottom`; else `stage` if Rust says the panel takes the stage (`PresentPanel.stage`), else none. (`ts:layout/drag.ts:33-47`)
- SHELL-PDRAG-4. **Preview rectangle**: left/right: the work area's full height, `min(560, 35 % of work width)` wide, at that edge; bottom: the centre's width, `min(320, 40 % of the centre height)` tall at its foot; stage: the whole centre. Words: `Dock on the left`, `Dock on the right`, `Dock at the bottom`, `Open on the stage`. (`ts:layout/drag.ts:50-71`)
- SHELL-PDRAG-5. Letting go over a target sends the same move the dock menus send: `docks{on:"move", panel, dock}` for a dock, `docks{on:"stage", panel}` for the stage; over no target nothing is sent. Escape (captured, swallowed) cancels. (`ts:layout/drag.ts:73-76,137-165`)

### Page-held state

The drag in progress; the swallow-next-click flag (one tick).

### Page-side logic that must move to Rust

- `targetAt` and `landing` (the 160/20 %/30 %/560/35 %/320/40 % constants) are layout rules; put them in a Rust function with unit tests (none today) taking `(point, work, center, stage)`.

### gpui-kit mapping

- **custom.** Not in any kit component documented here (`dock.md` has its own drag-and-drop for tabs but not a stage/float target). GPUI native `on_drag`/`on_drag_move` as shown in `resizable.md` and `list.md`. Test: `window.drag(from, to, cx)`, `pointer_down/move/up` with an assertion mid-drag (`test.md`).

### Scenes and coverage

- None. **Not covered at all:** thresholds, preview, Escape cancel, click swallowing.

---

## FLOAT: floating saved-view windows

Source: `ts:layout/floats.ts` (141 lines), `rs:layout/docks.rs` (`float`, `set_bounds`, `raise`, `FloatBounds`). `DESIGN.md §Views` saved-view paragraph (`views/saved`). Test: `a_saved_view_floats_as_a_dialog_each_one_stepped_from_the_last_and_the_one_shown_in_front`.

### Behaviours

- SHELL-FLOAT-1. A floating panel is a frame over the work area (above docks and the stage), stacked **back to front** in Rust's order, z-index `index + 1`, labelled with its title; frames of panels no longer floating go and their panels are parked. (`ts:layout/floats.ts:28-121`)
- SHELL-FLOAT-2. First float opens at `x 96, y 64, 560 x 520`; each next is stepped `28 px` down and right of the one in front; a float never opens smaller than `280 x 160`. (`rs:layout/docks.rs:221-229,575-590`; test above)
- SHELL-FLOAT-3. A left press on a float brings it in front (`docks{on:"raise", panel}`) unless it already is; Rust moves it to the end of the order. (`ts:layout/floats.ts:28-121`, `rs:layout/docks.rs:601-612`)
- SHELL-FLOAT-4. The header (the panel's own head, `.saved-head`; not a button, input, textarea or link in it) **moves** the frame; the **corner grip** resizes it (width at least 280, height at least 160). Both follow the pointer live and send `docks{on:"bounds", panel, bounds:{x,y,width,height}}` (whole pixels) once on release, only if changed. While held a redraw from Rust leaves the frame where the pointer has it; the body is marked `float-moving`. (`ts:layout/floats.ts:28-121`)
- SHELL-FLOAT-5. Placement is clamped on every draw: no bigger than the work area, at least `120 px` of its width stays inside horizontally (so it can be reached again), its top never above 0 and at least `40 px` stay above the bottom. (`ts:layout/floats.ts:28-121,329-343`)
- SHELL-FLOAT-6. Rust holds bounds no smaller than the minimum, the order, and persists them in `desktop.json`; the dock menu of a floating panel can dock it. (`rs:layout/docks.rs:592-600`)

### Page-held state

`front`, `held`, the frame DOM per panel.

### Page-side logic that must move to Rust

- Clamp rule (`place`, `KEEP`), grip and header hit rules, min sizes. Rust already clamps minimums (`FloatBounds::held`); add the work-area clamp.

### gpui-kit mapping

- **custom** (`GPUI.md`: `Dialog` neither moves nor resizes). Confirmed from `dialog.md`: width/height/margin only, no drag handle API; `sheet.md` `.resizable(true)` is an edge panel. Use an absolutely positioned element with `on_drag`/`on_drag_move`; keep a `ResizeHandle` (`resizable.md`, `resize_handle(id, Axis)`) for the corner if it suits.

### Scenes and coverage

- Scene `float`. **Not covered:** drag, resize, raise, clamp.

---

## EDGE: edge drag (sashes)

Source: `ts:layout/edge.ts` (57 lines), `rs:layout/dragged_width.rs`. Users: rail (SHELL-RAIL-32), docks (SHELL-DOCK-10), an open feature's outline/brief and the Code stage's tree/outline (other groups). Tests: `rs:layout/dragged_width.rs` (`a_drag_is_held_to_its_bounds_and_saved_once`, `a_rail_dragged_nearer_folded_than_its_minimum_folds`, `a_hand_edited_width_is_held_to_its_bounds_too`), `rs:session.rs` around `rs:session.rs:6360-6374` (rail width).

### Behaviours

- SHELL-EDGE-1. A left press on a sash starts a drag; the column keeps the point it was grabbed at under the pointer (it does not jump to put its border under it); the column follows at most once per frame; the body is marked `resizing` (so panes wait to report their size) and the sash `held`. (`ts:layout/edge.ts:23-56`)
- SHELL-EDGE-2. On release the column is drawn at the final width and `report(width)` sends it once (`max(0, width)`); a **double-click** reports `null` (give back the usual width) on the rail and the feature and code edges; a dock sash sends nothing (SHELL-DOCK-13). While held, nothing is sent. (`ts:layout/edge.ts:23-56`)
- SHELL-EDGE-3. Rust holds each dragged size to bounds and says it back (the page draws from what Rust keeps, which may send nothing new): `RAIL 200..560`, outline `220..1200`, brief height `40..900`, code tree `180..800`, code outline `160..800`; the rail alone **folds**: a width dragged below `(56 + 200) / 2 = 128` becomes `56`; undragged it is `None` and the page's own width stands (which narrows in a small window). Saved once per change in `desktop.json`. (`rs:layout/dragged_width.rs:10-70`; tests above)
- SHELL-EDGE-4. Sizes along the axis: the rail's is `pointer x - rail left`; a left dock's `pointer x - work left`; a right dock's `work right - pointer x`; the bottom dock's `centre bottom - pointer y`. (`ts:layout/docks.ts:248-252`, `ts:chrome.ts:1083-1087`)

### Page-held state

The width while held.

### Page-side logic that must move to Rust

- Frame-throttling and grab offset are view code; the **axis formulas** (SHELL-EDGE-4) become the container's own measure in the GPUI layout.

### gpui-kit mapping

- **`resize_handle(id, Axis).inside(HandleEdge::Trailing).on_drag(..).on_double_click(..)`** (`resizable.md` §Standalone Resize Handle) is exactly this contract, with `on_drag_move(&DragMoveEvent<T>)` giving `e.event.position.x - e.bounds.left()`. Panel groups (`h_resizable`, `ResizableState::sizes()`, `.on_resize`) are an alternative when the dock sits in a group. Not documented: whether the handle reports drag start/end (for `resizing`); spike.

### Scenes and coverage

- None for drags. Rust covers clamping and saving (above), not the grab offset.

---

## STAGE: stage hosting, the strip's words, empty stages

Source: `ts:chrome.ts:35-119,607-735,866-1041`, `rs:layout/stage.rs`, `rs:session.rs` (`rail()`, `go`), `rs:connection/page.rs:812-830` (`Stage`). `DESIGN.md §Process model` (title strip, stage paragraphs). Tests: `rs:layout/stage.rs` (see SHELL-DOCK-12), `rs:chrome/rail.rs` `dump_rail`.

### Behaviours

- SHELL-STAGE-1. One stage is up (`Stage`: `workspace`, `feature`, `checkouts`, `new` (the composer), `console`, `extension`, `view`, `code`) and the window draws only that stage's surface: Feature, Checkouts, Code, an extension's page and an agent's popped view each own a surface; `new` draws the composer; `workspace` draws the grid (SHELL-GRID) or the empty stage; `console` the console filling the stage. The Workspace's panes are hidden unless `workspaceShown` (its stage is up and no fullscreen console covers it; the composer covers it as every other stage does). (`ts:chrome.ts:35-65`, `rs:layout/stage.rs`; test `the_workspace_is_drawn_only_while_its_stage_is_up_and_no_fullscreen_console_covers_it`)
- SHELL-STAGE-2. A page (an agent's view or an extension's) opens **over** whatever stage is up and never replaces the stage under it; closing, docking or its view going returns to that stage; going to another stage closes it; the Code stage goes back to the stage it was opened over. (`rs:layout/stage.rs:1-12`; tests `a_page_goes_back_to_the_stage_it_opened_over`, `going_to_another_stage_closes_the_page`, `the_code_stage_goes_back_to_the_stage_it_was_opened_over`)
- SHELL-STAGE-3. **Fill the window** (leader `f`): the sidebar is put away, the strip (with the window buttons) stays; `filled` is set only while the Workspace stage is up with a pane on screen and the console not typed in. In a console, `f` is the console's fullscreen. (`css:4569-4573`, `ts:main.ts:88`, `rs:session.rs:3231-3240`, `rs:session.rs:1109-1118`)
- SHELL-STAGE-4. **Console fullscreen**: everything but the sidebar's project heading and the strip's window buttons is put away (stages, docks, the nav, the sidebar foot). (`css:4953-4966`)
- SHELL-STAGE-5. **The nav** (top of the sidebar): Search, New, Workspace, Features, Checkouts, Code, Console (then the extensions' and saved views' entries, other groups). The current stage's item is marked and `aria-current`; each shows its keys from the keymap in `anywhere` (`compose`, `workspace`, `feature`, `checkouts`, `code`, `console`) and Search the `palette` key. Rerendered on a `keymap` event. (`ts:chrome.ts:70-89`)
- SHELL-STAGE-6. **Off the Workspace** the stage's nav item is the selection, so the pane on screen is marked more quietly on the rail. (`ts:chrome.ts:126`)
- SHELL-STAGE-7. **The strip** (a draggable bar across the stage, SHELL-TITLE) says: on the **Workspace** with a pane shown, a crumb `<repository> / <branch or checkout>` (branch in mono), the pane's **title**, a status chip when the pane is an agent or its status is one of waiting / needs-review / failed / exited / done (chip words SHELL-RAIL-12), a muted resumed note, the grid size (`#size`, SCREEN-10), and a `⋯` button titled `More: rename or close this pane` that opens the pane menu (SHELL-RAIL-15's items without `Show`/`Open beside`) under itself, right-aligned; with no pane the strip is empty. (`ts:chrome.ts:658-713`)
- SHELL-STAGE-8. The strip on **another stage** (fullscreen console counts as the Console): crumb = the chosen repository's name (none for Console and Extensions; the word `Extensions` stands for the extension stage), then ` / ` and the stage's trail (Code's crumb, or `Features / #<number>` for an open feature); title = the open feature's title, the extension page's title, the popped view's title (else `View`), or the stage's name (`Features`, `Checkouts`, `Code`, `New`, `Console`, `Extensions`, `View`); right side = the stage's tools (Code, Features strip, Checkouts strip, popped view); Features/Checkouts add their meta; a popped view adds its status. An open feature is the stage's title, its way back being its crumb and Escape. (`ts:chrome.ts:91-99,609-657`)
- SHELL-STAGE-9. A pane resumed by this window shows `resumed from <clock>` plus ` · N turn(s) before` (`turn` singular for 1; no clock says `resumed`; no turns omits the second part). (`ts:chrome.ts:728-733`; clock formatting is SHELL-TIME)
- SHELL-STAGE-10. **The ended bar** (Workspace, shown pane `exited`): an info icon, the pane's note (muted, only if it has one), a button `Start <template> here` (the template is the window's first agent template when the pane was an agent; a shell, or an agent when the daemon offers no template, says `New shell here`) and `Close pane` (`close{pane}`). The start button appears only when the pane is still in the tree. Starting closes any + menu, unpicks the rail, then `spawn_shell{checkout}` or `spawn_agent{checkout, template}`. (`ts:chrome.ts:866-905`)
- SHELL-STAGE-11. **Empty stage, connecting** (the daemon has not sent a tree: `loaded` false): a spinner, `Connecting to argusd`, `on <host>` or `on this machine` then ` · the sidebar fills in when it answers`, the last failed try's words (`detail`) as an error, and one way out: with a remote host, `Use this machine` (`set_host{host:null}`), else `Settings`. (`ts:chrome.ts:921-941`; scene `connecting`)
- SHELL-STAGE-12. **Empty stage, no project**: a folder tile, `No projects`, `Add a folder of repositories, or run argus init in one.`, and `Add a project…` (`open_add_project{}`). (`ts:chrome.ts:942-954`)
- SHELL-STAGE-13. **Empty stage, project but no pane**: a terminal tile, `No pane on screen`, `Start an agent on a checkout, open a shell, or pick up a pane that is still running.`, a primary `Start an agent` button with the composer key's first chord (`show_composer{open:true}`), a button `Open a shell on <branch>` or `Open a shell` (`spawn_shell{checkout}`) for the **first checkout of the chosen repository (else of the first repository)**, a line `on <branch or name> in <repository>`, and a `Panes in <project>` strip of cards for **every pane in the project, most recently changed first, whatever the rail is folded to**: each card says `<repository> · <checkout>`, a status pill, the pane's title, its note (else `agent`/`shell`) and ` · <used> ago`; a click is `select{pane}`. (`ts:chrome.ts:955-1041`; scene `workspace-empty`; Rust sorts `running`, `rs:chrome/rail.rs:116-141`, test `every_pane_is_offered_as_running_however_the_rail_is_folded_most_recent_first`)
- SHELL-STAGE-14. **Stopped**: when `status.connection` is `stopped` a card over the window says `argusd was stopped`, `Someone ran argus server stop. Its panes ended with it; this is the last tree it sent.` with a `Reconnect` button (`reconnect{}`); the heading, rail, strip and Workspace body are dimmed ("held") because they show what the window last heard. Nothing reconnects until Reconnect is pressed, and Rust honours the call only while stopped. (`ts:chrome.ts:63-64`, `index.html` `#stopped`, `rs:connection/shared.rs` `ask_reconnect`; scene `stopped`; test `stopping_is_remembered_until_a_connection_succeeds`)
- SHELL-STAGE-15. The window under **1200 px wide** folds the rail to icons whatever width it was dragged to, hides its sash, and opens the right dock over the stage as a drawer (SHELL-DOCK-10); a rail wider than 120 px draws words, narrower draws icons (SHELL-RAIL-33). (`css:10440-10460`, `css:10251-10275`)

### Page-held state

None.

### Page-side logic that must move to Rust

- `drawHead`'s composition (`ts:chrome.ts:609-714`: which crumb and title for which stage, `STAGE_NAME`, the chip rule `shown.agent || PANE_SAYS.has(status)`), `resumedFrom` (`:728-733`), `drawEnded` and `endedTemplate` (`:866-905`), the empty-stage selection and the first-checkout rule (`:907-1001`), `chosenRepository`. Rust should hand the view a ready `StripPage`; `Stage` is already Rust's.

### gpui-kit mapping

- **Empty states: `Empty`** (`empty.md`, unconfirmed): `Empty::new().header(EmptyHeader::new().media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(..))).title(EmptyTitle::new().child(..)).description(EmptyDescription::new().child(..))).content(EmptyContent::new().child(Button::new(..)..))`, plus `Spinner::new()` (`spinner.md`, `.large()`, `.color(..)`) for connecting; the `Panes in <project>` cards custom.
- **Strip words:** the skill catalog lists `Breadcrumb` (`breadcrumb::Breadcrumb`) but `https://gpui-kit.com/component/breadcrumb.md` returns a 404 page (not in `llms.txt`): unconfirmed, use styled text. Chips: `Tag` (`tag.md`: `Tag::primary()/secondary()/danger()/success()/warning()/info().child(..)`) or `Badge`.
- **Stopped card:** `Dialog` with `.overlay_closable(false).keyboard(false).close_button(false)` and a Reconnect `Button` (options seen in `dialog.md` "Dialog Options"); `AlertDialog` (`alert-dialog.md`) is an alternative.
- **Responsive rail:** `Sidebar::collapsed(bool).width(px)` (`sidebar.md` "Responsive Sidebar"), plus a window-size query. Not documented: container queries; compute from the window's width.

### Scenes and coverage

- Scenes: `connecting`, `stopped`, `workspace-empty`, `workspace-exited`, `workspace-agent`. **Not covered:** the Console stage and fullscreen strips, the strip on any other stage (their own groups), ended bar for an agent pane, the `Use this machine` branch of connecting, filled pane, narrow window. No node or Rust test of `drawHead`/`drawStage`.

---

## RAIL: project rail, picked row, + menu

Source: `ts:chrome.ts:121-605,1066-1088`, `rs:chrome/rail.rs` (420 lines), `rs:layout/dragged_width.rs`. `DESIGN.md §Navigation model` and `§Process model` (rail paragraph). Tests: `rs:chrome/rail.rs` (`active_repositories_lead_and_the_quiet_follow_a_rule_by_last_use`, `an_active_repository_lists_checkouts_with_panes_and_its_primary`, `quiet_repositories_are_closed_until_opened_and_active_ones_the_other_way`, `every_pane_is_offered_as_running_however_the_rail_is_folded_most_recent_first`, `a_folded_checkout_keeps_its_row_and_counts_what_it_hides`, `the_checkout_holding_the_shown_pane_stays_unfolded`, `the_repository_holding_the_shown_pane_stays_open`, `counts_cover_every_project_and_the_heading_its_own`, `a_checkout_says_its_branch_and_git_state`, `a_checkout_says_how_many_agents_share_it_once_several_do`, `a_checkout_says_whether_it_is_the_main_one_or_a_worktree`, `ages_read_in_the_largest_unit_that_fits`), `rs:keys/keymap.rs` (`a_picked_rail_row_moves_starts_and_closes_without_the_leader`), `rs:session.rs:6360-6374` (rail width).

### Behaviours: what the rail lists (Rust, stays)

- SHELL-RAIL-1. Not loaded (`loaded` false): the rail shows only a wait placeholder (the stage is the one loading surface). Loaded with no project: `No projects yet.`. Otherwise the tree below. (`ts:chrome.ts:127-135`)
- SHELL-RAIL-2. Every repository of the project is listed, **active first** (those with panes), the quiet ones after, ordered by last use; an active repository is open until closed, a quiet one closed until opened; a repository holding the shown pane is never closed and a checkout holding it is never folded. Only an **open** repository lists checkouts, and a checkout is listed when it has panes or is the repository's primary one (so there is always somewhere to start a shell). A quiet repository carries `used` (`16m`, `2h`, `3d`, `2w`, `—`). `Repository.divider` marks the first quiet row; **the page never reads it** (`grep divider` in `ts:chrome.ts` finds only a comment), so no rule is drawn between active and quiet today although `DESIGN.md §Navigation model` says "drawn". (`rs:chrome/rail.rs:70-114,146-177`; tests above)
- SHELL-RAIL-3. A checkout row says its **branch** (`detached`, empty outside git), its git summary (`↑a ↓b +staged !unstaged`) only when it is not clean, how many agents share it when several do (`shared`), whether it is the `main` checkout or a `worktree`, how many listed panes it holds (`count`, an editor is not a pane) and the loudest status of them; folded, `panes` is empty and `count`/`status` speak for what is hidden. (`rs:chrome/rail.rs:146-199`)

### Behaviours: repository row

- SHELL-RAIL-4. A repository row: a chevron (down open, right closed), its name, a dot when the loudest status beneath it is loud (waiting / needs-review / failed in the attention colour, working in the accent), and a count (`used` for a quiet repository, else the pane count, blank when 0); the chosen repository (`rail.repository`) is marked `sel` and `aria-current`; quiet ones are dimmed; the whole row has the repository name as tooltip (the folded rail's only name). `aria-expanded` follows `open`. (`ts:chrome.ts:164-219`)
- SHELL-RAIL-5. Click on the row's button: `choose_repository{repository}`. A clock button (`Recent agent sessions`) on the row: `open_sessions{repository}`. (`ts:chrome.ts:195-217`)
- SHELL-RAIL-6. Right-click (heading = name): `Features` (`choose_repository` first when it is not the chosen one, then `show_features{open:true}`), `Recent agent sessions` (`open_sessions`), a rule, `Collapse` or `Expand` (`toggle_repository{repository}`). (`ts:chrome.ts:175-193`)

### Behaviours: checkout row

- SHELL-RAIL-7. A checkout row: a fold chevron button (only when `count > 0`; `Fold its panes away` / `Show its panes`; `aria-expanded`), a branch icon, its name, `people <n>` titled `shared by <n> agents` when shared, the git summary when not clean, and, **folded**, a count of the panes it hides with the loudest status dot (only when that status is loud) titled `<n> pane(s) folded away`. Tooltip `main checkout on <branch>` / `worktree on <branch>` (just `main checkout`/`worktree` without a branch). (`ts:chrome.ts:221-290`)
- SHELL-RAIL-8. Click on the row **picks** it (SHELL-RAIL-19). The chevron button folds or unfolds its panes (`toggle_checkout{checkout}`) without picking; folding a checkout whose pane is picked moves the pick to the checkout first (its row is about to go). (`ts:chrome.ts:235,246-249,415-420`)
- SHELL-RAIL-9. The row's `+` button (`Start a shell or an agent`; `aria-expanded`) toggles the checkout's + menu (SHELL-RAIL-28) without picking; the row is `hot` while its menu is open. (`ts:chrome.ts:274-288`)
- SHELL-RAIL-10. Right-click (heading = branch, else name): a `New shell` and one `New agent · <template>` per daemon template, a rule, `Review changes` (`open_review{checkout}`), `History` (`open_history{checkout}`), `Show in Checkouts` (`go_to_checkout{checkout}`), `Collapse`/`Expand` when it has panes (fold as in -8), and for a **worktree** only a rule and `Remove worktree…` (danger, `remove_checkout{checkout}`, which asks first on the Checkouts stage). (`ts:chrome.ts:292-317`)

### Behaviours: pane row

- SHELL-RAIL-11. Panes nest under their checkout; folded checkouts list none. (`ts:chrome.ts:141-149`)
- SHELL-RAIL-12. A pane row: a status dot unless the row says a **word**, a title, and at its end a pill with the word when the status is one of waiting (`needs you`), needs-review (`review`), failed, exited, done (other statuses show the dot alone, with the status as screen-reader text). `working` shows in the dot only. The row is marked `on` while it is the pane on screen, `picked` while picked, `sh` for a shell. Tooltip `<title> · <note or "agent, <status>" / "shell, <status>">`. (`ts:chrome.ts:113-115,344-403`; `ts:kit/dom.ts:199-207`)
- SHELL-RAIL-13. Click: pick the row then `select{pane}`: the pane comes on screen **but the keys stay on the rail** until Enter or a click on the terminal, so `x` closes it. (`ts:chrome.ts:376-382`)
- SHELL-RAIL-14. A close `✕` on the row (`Close <title>`) and a **middle click** on the row both send `close{pane}`; the rail drops the pane when the daemon's tree does (never predicted). (`ts:chrome.ts:400-402,736-747`, `ts:kit/middle.ts`; Rust `close` is `Kill`)
- SHELL-RAIL-15. Right-click (heading = title): `Show` (pick, `select{pane}`), `Open beside` (`open_beside{pane}`, refusals are notices, SHELL-GRID-6), `Rename…` (SHELL-RENAME), a rule, `Close` (danger, `close{pane}`). (`ts:chrome.ts:320-328,351-366`)
- SHELL-RAIL-16. Folded rail: a pane's tile shows the **initials of its task**: the text after the first ` · ` (else the whole title), non-letters/digits turned to spaces, then the first letter of the first two words, or the first two letters of one word, lower-cased (`claude · trim context` is `tc`); every name is in the tooltip. (`ts:chrome.ts:330-342`)
- SHELL-RAIL-17. **Drag a row onto the stage** (mouse down on the row's button): after 4 px a drag begins with the grid's zones (SHELL-GRID-11..13); dropped over a pane it lands as a header's drop would put it, anywhere else on the stage it joins the grid by the rule or is shown if the Workspace is empty; a drag with no travel is the click. (`ts:chrome.ts:376`, `ts:panes/split.ts:407-420`)
- SHELL-RAIL-18. After a redraw, the pane on screen is scrolled into view (nearest) **once per pane**, not per redraw, so a person reading the rail is not fought. (`ts:chrome.ts:153-162`)

### Behaviours: the picked row

- SHELL-RAIL-19. The rail's row last clicked (a checkout, or a pane and its checkout) is **picked**: it holds the keys until a press anywhere outside the rail or Escape; picking blurs the pane's text area; a picked row is drawn marked. A new tree does not move the pick; closing the picked pane moves it to its checkout. (`ts:chrome.ts:405-430,452-503`, `ts:main.ts:231-233`)
- SHELL-RAIL-20. `j`/`↓` and `k`/`↑` move the pick one row through **every checkout and its panes in the rail's order** across all repositories (a checkout is followed by its panes), stopping at the ends; a pane picked comes on screen (`select{pane}`) as a click does. (`ts:chrome.ts:435-449`; `rs:keys/keymap.rs:567-568`)
- SHELL-RAIL-21. `s`/`S` starts a shell on the picked row's checkout (SHELL-RAIL-31). (`ts:chrome.ts:463-465`)
- SHELL-RAIL-22. `a`/`A` starts the **default agent** (the first daemon template) on its checkout; with none, notice `argusd offers no agent to start`. (`ts:chrome.ts:466-471`)
- SHELL-RAIL-23. `x`/`X`: on a checkout, `remove_checkout{checkout}` (the Checkouts stage opens on it and asks first); on a pane, `close{pane}` and the pick falls back to its checkout. (`ts:chrome.ts:481-491`)
- SHELL-RAIL-24. `Enter`: on a checkout, toggles its + menu; on a pane, unpicks and gives the pane the keys. (`ts:chrome.ts:492-499`)
- SHELL-RAIL-25. `r`/`R` `open_review{checkout}`; `h`/`H` `open_history{checkout}`. (`ts:chrome.ts:472-477`)
- SHELL-RAIL-26. `z` folds or unfolds the picked row's checkout (`toggle_checkout`). (`ts:chrome.ts:478-480`)
- SHELL-RAIL-27. `Escape` (`closeLayer`) unpicks. Any other key with an action is the stage's (SHELL-KEYS-16); an unbound one tries extension commands in `anywhere`/`checkout`/`pane`. (`ts:chrome.ts:456-458`)

### Behaviours: the + menu

- SHELL-RAIL-28. `toggleStart(checkout, from)` opens a checkout's + menu where it was pressed (`from` is `rail` or the Checkouts `table`, which shares this state) with the first item highlighted, or closes it if that same one is open. A click anywhere else, `Escape`, or choosing an item closes it. (`ts:chrome.ts:524-571`, `ts:main.ts:220`)
- SHELL-RAIL-29. Items, in order: `Shell` (glyph `›`) then, after a rule, one `Agent · <template>` (glyph `◆`) per daemon template, **the first being the default** (the window's last-started first); the first two items show their key caps (`startShell`, `startAgent` in the `start` place, first chord only). The highlighted item is marked. (`ts:chrome.ts:509-514,536-560`)
- SHELL-RAIL-30. Keys while open (the `start` layer): `s` starts a shell; `a` the default agent (notice if none); `j`/`↓` and `k`/`↑` move the highlight with **wrap-around**; `Enter` starts the highlighted one; `Escape` closes. A click on an item does the same without picking. (`ts:chrome.ts:574-605`; `rs:keys/keymap.rs:655-659`)
- SHELL-RAIL-31. Starting closes the menu, **drops the pick** (the new pane takes the keys when it comes on screen) and sends `spawn_shell{checkout}` or `spawn_agent{checkout, template}`. Rust asks the daemon, never predicts, shows the pane once the tree holds it (`Made::Show`), and refuses an unknown template with `argusd has no agent template called <name>`. (`ts:chrome.ts:516-522`, `rs:session.rs:1288-1330`)

### Behaviours: width

- SHELL-RAIL-32. The rail's right edge drags (SHELL-EDGE-1..3): the width is drawn live, sent once on release as `rail_width{width}`, a double-click `rail_width{width:null}`; Rust holds it to `200..560` and **folds** a drag let go below 128 px to 56; kept in `desktop.json`; undragged the stylesheet's width stands. A width at or below 56 marks the window `rail-folded` (the strip's status chip returns because the rail row is unseen). (`ts:chrome.ts:1066-1088`, `rs:layout/dragged_width.rs`; test `a_rail_dragged_nearer_folded_than_its_minimum_folds`)
- SHELL-RAIL-33. Folded rail (container narrower than 120 px, or the window under 1200 px): stages by icon, panes by initials with the status dot at the corner, every name a tooltip; the heading, the extension nav, checkout rows, the wait placeholder, empty text, pane titles and pane ends are hidden. (`css:10251-10275`)
- SHELL-RAIL-34. While stopped, the rail is dimmed (SHELL-STAGE-14). (`ts:chrome.ts:63`)

### Page-held state

`railPick {checkout, pane|null}`, `start {checkout, from, at}`, the pane last scrolled to. (`ts:state.ts:62-73`, `ts:chrome.ts:160-162`)

### Page-side logic that must move to Rust

- `stepRail` row flattening (`ts:chrome.ts:435-449`), `railAction` (`:452-503`), `startAction` (`:574-605`), `starts()` and the `Agent ·`/`Shell` labels (`:509-514`), `paneMark` (`:330-342`), `statusColour`/`LOUD`/`PANE_SAYS` (`:33,107-115`), `foldCheckout`'s pick adjustment (`:415-420`), the scrolled-once rule (`:153-162`), `railWidth`'s fold threshold (`:1075`). `railPick` and `start` become Rust state in the session (one "rail cursor" model) so the keymap tests can cover them; AGENTS.md "client never predicts" still holds (they are view state, never daemon state).

### gpui-kit mapping

- **`Sidebar`** (`sidebar.md`) for the frame: `Sidebar::new().collapsed(bool).width(px).header(SidebarHeader::new().child(..).dropdown_menu(|menu,_,_| ..)).child(SidebarGroup::new(..).child(SidebarMenu::new().child(SidebarMenuItem::new(..).icon(..).active(bool).suffix(..).on_click(..).context_menu(|menu,_,_| ..)))).footer(SidebarFooter::new()..)`; theme tokens `sidebar`, `sidebar_foreground`, `sidebar_border`, `sidebar_accent`. It has **no** width drag (use `resize_handle`, EDGE), no per-item middle click, no drag-out.
- **`Tree`** (`tree.md`) for the rows: `tree(&state, |ix, entry, selected, window, cx| ListItem::new(ix).selected(selected).pl(px(16.) * entry.depth() + px(12.)).child(..).on_click(..))`, `TreeItem::new(id, label).expanded(bool).child(..)`, `TreeState::{items, set_items, set_selected_index, set_selected_item, selected_entry, scroll_to_item(ix, ScrollStrategy::Center)}`. Our expand/fold state is Rust's (`open` per row), so `set_items` is rebuilt from every `rail` event with `expanded` set from it; the picked row (SHELL-RAIL-19) is Rust's `railPick`, mapped onto `set_selected_item`. Key handling is documented (`tree.md`'s keyboard table: Up/Down select, Left collapse or parent, Right expand, Enter toggle, Space configurable), which collides with the rail's `j`/`k`/`Enter`/`x` contract: it must be off or routed to Rust's keymap (SHELL-KEYS-16), and `update_item_children` appears in an example but not in the API table.
- Row menus: `div().context_menu(|menu,window,cx| menu.label(..).menu(..).separator().menu_with_disabled(..))` (`menu.md`); danger styling and middle click are **not documented**: custom element via `menu_element(..)` and a native mouse handler. `Remove worktree…` asks first on the Checkouts stage (other group).
- Status dots and pills: `Badge::new().dot()` (`badge.md`: `.count(n)`, `.dot()`, `.icon(..)`) or `Tag` (`tag.md`); the people/branch/clock/chevron glyphs: `Icon`/`IconName` (names to be checked against the bundled Lucide set; the page draws its own SVGs, `ts:kit/dom.ts:10-110`, so each needs a mapping or an asset).
- Drag from the rail: custom (`on_drag`, SHELL-GRID-13). Test: `window.find(..).click`, `right_click`, `click_with_options(..Middle..)` (unconfirmed), `press("j")`, `drag_to(..)`.

### Scenes and coverage

- Scenes: `workspace-agent` (a pane on the rail), `workspace-empty` (project 2), `workspace-waiting`/`workspace-failed`/`workspace-exited` (status words), `switcher`, `connecting`, `stopped`. `dump_rail` is a text dump. **Not covered by a scene:** picked row, folded checkout, + menu open, context menus, folded rail, width drag. **Not covered by a test:** SHELL-RAIL-8 to -10, -13..-31 (all page logic); Rust covers listing and keymap membership only.

---

## SWITCH: project switcher and the removal question

Source: `ts:chrome/projects.ts:1-316`, `rs:chrome/switcher.rs`. `DESIGN.md §Process model` (switcher paragraph). Tests: `rs:chrome/switcher.rs` (`the_hosts_offered_are_the_config_s_then_the_visited_then_the_current`, `only_a_workspace_not_open_is_asked_for`, `removing_a_project_asks_first_and_only_a_yes_sends`); scene `switcher`.

### Behaviours

- SHELL-SWITCH-1. The heading is empty until the tree has loaded; then it is a button (`Switch project`, `aria-haspopup`, `aria-expanded`) showing the project's name, or `Projects` with none (it is how the first project is added). A press toggles the menu and folds its sub-lists. (`ts:chrome/projects.ts:195-222`)
- SHELL-SWITCH-2. The menu: a `Projects` label (when there are any) and one row per project, the open one checked, each with its tally (`N need you` loud, else `N working` active, else `quiet` muted); a rule (only when there are projects); `Add a project…`; `Switch workspace…` (only when the daemon lists workspaces; shows the open workspace's name; a press unfolds the workspaces in place); `Connect to a host…` (shows the connected host's name; unfolds the hosts); and, with a project, a rule and `Remove <name>…` (danger). (`ts:chrome/projects.ts:118-193`)
- SHELL-SWITCH-3. A project row closes the menu and sends `select_project{project}`; `Add a project…` closes and sends `open_add_project{}`; `Remove <name>…` closes and sends `remove_project{project}` (which asks, SHELL-SWITCH-6). (`ts:chrome/projects.ts:51-55,133-137,184-188`)
- SHELL-SWITCH-4. Unfolded **workspaces**: one radio row each (the open one checked) with `<n> project(s)`; choosing closes the menu and sends `open_workspace{workspace}` only if that one is not open. Rust asks the daemon only for a listed, not-open workspace; every client follows the daemon. (`ts:chrome/projects.ts:64-83`, `rs:chrome/switcher.rs:64-69`; test `only_a_workspace_not_open_is_asked_for`)
- SHELL-SWITCH-5. Unfolded **hosts**: `This machine` (checked when no remote is current) then each host (checked if current), or `No hosts in ~/.ssh/config, or set ARGUS_DESKTOP_HOST` when there are none; choosing closes the menu and sends `set_host{host}` (`null` for this machine) unless it is the current one. Hosts offered: the ssh config's, then those visited, then the current one if neither names it. Switching drops the old bridge and empties the tree back to connecting. (`ts:chrome/projects.ts:86-116`, `rs:chrome/switcher.rs:46-56`; test `the_hosts_offered_are_the_config_s_then_the_visited_then_the_current`)
- SHELL-SWITCH-6. The **removal question** (while `rail.removing` is set): a scrim and an alert dialog `Remove <name>?` with `Its N repository/repositories leave the sidebar. The folders stay where they are, and adding it again brings them back.`, and, when panes run in it, a warning `N pane is/panes are running in it; argusd removes it only once they are closed.`. Buttons: `Keep it` (Esc), focused without a ring, and `Remove` (danger). `Keep it`, the ✕, a press on the scrim and Escape send `answer_remove_project{yes:false}`; `Remove` sends `{yes:true}`. It is drawn once while held, removed when `removing` becomes null. (`ts:chrome/projects.ts:226-316`)
- SHELL-SWITCH-7. Rust: asking for a project the tree does not hold asks nothing; a no puts the question away; a yes sends `RemoveProject` once, the daemon leaving the folders alone and refusing while any pane runs; the question stays only while its project is in the tree. (`rs:chrome/switcher.rs:72-112`; test `removing_a_project_asks_first_and_only_a_yes_sends`)
- SHELL-SWITCH-8. Closing: the menu closes on Escape (it draws in the heading, not the overlay layer, so the Escape ladder asks it specially, SHELL-KEYS-19), on a press outside the heading (SHELL-SEAM-13), and after any choice. (`ts:chrome/projects.ts:19-32`)

### Page-held state

`menuOpen`, `workspacesOpen`, `hostsOpen` (`ts:state.ts:62-73`).

### Page-side logic that must move to Rust

- The tally precedence and plural forms (`ts:chrome/projects.ts:36-41,80,234,281`), `projectMenu`'s which-sections-appear rules (`:118-193`). The `…` menu items can be built in Rust as a `Vec<MenuItem>`; the unfold flags stay view state.

### gpui-kit mapping

- **`SidebarHeader::dropdown_menu`** (`sidebar.md` "Interactive Header with Popup Menu": `SidebarHeader::new().child(h_flex()..).dropdown_menu(|menu,_,_| menu.menu("Acme Corp", Box::new(SelectCompany("acme"))).separator()..)`) or `Button::dropdown_menu` with `PopupMenu` (`menu.md`): `menu_with_check(label, checked, action)` for the checked rows, `.label(..)` for `Projects`, `.submenu(label, window, cx, |submenu,..| ..)` for workspaces and hosts, `menu_with_disabled`, `.menu_element(..)` for the tally. Not documented: a menu row that unfolds **in place** (ours do) rather than as a submenu; `submenu` is the documented way, so the port's behaviour may differ (flag).
- **Removal question: `AlertDialog`** (`alert-dialog.md`): `.description(..)`, `.show_cancel(true)`, `.ok_variant(ButtonVariant::Danger)` (unconfirmed), `.on_ok(|_,window,cx| ..)` / `.on_cancel(..)` return `true` to close, `.on_close(..)`; ESC closes it. The default button focus (`Keep it`) is not documented: spike.

### Scenes and coverage

- Scene `switcher` (the heading clicked). **Not covered:** workspace/host unfolds, removal question (no scene), checkout of `Remove` flow; Rust covers the model.

---

## ADD: add-project dialog

Source: `ts:chrome/projects.ts:318-639`, `rs:chrome/add_project.rs` (526 lines). `DESIGN.md §Process model` (add-a-project paragraph, `ListDirectories`, `PreviewProject`). Tests: `rs:chrome/add_project.rs` (`opening_lists_where_the_daemon_starts_and_a_listing_asks_for_its_preview`, `an_answer_for_a_folder_since_left_is_dropped`, `a_folder_adds_what_the_preview_found_skipping_what_was_typed`, `one_repository_is_only_a_folder_that_is_one`, `a_new_repository_is_made_in_a_folder_named_inside_the_one_browsed`, `a_refused_add_keeps_what_was_typed_and_says_why`, `only_a_folder_the_listing_holds_is_gone_into`, `a_path_is_joined_in_the_daemons_own_spelling`); scene `add-project`.

### Behaviours

- SHELL-ADD-1. Opening (`open_add_project{}`, from the switcher or the no-project stage) lists where the daemon starts a browse (`ListDirectories`); the folder is on the **daemon's** machine, so listing and scanning go through the daemon, and each answer is taken only if it is for the last thing asked (a slow listing never lands in a folder since left). (`rs:chrome/add_project.rs:1-10,116-160`; tests `opening_lists_where_the_daemon_starts_and_a_listing_asks_for_its_preview`, `an_answer_for_a_folder_since_left_is_dropped`)
- SHELL-ADD-2. The dialog holds the keys: it takes them from a pane when it opens and gives them back when it closes (SHELL-SEAM-10); a newly opened dialog focuses the path field; keys never reach the stage under it. (`ts:chrome/projects.ts:533-541,610-639`)
- SHELL-ADD-3. A **kind** control with three options: `A folder of repositories`, `One repository`, `A new repository` → `add_project{gesture:{on:"kind", kind}}` (`folder`, `repository`, `new`). (`ts:chrome/projects.ts:324-328,377-394`)
- SHELL-ADD-4. The **path row**: an up button (`Up one folder`, disabled when the listing has no parent) sending `browse{path:parent}`, and the path field (mono, no spellcheck); `Enter` in it (the `writing` place's `choose`) sends `browse{path:<typed>}`. A redraw never overwrites a path being typed (the sent path replaces it only when the field is not focused). (`ts:chrome/projects.ts:396-424,626-628`)
- SHELL-ADD-5. The **folders list** (labelled `Folders in <path>`): browsing with nothing listed yet shows four skeleton bars; a listing error shows its text in the danger colour; an empty list says `No folders in here.`; otherwise one row per folder (`▸`, name, `git` or `folder`), tooltip `Open <name>`, a click sends `add_project{gesture:{on:"into", name}}`. Rust goes into a folder only if the listing holds it, joining the path with the daemon's own separator. (`ts:chrome/projects.ts:426-452`; tests `only_a_folder_the_listing_holds_is_gone_into`, `a_path_is_joined_in_the_daemons_own_spelling`)
- SHELL-ADD-6. The **outcome** beside it: for `new`, a field `New repository's folder` (placeholder `scratch`), the full target path if there is one (tooltip too) and `Made inside the folder on the left, and git init runs there. A folder that is already a repository is added as it is.`; while previewing (or a path with no depth yet and no error), `Looking for repositories…` with three skeleton bars; for `repository`, `Would add` + the default name when the folder is exactly one repository, else `Not a repository` + `Pick a folder marked git, or add this one as a folder of repositories.`; for `folder`, `Would add N repository/repositories`, the list (mono, ellipsised) and, once scanned, `Searched up to D folders deep. A repository's own submodules and worktrees stay part of it. New ones cloned here later join on their own.` (`ts:chrome/projects.ts:455-510`)
- SHELL-ADD-7. Fields: `Project name` (placeholder = the default name, the target's last segment) and, for `folder` only, `Also skip` (placeholder `vendor, infra/legacy`, with `<always skipped list> are always skipped` beside). Typing sends `add_project{gesture:{on:"fields", name, skip, folder}}` after a **250 ms** pause (not a call per letter); Rust re-scans only when the skip list changed, so the preview skips what the add will. (`ts:chrome/projects.ts:345-357,516-580`, `rs:chrome/add_project.rs:183-203`)
- SHELL-ADD-8. A field **keeps what is typed over what was sent**: a redraw reads the field's own text before the echo from Rust (an echo landing after the next letter would drop it) and puts the caret and selection back in the field that had focus. (`ts:chrome/projects.ts:332-343,610-639`)
- SHELL-ADD-9. The **Add** button says `Add <name>` where name is the typed name trimmed, else the default name, else `project`; while adding it says `Adding…`; it is disabled while `blocked` (its tooltip is the reason) or adding. A press cancels the pending fields timer and sends `add_project{gesture:{on:"add", name, skip, folder}}` with what the fields say now (calls need not arrive in order). Reasons Rust gives: `pick a folder`, `that folder cannot be read`, `name the new repository's folder`, `the new folder is one name, not a path`, `looking for repositories`, `this folder is not a repository`. (`ts:chrome/projects.ts:359-364,594-606`, `rs:chrome/add_project.rs:226-241`)
- SHELL-ADD-10. A refusal (the daemon's reason) is shown in the dialog as an alert in the danger colour and **keeps what was typed**; the dialog stays up until the daemon has made the project and a tree holds it. A repeated add while one is in flight is ignored. After a reconnect the folder is listed again. (`ts:chrome/projects.ts:581`, `rs:chrome/add_project.rs:250-285`; test `a_refused_add_keeps_what_was_typed_and_says_why`)
- SHELL-ADD-11. Close: the ✕, `Cancel` (with an `Esc` cap), a press on the scrim and `Escape` (`closeLayer` in `anywhere`) all send `add_project{gesture:{on:"close"}}` and clear the pending fields timer; closed, Rust sends the page its `Default` once. The footer says `Only Argus's config changes` (and, for `new`, `, and the new repository`). (`ts:chrome/projects.ts:366-369,525-607`)
- SHELL-ADD-12. A folder add scans for repositories at or beneath it, stopping at each (submodules stay part), skipping `.git`, `.argus`, `node_modules`, `target` and the typed list, no symlinks, at most eight deep; a worktree or bare repository is not a repository of its own; `new` runs `git init` (an existing repository is added without re-init). (`DESIGN.md §Navigation model`; daemon-side)

### Page-held state

The fields' own text and caret, the 250 ms timer, and which field is focused (all DOM today).

### Page-side logic that must move to Rust

- `outcome()`'s message selection and wording (`ts:chrome/projects.ts:455-510`), the `Add <name>` label (`:517`), `typed()`/caret restore (`:332-343,610-639`; in GPUI retained `InputState` entities solve it), the 250 ms debounce (`:349-357`). The debounce stays view-side (a timer on the view) but its constant and "send on Add what the fields say now" rule belong in one Rust fn with a test.

### gpui-kit mapping

- **`Dialog`** (`dialog.md`): `window.open_dialog(cx, |dialog,_,_| dialog.title("Add a project").child(..).footer(|_,_,_,_| vec![Button..]))`, `.keyboard(true)` (ESC), `.overlay_closable(true)`, `window.close_dialog(cx)`. **The builder may run again** when the dialog layer is rebuilt, so inputs must be persistent `Entity<InputState>` created before opening, and initial focus uses a one-time `Cell<bool>` guard (`dialog.md` "Builder lifetime and initial focus"): this is how SHELL-ADD-8 is kept.
- Fields: `Input` + `InputState::new(window,cx).placeholder(..)`, events `InputEvent::{Change, PressEnter{secondary}, Focus, Blur}`, `set_value(text, window, cx)` (does **not** emit `Change`, `input.md`) so an echo can be applied without a loop; `Input::prefix/suffix`, `.cleanable(true)`.
- Kind control: `TabBar::new(id).segmented().selected_index(i).on_click(..)` (`tabs.md`) or `Radio`/`RadioGroup` (catalog); folders list: `List` (`list.md`: `ListDelegate::{items_count, render_item, loading, render_loading}` with `Skeleton`), rows with `ListItem`; up button `Button::ghost().icon(IconName::ArrowUp)`.
- Not documented: path-completion (listed in `GPUI.md`'s hard parts "path completion"); `Input` has no completion API in `input.md` (completion menu is mentioned only as "an open completion menu" in the Editor note). Rust already supplies the listing; completion is the folder list.

### Scenes and coverage

- Scene `add-project` (the initial browse). **Not covered by a scene:** each kind, an error, adding. Covered by Rust tests (model), none for the page's debounce, typing-preservation or button label.

---

## MENU: menus and right-click

Source: `ts:kit/menu.ts` (131 lines), `ts:kit/middle.ts`. No Rust. Used by RAIL, DOCK, CONSOLE, SWITCH (and other groups' filters). `DESIGN.md` none (§Process model says "a right-click on anything that offers no menu opens nothing").

### Behaviours

- SHELL-MENU-1. A menu is a list of items: a **choice** (label, action, optional `disabled` reason, optional hint, optional `danger`), a **heading**, or a **rule**. A disabled item cannot be chosen and shows its reason in place of its hint. (`ts:kit/menu.ts:10-20,41-71`)
- SHELL-MENU-2. A menu opens in the overlay layer at the pointer (a right-click) or under a button (x = the button's left, or its right edge when `align: end` so the menu hangs from its right; y = bottom + 4), is kept inside the window with an **8 px** margin on every side, and its first enabled item takes the focus. (`ts:kit/menu.ts:28-39`)
- SHELL-MENU-3. Choosing an item **closes the menu first, then runs** its action. (`ts:kit/menu.ts:62-66`)
- SHELL-MENU-4. While a menu is open it holds the keys: Down/Up (and `j`/`k` in the `menu` place) move focus among the enabled items with wrap-around, caught before the stage's keys; Enter is the focused button's own; Escape closes it (SHELL-KEYS-19). A press outside the layer closes it. Stage actions are blocked while a menu is up (SHELL-KEYS-18). (`ts:kit/menu.ts:100-131`, `rs:keys/keymap.rs:662-663`)
- SHELL-MENU-5. A right-click on an element that offers a menu opens it (built when pressed) and stops there; a right-click on anything else opens **nothing**, so the webview's own menu (Back, Reload, Inspect) never shows over the window; a text field keeps the native menu for cut and paste; with text selected elsewhere, a one-item menu `Copy` (heading `Selection`) writes it to the clipboard. (`ts:kit/menu.ts:74-107,114-124`) **Gap:** that Copy writes through `navigator.clipboard` in the page, while the system comment in `ts:system/copy.ts` says Rust writes the clipboard so the page is never granted it; the port should write through Rust's clipboard path.
- SHELL-MENU-6. A **middle click** closes what it lands on, as a browser tab does: a dock tab, a view's tab or strip, a console tab, a rail pane row; the middle press itself is prevented (the webview's autoscroll would swallow the click), and the click does not bubble. Each surface says what its close is. (`ts:kit/middle.ts:6-19`)

### Page-held state

None (the overlay layer's children).

### Page-side logic that must move to Rust

- The clamp-into-window arithmetic and the focus-first-enabled rule (view code); menu **assembly** is per surface (see RAIL, DOCK, CONSOLE, SWITCH).

### gpui-kit mapping

- **`PopupMenu`** (`menu.md`): `div().id(..).context_menu(|menu,window,cx| ..)` (right-click), `Button::new(..).dropdown_menu(|menu,window,cx| ..)` and `dropdown_menu_with_anchor(Anchor::TopRight, ..)` (under a button), `.label(..)` (heading), `.separator()`, `.menu(label, Box<dyn Action>)`, `.menu_with_disabled(label, action, bool)`, `.menu_with_check(..)`, `.item(PopupMenuItem::new(..).disabled(..).icon(..).on_click(|window,cx| ..))` for items without an `Action`, `.menu_element(action, |window,cx| element)` for a custom row, `.submenu(..)`, `.scrollable(true).max_h(px)`, `.min_w/.max_w`. **Not documented:** a disabled-reason hint, danger styling of one item, whether keyboard navigation wraps and accepts `j`/`k`, whether a menu can be placed at an arbitrary point (a context menu is placed by the library), and the 8 px clamp; spike each. Middle click and the text-field `Copy` menu: `Input::context_menu(|menu,..| menu.menu("Copy", Box::new(input::Copy)))` is documented for text fields (`input.md` "Context Menu"); everywhere else a right-click opens nothing in GPUI by default (no webview menu to suppress). `menu.md` documents no danger styling, no disabled reason, no `j`/`k` or wrap, no placement at a point and no 8 px clamp (all unconfirmed), and it writes `check_side` in one place and `check_size` in another.
- Test: `window.within("popup-menu")` scoping is documented for submenus (`test.md`); `right_click(id, cx)`, `press("down")`.

### Scenes and coverage

- Scene `switcher` (a menu-like popover); `notice`. **Not covered:** every context menu, clamping, wrap.

---

## RENAME: rename dialog

Source: `ts:panes/rename.ts` (63 lines), `rs:session.rs:1336-1345`. No tests of the page; Rust: none named.

### Behaviours

- SHELL-RENAME-1. `Rename…` on a pane row, a strip menu, a console tab or a split side opens a dialog `Rename <title>` over a scrim in the overlay layer: a labelled `Name` field filled with the current title and **selected**, focused; a note `An agent may rename its own pane later.`; buttons `Cancel` (Esc cap) and `Rename`. (`ts:panes/rename.ts:90-143`)
- SHELL-RENAME-2. `Enter` (the `writing` place `choose`) or `Rename` closes it and sends `rename_pane{pane, title}` with the trimmed text **only if** it is non-empty and different from the old title. Escape, ✕, `Cancel` and a press on the scrim close it and send nothing. (`ts:panes/rename.ts:92-100`)
- SHELL-RENAME-3. The row keeps its old name until the daemon's tree says the new one (a refused rename shows what is true). A daemon that cannot rename says `argusd is too old to rename panes: run argus server restart`; Rust sends the rename only for a pane the tree or the consoles hold. (`rs:session.rs:1336-1345`)

### Page-held state

None.

### Page-side logic that must move to Rust

- "send only if changed and non-empty" (`ts:panes/rename.ts:93-95`), a one-line rule; belongs in `Session::rename`.

### gpui-kit mapping

- `Dialog` (`dialog.md`) + `Input` (`input.md`: `InputState::new(..).default_value(title)`, `InputEvent::PressEnter`, `.clean_on_escape()`); one-time focus guard; no select-all-on-open API is documented in `input.md` (the doc names `set_value` and `.default_value`): spike. 

### Scenes and coverage

- No scene. Not covered.

---

## FOOT: sidebar foot, status, tallies

Source: `ts:chrome.ts:757-856`, `rs:chrome/status.rs` (207 lines). `DESIGN.md §Process model` (foot: "the connection, the chime and the settings at its foot"; "a daemon from another build") and `§Panes and terminal state` (chime). Tests: `rs:chrome/status.rs` (`a_daemon_from_another_build_is_an_alarm_until_the_next_connection`, `a_late_greeting_from_another_build_raises_the_alarm`, `stopping_is_remembered_until_a_connection_succeeds`, `the_page_hears_the_status_once_per_change`, `switching_the_chime_says_so_and_asks_to_be_saved_once`).

### Behaviours

- SHELL-FOOT-1. The foot has up to three rows. Tallies (only while connected): `▲ N need you` in the attention colour when any agent in **any project** needs a person (waiting, needs-review or failed), and `⋮ N working` in the accent when any is working; nothing when neither. (`ts:chrome.ts:757-766`; counts from `rs:chrome/rail.rs:48-49`)
- SHELL-FOOT-2. An alarm row (attention colour) shows `Status.alarm` only while connected: `argusd <v>, window <ours>: run argus server restart` when the daemon is another build, or the protocol refusal; it stays until the next connection and a late greeting can raise it. (`ts:chrome.ts:786-789`, `rs:chrome/status.rs:80-95,190-200`)
- SHELL-FOOT-3. The status row has the same shape in every state so the foot does not rearrange: a dot, then `argusd · <host>` (`local` for this machine) while connected, plain `argusd` while connecting or stopped. The dot is the attention colour while connecting and hollow while stopped; its tooltip and label say `argusd · connecting` / `argusd · stopped` / the connected words. A remote host is named so the rail is never taken for this machine's. (`ts:chrome.ts:767-799`)
- SHELL-FOOT-4. A **console chip** (`Show the console` / `Hide the console` with its keys, e.g. `(\`)`), pressed while the console is not hidden, with a count of consoles when there are any; press sends `console{gesture:{on:"toggle"}}`. (`ts:chrome.ts:802-823`)
- SHELL-FOOT-5. A **chime switch**: a bell (on) / struck bell (off), pressed when on; labels `Mute the chime when an agent needs you` / `Chime when an agent needs you`; press sends `set_chime{on:!on}`. Rust saves it once per change in `desktop.json`; the chime is on by default. (`ts:chrome.ts:825-839`, `rs:chrome/status.rs:103-118`)
- SHELL-FOOT-6. A **settings** gear titled `Settings (<keys>)` opens the Settings dialog (SHELL-SETTINGS-1). (`ts:chrome.ts:841-856`)
- SHELL-FOOT-7. Status model (Rust): `Connecting` (with the last failed try's words), `Connected` (with the greeting; an alarm), `Stopped` (nothing reconnects until Reconnect); the page hears the status once per change and again after a reload. Switching host goes back to connecting and drops the alarm. (`rs:chrome/status.rs:38-95`; tests above)

### Page-held state

None.

### Page-side logic that must move to Rust

- The tally text and shapes, the `argusd · <host|local>` wording, label strings, key-hint titles (`ts:chrome.ts:757-856`). Hand the view a `FootPage`.

### gpui-kit mapping

- **`SidebarFooter`** (`sidebar.md`: `SidebarFooter::new().justify_between().child(..)`), not `StatusBar` as `GPUI.md` lists: `StatusBar` (`status-bar.md`: `StatusBar::new().left(..).right(..).child(..)`, ghost xsmall `Button`s, `Separator::vertical()`, theme tokens `status_bar`) is a **window-bottom** bar; the page has no such bar (the foot lives in the sidebar). Keep `StatusBar` for a later bottom bar if wanted. Buttons: `Button::new(..).ghost().xsmall().icon(..)` with `.tooltip(..)`; the chip's count: `Badge::new().count(n)` (`badge.md`); chime: `Toggle`/`Switch` (`switch.md`: `Switch::new(id).checked(bool).on_change(|checked,_,_| ..)`) or a pressed `Button`.

### Scenes and coverage

- Scenes: `workspace-agent` (tallies), `connecting`, `stopped`, `notice`. **Not covered:** the alarm row (no scene with a version mismatch), chime states; Rust covers the model.

---

## NOTICE: notices that pass

Source: `ts:chrome.ts:1043-1064`, `Event::Notice` (`rs:connection/page.rs`); producers throughout `rs:session.rs`. Scene `notice`.

### Behaviours

- SHELL-NOTICE-1. A notice is one line along the bottom of the window with an info icon, the text and a dismiss `✕` (`Dismiss`); it replaces the one showing and restarts a **6 s** timer; dismissing or the timer hides it. It is `role="status"`. (`ts:chrome.ts:1045-1060`)
- SHELL-NOTICE-2. What notices say (Rust, exact text is the contract of other surfaces): the daemon's refusal reason (`RequestRefusal`), `argusd offers no agent to start` (page), `could not copy: <err>`, `could not read <name>: <err>`, `nothing on the clipboard to paste`, `pasted <name>` / `<name> is too large to paste (<n> MB)` / `argusd is too old to paste files: …`, `argusd is too old to rename panes: …`, `no pane is on screen`, the palette's `the palette has no command …`, the grid refusals (SHELL-GRID-6), `the pane with the keys is not split beside another`, `no extension view has been opened`, a rejected backdrop's reason, `<title> was closed`, and an opener failure `only an http or https link is opened`. (`rs:session.rs` `note`, `passing.push(Event::Notice ..)`; `rs:connection/commands.rs`, `rs:system/opener.rs`)

### Page-held state

The timer and the line.

### Page-side logic that must move to Rust

- None beyond the 6 s constant; the timer is view state.

### gpui-kit mapping

- **`Notification`** (`notification.md`): `window.push_notification("text", cx)`, `Notification::info(..)`, `.autohide(bool)` (default **5 s**, pauses while hovered or focused, so 6 s is a deliberate difference), `.placement(Anchor::BottomCenter)` (supported anchors listed, default top right), unique `.id::<T>()` to replace the previous one (unconfirmed for in-app toasts: the doc states it only for system notifications) (matches "replaces the one showing"), a close affordance. Not documented: a dismiss button on every toast (the "With Action Button" example adds one).

### Scenes and coverage

- Scene `notice` (a refused spawn). Not covered: timer, replace, dismiss.

---

## TITLE: title strip and window buttons

Source: `ts:system/frame.ts` (51 lines), `rs:system/window_frame.rs` (194 lines), `rs:connection/commands.rs`, `rs:main.rs:109-137`. `DESIGN.md §Process model` (title strip). Tests: `rs:system/window_frame.rs` (`a_colorref_reverses_the_stylesheets_byte_order`, `the_border_takes_the_line_colour_the_stylesheet_names`, `every_theme_has_a_palette_and_terminal_colours`).

### Behaviours

- SHELL-TITLE-1. The system title bar is put away (`decorations(false)`; macOS keeps its traffic lights over an overlay title bar with a hidden title); the sidebar runs to the window's top edge and a strip across the stage names what it shows beside the window's own buttons. The window opens `1200 x 800` with a `720 x 480` minimum, titled `Argus`, built transparent (always: it can only be set at creation). (`rs:system/window_frame.rs:14-24`, `rs:main.rs:109-124`)
- SHELL-TITLE-2. A left press on the strip's or the sidebar brand row's empty space (not on a button, input, link or menu) moves the window (`drag_window{}`); a double press maximizes or restores (`toggle_maximize{}`). (`ts:system/frame.ts:16-24`; `rs:system/window_frame.rs:60-80`)
- SHELL-TITLE-3. Buttons: minimize (`minimize{}`), maximize/restore (`toggle_maximize{}`), close (`close_window{}`, which ends the program). On macOS the page's buttons are hidden and room is left for the traffic lights. (`ts:system/frame.ts:26-28`, `rs:system/window_frame.rs:60-80`)
- SHELL-TITLE-4. The maximize button turns to restore (and its label says `Restore` / `Maximize`) while the window fills the screen, read from the window's size: no call. (`ts:system/frame.ts:46-51`)
- SHELL-TITLE-5. **Windows 11 snap layouts:** resting the pointer on maximize for **600 ms** sends `snap_layouts{}`, which focuses the window and presses Win-Z; leaving or pressing the button before then cancels it. (`ts:system/frame.ts:11,30-40`, `rs:system/window_frame.rs:83-97,116-135`)
- SHELL-TITLE-6. The one-pixel border Windows 11 draws round a frameless window is tinted to the sidebar's line colour of the theme drawn (`border(theme)`: a table of 12 colours); repainted when the theme or the system's light/dark changes; Windows 10 draws none (the failure is ignored). (`rs:system/window_frame.rs:26-58`)

### Page-held state

None.

### Page-side logic that must move to Rust

- The 600 ms hover timer (window-side), `drawMaximized` (ask the window), the 12-entry border table (already Rust), the drag-region rule.

### gpui-kit mapping

- **`TitleBar`** (`title-bar.md`): `TitleBar::new().child(..)`, `TitleBar::window_options()` (also sets `app_owns_titlebar_drag`), `.on_close_window(..)` (Linux), 34 px height, left padding 80 px macOS / 12 px elsewhere, Windows buttons 34 px each with `WindowControlArea`; Linux drag, double-click maximize, right-click window menu. **Not documented:** Windows 11 snap-layout hover (what `GPUI.md` flags) and DWM border colour via the raw HWND (`GPUI.md` "via the raw HWND"): spike. The sidebar running to the top edge conflicts with a title bar spanning the width; check whether `TitleBar` can be placed over the stage column only. Windows double-click maximize is not stated in `title-bar.md` (unconfirmed).
- Test: `simulate_window_resize`; window buttons through `window.find(..).click`.

### Scenes and coverage

- Every scene shows the strip. **Not covered:** drag, maximize, snap, border colour tests (border table only).

---

## SETTINGS: Settings dialog

Source: `ts:settings/preferences.ts` (656 lines), `rs:settings/preferences.rs` (272 lines), `rs:settings/remembered.rs` (519 lines). `DESIGN.md §Configuration` (`desktop.json`), `§Editors and overlays`. Tests: `rs:settings/preferences.rs` (`a_change_is_heard_and_saved_once`, `choosing_what_is_already_chosen_is_no_change`, `the_opacity_never_goes_below_the_least_the_slider_offers`, `the_zoom_steps_as_a_browser_does_and_stops_at_either_end`, `a_hand_edited_zoom_is_held_to_the_steps_ends_and_steps_onto_them`, `an_empty_editor_command_leaves_the_daemon_to_the_environment`, `fonts_found_are_heard_with_the_choices`, `a_font_family_keeps_nothing_that_could_end_its_quotes`, `a_backdrop_this_machine_cannot_draw_is_refused_and_clear_never_is`), `rs:settings/remembered.rs` (`settings_saved_are_the_settings_loaded`, `a_blank_host_is_this_machine`, `a_missing_or_broken_file_remembers_nothing`, `a_file_from_before_the_settings_dialog_looks_as_the_window_always_did`, `following_the_system_is_mocha_in_the_dark_and_latte_in_the_light`, `the_settings_file_names_each_choice_as_the_page_does`); scene `settings`.

### Behaviours

- SHELL-SETTINGS-1. Opens from the foot's gear, `Ctrl ,`, leader `,`, the palette's `Open the settings`, or the connecting stage's `Settings` button; does nothing when already open or when no `settings` event has arrived. Opening closes the palette and the sheet (SHELL-KEYS-13), blurs the pane's text area and focuses the close button without a ring; closing returns focus to what had it. (`ts:settings/preferences.ts:285-290,546-569`)
- SHELL-SETTINGS-2. It closes on Escape **from any control, a field included**, on the settings key again, on ✕, and on a press on the backdrop; its keys never leave it (the layer answers its own, `settings` has no places). (`ts:settings/preferences.ts:537-557`, `rs:keys/keymap.rs:857`)
- SHELL-SETTINGS-3. Layout: header `Settings` + `Saved as you change them` + ✕; a left nav of six sections with icons (Appearance, Editor, Notifications, Features, Views, Connection), a version line `argus <version>` at its foot; a right body of those sections in one scrolling page, each titled with one line under it (`The theme, the background and the fonts`; `Where a file opens, and with what`; `When Argus tells you something needs you`; `How the feature index opens`; `Where the links in a saved view open`; `The daemon this window reaches`). A nav button scrolls its section to the top; **the section in view is lit in the nav** (the last section whose top is within 24 px of the body's top). (`ts:settings/preferences.ts:106-133,478-535`)
- SHELL-SETTINGS-4. **Every change is saved as it is made**: a control sends `setting{change}` at once (`theme`, `opacity`, `backdrop`, `editor`, `editorCommand`, `notify`, `notifyInBackground`, `featureLayout`, `savedLinks`, `font`, `uiFont`) and the dialog draws what Rust sends back, not what the control did. A choice equal to the current one is no change (not heard, not saved). (`ts:settings/preferences.ts:137-139`, `rs:settings/preferences.rs:46-79`; tests `a_change_is_heard_and_saved_once`, `choosing_what_is_already_chosen_is_no_change`)
- SHELL-SETTINGS-5. The dialog is **updated in place**, never rebuilt, so a slider being dragged or a command being typed keeps its hold while the choices come back: the opacity slider's value is not overwritten while it has the focus, the editor command's text is not overwritten while it has the focus. (`ts:settings/preferences.ts:1-8,606-653`)
- SHELL-SETTINGS-6. **Appearance.** (a) Twelve theme cards (Argus Night, Ledger, Carbon, Flexoki, Kanagawa, Rosé Pine, Nord, Radix, Mocha, Macchiato, Frappé, Latte), each a small preview in its own palette and its name, a radio group `Theme`; the card marked is the flavour **drawn** (the system's resolves to Mocha in the dark, Latte in the light); a card sends `theme`. (b) `Follow the system` switch (`Mocha when it is dark, Latte when it is light`), on when the theme is `system`; turning it off keeps the flavour that was being drawn, turning it on sends `system`. (c) `Background opacity` slider from `minOpacity` (30) to 100 step 1 with its `N%`, sending `opacity` on every move; Rust clamps `30..100`, also for a hand-edited file. (d) `What shows through` (Clear / Mica / Acrylic) **only when more than Clear is offered**; an unavailable choice is disabled with its reason as tooltip and the reasons are listed under the row joined by `. `. (e) `Code font` (monospaced installed families) and `Interface font` (all of them), SHELL-FONT; each has a note when its family does not exist. (`ts:settings/preferences.ts:84-88,290-476,606-653`; tests `the_opacity_never_goes_below_the_least_the_slider_offers`, `following_the_system_is_mocha_in_the_dark_and_latte_in_the_light`)
- SHELL-SETTINGS-7. **Editor.** `Files open` (`A known GUI editor always opens on its own`): Floating / In the checkout / Separately → `editor{mode}`; `Editor command` (`Empty uses $VISUAL, then $EDITOR`, mono, placeholder `nvim`) → `editorCommand` on every keystroke. Rust: an empty command (after trim) leaves the daemon to the environment; `Separately` launches without a pty and is not asked for over ssh; `In the checkout` is floated by the daemon as an editor is never a listed pane. (`ts:settings/preferences.ts:90-94,380-391,445-449`, `rs:settings/preferences.rs:104-110`; test `an_empty_editor_command_leaves_the_daemon_to_the_environment`)
- SHELL-SETTINGS-8. **Notifications.** `When an agent needs you` switch (`A system notification naming the pane: waiting, needs-review or failed`) → `notify{on}`; `Only while this window is in the background` switch → `notifyInBackground{only}`, **disabled while the first is off**. **Features**: `Feature index` List / Board → `featureLayout` (what a repository without its own opens as). **Views**: `Links in a saved view open` Dialog / Column → `savedLinks{opens}`. **Connection**: the placeholder `Nothing to set here yet.` (`ts:settings/preferences.ts:392-475`)
- SHELL-SETTINGS-9. Defaults: theme `system`, opacity 100, backdrop `clear`, editor `floating`, editor command empty, notify on, notify-in-background-only on, layout `list`, saved links `dialog`, fonts empty, zoom 100. A file from before the dialog looks as the window always did; each field keeps its default when the file predates it. Stored flat in `desktop.json` beside the dock sizes, written beside the file and renamed over it, so a close mid-write keeps the old settings. (`rs:settings/remembered.rs:96-140,218-290,350-390`; tests `a_file_from_before_the_settings_dialog_looks_as_the_window_always_did`, `settings_saved_are_the_settings_loaded`)
- SHELL-SETTINGS-10. A backdrop this machine cannot draw is refused with its reason as a notice and not stored; `clear` is never refused; a stored one that cannot be drawn falls back to `clear` on load. (`rs:settings/preferences.rs:33-60`; test `a_backdrop_this_machine_cannot_draw_is_refused_and_clear_never_is`)
- SHELL-SETTINGS-11. Font names are made safe in Rust (a quote, `'`, `\`, `;`, `{`, `}` and control characters removed, trimmed) for the file and the setting alike, so a hand-edited name cannot break the font stack. (`rs:settings/preferences.rs:140-150`; test `a_font_family_keeps_nothing_that_could_end_its_quotes`)
- SHELL-SETTINGS-12. The installed families arrive a moment after the window opens (`fonts` empty until read; the picker says so); each family is offered under **every name the platform might know it by** (legacy and typographic) because a webview knows it by one. (`rs:settings/fonts.rs:1-70`, `ts:settings/font_picker.ts:63-66`)
- SHELL-SETTINGS-13. Row detail copy a port must reproduce (`ts:settings/preferences.ts:341,358,374,436,463,471,600`): opacity, 'The desktop shows through the background; text and borders stay solid'; backdrop, 'Clear shows the desktop; Mica and Acrylic blur it'; code font, 'Panes and code, monospaced; a Nerd Font draws its icons'; interface font, 'Menus, lists, headings and prose'; feature index, 'Remembered per repository; this is what a new one opens as'; saved links, 'A dialog over the stage, or the next column beside the view'. The missing-font note reads 'No font here is called that, so <face> draws'.

### Page-held state

The dialog's controls (retained), the previously focused element, `dark` media query. (`ts:settings/preferences.ts:186-207`)

### Page-side logic that must move to Rust

- `drawn(theme)` (system to mocha/latte, `:142-145`; Rust has `Theme::resolve`, use it), `FLAVOURS` names (`:33-82`), `defaultFaces()` which **reads the bundled face names from the DOM's computed CSS** (`:148-160`; the Rust theme table must own them), `draws(family)` which asks a canvas whether the webview can draw each installed family (`:571-595`) and filters the list with it (`:632`): GPUI sees the same installed fonts as Rust, so drop it but check the legacy/typographic naming (SHELL-SETTINGS-12), `missingNote` (`:598-602`), the scroll-spy rule (`:500-511`), the section/about/icon tables (`:106-133`), `setSegment`/`setToggle` bookkeeping (view code).

### gpui-kit mapping

- **`Settings`** (`settings.md`, imported from `gpui_kit::component::setting`): `Settings::new(id).pages(vec![SettingPage::new("General").icon(..).default_open(true).group(SettingGroup::new().title(..).item(SettingItem::new(title, SettingField::switch(|cx: &App| bool, |val, cx: &mut App| ..)).description(..).disabled(bool).keywords([..]).layout(Axis::Vertical)))])`; fields `SettingField::{switch, checkbox, input, dropdown(options, get, set), number_input, render(|options,_,_| ..), element(..)}`; `.with_group_variant(GroupBoxVariant::..)`, built-in **search** over titles, descriptions and keywords. **Differences to settle:** its pages are selected (a sidebar of pages), not one scrolling document with a scroll-spy (SHELL-SETTINGS-3 lit-nav is custom or dropped); its fields read and write through `&App` closures, so Rust's `Preferences` must live in an app `Global` (`references/gpui/global.md`); its `SettingItem::disabled` covers SHELL-SETTINGS-8's dependent switch; theme cards, the slider, segmented controls and the font picker are `SettingField::render`/`element` custom fields. The pages-not-one-scroll difference is an inference from `settings.md` (unconfirmed).
- **`Slider`** (`slider.md`): `SliderState::new().min(30.).max(100.).step(1.).default_value(..)`, `Slider::new(&state)`, `SliderEvent::Change(value)` with `value.start()`; **retained state is the point** (`GPUI.md` "retained state so a dragged slider keeps its hold"). Not documented: setting the slider's value programmatically after construction (only `default_value` and `.value()` are shown): needed to apply Rust's echo while not dragging; spike. `SliderEvent::Release(SliderValue)` is also listed in `slider.md` and fits the send-once-on-release need (SHELL-SETTINGS-5) better than `Change`; no `set_value` is documented (unconfirmed).
- **`Switch`** (`switch.md`: `Switch::new(id).checked(bool).on_change(|checked,_,_| ..)`; "the owner stores the value and calls `cx.notify()`"), **`Select`** (`select.md`: `SelectState::new(items, Some(IndexPath), window, cx)`, `SelectItem::{title, value, matches, display_title}`, `.searchable(true)`) for the backdrop if a dropdown is preferred, **`TabBar::segmented()`** (`tabs.md`) or `RadioGroup` for the segmented choices, **`Input`** (`input.md`) for the editor command (`InputEvent::Change`, `set_value` does not emit `Change`, so the echo is loop-free while focused).
- Test: `window.click`, `press`, `input`, `pointer_down/move/up` for the slider mid-drag (`test.md`).

### Scenes and coverage

- Scene `settings` (seven fonts offered). **Not covered:** every change's effect, the disabled dependent switch, the lit nav, in-place update (slider hold), an unavailable backdrop (the scene's platform offers only Clear off Windows). Rust covers the model fully.

---

## FONT: font picker

Source: `ts:settings/font_picker.ts` (183 lines), `rs:settings/fonts.rs`. Test: `rs:settings/fonts.rs` (families by name, once, monospaced if any face is; names with a leading dot or a quote skipped).

### Behaviours

- SHELL-FONT-1. A field that is both a text box and a combo list: a family typed, or picked from the installed ones listed under it, each **drawn in its own face**; the field itself is drawn in the chosen family (or the bundled fallback when empty, with that face's name as placeholder). `aria-expanded` follows the list. (`ts:settings/font_picker.ts:36-55,174-182`)
- SHELL-FONT-2. Focus or a click opens the list on the **chosen family**; with nothing typed the list is whole: first row the default face (`<name>`, family `""`), then every family; the chosen row has a ✓. Typing narrows the list to families **containing** the typed text (case-insensitive) and resets the highlight to the top; rows then omit the default row. Notes: `Reading the installed fonts…` while none are known, `None installed matches; Enter uses it as typed` when nothing matches (notes are not choosable). (`ts:settings/font_picker.ts:57-102`)
- SHELL-FONT-3. Down/Up (the `palette` place, so no `j`/`k`) open a closed list or move the highlight among choosable rows, **clamped at the ends**, scrolled into view. Enter picks the highlighted family (`choose(family)`, list closed, field shows it) or, with the list closed or on a note, commits what was typed (trimmed, only if typed and different) and closes. A press on a row picks it and the field keeps the keys. (`ts:settings/font_picker.ts:109-164`)
- SHELL-FONT-4. **Escape with the list open** closes only the list: it forgets what was typed, resets the field to the chosen family and the event goes no further, so the dialog stays (the next Escape closes the dialog). Leaving the field commits what was typed, closes the list and resets the field to the chosen family. A half-typed name never redraws the window in the fallback face because nothing is chosen until picked, Enter or blur. (`ts:settings/font_picker.ts:1-5,136-163`)
- SHELL-FONT-5. An echo from Rust updates the chosen family and the families without disturbing a field being typed in (the text stays; the list redraws only if the families changed); a theme change renames the default face. (`ts:settings/font_picker.ts:166-182`)
- SHELL-FONT-6. Each pick/commit is one `setting{change:{set:"font"|"uiFont", family}}`; `Code font` offers only families that report monospaced (the panes' grid needs even cells). (`ts:settings/preferences.ts:347-366,635-642`)

### Page-held state

Typed flag, highlight index, row list, families, chosen. (`ts:settings/font_picker.ts:28-34`)

### Page-side logic that must move to Rust

- Filtering (substring, `typed`), row model with notes, highlight clamp, commit-on-blur/Enter rule: a pure `FontPickerModel` with tests (none today).

### gpui-kit mapping

- **`Combobox`** (`combobox.md`): `ComboboxState::new(SearchableVec::new(items), vec![IndexPath], window, cx).searchable(true)`, `Combobox::new(&state).placeholder(..).search_placeholder(..)`, `SearchableListItem::{title, value, matches(query), disabled}`, `ComboboxEvent::{Change, Confirm}`, `.render_trigger(|ctx,_,cx| ..)` (custom trigger: draw the chosen family in its own face), `.footer(..)`, `set_selected_values`. Custom item rendering (per-row faces) is claimed in the doc's comparison table, but no API for it is shown (unconfirmed). **Gaps to spike:** (1) selection only: there is no documented way to commit a **typed value that is not in the list** (SHELL-FONT-3/4 `Enter uses it as typed`); (2) per-row font faces: the table says custom item rendering is supported but this page does not show its API; (3) the exact Escape/blur behaviour. `Select` (`select.md`) with `SelectItem::display_title()` is the other candidate and has the same limits. If (1) fails, the field is `Input` + `List`/`Popover` custom.

### Scenes and coverage

- Scene `settings` (list closed). Not covered: any behaviour here; Rust covers the families list.

---

## LOOK: theme, opacity, backdrop, zoom

Source: `ts:settings/preferences.ts:142-183`, `rs:settings/backdrop.rs` (113 lines), `rs:system/window_frame.rs`, `rs:connection/commands.rs:72-95`. Tests: `rs:settings/backdrop.rs` (`off_windows_only_clear_is_offered`, `windows_11_offers_all_three`, `windows_10_greys_mica_out_and_says_why`, `clear_can_always_be_drawn`).

### Behaviours

- SHELL-LOOK-1. The window's look follows the settings **whether or not the dialog is open**, on every `settings` event: the theme, the opacity, the two font families. (`ts:settings/preferences.ts:165-176,606-609`)
- SHELL-LOOK-2. **Theme**: thirteen values (`system` and twelve flavours: mocha, macchiato, frappe, latte, night, ledger, carbon, flexoki, kanagawa, rosePine, nord, radix); each is a role set (backgrounds, foregrounds, accent, lines, status colours, the 16 ANSI colours) with derived tints; `system` follows the OS light/dark (Mocha / Latte) and is re-applied when the OS changes. Only Latte is light. (`rs:settings/remembered.rs:146-195`, `GPUI.md` "What else changes: Theme")
- SHELL-LOOK-3. **Opacity**: `opacity/100` is the alpha of the background fills only; text and borders stay solid; it never rebuilds the window (the window is always created transparent). (`ts:settings/preferences.ts:171`, `rs:settings/backdrop.rs:1-9`)
- SHELL-LOOK-4. **Fonts**: a chosen family sits in front of the bundled faces, which still draw what it lacks (Nerd Font symbols); empty uses the bundled faces alone; a changed mono family re-measures every pane's cell (SHELL-SCREEN-13). (`ts:settings/preferences.ts:172-183`)
- SHELL-LOOK-5. **Backdrop** (what shows through): `clear` (the desktop), `mica`, `acrylic`; Windows 11 (build ≥ 22000) offers all three, Windows 10 (build ≥ 17763) offers Acrylic and greys Mica with `Mica needs Windows 11`, below 17763 Acrylic says `Acrylic needs Windows 10 version 1809 or later`; elsewhere only `clear`. Putting one on clears Mica and Acrylic first, Mica takes its tint from whether the theme drawn is dark, Acrylic has no tint of its own (the page's background at the opacity chosen is the tint). Applied at start, on a theme or backdrop change, and when the OS's light/dark changes. (`rs:settings/backdrop.rs:17-95`, `rs:connection/commands.rs:79-84`, `rs:main.rs:129-135`)
- SHELL-LOOK-6. **Zoom** (`Ctrl +`/`-`/`0`, SHELL-KEYS-4): the whole window is drawn larger or smaller, panes' cells included, and each pane refits its grid to the new size; kept in `desktop.json`; applied at start. (`rs:connection/commands.rs:69-77`, `rs:main.rs:125-128`)

### Page-held state

None.

### Page-side logic that must move to Rust

- Everything in CSS: the 12 palettes with `color-mix` derivations (about 10.5 k lines of CSS across the app; `GPUI.md`), the type scale and spacing tokens, bundled font loading (`add_fonts`), the terminal palette (`--ansi-N`). A Rust `Theme` role table mapped onto `cx.theme()` is the port's biggest non-logic cost.

### gpui-kit mapping

- **Theme**: `cx.theme().{primary, background, foreground, border, muted, sidebar, sidebar_foreground, sidebar_border, sidebar_accent, status_bar, ..}`, `Theme::update(cx, |theme| ..)` (shown in `notification.md`), `ThemeRegistry::watch_dir(..)` and JSON themes with `"colors": {"button.primary.background": "#4F46E5"}`, gradient strings (`theme.md`); `Hsla` values so alpha is expressible per token (opacity). 
- **Backdrop:** `Window::set_background_appearance` with `WindowBackgroundAppearance::{Opaque, Transparent, Blurred, MicaBackdrop, MicaAltBackdrop}` per `GPUI.md` (checked in the `gpui-pre` 0.3.9 source by that phase; not in the kit docs); whether `Blurred` is Acrylic on Windows is the spike's. `window-vibrancy` and `windows-version` go; the offered list needs the build number (`windows-version` can stay). (unconfirmed: cited from GPUI.md, not a kit doc.)
- Zoom: not documented in kit; a scale factor on rem/`window` (custom).

### Scenes and coverage

- Scenes render in the default theme only; no scene for a theme, opacity, backdrop or zoom. `rs:system/window_frame.rs` `every_theme_has_a_palette_and_terminal_colours` guards the border/palette tables. Not covered: all behaviours except the model.

---

## TREE: file tree

Source: `ts:kit/file_tree.ts` (101 lines), `ts:kit/ignored_fold.ts` (40 lines). Consumers: the Code stage and the Checkouts files panel (other group), which keep their own clicks, keys and row actions. Node tests `kit/ignored_fold.test.mjs`: `ignored folders collapse into one row that names them`, `an ignored entry below the top stays where it is`, `a tree with nothing ignored has no fold`.

### Behaviours

- SHELL-TREE-1. A row is: a chevron (a right chevron that turns when open; none for a file), a folder or file icon, the name, optional **extra controls** shown on the selected row between name and mark, and a change **mark** (`M`, `?`, or a count on a folder; tone `changed` or `untracked`; its title doubles as its accessible name). Rows are indented by depth, carry the path as tooltip, and are marked `on` (selected), `ignored` (dimmed) and `dir`. (`ts:kit/file_tree.ts:11-63`)
- SHELL-TREE-2. The row's element is a **button** where the surface tabs between rows, or a **div** where the tree holds the focus itself; the click, double-click and right-click are the surface's. Rust has already ordered the rows and said which are open. (`ts:kit/file_tree.ts:26-37`)
- SHELL-TREE-3. **Ignored fold**: ignored entries at the top of the tree (depth 0: build output, caches, the agents' own folders) and whatever was opened under them are held in one row at the foot, `N ignored · a, b, c` (tooltip `Ignored by git`), which opens to show them dimmed; deeper ignored entries stay where they are (folding one out of its folder would lose where it was); a tree with nothing ignored has no fold row. Whether each surface's row is open is **kept outside the redraw** (per surface), so every rebuild keeps it. (`ts:kit/ignored_fold.ts:1-40`, `ts:kit/file_tree.ts:65-101`; node tests above)

### Page-held state

`unfolded` set per surface. (`ts:kit/file_tree.ts:67`)

### Page-side logic that must move to Rust

- `foldIgnored`, `foldLabel` (pure, node-tested): port to Rust with the three tests; the per-surface `unfolded` flag belongs with each surface's view state.

### gpui-kit mapping

- **`Tree`** (`tree.md`): `tree(&state, |ix, entry, selected, window, cx| ListItem::new(ix).selected(selected).pl(px(16.) * entry.depth() + px(12.)).child(h_flex().gap_2().child(icon).child(label)).on_click(..))`, `TreeItem::new(id, label).expanded(bool).disabled(bool).children(..)`, `entry.{item, depth, is_folder, is_expanded}`, `TreeState::{set_items, set_selected_index, selected_entry, scroll_to_item}`. The change mark and the fold row are custom row content/items; the fold row is a synthetic `TreeItem` whose `children` are the held rows. Key navigation is documented (`tree.md`'s keyboard table: Up/Down select, Left collapse or parent, Right expand, Enter toggle, Space configurable), so the file tree's own keys must be mapped onto it. Not documented: middle/right click per row (use `div().context_menu`), whether `expanded` state survives `set_items` (unconfirmed).

### Scenes and coverage

- Scenes `code`, `checkouts-files` draw it. Node tests cover the fold rule only.

---

## SYSTEM: chime, notifications, clipboard, opener, logging

Source: `ts:system/chime.ts` (43), `ts:system/copy.ts` (21), `rs:system/{clipboard,notifications,opener,logging}.rs`. `DESIGN.md §Panes and terminal state` (chime), `§Process model` (clipboard, logging). Tests: `rs:system/notifications.rs` (`an_agent_turning_to_need_you_is_named_with_where_it_runs`, `a_remote_agent_is_named_with_its_host`, `starting_work_or_going_idle_raises_nothing`, `a_window_in_front_is_told_nothing_unless_asked_to_be`, `the_pane_being_watched_is_never_announced`, `turned_off_it_raises_nothing`), `rs:system/opener.rs` (`a_path_in_the_checkout_is_shown_and_none_names_the_checkout_itself`, `a_path_that_leaves_the_checkout_or_is_not_there_is_not_shown`, `a_link_out_of_the_checkout_is_not_shown`, `the_file_manager_gets_the_path_as_one_argument`, `only_a_web_link_reaches_the_opener`), `rs:system/clipboard.rs` (`a_clipboard_picture_is_encoded_as_a_png`), `rs:system/logging.rs` (`a_panic_is_logged_with_where_it_happened`).

### Behaviours

- SHELL-SYSTEM-1. **Chime.** When Rust says `chime` (an agent came to need the person, not for the pane on screen in a focused window) the window plays two soft sine notes, rising: **660 Hz** then **880 Hz** starting 0.12 s later, each with a 10 ms rise to gain 0.18 and an exponential fall to 0.001 by 0.35 s, stopped at 0.4 s; synthesized, no asset. The audio context is created or resumed on the person's first pointer press or key press (a webview holds audio until the page is used). Whether it plays is Rust's: on by default, switched from the foot (SHELL-FOOT-5). (`ts:system/chime.ts:1-43`, `DESIGN.md §Panes and terminal state`)
- SHELL-SYSTEM-2. **System notification.** An agent turning to `waiting`, `needs-review` or `failed` raises one, unless: notifications are off; or the person keeps them to the background and the window is focused; or it is the pane being watched (on screen in a focused window). Title = the pane's title; body = `[<subagent> ]is waiting for you | needs a review | failed · <repository> / <checkout>`, first letter capitalised, with ` · on <host>` appended for a remote daemon (so a remote agent is never taken for a local one). Idle or working transitions raise nothing. Best effort: a failure logs a warning. (`rs:system/notifications.rs:30-95,100-107`; tests above)
- SHELL-SYSTEM-3. **Clipboard writes**: a drag in a pane (`copy`, SHELL-INPUT-8), a view's copy button (`copy_text{text}`), an OSC 52 from a child (`clipboard` event, written by the view), a session's own `take_copied` (notice `copied <text>`), and the menu's `Copy` (SHELL-MENU-5). A failure says `could not copy: <err>`. A copy button says `Copied` (in place of its label) for 1200 ms. (`ts:system/copy.ts:6-21`, `rs:connection/commands.rs:109-125`, `rs:connection/shared.rs` `act`)
- SHELL-SYSTEM-4. **Clipboard reads** (the paste chord, SHELL-KEYS-8): a picture as PNG (the clipboard hands raw RGBA, encoding is ours), copied files, text. (`rs:system/clipboard.rs:1-49`)
- SHELL-SYSTEM-5. **Opening things**: `open_url{url}` opens only `http`/`https` (else a notice, because the opener would as happily run a `file:` URL) via `open` (macOS), `rundll32 url.dll,FileProtocolHandler` (Windows), `xdg-open` (elsewhere), passed as one argument and never through a shell, the child reaped on a thread; a file or folder of a checkout is shown in the file manager only if it is inside that checkout (a path with `..` or an absolute one, or a link that leaves, is refused), a file selected in its folder, a folder opened, the checkout's own folder with none. (`rs:system/opener.rs:1-100`; tests above)
- SHELL-SYSTEM-6. **Logging**: diagnostics go to `argus-desktop.log` beside the config (instance-scoped), the previous run kept as `.1`, stderr kept for a window run from a terminal, a panic written with where it happened; failing to open the file never stops the window. The release build is a GUI program on Windows (`windows_subsystem = "windows"`). (`rs:system/logging.rs:1-60`, `rs:main.rs:12`)

### Page-held state

The audio context and the copy button's label timer.

### Page-side logic that must move to Rust

- The chime is page logic today (`ts:system/chime.ts`, a WebAudio graph): move to a Rust audio path producing the same two tones (GPUI has none: `GPUI.md` "audio crate"); test by sample math (frequency, envelope). The copy button's `Copied` timer is view code.

### gpui-kit mapping

- **Notifications to the OS: `Notification::..system()`** (`notification.md` "System Notification"): `Notification::info("..").title("..").system()`, `NotificationDelivery::{InApp, System, InAppAndSystem}`, same `.id::<T>()` replaces the previous, click fires `on_click`; requirements: macOS must run from a bundled `.app` (silently disabled under `cargo run`), Windows needs `cx.set_app_identity(identifier, name)` early, Linux needs an XDG daemon; `gpui_kit::component::init` registers the response handler (do not register a second). This replaces `tauri-plugin-notification` (`GPUI.md` mentions `notify-rust`; the kit has its own). Not documented: how the kit tells whether the window is focused (SHELL-SYSTEM-2 stays Rust).
- **Clipboard:** `Clipboard::new(id).value(text).on_copied(|value,window,cx| ..)` (unconfirmed) (`clipboard.md`: a copy button whose icon turns to a check) matches the copy button; GPUI `cx.read_from_clipboard()` and `ClipboardItem`/`ClipboardEntry::{Image, ExternalPaths}` (`input.md` "Paste Hook", `test.md`) can replace `arboard`, or `arboard` stays (`GPUI.md`). Test: `cx.read_from_clipboard()`, `cx.opened_url()`.
- **Chime:** none in the kit (`GPUI.md`: audio crate). **Opener:** GPUI `cx.open_url` is shown in `test.md` (`cx.opened_url()`); `Link` (`menu.md` `.link(..)`) opens external links; the `file:`-scheme refusal stays ours.

### Scenes and coverage

- `notice` draws a notice. Covered by Rust tests: notifications, opener, PNG, logging. **Not covered:** chime (no test), copy button label, OSC 52 writes.

---

## TIME: local-time formatting

Source: `ts:kit/clock.ts` (55 lines). Used here by the strip's resumed note (SHELL-STAGE-9); other groups use it for sessions and transcripts. No test.

### Behaviours

- SHELL-TIME-1. Times arrive as RFC 3339 strings and are said in the person's own clock: `clock(at)` = `HH:MM` (24-hour, 2-digit) + ` today` / ` yesterday` / ` <Mon> <D>` (locale short month and day); an unparseable or empty time says nothing (`null`). (`ts:kit/clock.ts:1-55`)
- SHELL-TIME-2. `ago(at)`: under a minute `just now`; `Nm ago`; `Nh ago` under a day; `Nd ago` under a week; past a week the `clock` form. (`ts:kit/clock.ts:1-55`)
- SHELL-TIME-3. `hhmm(at)` is the time alone; `span(start, end)` is `HH:MM–HH:MM <day>` when both fall on one day, else `<clock> – <clock>`; with either unparseable it falls back to `clock(start ?? end)`. (`ts:kit/clock.ts:1-55`)
- SHELL-TIME-4. Different from Rust's `argus_protocol::ago` (`16m`, `2h`, `3d`, `2w`, `—`, used by the rail's `used`): two formats exist and both must be kept. (`rs:chrome/rail.rs` test `ages_read_in_the_largest_unit_that_fits`)

### Page-side logic that must move to Rust

- All of it (`ts:kit/clock.ts:1-55`): the file says it is on the page "because only the page knows its time zone". In Rust that needs the local zone; `argus-desktop` has no date crate today (`crates/argus-desktop/Cargo.toml`), so `chrono` or `time` comes in. Tests need an injected `now` and zone (none exist).

### gpui-kit mapping

- None (formatting, no component).

### Scenes and coverage

- Scenes use a fixed `NOW` (`scene:*` `NOW = 1_792_460_000`, "a Monday afternoon") so ages read deterministically; no scene pins a time zone. Not covered by any test.

---

# Cross-cutting

## Page-side logic that must move to Rust, in one table

| What | Where today | Why it is logic | Test today |
|---|---|---|---|
| Chord match, action lookup | `ts:keys/keymap.ts:44-65` | a third copy of `Chord::matches` | node none; Rust yes |
| Layer dispatch: open predicates, `run`, `NOT_THEIRS`, rail/stage/extension keys | `ts:keys/keymap.ts:95-406` | decides who handles a key | `focus.test.mjs` (walk only) |
| Key naming and leader lookup | `ts:keys/chords.ts:24-66` | formatting rules | none |
| Sheet sections: merges, arrow caps, filter, opening section | `ts:keys/sections.ts` | pure rules | `sections.test.mjs` |
| Palette: fuzzy score, grouping, scoping, highlight by key | `ts:keys/palette.ts:142-345` | ranking | none |
| Focus hand-offs by event diff | `ts:main.ts:43-105` | decides focus | none |
| Grid math: rects, gutter drag, thousandths, drop zone, keyed split, minimum px | `ts:panes/split_math.ts`, `ts:panes/split.ts:224-234` | layout | `split_math.test.mjs` |
| Panel-drag target and preview geometry | `ts:layout/drag.ts:33-71` | layout constants | none |
| Float clamp | `ts:layout/floats.ts:28-121` | layout | none |
| Terminal colour resolution, contrast, flags, cursor/selection geometry, rows/cols from pixels, timers | `ts:panes/screen.ts`, `contrast.ts`, `size_label.ts` | rendering rules | `size_label.test.mjs` only |
| Wheel notch accumulation, select-vs-report, move de-dup | `ts:panes/input.ts:74-161` | input rules | none |
| Console strip items and dots; `Close the others` loop | `ts:panes/console_strip.ts`, `ts:panes/console.ts:61-67` | menu assembly | `console_strip.test.mjs` |
| Rail row flattening, `railAction`, `startAction`, pane initials, status colour | `ts:chrome.ts:33-115,330-342,407-605` | rail cursor model | none |
| Strip composition, empty-stage choice, ended bar | `ts:chrome.ts:609-714,866-1041` | view-model | none |
| Add-project outcome message, `Add <name>`, debounce | `ts:chrome/projects.ts:349-357,455-517` | view-model | none |
| Switcher tallies, plurals, which sections appear | `ts:chrome/projects.ts:36-193` | view-model | none |
| Foot tallies and wording | `ts:chrome.ts:757-856` | view-model | none |
| Settings: drawn flavour, default faces from CSS, font probe | `ts:settings/preferences.ts:142-160,571-602` | environment queries | none |
| Font picker model | `ts:settings/font_picker.ts` | filter and commit rules | none |
| Ignored fold | `ts:kit/ignored_fold.ts` | pure | `ignored_fold.test.mjs` |
| Local-time formatting | `ts:kit/clock.ts` | zone-aware | none |
| Chime synthesis | `ts:system/chime.ts` | audio | none |
| Menu clamp and focus-first (view) | `ts:kit/menu.ts:28-39` | view | none |

## gpui-kit mapping, one table

Each component below was fetched from `https://gpui-kit.com/component/{name}.md` for this spec unless noted.

| Surface | Components | Fit | Biggest gap |
|---|---|---|---|
| SCREEN | custom `Element` (skill `element.md`) | custom | no kit component; `Skeleton` only for the wait bars |
| INPUT | custom `EntityInputHandler` | custom | preedit, Shift on press unproven |
| KEYS | `bind_keys`/`key_context`/`on_action` (skill `action.md`), `Kbd` | custom interceptor | leader, layer order, pre-child interception |
| PALETTE | `Command` or `List` | partial | substring-only local filter, Esc clears first, no marks |
| SHEET | `Dialog` (or `Sheet`), `List`/`Sidebar`, `Input`, `Kbd`, `Tag` | fits | Sheet is an edge panel; ours is centred |
| GRID | `resize_handle`, `h_resizable`/`v_resizable` | custom layout | arbitrary zones, drop zones |
| CONSOLE | `TabBar`/`Tab`, `Button`, `PopupMenu` | partial | middle-click, per-tab menu, reorder |
| DOCK | `Resizable` + `TabBar` + `PopupMenu`, or `Dock` | partial | `Dock` owns its own state; stage/float places |
| PDRAG | `on_drag`/`on_drag_move` | custom | none in kit |
| FLOAT | custom | custom | `Dialog` neither moves nor resizes |
| EDGE | `resize_handle(...).inside().on_drag().on_double_click()` | fits | start/end events for `resizing` |
| STAGE | `Empty`, `Spinner`, `Dialog`, `Tag`; `Breadcrumb` unconfirmed (404) | mostly | breadcrumb |
| RAIL | `Sidebar`, `Tree` + `ListItem`, `PopupMenu`, `Badge`/`Tag` | partial | no width drag, middle-click, danger menu item, Tree key contract |
| SWITCH | `SidebarHeader::dropdown_menu`, `PopupMenu`, `AlertDialog` | partial | in-place unfold vs submenu |
| ADD | `Dialog`, `Input`, `TabBar::segmented`, `List`, `Button` | fits | builder re-runs: persistent entities |
| MENU | `PopupMenu` | partial | disabled reason, danger, point placement, clamp |
| RENAME | `Dialog`, `Input` | fits | select-all on open |
| FOOT | `SidebarFooter`, `Button`, `Badge`, `Switch` | fits | `StatusBar` is not the foot |
| NOTICE | `Notification` | fits | 5 s default vs 6 s |
| TITLE | `TitleBar` | partial | snap layouts, DWM border, sidebar-to-top layout |
| SETTINGS | `Settings`, `Slider`, `Switch`, `Select`, `Input`, `TabBar::segmented` | partial | pages vs one scroll, slider set-value |
| FONT | `Combobox` or `Select` | partial | typed value not in list, per-row face |
| LOOK | `cx.theme()`, `Theme::update`, `set_background_appearance` (GPUI.md) | custom | 12 palettes, Acrylic |
| TREE | `Tree`, `ListItem` | fits | key contract |
| SYSTEM | `Notification::system()`, `Clipboard`, `cx.read_from_clipboard()` | mostly | chime has none |
| TIME | none | none | needs a date crate |

## Scene coverage

Scenes of `rs:screens/mod.rs:1848-2113` that show this group, and what each exercises:

| Scene | Surfaces it shows | Behaviours it exercises |
|---|---|---|
| `connecting` | STAGE, FOOT, RAIL | SHELL-STAGE-11, SHELL-FOOT-3, SHELL-RAIL-1 |
| `stopped` | STAGE, FOOT | SHELL-STAGE-14 |
| `workspace-agent` / `-shell` / `-waiting` / `-failed` / `-exited` / `-editor` | SCREEN, RAIL, STAGE, FOOT | SHELL-SCREEN-2..8, SHELL-RAIL-4,7,12, SHELL-STAGE-7,10, SHELL-FOOT-1 |
| `workspace-split` | GRID | SHELL-GRID-1..5 (two panes) |
| `workspace-console` | CONSOLE, DOCK | SHELL-CONSOLE-5,6, SHELL-DOCK-3 (a single panel: no tab strip) |
| `workspace-empty` | STAGE, RAIL | SHELL-STAGE-13 |
| `new` | STAGE | SHELL-STAGE-1 (composer, another group) |
| `add-project` | ADD | SHELL-ADD-3..6 (first browse) |
| `palette` | PALETTE | SHELL-PALETTE-4 (empty query) |
| `shortcuts` | SHEET | SHELL-SHEET-2..5 |
| `settings` | SETTINGS, FONT | SHELL-SETTINGS-3,6 |
| `notice` | NOTICE | SHELL-NOTICE-1 (appearance) |
| `docks`, `float`, `agent-views`, `saved-view`, `extension-stage` | DOCK, FLOAT | SHELL-DOCK-3..9 (partial), SHELL-FLOAT-1 |
| `switcher` | SWITCH, MENU | SHELL-SWITCH-1,2 (heading clicked) |

A scene is a window in `VisualTestContext` after the port; the screenshots move to `render_to_image` and the audit is rebuilt on `simulate_*` (`GPUI.md` §Tests). `audit.mjs` hovers, clicks, right-clicks and tabs every control and reports controls with no feedback; it asserts no rules.

### Behaviours no scene or test covers today

- All pointer behaviour except Rust-side models: SHELL-INPUT-5..10, SHELL-GRID-7..13, SHELL-PDRAG-*, SHELL-FLOAT-3..5, SHELL-EDGE-1,2, SHELL-RAIL-8..10, -13..-18, -28..-31, SHELL-CONSOLE-6..9, SHELL-DOCK-4..6, SHELL-MENU-*.
- Keyboard layering: SHELL-KEYS-3, -10..-22 (only the walk and Rust membership tests exist), SHELL-SEAM-8..10.
- Rendering rules with no test at all: SHELL-SCREEN-5..9, -12, -13 (only `size_label` has a test).
- View-models with no test: SHELL-STAGE-7..13, SHELL-ADD-6..9, SHELL-SWITCH-2, SHELL-FOOT-1..6, SHELL-PALETTE-5..9, SHELL-SHEET-1,8,9.
- Settings in-place update and the lit nav (SHELL-SETTINGS-3,5); every font picker rule (SHELL-FONT-*); the chime (SHELL-SYSTEM-1); time formatting (SHELL-TIME-*).

## Gaps and decisions the port must take

These are places where today's page disagrees with its own docs, or where the kit does not match; the spec records the page's behaviour above.

1. **Palette `Tab next group`** is advertised and not implemented (SHELL-PALETTE-12 gap).
2. **Active/quiet rule**: `Repository.divider` is sent and never read; `DESIGN.md §Navigation model` says the rule is drawn (SHELL-RAIL-2).
3. **Menu `Copy`** uses `navigator.clipboard` in the page while the system comment says Rust owns the clipboard (SHELL-MENU-5).
4. **Dead calls**: `step_row`, `focus_towards`, `move_held` are in `Call` and `Session` but the page never sends them (SHELL-GRID-15); `Stage::View`'s keys and `Call::OpenView` are other groups'. `Call::PasteClipboard` is answered in Rust and never sent by the page (SHELL-SEAM-20).
5. **IME preedit** is not drawn (SHELL-INPUT-3).
6. **Sheet vs Dialog** for the shortcuts (`GPUI.md` says `Sheet`; the page is a centred dialog).
7. **`Dock`** component vs Resizable+TabBar (SHELL-DOCK mapping).
8. **`StatusBar`** (`GPUI.md`) vs `SidebarFooter`: the page has no window-bottom bar.
9. **`Command`** is a substring filter that clears its query on Escape (SHELL-PALETTE mapping).
10. **Settings** component uses pages, not one scrolling document (SHELL-SETTINGS mapping).
11. **Notification** default 5 s vs the page's 6 s (SHELL-NOTICE mapping).
12. **Time zone** needs a date crate (SHELL-TIME).
13. `Breadcrumb` has no fetched doc (`breadcrumb.md` 404); the skill catalog lists it.

## Spikes the mapping needs before the port (not documented in the kit docs fetched)

Interception of a key before focused children (SHELL-KEYS-3); `Tree`'s own key handling (SHELL-RAIL mapping); `PopupMenu` placement at a point, disabled reason, danger item, wrap and `j`/`k`; `Tab` middle-click, context menu, reorder; `Slider` set-value; `Combobox` typed values and custom rows; `TitleBar` snap layouts and DWM border; `resize_handle` start/end events; `Input` select-all on open; `AlertDialog` default-button focus; `Dock` one-tab-per-edge and a stage drop target; Shift on a mouse press; `Notification` dismiss button; `Command::searchable(false)` with an `Input` in `header`.
