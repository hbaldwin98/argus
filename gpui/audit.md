# GPUI port: what is left

Six readers went through every page and feature of the Tauri window and
compared each to `crates/argus-gpui`. This file is the plan for the rest of
the port. It is a plan, not a spec. The behaviour ids live in
`gpui/parity/`. Each item names the section it is checked against.

A note on sizes: S is under a day, M a few days, L about a week, XL more
than a week and carries a spike or a decision first.

## 1. Where it stands

The readers listed 140 rows. 8 are done, 33 are partial, 99 are missing.
Eleven rows were found twice by different readers (the palette, the rail
keys, stage hosting, the rail drag, notices, the browser cell, extension
console views, the key dispatcher, the resize hold, the key places). Merged,
that is 129 items: 121 to build, 8 done.

What the GPUI window does today:

- It connects. The owner runs the bridge, reconnects only while stopped,
  switches host, opens URLs, logs, and writes the clipboard.
- It draws a frameless window with a drag strip and the three controls.
  Snap layouts and double-click come from `WindowControlArea`. Close quits.
- It draws one theme, Ledger, from typed tokens, checked against
  `style.css`. Inter, JetBrains Mono and Nerd symbols are bundled.
- The rail: repositories, checkouts, pane rows, status dots, pills, fold,
  pick, middle-click close, held dimming while reconnecting.
- The nav table (with a Git item), the foot (tallies, alarm, daemon dot,
  console chip, chime switch), the brand row and a flat project list.
- One Workspace terminal: cells, colours, cursor, keys, IME text, paste,
  resize reports. Mouse, selection and wheel are not there.
- The console: tabs, split sides, menus, focus, rename. This is the best
  ported surface (55 tests).
- Docks: tabs, sash, dock menu, drag to move, show, hide, close. Bodies
  read "Not in the GPUI window yet."
- The Git stage: Changes, History, jump field, comment box, split view,
  the shared diff view, keys. This is done against the old dialogs.

What it cannot do yet, in the order a person would hit it:

- It never says why anything was refused, because `Event::Notice` is
  dropped.
- Ctrl+K, Ctrl+comma and `?` do nothing, because `Event::Run` is dropped
  and there is no key layer above the terminal.
- It shows one pane. Two or more panes in the grid are not drawn.
- No text selection, no wheel, no mouse reports in a terminal.
- No settings, no other theme, no zoom, no palette, no way to add a first
  project.
- Features, Checkouts, Code, Browsers, views and extensions have a nav
  entry or a key and nothing behind it.

## 2. The ranked work list

Ranked by what is hit daily first (Workspace and panes, shell, console,
Git), then shared pieces before the surfaces that need them, then size and
risk. Where two readers found the same thing it is one item here, and the
other reader's wording is folded in.

### Phase 1: the shared floor

These are small, and nearly everything below leans on them.

1. **Notices** (missing, S). Missing: `Heard` holds the latest notice; a
   bottom line with an info icon, text and dismiss, a 6 s timer, a new one
   replaces the old. The "argusd offers no agent to start" text must come
   from the session, not the view. Needs: nothing. Core: move that one
   page-made notice into `Session`. Risk: low; kit `Notification` defaults
   to 5 s and pauses on hover, so a plain host may be simpler. Check:
   SHELL-NOTICE-1,2; CHECKOUTS-44; REVIEW-7/31/42.
2. **Time formatting** (missing, S). Missing: one zone-aware formatter with
   `now` and zone injected ("HH:MM today", "Nm ago", spans). Needs: a date
   crate in `argus-desktop-core`. Core: new module there. Risk: month names
   and locale; `argus_protocol::ago` ("16m") is a different text. Check:
   SHELL-TIME-1..4.
3. **Menu primitive** (partial, S). Missing: one `MenuItem` model
   (heading, rule, disabled reason, danger, hint) rendered once; anchored
   at a point or under a button; j/k and wrap; selection Copy. Needs:
   nothing. Core: core builds `Vec<MenuItem>` for rail, strip, dock,
   console, switcher, board and checkouts menus. Risk: kit `PopupMenu` may
   not do danger, reasons or wrap; a small custom popover may be needed.
   Check: SHELL-MENU-1..6.
4. **Dialog and overlay host** (partial, M). Missing: one controller that
   knows which app overlay is open (palette, sheet, settings, add-project,
   question, extension dialogs), enforces that palette, sheet and settings
   are exclusive, owns the Escape ladder, a non-dismissable variant for the
   stopped card, and a "do not steal focus" variant for extension
   questions. Needs: nothing. Core: optional hand-off table as one function
   (SEAM-8..10). Risk: kit dialog builders re-run, so each dialog needs
   retained entities and a once-only focus guard (the `rename.rs` pattern).
   Check: SHELL-SEAM-8..10, KEYS-13,14,19.
5. **Shared kit parts** (partial, M). Missing: status and agent pill,
   progress bar, skeleton bars, tooltip, checkbox and radio rows, sectioned
   list rows, a field wrapper that survives snapshots (never `set_value`
   while focused), a resizable pair that reports on release. Build these
   before the first surface that needs them, or every surface copies them.
   Needs: theme tokens (done). Core: none. Risk: low. Check: board.md,
   review-agents.md, shell.md SHELL-RAIL-8.
6. **Root key layer** (missing, L). Missing: an interceptor ahead of the
   focused child that walks the core `LAYERS` table; `Event::Run` handling
   (palette, sheet, settings); the Escape ladder; the Anywhere place sent
   as `Call::Command{action}` (the session already runs it); the Window
   place (zoom keys) heard first; extension commands by scope; a layer
   trait (`open`, `keys`, `escape`) each surface implements; a Rust test
   that every `Place` is hooked. The 26 places arrive with their surfaces.
   Needs: the dialog host, menus. Core: expose `Chord::matches`; add a
   small layer-state table so the walk and the ladder are tested once.
   Risk: the long pole. The Menu layer must be asked before Sessions
   though `LAYERS` lists it after. Leader and chords do not fit
   `bind_keys`. A capture-phase interceptor is not in the gpui-kit docs;
   spike first. Check: SHELL-KEYS-1..23, SHELL-SEAM-8..10.
7. **Leader and pane chords, visible outcomes** (partial, S). The session
   already handles the keys. Missing: the events they cause (Run, Clipboard,
   Notice, Chime) reaching a surface, and stage digits that land somewhere.
   Needs: 1, 6 and the stages. Core: none. Risk: low; assert events, not
   calls. Check: SHELL-KEYS-5..9.
8. **Resizing hold as a set of holders** (partial, S). `Heard.resizing` is a
   bool written only by the work area. Make it a set so two drags do not
   clear each other. Needs: nothing. Core: forget the last-sent size for
   outline, brief and code sizes too (CROSS-28). Risk: do it before the
   second sash lands. Check: SHELL-EDGE-1..4, CROSS-20, CROSS-28.

### Phase 2: the Workspace and its panes

9. **Terminal screen** (partial, M). Missing: selection overlay (30% wash,
   reading order); skeleton bars after 250 ms with no first frame; palette
   re-read on theme change (`Terminal::new` reads it once); screens for
   `Beside(n)` and `View(n)` sides (Heard keeps three); a mouse-aware flag
   in `Screen`; scenes for contrast, hollow cursor, selection. Needs: a map
   of screens keyed by `Side` in Heard. Core: optional ready "grid label"
   string. Risk: fractional cell width from `advance("M")` may differ from
   the page by a sub-pixel and change cols by one. Check:
   SHELL-SCREEN-1..17.
10. **Terminal keyboard: window keys** (partial, S). Missing: zoom keys heard
    before any pane or field; the macOS rule that a Meta chord is the
    window's unless the Pane place binds it; keys the adapter cannot name
    reaching text; preedit drawn (optional). Needs: 6. Core: one function
    "is this the window's chord" both windows call. Risk: `on_key_down`
    fires before the platform input handler; keep the `stop_propagation`
    that stops a double type. Check: SHELL-INPUT-2..4, SHELL-KEYS-3,7,8.
11. **Terminal mouse** (partial, M). Missing: cell hit-test and clamp, drag
    select, copy on release, `Call::Mouse` (down, drag, up, move, de-dup by
    cell, Shift selects), `Call::Wheel` with notch accumulation, press
    routing for split cells (`focus_view`) and view sides. Needs: 9, 12.
    Core: move clamp, select-vs-report, move de-dup and the wheel
    accumulator into small tested core functions. Risk: wheel units differ
    per platform; 100 px and 3 lines must be re-derived. Check:
    SHELL-INPUT-5..10,13.
12. **Workspace split grid** (missing, L). Missing: one Terminal entity per
    `Beside(n)` kept across redraws; 28 px cell headers with close, keyed
    ring, status dot; vertical and horizontal gutters with minimums;
    `set_shares` once on release, `even_shares` on double-click, the 600 ms
    held-sizes rule; focus to the keyed cell; a non-terminal cell kind
    reserved from the start (browsers); scenes for 3+ panes. Needs: 8, 9.
    Core: pass fractions instead of thousandths; move cell rects, gutter
    drag and pixel minimums next to `split_grid.rs`. Risk: reflowing a pane
    mid-drag reflows the child program; minimums need live cell size.
    Split as `cells.rs`, `gutters.rs`, `header.rs`, `drop.rs`, `focus.rs`.
    Check: SHELL-GRID-1..10,14..17.
13. **Pane drags onto the grid and stage** (missing, M). Missing: header
    drag (lift at the shared threshold, Escape cancels, ✕ does not start
    one); rail pane rows as drag sources; zone hit-test and lit zone;
    stage-drop marker; a registry of cell and stage bounds published by
    views; one `Dragger` with a payload enum (`CellHeader`, `RailPane`,
    `Panel`). Needs: 12, `kit/drag.rs`. Core: `dropZone` and `zoneRect`
    become Rust with the existing `split_grid.rs` drop tests. Risk: a drag
    over the terminal must not select; the release must be consumed.
    Check: SHELL-GRID-11..13,17.
14. **Move a pane to another checkout** (missing, S). Missing: checkout
    rows as drop targets with a highlight; hide targets for a browser pane
    and the pane's own checkout; a notice on refusal. Needs: 13, 1. Core:
    `move_to_checkout` should refuse a browser pane explicitly; confirm a
    daemon without the capability gets a visible notice. Risk: the drag host
    must hit-test rows of another view; use the registry from 13. Spec gap:
    no behaviour ids (section 3). Check: commit 88dcce24, `session.rs`.
15. **Stage router and stage hosting** (partial, M). Missing: one function
    that picks the surface from `Rail.stage`, the window Git stage and the
    console place; hide the rail when `Rail.filled`; fullscreen console
    hides docks, nav and foot; the 1200 px breakpoint; dim strip and body
    while held. Needs: each stage's view as it lands. Core: strip
    composition moves to a `StripPage`. Risk: two sources of truth (session
    stage, window Git); keep that rule in one function. Check:
    SHELL-STAGE-1..4,15.
16. **Stage strip and header** (partial, M). Missing: status chip, "resumed
    from" note, size label, the "..." pane menu (Rename, Close), per-stage
    crumb, title and tools, the Views launcher. Needs: 2, 3, a tools slot.
    Core: a `StripPage` view-model instead of porting `drawHead`'s ladder.
    Risk: tools belong to the stage views, so the strip needs a slot API,
    not a central switch. Check: SHELL-STAGE-7..9.
17. **Parked view, size label, ended bar** (partial, S). Missing: the
    `↑ offset/depth` indicator (`Screen::parked()` is held, unread); the
    size label for 1 s after a resize, on a timer that does not keep a
    repaint loop alive; the ended bar with its template choice. Needs: 11,
    16. Core: the ended bar's template must come from Rust; the page guesses
    the first template. Risk: low. Check: SHELL-SCREEN-10, SHELL-STAGE-10.
18. **Empty, connecting and stopped stages** (missing, M). Missing: the
    Connecting card (with "Use this machine" or Settings), No projects,
    No pane on screen with start buttons and the "Panes in <project>"
    strip, and the modal Stopped card wired to `Call::Reconnect`. Needs: 4,
    15, 31 (Add project), 35 (Settings). Core: move the first-checkout rule,
    empty-stage choice and strip composition to Rust. Risk: low. Without
    this a clean machine has no way in. Check: SHELL-STAGE-10..14.
19. **Rail rows, the rest** (partial, M). Missing: repository clock button
    (`open_sessions`), `toggle_repository` on right-click, scroll the shown
    pane into view once, the checkout "+" button, tooltips as hover text,
    people icon, `RailView` cloning the whole `Rail` on every render
    (observe only its slice). Needs: 20, 119. Core: none. Risk: long trees
    may need virtualising. Check: SHELL-RAIL-1..14,18,34.
20. **Rail menus and the "+" menu** (missing, M). Missing: right-click menus
    for repository, checkout, pane; the in-row start menu with its own key
    layer (`start`); Git items map onto the window's Git entry. Needs: 3, 6,
    27. Core: build the item lists (`starts()`, template labels) in Rust.
    Risk: danger and disabled-reason styling. Check: SHELL-RAIL-6,10,15,
    28..31, SHELL-MENU-1..6.
21. **Rail keys** (partial, S). Missing: j/k/arrows, s, a, r, h, z, Enter on
    a checkout, x on a checkout, extension fallthrough. `rail/keys.rs`
    matches literal key strings and bypasses `Place::Rail`. Drive it from
    core. Needs: 6, 20. Core: move the pick and start-menu cursor into
    `Session` as a rail cursor. Risk: view-only pick cannot be covered by
    keymap tests. Check: SHELL-RAIL-19..27, SHELL-KEYS-16.
22. **Nav, the rest** (partial, M). Missing: Browsers item (count, fold
    chevron, sub-rows, drag), extension and saved entries, a table with a
    dynamic tail, key-cap rule (always vs on hover). Needs: 80, 96, 97.
    Core: `Nav::Browsers` ↔ `Action::Browsers` / `ShowBrowsers` /
    `Stage::Browsers`. Risk: `Nav::ALL` is a fixed array; extend the
    "each stage lights at most one item" test. Check: `nav.rs`, `chrome.ts`.
23. **Rail width: sash, fold, narrow** (missing, M). Missing: drag the edge
    (grab offset kept, sent on release), double-click reset, fold below 128
    to 56, icon rail with pane initials, the right dock as a drawer under
    1200 px. Needs: 8, the size decision. Core: `paneMark` and the fold
    threshold behind tested core functions. Risk: rows must stay hit-testable
    at 56 px; initials use Unicode letter classes. Check: SHELL-RAIL-16,32,33,
    SHELL-EDGE-1..4, SHELL-STAGE-15.
24. **Console, the rest** (partial, S). Missing: fullscreen putting away the
    chrome (15), skeleton for a tab being switched to, tab strip overflow,
    extension console sides (99). Optional: a `close_others` call. Needs:
    9, 15. Risk: new buttons must `keep()` their press from the panel drag.
    Check: SHELL-CONSOLE-1..14.
25. **Docks, the rest** (partial, M). Missing: real panel bodies (items
    92, 93, 94, 98), extension tools in the tab strip, the narrow drawer, a test
    that a `DockGesture::Size` release always gets an answer. Needs: 26, 91.
    Core: none. Risk: low. Check: SHELL-DOCK-1..13.
26. **Floating panels, frame** (missing, M). Missing: raise, move, resize
    (min 280x160), clamp (120 px across, 40 px of header), first at 96,64
    then +28, bounds on release. A custom frame, since kit `Dialog` does not
    move. Needs: `kit/drag.rs`. Core: move the clamp beside
    `FloatBounds::held`. Risk: stack under menus and dialogs (CROSS-21).
    Check: SHELL-FLOAT-1..6.
27. **Rename reachable** (partial, S). The dialog exists for console tabs.
    Missing: reach it from the rail pane row, the strip "..." and split
    headers. Move `asked()` into `Session::rename`. Needs: 16, 20. Risk:
    low. Check: SHELL-RENAME-1..3.
28. **Foot gear and chords in tooltips** (partial, S). Missing: Settings gear
    and tooltip key chords. Optional `FootPage` in core so wording is not
    written twice. Needs: 35. Risk: low. Check: SHELL-FOOT-1..7.

### Phase 3: Git, the one finished stage

29. **Changes side switch and commit label** (partial, S). Missing: the
    Unstaged N / Staged N radio sending `ReviewClick::Side`, and the commit
    short hash and summary on a commit review. `B` works, so the gap hides.
    Needs: kit segment (done). Core: none. Risk: low; add tests, grep finds
    none. Check: REVIEW-2,6,7.

### Phase 4: project management, search and the look

30. **Project switcher** (partial, M). Missing: the brand-row menu with
    tallies, Add a project, Switch workspace, Connect to a host, Remove;
    it must also show with a single project. `Rail.workspaces`, `hosts` and
    `OpenWorkspace`/`SetHost` already exist. Needs: 3, 31, 32. Core: build
    items, tally precedence and plurals in Rust. Risk: blocks first run on
    a clean machine. Check: SHELL-SWITCH-1..8.
31. **Add-project dialog** (missing, M). Missing: kind control, path row with
    Up, folder list with skeletons, outcome copy per kind, name and "Also
    skip" fields (250 ms debounce), disabled Add with reason, refusal in
    danger colour. Retained inputs. Needs: 4, 5, 30. Core: move `outcome()`
    wording, the Add label and the debounce into Rust. Risk: no path
    completion in the kit; the folder list stands in. Check: SHELL-ADD-1..12.
32. **Project removal question** (missing, S). Alert dialog, "Keep it" has
    default focus, Escape and scrim answer no. `Rail.removing` is held,
    unread. Needs: 4. Check: SHELL-SWITCH-6,7.
33. **Command palette** (missing, L; merged from two readers). Missing: the
    overlay; groups (Commands, Panes, Features, Checkouts), `>` and `#`
    scopes, subsequence score, marked characters, first 6 per group,
    highlight by stable key, `Run` handling. `Heard` holds no palette page.
    Needs: 4, 6. Core: a ranking module next to `keys/palette.rs`
    (`rank(query, rows)`), unit and proptest coverage; today only
    untested TS has it. Risk: kit `Command` is substring-only and clears on
    Escape; use a list with `perform_search` or a custom list. Decide the
    "Tab next group" hint (section 5). Check: SHELL-PALETTE-1..13.
34. **Shortcuts sheet** (missing, S). `Heard` keeps the keymap groups. Missing:
    the view, the find field, and `whereNow()` which section opens first.
    Needs: 4, 6. Core: move the section choice into Rust. Check:
    SHELL-SHEET-*.
35. **Settings dialog** (missing, L). Missing: the whole sheet (six sections,
    thirteen `SettingChange` variants, theme cards, opacity slider, backdrop
    segment, editor section with Vim and grammars, notifications, features,
    views); `Heard` holding `SettingsPage`; the look applied on every
    event whether the dialog is open or not. Every control sends at once
    and draws only what Rust sends back. Needs: 4, 5, 36, 37, 38, 39. Core:
    none; move `FLAVOURS` names and the missing-font check into Rust.
    Risk: kit Settings is page-based and wants `&App` closures; slider
    `set_value` is unconfirmed; a custom dialog costs more. Check:
    SHELL-SETTINGS-1..13.
36. **Themes** (partial, L). Missing: eleven more theme constants checked
    against `style.css`; runtime switching (Tokens global, kit theme,
    notify, terminal palette); "system" from `cx.window_appearance()`;
    palette previews for the cards; bundled faces per theme or an agreed
    fallback. Needs: 37. Core: optionally put the twelve palettes in core
    so the window border, previews and tests share one table. Risk: views
    that cached colours in a field go stale (terminal `Palette`, the diff
    tone cache). Latte is the only light theme and exercises contrast.
    Check: SHELL-LOOK-2, SHELL-SETTINGS-6a,b.
37. **Fonts** (partial, L, with 41). Missing: user families for UI and mono
    ahead of the bundled faces; installed-family list from
    `all_font_names()` through `settings::fonts::families`; terminal
    re-measure on a mono change. Needs: 35. Core: none. Risk: eight families
    of woff2 to convert and license for the other themes. Check:
    SHELL-LOOK-4,6, SHELL-SETTINGS-12.
38. **Font picker** (missing, M). Missing: the combo list (open on focus,
    substring narrowing, Down/Up clamped, Enter picks or commits as typed,
    Escape closes only the list, blur commits), notes, mono-only filter.
    Needs: 35, 37, 5. Core: a `FontPickerModel` in core with tests (none
    exist). Risk: 1000+ rows each in its own face need virtualising. Check:
    SHELL-FONT-1..6.
39. **Window look: backdrop and opacity** (missing, M). Missing: always open
    Transparent; opacity as alpha on background tokens only (not borders or
    text); `set_background_appearance` on every Theme, Backdrop, Opacity and
    OS change; Mica tint from dark; the terminal's opaque quad. Needs: 36,
    35. Core: fix `MICA_BUILD` (22000 in core, 22621 in the spike) with a
    test. Risk: every surface fill must read an alpha-aware token; Acrylic
    is spiked only as `Blurred`. Check: SHELL-LOOK-3,5, SETTINGS-6c,6d,10.
40. **Window frame parity** (partial, S). Missing: DWM border colour per
    theme (12-entry table) on theme or OS change; macOS traffic-light room
    and double-click; Linux edges unchecked; drop dead window `Call`
    variants once Tauri goes. Needs: 36. Core: move the table out of
    `argus-desktop/src/system/window_frame.rs` into core or the theme.
    Risk: macOS and Linux cannot be checked locally. Check: SHELL-TITLE-1..6.
41. **Zoom** (partial, in 37; L). Missing: Ctrl +/-/0 (50..200%), whole
    window and terminal cells follow, kept in `desktop.json`. Tokens are
    plain `f32` and views also use literal `px()`; decide a scale scheme
    (a multiplier in every size accessor, or rem size) and forbid raw
    literals. Needs: 6, 9. Core: none. Risk: the riskiest item here; touches
    every pixel and the terminal fit loop. Check: SHELL-KEYS-3,4.
42. **Clipboard: OSC 52 and copy feedback** (partial, S). Missing:
    `Event::Clipboard` written through the same outlet; "Copied" for 1.2 s;
    selection Copy for non-input text; `CopyText` handled by the owner.
    Needs: 11. Core: route OSC 52 through `effects`. Check:
    SHELL-SYSTEM-3,4, SEAM-11.
43. **System notifications** (missing, S). `Owner::act` logs "alert not
    raised". Hand `Alert` to the OS (kit `Notification::system` or
    notify-rust); confirm the window sends `Call::Focus`. Core: none.
    Risk: Windows needs an app user model id or toasts drop. Check:
    SHELL-SYSTEM-2.
44. **Chime** (missing, S). Audio output and the two-note envelope (660 Hz
    then 880 Hz at 0.12 s, 10 ms rise, exponential fall). Core: envelope as
    a pure function with a sample test. Risk: new audio dependency and a
    device-less CI path. Check: SHELL-SYSTEM-1.

### Phase 5: shared renderers

Build these before any surface that draws agent content.

45. **Per-view state store and event intake** (missing, M). `Heard` fields
    and arms for `ViewsPage`, `SavedViewsPage`, `ExtensionsPage`,
    `ExtDialogPage`, Features, Capture, Sessions, Link, Reconcile,
    Checkouts, Code, Browsers (kept per pane, not through whole-model
    notify); calls wired (`open_view`, `press_view`, and the rest); one
    retained store keyed by view path that survives snapshots. Needs: 8.
    Core: none. Risk: keying by index or losing the store on a snapshot
    resets a tab or a half-typed form each time an agent republishes.
    Check: cross-cutting CROSS-13, `ctx.ts`.
46. **Markdown parser** (missing, L). Port to Rust in core: headings, lists
    one level, tables, quotes, fences, inline marks; raw HTML as text,
    images as links, only http(s) links, depth caps, raw text on failure.
    Needs: nothing. Core: new module, proptest, and a cargo-fuzz target
    (input comes from outside). Risk: security-sensitive; kit
    `TextView::markdown` documents none of these guarantees. Selectable
    text in wrapped runs is hard. Check: MARKDOWN-1..9.
47. **Values, chips, references** (missing, M). Text, number, bool, date,
    person with the same tint hash, chips, links, tickets, progress,
    duration, inline code, paths/commits/features/panes via `open_ref`.
    Needs: 2. Core: `ago`, duration, initials, tint, `cellText`, the
    "inside" path test (a second copy of `is_checkout_relative`). Risk: no
    node test covers any of it; write Rust tests first. Check: VALUES-1..8.
48. **Block renderer core** (missing, L). Dispatch by path, section, columns
    (stack when narrow), tabs (kept per path, clamped), divider, unknown
    block text, "opens" mark. Needs: 45, 47. Core: none. Risk: container
    queries become measured widths; text selection and tooltips are free in
    the webview only; nested scroll needs wheel routing. Check: BLOCKS-1..6,
    23,25.
49. **Action buttons** (missing, S). Tone button; disabled with a reason when
    the daemon cannot take presses; spinner while pending; inline confirm
    "Yes No" that survives redraws; `PressView` and `PressSavedView` with
    selection. Needs: 45. Check: AGENTVIEWS-12, FORM-3.
50. **Simple blocks** (missing, L). Fields, list, stats, code with Copy,
    callout, card, grid, details, row, embed, progress, timeline with lanes,
    tree, matrix, checklist, log with tail-follow, board. Seventeen blocks;
    the spec's M is closer to L. Needs: 46, 47, 48, 49, 52. Core: embed
    verdict, lane assignment, matrix tint, progress clamp, checklist count.
    Risk: callout glyph differs if the kit icons are used. Check:
    BLOCKS-7..22.
51. **Table block** (missing, L). Sort (asc, desc, agent's `sorted_by` marked),
    drop columns by width, filter over 8 rows, select one or many with
    tri-state all, TSV copy, row detail, hover actions, "Showing n of N".
    Extension tables reuse it. Needs: 47, 49. Core: sort, filter, TSV in
    Rust with an explicit ordering rule replacing `localeCompare`. Risk: kit
    `DataTable` fits neither selection nor dropping; custom grid, virtualised.
    Check: TABLE-1..8.
52. **Chart and spark** (missing, M). Line, bar, area, spark; ticks, compact
    labels, dashes per series, gaps, tooltips, legend. Needs: tooltip. Core:
    `chart_layout` returning ticks and polylines, tested. Risk: dashed
    strokes were not proven in the spike. Check: CHART-1..9.
53. **Sequence diagram painter** (missing, XL, shared by two items). One
    element serves the diagram block and the Flows lens: lifelines, call and
    reply arrows, self-call, frames, badges, selection, drag guides. Needs:
    spike on dashes and arrowheads. Core: `layout(&Sequence) -> Drawn` with
    constants and tests (none today); fit and nearest-target as pure
    functions. Risk: thin arrow hit-testing; the 7.2 px/char estimate
    (measure or keep). Check: DIAGRAM-1..5, BOARD-SEQ-1..20.
54. **Graph block** (missing, L). Layered layout (348 lines of TS, node
    tested); nodes, edges, loops, "opens" links. Needs: 55, 53's painter.
    Core: port `graph_layout` with its invariant tests and a determinism
    property. Check: GRAPH-1..4.
55. **Figure: open larger** (missing, M). Modal with zoom about the pointer,
    pan, Fit, Source, keys. Needs: 4, `kit/drag.rs`. Core: port `zoom.ts`
    with its test. Risk: the figure treats under 4 px as a click; the shared
    helper says 5 px. Check: FIGURE-1..6.
56. **Patch block** (missing, S). Unified diff string to the review's
    `DiffRow`, through `kit/diff`. Needs: nothing. Core: `parse_patch` with
    tests for `-- x` vs `--- a/x`, no-newline notes, fallback. Risk: the
    diff view owns its scroll; needs a bounded height. Check: PATCH-1..3.
57. **Form block** (missing, M). Fields, number bounds, choice, toggle,
    date, submit action; typed values kept across redraws; answers capped
    at 4096 bytes. Needs: 5, 49. Core: optionally answer assembly. Risk:
    defaults must not be put back under the person's fingers. Check:
    FORM-1..4.
58. **Changed-block marking** (missing, S). Flash once if looked at, else
    unseen dot on the tab and flash on bring-forward. Needs: 48, 92. Check:
    AGENTVIEWS-13, BLOCKS-2.

### Phase 6: Checkouts, Files, Code

59. **Checkouts stage** (missing, L). Strip, table with fold row, State
    cell, selection, five skeleton rows, empty states, key bar. Needs: 1, 3,
    45. Core: move `stateOf`, `branchBeside`, plurals, key-bar content and
    the row partition into `CheckoutsRow`, with the node tests becoming
    cargo tests. Risk: kit `DataTable` has no spanning row; use a flat
    virtual list. Selection stays Rust's. Check: CHECKOUTS-1..27,48,51.
60. **Checkouts side column** (missing, M). Path from `/.argus/`, note,
    actions, facts, "Running here" panes, foot; yields to Files. Needs: 59,
    20. Core: `repoRelative` to Rust. Check: CHECKOUTS-33..38.
61. **Checkouts menus** (missing, M). Strip and per-row menus. Needs: 3, 59.
    Core: Rust builds the item list per row kind. Risk: kit hint text comes
    from action bindings Argus does not use. Check: CHECKOUTS-14,25,26.
62. **Checkouts dialogs** (missing, M). New worktree (name dialog) and the
    removal/force-delete question. Needs: 4, 59. Core: make the name dialog
    Rust-held like `Confirm`. Check: CHECKOUTS-39..43,49,50.
63. **Stage key router and Checkouts keys** (missing, S). Extract
    `review/keys.rs::bound` into `keys/` so Checkouts, Files and Code share
    "look up the place, hold the rest". Needs: 6, 59. Check: CHECKOUTS-28..32.
64. **Files panel** (missing, L). Tree read a level at a time, filter with
    `#`, breadcrumb, All/Changed, ignored fold, row menu, skeletons. One flat
    tree widget in `kit/` serves Files and Code. Needs: 59, 3, 42. Core:
    ignored-fold grouping, `markOf`, `pathOf` and the `#` route into
    `FilesPage` (FILES-23). Risk: focus rules; reveal disabled on a remote.
    Check: FILES-1..24.
65. **Code stage frame** (missing, M). Open rules, three columns, sashes
    that report on release and reset on double-click, retained last page.
    Needs: 45, 63. Core: none. Risk: keep the last `CodePage` when
    `open:false`. Check: CODE-1..11.
66. **Code files column** (missing, L). Find field, scope row, tree, ignored
    fold, go-to list (50 matches). Reuse `review/files.rs` jump handling.
    Needs: 64, 65. Core: per-folder changed counts and field display text in
    `CodePage`; one typed `Query` for the `#` rule (written three times).
    Risk: `set_value` does not emit Change; Ctrl-P collides with Vim. Check:
    CODE-12..24.
67. **Code search** (missing, M). Options Aa, ab, .*, include glob, grouped
    hits with highlights, stepping. Needs: 65, 66. Core: none. Risk:
    Alt-chords must be caught before the Input. Check: CODE-25..33,52,53.
68. **Code start panel** (missing, S). Changed list, opened lately, hint.
    Core: move opened-lately and the 10-row cap into `CodePage`. Check:
    CODE-34.
69. **Code file view, read-only** (missing, L). Gutter marks, removal
    markers, version pill, facts, states, scroll memory. Reuse `kit/diff`
    measure and gutter code; do not fork them. Needs: 65. Core: send the
    facts line and the interleaved row list. Risk: variable-height rows keep
    ruler marks honest. Check: CODE-35..42.
70. **Code ruler** (missing, M). Marks, band, drag. Needs: 69. Risk: marks
    from measured heights. Check: CODE-43,44.
71. **Code outline and changes list** (missing, S). Check: CODE-45,46.
72. **Code keys and focus** (missing, M). Two regimes (read-only and editor).
    Needs: 63, 69. Core: move scroll and focus policy into tested logic.
    Check: CODE-47..49,41,42.
73. **Comment box for Code** (partial, S). Give the box a small "ways" trait
    (send, cancel, recipient) instead of a second copy; in-flow block above
    the editor; Alt-C. Needs: 69. Check: CODE-49..51.
74. **Hops into Code and back** (missing, S). Until Code lands `o` and `c`
    in Git park the dialog and strand the person. Hold them meanwhile.
    Needs: 65. Check: REVIEW-32, HISTORY-7, CODE-8.
75. **Editor widget** (missing, XL). Spike first: kit `Editor` has no known
    gutter mark, line background or Vim hook. Sessions, switching without
    blanking, status bar, change stepping. Needs: 69, 35. Core: windowless
    session, timer and status modules; add editor keys to `keymap.rs`.
    Check: editor.md OPEN, SWITCH, KEYS, GUTTER, STATE, SET.
76. **Live sync** (missing, L). Port `sync.ts` to Rust over a rope with the
    change-set algebra and its random-interleaving property test. UTF-16
    units convert at the edge. Reconnect must respond to the connection
    event, not a redraw. Needs: 75. Check: editor.md SYNC, SAVE, DISK,
    CLASH, RECONN.
77. **Vim** (missing, XL). A windowless state machine in core over a rope,
    property-tested. Decide the subset first. Registers shared across
    editors. Needs: 75, 42. Check: editor.md VIM, KEYS.
78. **Grammars and colours** (missing, XL). Decide wasm vs bundled native
    grammars first. One capture-to-class table shared with the daemon in
    `argus-protocol`; the registry and its SHA-256 check become Rust data
    and a cargo test. Needs: 75. Risk: supply chain, wasm runtime weight.
    Check: editor.md GRAM, COLOUR.

### Phase 7: Browsers

79. **Browser pictures** (missing, M). Decode `BrowserShot` (JPEG feature;
    scenes use PNG, so sniff), latest wins, drop on tab-less, one frame per
    pane with a per-pane signal like `Painted`. Core: optional raw bytes.
    Risk: decode off the UI thread; share a decoded image across places.
80. **Browsers stage** (missing, M). Tiles, click shows the pane, empty
    state with the `argus-hook browser` hint, title "Every agent". Needs:
    79, 15. Check: `browser/stage.ts`.
81. **Browser pane cell** (missing, L; merged). Cell kind in the grid, blank
    state, retained per browser, keyed ring, Close. The grid must reserve
    this kind from the start. Needs: 12, 79. Core: move `keyPane` and
    `keyedBrowser` into core. Check: `browser/pane.ts`.
82. **Viewport reporting** (missing, S). `WxH@scale` on resize, not under 50
    px, deduped, throttled. Core: dedupe in `Browsers` so a reconnect
    resends it. Needs: 81.
83. **Driving the page** (missing, M). Click mapped through the letterbox,
    wheel, named keys, text; Ctrl/Meta chords stay the window's. Core: move
    `within`, `keyInput`, `address` to core with proptests. Needs: 81, 6.
84. **Address bar** (missing, S). `Place::Address`: Enter navigates, Escape
    restores, no overwrite mid-edit. Core: `address()` in core; the
    `host:port` scheme quirk is decided, not copied.
85. **Tab strip** (missing, M). Pick, close, middle-click, plus, scroll kept
    across updates.
86. **Toolbar** (missing, S). Back, Forward, Reload, Close, New tab.
87. **Status chip, take control, follow agent** (missing, S). Core sends one
    status word instead of two ladders.
88. **Browser rows in the rail** (partial, S). Distinct icon, click shows a
    cell not a terminal, closing does not kill a PTY.
89. **Browser keys** (partial, S). Route `Place::Address` and the
    page-has-keys rule; keep it apart from `page_key.rs`.
90. **Browser scenes** (partial, S). Three core scenes plus tab-less and
    held.

### Phase 8: Views and extensions

91. **Panel bodies** (partial, S). Real bodies for Views, Saved and
    Extension panels; one content element docked, floating or on stage.
92. **Agent views panel** (missing, M). Tab strip, launcher (place decided,
    section 5), chosen view from Rust, spinner and unseen dot.
93. **Agent view header and popped page** (missing, M). Menu, meta line,
    status pill from Rust, kept scroll, a timer or Rust re-send so "ago"
    does not go stale.
94. **Saved view panels** (missing, L). Columns, headers, details, delete
    question, stale mark, link rule from `saved_links`. Several vertical
    scrollers inside a horizontal one. Core: built-at text and commit cuts.
95. **Floating saved views** (missing, M). Content for item 26's frame.
96. **Saved Views fold in the rail** (missing, M). Grouping, Here/All,
    title stripping, nesting, stale dot, drag. Core: move `grouped`,
    `underGroup`, `nested`, `count`, `forFeature` into Rust.
97. **Extensions fold in the rail** (missing, S). Icons by exhaustive
    match (the page lookup accepts `constructor`).
98. **Extension view** (missing, L). Agent table plus selection, status
    echo, responsive action bar, `Layer::Extension`. Core: move the key to
    action match and the echo rule. Do not port `views.ts` file for file.
99. **Extension console view** (missing, M; merged). A further side for
    extension consoles, one "keyed" side from the session, focus kept when
    moving between dock and stage.
100. **Extension confirm** (missing, S). Check: EXTVIEW-17..19.
101. **Extension questions** (missing, M). Confirm, prompt, pick, form; one
     that arrives on its own must not steal focus from a terminal. Test
     with the real terminal focused.
102. **Extension commands in palette and sheet** (missing, S). Entries move
     into Rust; extension keys never shadow Argus keys.

### Phase 9: Features and agents

103. **Feature stage frame** (missing, M). States, counts, List|Board radio,
     filter that survives snapshots, fold toggle, key bar.
104. **Feature list and Peek** (missing, M). Custom rows, not kit `List`.
     Row order is Rust's; never sort in the view.
105. **Feature board** (missing, L). Lanes, side stack, cards, folded
     Archived, cut-off chip. Keep lane scroll across pushes.
106. **Card drag** (partial, M). Ghost, banner, drop. Core:
     `Call::DropFeature` so the accept/refuse and "follows the work" text
     are Rust's.
107. **Move, Comes after, Hold** (missing, M). Core: one `FeatureAction`
     call for hold and archive.
108. **Index keys** (missing, S).
109. **Capture note** (missing, M). Spike the line-number gutter against soft
     wrap first.
110. **Open feature frame** (missing, M). Lens radio, status line, feature
     menu. Core: reconcile entry and ticket label as fields, not sentences
     to unpick; saved-view filtering into `FeatureView`.
111. **In-place forms** (missing, M). One `Form` entity and `Editing` enum.
112. **Work lens** (missing, L). Outline, detail pane, legend, sash, scroll
     by cursor identity.
113. **Record lens** (missing, M). Core: Copy as ADR through
     `Call::CopyText` with the notice from Rust.
114. **Flows lens** (missing, M). Core: bind choices carry id and bound flag.
115. **Tracker link dialog** (missing, M).
116. **Ticket chips** (missing, S). Core: match ticket to view in Rust.
117. **Reconcile dialog** (missing, L). Keep the lines list scroll.
118. **Compose box** (missing, M). A `ComposerPage` in core for the
     effective-pick rules and the draft. Check kit `Select` keeps its popup
     open across `set_items`.
119. **Sessions dialog** (missing, L). Core: one `foldable` flag, the
     meta wording, filter menu items. Keep the list offset when the reader
     closes.
120. **Transcript reader** (missing, L). Own view, own scroll handles.
     Skip entries of an unknown kind. Add a scene (none exists).
121. **Shortcuts-sheet and palette scenes** (missing, S). Scenes for every
     uncovered state the specs list, so `render_to_image` gates each item.

### Done (no work, listed so they are not flagged)

- Opener, logging, release plumbing. Both windows write the same log file
  during the port; see section 5.
- Connection life: bridge, reconnect, host switch.
- Git stage shell, Changes file column and jump field, shared diff view,
  History tab.
- Panel drag to a dock or the stage.
- Browser demand and reconnect (`Browsers::drawn`); `Heard` must clear its
  own frames when the connection changes.

## 3. Spec gaps

Add each to `gpui/parity/` before its item is built. A verifier will
otherwise flag them or miss them.

1. **Browsers** (everything: nav item, count, fold, sub-rows, stage, key 6
   and leader 6, the cell, tabs, toolbar, address, status, viewport,
   frames, input, auto-open-beside, `Place::Address`, scenes). Items 22,
   79-90.
2. **Move a pane to another checkout** (commit 88dcce24): drop-target
   highlight, `move_to_checkout`, capability gate, browsers excluded,
   daemon-side `cd`. Item 14.
3. **Settings: Vim keys and Install grammars** (Editor section). The spec
   says six sections and older rows.
4. **GPUI-only decisions already shipped**: the Git nav item replacing the
   review and history dialogs, held rail on reconnect (decision 49),
   `Link::Reconnecting`, quit instead of close.
5. **Window size and Mica threshold**: 1200x800/720x480 against
   1280x800/900x560, and build 22000 against 22621. The spec has no rule
   for which wins.
6. **Extension and saved nav entries** and their dock mark: the nav needs a
   dynamic-tail rule.
7. **Per-theme bundled fonts**: the specs treat fonts as one list.
8. **Hover**: key caps always shown in the page, on hover in GPUI; hover-only
   close buttons and `title` tooltips have no entry.
9. **System appearance**: how "system" re-resolves in GPUI.
10. **`Call::GoToFeature`** is in core and palette code; board.md has no id.
11. **Board**: what to do when the peeked feature is hidden by a fold; the
    brief height has no sash; Escape on the bare index; no focus-trap rule
    for Link, Reconcile, Sessions, hold.
12. **Notices** have no surface entry of their own, yet ~30 rules lean on
    them.
13. **Parked dialogs**: how the window shows Git again after Code, and
    what is under it meanwhile.
14. **Editor**: no scene for clash, offline, failed save, closed file,
    installing; Vim untested on main; `gc` and `=` with no language;
    `Synced` starts at version 0; reconnect depends on a redraw; Ctrl-P
    against Vim's normal mode.
15. **Terminal find and links**: neither exists on main. Nothing to port.
16. **Extension console sides** need a side beyond Workspace, First and
    Second; no ids for panel placement or `focus_view` routing.
17. **Views**: text selection and copy from rendered content, tooltip and
    Tab reach, nested scroll and wheel owner, where the Views launcher
    lives, what an http link from a view should do now that browser panes
    exist, "ago" freshness, responsive layout mechanism, scenes for most
    blocks.
18. **Drag**: threshold mismatch (page 0/4/6 px, helper and cross-cutting
    5 px, figure 4 px); payloads for saved-view and extension rows dragged
    to a dock.
19. **Remote daemons**: how a nav entry shows a daemon too old for its
    surface (a notice only).
20. **Clipboard**: Rust (`CopyText`) or the platform, and the Vim `+`
    register.
21. **GoToPane and the peek line** for a browser pane: what they show.

## 4. Structure

The AGENTS.md rule stands: argus-gpui folders mirror the page's domain
names, shared parts go to `kit/`, and a module answers one question. Where
the page is weak, the port should not copy it.

- **Strip by slot, not switch.** `chrome.ts` (1101 lines) answers seven
  questions and imports every stage to write the strip. Keep the GPUI split
  (`nav`, `foot`, `rail/*`, `header`). Stages hand the strip their tools.
- **Entities observe a slice.** The page redraws everything per event.
  Each surface is an entity reading only the slice of `Heard` it uses.
  `RailView` cloning the whole `Rail` goes.
- **Rail cursor in `Session`.** The pick and start-menu cursor live in page
  state and `rail/keys.rs` matches literal key names. Move it to core so
  both windows and the keymap tests share it.
- **Switcher, removal, add-project are three modules**, not `projects.ts`.
- **The look is global.** `applyLook` ran inside the settings dialog. In
  GPUI the look lives in `theme/` and `window/`, applies with the dialog
  closed, and drops the canvas font probe (GPUI sees the same fonts).
- **Ranking, geometry and zoom as pure Rust with tests**: palette, drop
  zones, gutters, wheel, selection clamp, chart, diagram and graph layout,
  `parse_patch`, markdown, sort and filter, address, letterbox hit-test,
  `FontPickerModel`. The page's tests are node-only or absent.
- **One of each**: dialog shell (a dozen files rebuild scrim, focus,
  Escape), keycap, key-naming (`chords.ts`, `keys/caps.rs`, a third
  `Chord::matches`), time vocabulary, the capture-to-class table, the
  ignored fold, the `#` query rule, the sequence painter, `Landing` enum.
- **One `Dragger`, many payloads.** Four page drags disagree on threshold
  and Escape. `kit/drag.rs` is the shape; drop targets are published by
  views into a registry.
- **Words arrive as data.** Where the page does string surgery (subtask
  counts, bind-choice ids, reconcile sentence, card footer split, tickets
  label, chip status ladders, `viewOf`), add typed fields to the snapshots.
- **View-models in core**: `StripPage`, `FootPage`, `ComposerPage`, the
  checkouts presentation, menu item lists. Each replaces page logic whose
  tests cannot run in Rust.
- **Retained state per surface**, keyed by path or cursor identity:
  scroll, forms, drafts, fold states. The page keeps module-level maps and
  restores scroll by class name.
- **Large files split by question**: `work.ts` (1132), `feature_stage.ts`
  (1068), `code/stage.ts` (955), `sequence.ts` (702), `extensions/views.ts`
  (669), `sessions_dialog.ts`. Do as `review/` and `console/` were done.
- **Keep the lens modules independent.** No lens imports another for
  helpers; the decision detail belongs to the Record lens.
- **No import cycles between `views/` and `extensions/`.** One table
  module with an options type.
- **Stage keys once.** Extract the look-up-and-hold routine into `keys/`.

## 5. Open questions for a person

1. **Window size.** Keep 1280x800 with a 900x560 minimum, or return to
   1200x800 and 720x480? Below 900 the folded rail and drawer cannot be
   reached.
2. **Mica threshold.** Build 22000 (core) or 22621 (the spike)? If the
   latter, say so in Settings or hide the choice below it.
3. **Zoom scheme.** A multiplier in every size accessor, or rem size? Is a
   ban on raw `px()` literals acceptable?
4. **Per-theme fonts.** Ship the other families (about 8, with licences),
   or fall every theme back to Inter and JetBrains Mono?
5. **Grammars.** Wasm with install on request, or bundled native grammars
   and no install? Which Vim subset is promised?
6. **Ctrl-P in the editor.** The stage's go-to, or Vim's k?
7. **Escape on the bare Feature index.** Nothing (as built), or climb to the
   Workspace (as DESIGN.md says)?
8. **Palette footer "Tab next group"**: build it or drop the hint?
9. **`Repository.divider`.** Sent, never drawn. Draw it or remove it?
10. **Key caps on the nav**: always shown or on hover?
11. **Dimming on reconnect.** GPUI dims on Reconnecting, the page only on
    Stopped. Keep decision 49?
12. **Drag threshold.** Page 4/6 px, helper 5 px. Take 5 everywhere?
13. **Time text and locale.** "4m" or "4 min ago"? English only, or the
    system locale for dates and numbers?
14. **Where the Views launcher lives** with no title strip: stage header or
    rail foot?
15. **Fold states** (Browsers list, Views twist): local view state, or kept
    in `desktop.json`?
16. **Tooltips** for sighted users: add hover text where the page had
    `title`?
17. **Text selection** in rendered view content: required, or accepted loss?
18. **Browser pane**: Close is irreversible for every client, no confirm.
    Keep? Back and Forward show no state; add a flag to the protocol?
    Treat `host:port` as a host, not a scheme?
19. **Links from a view or the terminal**: system browser or a Browser pane?
20. **Logging.** Both windows write `argus-desktop.log` during the port.
    Rename the GPUI one?
21. **Quit on close.** Keep it after Tauri goes, or fix teardown?
22. **Close the others**: add a `close_others` call or keep N closes?
23. **Opened-lately and the Ended bar template**: both are page guesses.
    Move to Rust as designed, or drop?
24. **Order.** Do Browsers (79-90) before or after Checkouts and Code?
    The plan puts them after because Code gates the Git hop and the editor
    is the longest pole.
