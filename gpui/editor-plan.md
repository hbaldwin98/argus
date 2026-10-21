# The Code editor on GPUI: plan

How the GPUI window gives the Code stage's editor what `main`'s CodeMirror
editor has: Vim mode, tree-sitter colours from grammars installed on
request, and live sync with agents (`GPUI.md` §"The Code editor"). Every
claim about `gpui-kit` below cites the 0.8.0 source in the cargo registry.
Anything the source does not show is marked **unverified**, and a spike has
to answer it before anything is built on it.

## How to read this

- **Registry paths.** `R` is `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`.
  `base:X` is `R/gpui-base-0.8.0/X`, the engine `gpui-kit` builds every text
  control on. `comp:X` is `R/gpui-component-0.8.0/X`, the styled components
  and the tree-sitter highlighter. `kit:X` is `R/gpui-kit-0.8.0/X`. `pre:X`
  is `R/gpui-pre-0.3.9/X` and `win:X` is `R/gpui-pre-windows-0.3.9/X`.
  `ts26:X` and `ts27:X` are `R/tree-sitter-0.26.13/X` and
  `R/tree-sitter-0.27.0/X`, and `tsl:X` is `R/tree-sitter-language-0.1.8/X`.
- **Argus paths.** These follow `gpui/parity/shell.md`. `ts:X` is
  `crates/argus-desktop/page/src/X`, `rs:X` is
  `crates/argus-desktop-core/src/X` and `tauri:X` is
  `crates/argus-desktop/src/X`. `argusd:X` is `crates/argusd/src/X` and
  `proto:X` is `crates/argus-protocol/src/X`.
- **Offsets.** `gpui-kit` counts UTF-8 bytes (`base:src/input/base/state.rs:3338-3344`).
  The wire counts UTF-16 units, with each CRLF read as one LF
  (`proto:code/edit.rs:12-16`). Every crossing between the two converts,
  and the code that converts gets a property test.
- **Licences.** `gpui-kit`, `gpui-component` and `gpui-base` are Apache-2.0
  (`kit:Cargo.toml:35`, `comp:Cargo.toml:34`, `base:Cargo.toml:34`), and so
  is `gpui-pre` (`pre:Cargo.toml:33`). Argus is MIT OR Apache-2.0, so a file
  copied from them (for example the `input_adapter` debounce) is Apache-2.0
  only in Argus's tree and cannot be offered under MIT. A copy keeps its
  licence header, ships with the upstream `NOTICE` and carries a change
  statement. Decide before copying whether that file's crate may be
  Apache-2.0 only.

## What `main` does today

- **Vim.** `main` uses `@replit/codemirror-vim` 6.4.0
  (`crates/argus-desktop/page/package.json:20`). Vim is on unless Settings
  turns it off (`ts:code/editor.ts:123-125`), and it is reconfigured live
  (`ts:code/editor.ts:665-669`). Its keys are heard before any other keymap
  (`ts:code/editor.ts:478-479`). Argus adds `:w`, `]c` and `[c` through
  `Vim.defineEx`, `defineAction` and `mapCommand` (`ts:code/editor.ts:269-277`).
- **Grammars.**
  - Each grammar is an npm package's prebuilt `.wasm` parser plus its
    `highlights.scm`. Pinned grammars are checked by SHA-256
    (`ts:code/grammar/known.ts:1-7,28-47`). For any other extension, `main`
    searches npm and installs only after the person says yes
    (`ts:code/grammar/fetch.ts:1-6,93-134`).
  - The bytes and the joined query live in the webview's IndexedDB as
    `{ name, label, source, wasm: Uint8Array, query: string }`
    (`ts:code/grammar/store.ts:1-61`).
  - They load through `web-tree-sitter` 0.27.0
    (`ts:code/grammar/load.ts:16-51`, `package.json:26`).
  - The daemon installs nothing. `argusd` links ten grammars statically for
    the read view and the outline (`argusd:syntax/grammar.rs:8-20`,
    `argusd:syntax/highlight.rs:67-82`, `crates/argusd/Cargo.toml:38-49`).
- **Colours.** A capture is coloured by its first word. That table is the
  daemon's, so the read view and the editor agree
  (`ts:code/colours.ts:26-55`).
- **Live sync.**
  - The daemon referees. A change applies only to the version it was made
    on (`proto:code/edit.rs:1-6`, `ts:code/sync.ts:1-8`).
  - A disk change is applied without entering undo history and without
    counting as typing. It is lit for a moment
    (`ts:code/editor.ts:594-610`).
  - Local changes are reported from CodeMirror's change set as ranges
    (`ts:code/editor.ts:511-514`, `ts:code/sync.ts:12-24`).

## 1. How input reaches `EditorState`, and whether a Vim layer can drive it

### What `EditorState` is

- `EditorState` is `InputBaseState<EditorMode>`, the shared engine in its
  code-editor kind (`base:src/input/editor/mod.rs:5-11`).
- `gpui-component` re-exports it (`comp:src/input/mod.rs:25-41`) and wraps
  it in a styled `Editor` element (`comp:src/input/editor.rs:16-55`).
- The styled `Editor` renders through `Input::from_state` and adds a font,
  a line height and `readonly` (`comp:src/input/editor.rs:147-178`).

### Actions and key bindings

- The actions live in the `input` namespace: `Backspace`, `Delete`,
  `MoveUp`, `MoveDown` … `Undo`, `Redo`, `Search`, `Replace`, `GoToDefinition`
  (`base:src/input/base/state.rs:76-126`). `Enter { secondary, shift }` is
  separate (`state.rs:53-74`).
- They are bound globally with `KeyBinding::new(.., Some("Input"))` in
  `init` (`state.rs:136-311`).
- Several bindings clash with Vim:
  - `escape` → `Escape` (`state.rs:182`)
  - `ctrl-[` → `Outdent` (`state.rs:198`)
  - `ctrl-a` → `SelectAll`, or `MoveHome` on macOS only (`state.rs:253,267`)
  - `ctrl-e` → `MoveEnd` on macOS only; unbound on Windows and Linux
    (`state.rs:266-271`)
  - `ctrl-f` → `Search` (`state.rs:305`)
  - `ctrl-h` → `Replace` (`state.rs:309`)
  - `ctrl-v` → `Paste` (`state.rs:265`)
  - `ctrl-x` → `Cut` (`state.rs:261`)
  - `ctrl-y` → `Redo` (`state.rs:297`)
  - `ctrl-z` → `Undo` (`state.rs:295`)

### Key contexts

- The element declares `key_context("Input")` (`state.rs:4841`).
- Editor mode adds `"Input mode=editor"` (`base:src/input/editor/mod.rs:183-193`).
- The docs give bindings the predicate `Input && mode == editor`
  (gpui-kit.com/component/editor.md).
- Action listeners are attached in `Render`. Undo and redo are at
  `state.rs:4856-4857`.

### Text input

Printable characters do not arrive as actions. They come through the
platform input handler (`EntityInputHandler`, `state.rs:4303`), whose
`replace_text_in_range` inserts them (`state.rs:4345-4466`). On Windows,
`WM_CHAR` calls that handler directly (`win:src/events.rs:465-472`).

### Can an application see every keystroke first? Yes

`App::intercept_keystrokes` runs "_before_ all other action and event
mechanisms". Calling `cx.stop_propagation` in it prevents action dispatch
(`pre:src/app.rs:2580-2604`). Here is the order in `dispatch_key_event`:

1. **Interceptors run first** (`pre:src/window.rs:6076-6093`). When one
   stops propagation, the window goes straight to
   `finish_dispatch_key_event`. Bindings and actions are skipped, but
   `dispatch_key_down_up_event` still runs, and its capture loop calls a
   listener before it checks `cx.propagate_event`
   (`window.rs:6266-6276`). So the first capture-phase key-down listener on
   the dispatch path still fires once.
2. **Bindings come next** (`window.rs:6095-6165`). They run only when the
   input preference is `KeyBindings`. When the focused handler accepts text
   and the key prefers character input, the preference is
   `CharacterInput`, and a binding on a plain letter never fires
   (`window.rs:5990-6007`, `window.rs:6151`).
3. **Capture-phase `capture_key_down` listeners come last** in the
   key-down pass (`window.rs:6264-6276`). They run after the bindings.

Two conclusions follow:

- A wrapper `div().capture_key_down(..)` is **too late**: the Editor's
  `escape` and `ctrl-[` bindings have already fired.
- Context-predicate bindings cannot catch plain letters in a text input.

The interceptor is the one entry point a Vim layer can use. It is global
across windows, so it checks that the focused handle is the editor's
(`Focusable`, `state.rs:4807`). The `KeystrokeEvent` also carries the
context stack (`window.rs:2591-2596`).

**Does stopping a key also stop its character? On Windows, yes.**

- The message loop sends `WM_KEYDOWN` through `translate_accelerator`
  first, and calls `TranslateMessage` only when the keydown was not handled
  (`win:src/platform.rs:502-516,605-608`).
- A handled key returns `propagate: false` (`win:src/events.rs:437-441`,
  `pre:src/window.rs:5757-5758`).
- So no `WM_CHAR` is generated.
- **Unverified on macOS and Linux.** The spike tests this: in normal mode,
  `j` moves the cursor and inserts nothing.
- IME composition (`WM_IME_*`) does not go through keydown and is
  **unverified**. In normal mode the Vim layer should leave IME alone.

### The public API a Vim layer drives

Unless noted, every method is on `InputBaseState<M>` in
`base:src/input/base/state.rs`, and every offset is a UTF-8 byte.

| Need | Public API |
|---|---|
| Read text | `text() -> &Rope` (1496), `value()` (1466), `selected_text()` (3996). `RopeExt` and `RopeLines` are re-exported (`base:src/input/mod.rs:121`). |
| Cursor | `cursor() -> usize` (3276), `cursor_position() -> Position` (1501), `set_cursor_position(pos, window, cx)` (1509). The setter also focuses (1522). |
| Selection | `selected_range()` (3344, active selection only), `set_selected_range(range, cx)` (3359), `add_selection(range, cx)` (3385, multi-cursor), `select_all` (3348), `unselect` (3669). |
| Edit | `insert(text, ..)` (1159) and `replace(text, ..)` (1178, replaces the selection) set Atomic intent and bypass readonly (1159-1191), but act only at the cursor or selection. `replace_all(text, ..)` (1131, undoable whole text), `set_value(..)` (1099, clears undo, resets selection and scroll). The trait's `replace_text_in_range(Some(utf16_range), text, ..)` (4345) edits any range, but it is the typing path: it runs auto-close-pair insertion, skip-over and `on_text_typed` completion triggers, because the silent variant is `pub(crate)` (1920-1931, 1996, 4382-4396, 4458). Inserting a single `(` or `)` can write different text than asked. |
| Undo and redo | No method. `undo` and `redo` are `pub(super)` (3194, 3198). `window.dispatch_action(Box::new(Undo), cx)` reaches them when the editor is focused, but it is deferred to the end of the effect cycle (`pre:src/window.rs:2546-2558`), so it is asynchronous: a composite command that reads the cursor or text right after `u` sees stale state. |
| Scroll | `scroll_offset()` (3320), `set_scroll_offset(point, cx)` (3327, applied after the next layout), `visible_row_range()` (3315), `line_height()` (3333). `set_selected_range` scrolls to the cursor through `move_to` → `scroll_to` (`base:src/input/base/movement.rs:62-96`). |
| Rows | `display_map()` (671) returns `DisplayMap`, with `buffer_pos_to_display_pos`, `display_row_to_buffer_line`, `buffer_line_to_display_row` and `display_row_count` (`base:src/input/editor/display_map/display_map.rs:45-130`). These handle wrap and folds for `j`, `k`, `zz` and `H`/`M`/`L`. |
| Search | `open_search`, `set_search_query`, `next_search_match`, `previous_search_match`, `close_search` (`base:src/input/editor/search.rs:104-216`). |
| Marks and lighting | `create_range_decorations_collection` (Fill or Frame), `create_decorations_collection` (`base:src/input/editor/decorations.rs:496-560`). Ranges follow edits. |
| Readonly | `set_readonly` (1274). The styled `Editor` also sets it on every render (`comp:src/input/editor.rs:83-86,164`). |

### What a Vim layer needs that is not public

| Gap | Evidence | Smallest path |
|---|---|---|
| A block caret in normal mode | The caret width is a constant `px(1.)` (`base:src/input/base/blink_cursor.rs:7`), drawn at `element.rs:759`. There is no shape API. | A Fill range decoration over the character under the cursor, beside the 1px caret. It is only partial: empty ranges are removed (`decorations.rs:497-509`), so it shows nothing on an empty line, at end of line or at EOF, and a Fill is a 12%-opacity background with no inverted glyph. A real block caret needs an upstream `CursorShape` (bar, block, underline). |
| One undo step per Vim command (`cw`, `dd`, `.`, an insert session) | `begin_transaction`, `commit_transaction` and `break_transaction_coalescing` are `pub(super)` (`base:src/input/base/undo_manager.rs:120-144,303`). With no pending intent, an insertion with an empty range and no newline is classed `EditIntent::Typing` (`state.rs:3164-3174`), and Typing entries coalesce with adjacent ones (`undo_manager.rs:439`, test `adjacent_typing_transactions_coalesce`). | `replace_text_in_range` does not guarantee one entry per operator: a one-line `p`, or several insertions in a row, can merge. Only `insert`/`replace` (Atomic) are guaranteed separate. Grouping is approximate until an upstream public `transact(|state| ..)`. |
| A backward visual selection (`vh`, `vk`, backward visual-line) | `set_selected_range` forces `reversed=false` (`state.rs:3359-3372`); `set_selection` and `select_to` are `pub(super)`/`pub(crate)`; `add_selection` takes no direction. | None on the public API: the caret lands at the wrong end. Needs an upstream setter that takes a direction. |
| Every selection read and restored (visual block, multi-cursor) | `selections` is private. Only the active one is readable (`state.rs:3338-3345`). | Vim visual-block can rebuild its selections from its own state with `add_selection`. Only remote edits need the patch (see 4). |
| Undo and redo without focus | `pub(super)` (3194-3199) | Dispatch the `Undo` and `Redo` actions while the editor is focused. That is enough, since Vim's `u` only runs while it is focused. |

Most of a Vim layer can be built on the public surface today, but not all
of it: the block caret, exact undo grouping and backward visual selections
need upstream changes, and an edit through `replace_text_in_range` is not
the text the Vim command asked for whenever auto-close or skip-over applies.

## 2. Existing Vim work for GPUI

- **Zed's `vim` crate.**
  - Licence: `license = "GPL-3.0-or-later"`
    (https://raw.githubusercontent.com/zed-industries/zed/main/crates/vim/Cargo.toml).
  - Its normal dependencies are Zed's `editor`, `language`, `multi_buffer`,
    `project` and `workspace` crates (`search` is only a dev-dependency).
    They are unpublished and also GPL.
  - Argus is MIT OR Apache-2.0 (`GPUI.md` §"The stack"), so it can be
    neither linked nor copied. It also drives Zed's `Editor`, not
    `gpui-kit`'s.
- **`gpui-kit` / `gpui-component`.** No Vim mode exists in the source. The
  editor's docs page mentions Vim only to say there is "not a persistent
  Vim Visual Block mode", and a web search of the longbridge
  repository found none.
- **`hjkl-engine` / `hjkl-vim`** (https://docs.rs/crate/hjkl-engine/latest,
  https://docs.rs/crate/hjkl-vim/0.41.1).
  - MIT. The engine describes itself as "Vim FSM, motion grammar, and ex
    commands. Pre-1.0 churn."
  - It is generic over a buffer backend and a host (`Editor<B, H>`). Its
    `Buffer` trait splits into Cursor, Query, BufferEdit and Search, but its
    README lists "Buffer/Host trait extraction" as still pending.
  - Version 0.43.3 was released on 2026-10-21, today. It pulls its own
    `hjkl-buffer` and `ropey`.
  - It depends on `ratatui` and `crossterm` unconditionally; its README says
    they stay until the Buffer/Host extraction lands. That is a cost of the
    fallback: terminal crates in a GPUI binary.
  - Usable in principle, but the trait boundary is still moving, and one
    owner releases it often.
- **Nothing else.** A crates.io search found no other editor-independent
  Vim state machine. `vsm` is a session-file manager.

**Recommendation: write a small Vim state machine in Argus.**

- Put it in its own module in `argus-gpui` (`code/vim/`). It is a pure
  function: `(mode, pending, key) -> commands`. A thin adapter applies
  those commands to an `EditorState` through the API in section 1.
- Scope it to the subset below. **No usage data or user source says this is
  what `main`'s users rely on**; it is this plan's assumption, and
  `codemirror-vim` provides far more today:
  - normal, insert, visual, visual-line and visual-block modes
  - counts
  - the motions `hjkl w b e 0 ^ $ gg G f t ; , % { }`
  - the operators `d c y > <` with text objects `iw aw i" a" i( a(`
  - `x p P u ctrl-r . / n N`
  - the ex commands `:w` and `:N`
  - `]c` and `[c`
- **Gaps in that subset.** It has no way into insert mode except `c`: it
  leaves out `i a I A o O`. It also leaves out `r J ~ D C cc`, `ctrl-d`/
  `ctrl-u`, `* #`, registers, marks, macros and `:s`. Add the insert-entry
  commands at least before calling it Vim mode, and list the rest as known
  losses against `main`.
- It is testable without a window: a property test checks that motions
  never leave the text.
- **Cost:** unestimated. An earlier "one to two weeks" had no basis, and the
  subset above is incomplete. The spike in step 6 should size it. There is
  no licence exposure and no outside churn.
- Keep `hjkl-engine` as the fallback if the subset grows past that. Check
  first that its `Buffer` trait has settled.

## 3. Tree-sitter

### Grammars `gpui-kit` compiles in

- The `tree-sitter` feature enables `dep:tree-sitter` and
  `tree-sitter-json` (`comp:Cargo.toml:77-80`).
- Each `tree-sitter-<lang>` feature adds one grammar
  (`kit:Cargo.toml:84-222`, `comp:Cargo.toml:81-...`).
- `tree-sitter-languages` bundles them all (`comp:Cargo.toml:153-189`):
  astro, bash, c, cmake, cpp, csharp, css, diff, ejs, elixir, erb, go,
  graphql, html, java, javascript, jsdoc, kotlin, lua, make, markdown, php,
  proto, python, ruby, rust, scala, sql, svelte, swift, toml, tsx,
  typescript, yaml and zig.
- The docs say all of them together "add tens of megabytes to an
  application" (gpui-kit.com/component/editor.md).

### Blocker: two copies of tree-sitter

- `gpui-component` depends on `tree-sitter = "0.26.13"`
  (`comp:Cargo.toml:420-422`).
- The workspace pins `tree-sitter = "0.27"` for `argusd` (`Cargo.toml:54`).
  The lockfile has only 0.27.0, because the `gpui-kit` feature is off
  (`Cargo.lock:10002-10003`).
- Both versions declare `links = "tree-sitter"` (`ts26:Cargo.toml:22`,
  `ts27:Cargo.toml:22`). Cargo allows one package per `links` value in a
  resolve. **Unverified:** turning on `gpui-kit/tree-sitter` should fail to
  resolve until one side moves.
- `crates/argus-gpui/Cargo.toml:9-14` already keeps the feature off for
  this reason.
- **Fix:** move `argusd`'s tree-sitter crates to 0.26 so both match.
  `tree-sitter-highlight` 0.26.13 exists on crates.io, so the step is
  feasible, but **unverified** is whether `argusd`'s highlight and symbols
  code compiles against the 0.26 API; the spike answers that. The grammar
  crates are of mixed versions (0.23–0.25 for the language ones,
  `tree-sitter-md` 0.5.3, `tree-sitter-toml-ng` 0.7.0 and
  `tree-sitter-yaml` 0.7.2, `Cargo.lock:10073-10125`) and all depend on
  `tree-sitter-language`, not on `tree-sitter`, so they should not care.
- **Other ways round the `links` clash:**
  - keep `argus-gpui` in its own workspace (excluded, with its own
    `Cargo.lock`)
  - patch `gpui-component` onto tree-sitter 0.27
  - use `gpui-kit`'s `syntect` feature (`kit:Cargo.toml`) for highlighting
    without tree-sitter
  - wait for a `gpui-kit` release on 0.27
- **Duplicate grammar crates.** `gpui-component` wants
  `tree-sitter-javascript ^0.23.1`, `css ^0.23.2` and `python ^0.23.6`
  (`comp:Cargo.toml:448,480,516`), while `argusd` uses 0.25. Enabling those
  features, or the `tree-sitter-languages` fallback, brings two copies of
  each, and both export the same C symbol (`tree_sitter_javascript` and so
  on). That is safe only while no binary links both sets.

### Loading a grammar at runtime: yes, through public API

- `LanguageRegistry::singleton().register(name, &GrammarConfig)` adds a
  language (`comp:src/highlighter/registry.rs:515-539`).
- A `GrammarConfig` holds an optional `tree_sitter::Language` and the
  highlights, injections and locals query strings
  (`registry.rs:62-105`).
- `register_parser_factory(name, LanguageParserFactory)`
  (`registry.rs:541-551`) registers a closure that returns
  `(tree_sitter::Parser, tree_sitter::Language)`. It is documented for
  "WASM-compiled parsers loaded at runtime" and takes precedence over a
  statically linked grammar (`registry.rs:497-505`, `567-590`).
- Registering bumps a generation counter, so cached highlighting can
  resolve again (`registry.rs:553-559`).
- The default input highlighter is built for any language that
  `has_parser` (`comp:src/highlighter/input_adapter.rs:22-28`).

Two ways to load a grammar:

- **WebAssembly.**
  - tree-sitter's `wasm` feature (`ts26:Cargo.toml:74-77`) pulls in
    `wasmtime-c-api-impl` 36 (`ts26:Cargo.toml:98-101`).
  - `WasmStore::new(&engine)` and `load_language(name, bytes)`
    (`ts27:binding_rust/wasm_language.rs:46-82`; the same functions exist in
    `ts26`) produce a `Language`. That language parses only with a parser
    that holds the store (`Parser::set_wasm_store`,
    `wasm_language.rs:115-120`).
  - The types fit the factory, but the factory must **cache**. It is
    `Fn + Send + Sync` and runs on every background parse
    (`input_adapter.rs:86`, after each 150 ms debounce) and on every
    injection-layer parse (`highlighter.rs:952`, up to 512 per update),
    from background threads at the same time as other calls. A factory that
    loads the bytes each time compiles the module with wasmtime/cranelift
    per keystroke. Load the `Language` once, share the engine, and give each
    call a fresh `WasmStore::new(&engine)`: `ts_wasm_store_start` and
    `ts_wasm_store_add_language` (`wasm_store.c:1653,1735`) instantiate an
    already-loaded language into any store.
  - A grammar registered after an editor is open is not picked up: the
    highlighter is created only when `has_parser` is true at the time the
    factory runs (`input_adapter.rs:22-28`, `ensure_highlighter`). An open
    editor needs its language reset or its factory reinstalled.
- **Native library.**
  - `LanguageFn::from_raw` is itself `unsafe` and takes
    `unsafe extern "C" fn() -> *const ()` (`tsl:src/language.rs:14`). With
    it, a `tree_sitter_<lang>` symbol loaded with `libloading` becomes a
    `Language`.
  - It needs a compiled `.dll`, `.so` or `.dylib` per platform. npm
    packages ship `.wasm`, not that.

**How this matches `main`.** `main` installs exactly the `.wasm` and
`highlights.scm` that a `GrammarConfig` plus a wasm parser factory needs.
That means:

- The pinned list (`ts:code/grammar/known.ts`) carries over unchanged.
- The npm search (`ts:code/grammar/fetch.ts:93-134`) and the yes-before-install
  rule carry over unchanged.
- Storage moves from IndexedDB to a directory under the instance's config
  directory: one `<name>.wasm` and `<name>.scm` per grammar.
- Downloading and hashing move to Rust. `gpui-pre-reqwest` is a 0.12.15
  `reqwest` fork pulled in by `gpui-kit-assets`, and it expects a tokio
  runtime; the real `reqwest` 0.13 is in the tree only through tauri
  (`argus-desktop`). Pick one HTTP client on purpose. `sha2` is already in
  the lockfile (0.10.9 and 0.11.1), so it is not a new dependency.

Three things are **unverified** and belong to the spike (item 2 is partly
answered: the 0.26 runtime accepts grammar ABI 13 to 15,
`ts26:binding_rust/bindings.rs:3-4`, so only each grammar's own ABI needs
checking):

1. Whether the npm `.wasm` files, built for `web-tree-sitter`, load in
   native tree-sitter's `WasmStore`. Zed builds its extension grammars with
   wasi-sdk, so the npm builds may need to be checked against
   `tree-sitter build --wasm` output.
2. Whether the grammars' ABI matches runtime 0.26.
3. How much `wasmtime` adds to build time and binary size, and whether it
   builds on Windows MSVC.

If (1) fails, the fallback is to compile in `tree-sitter-languages`, or a
chosen subset, and keep runtime install for the rest as unsupported until
fixed.

### From captures to theme colours

- `HighlightTheme` holds `SyntaxColors`. `SyntaxColors::style(name)`
  matches 41 fixed names (`comp:src/highlighter/registry.rs:18-60,243-308`).
- A dotted name it does not know falls back to its first part, so
  `keyword.modifier` resolves as `keyword` (`registry.rs:294-305`).
- `HighlightTheme` implements `HighlightStyleResolver`
  (`registry.rs:493-497`). Each capture's name is resolved against it in
  `SyntaxHighlighter::styles` (`comp:src/highlighter/highlighter.rs:1181-1213`).
- The styled `Input` projects `cx.theme().highlight_theme` into the
  editor's style on every render (`comp:src/input/input.rs:560-575`).
  `Theme.highlight_theme` is a public `Arc<HighlightTheme>`
  (`comp:src/theme/mod.rs:155`) and is set through `Theme::global_mut`
  (`theme/mod.rs:251`).

**Mapping `main`'s colours.** `main`'s table uses first words
(`ts:code/colours.ts:30-49`). Most map straight onto `SyntaxColors` fields:
`attribute`, `boolean`, `comment`, `constant`, `constructor`, `function`,
`keyword`, `label`, `number`, `operator`, `property`, `punctuation`,
`string`, `tag` and `type`. The exceptions:

- `escape`, `module` and `namespace` have no field, so with the stock
  resolver they stay uncoloured.
- **Fix:** rewrite those capture names in the query text at load time
  (`@namespace` → `@type`, `@module` → `@type`, `@escape` →
  `@string.escape`). Do it in the one Argus module that also builds the
  `HighlightTheme` from Argus's theme tokens.
- Otherwise use the unstyled `gpui_base::input::Editor`
  (`base:src/input/editor/mod.rs:216-234`) with an Argus
  `HighlightStyleResolver` passed through `set_editor_style`
  (`base:src/input/base/state.rs:1017-1020`). That loses the styled
  chrome.

## 4. Live sync

### Applying a remote edit without moving the person's cursor

**`EditorState` does not offer this.**

| Path | What happens | Source |
|---|---|---|
| `replace_text_in_range(Some(r), ..)` | Drops every selection but the active one. Then `replace_text_in_ranges` keeps only selections that exactly match an edit range, and puts a new cursor at the end of each edit. The person's cursor jumps to the agent's edit. | `state.rs:4427-4429`, `4195-4232` |
| The same | The edit is recorded in undo history, so `u` would revert the agent's write. `main` keeps it out of history (`ts:code/editor.ts:607`). | `state.rs:4125-4138`, `3144` |
| The same | Refused when readonly. | `state.rs:4352-4354`, `4055` |
| `set_value` | Not recorded and emits no `Change`. But it clears undo history and resets selection and scroll. | `state.rs:1099-1119` |
| `replace_all` | Recorded, and resets selection and scroll. | `state.rs:1131-1143` |

A workaround is possible on the public API, but it is lossy:

1. Read `selected_range()` and `scroll_offset()`.
2. Apply each remote change with `replace_text_in_range`.
3. Map the old selection through the changes.
4. Call `set_selected_range` and `set_scroll_offset`.

It loses extra cursors (not readable) and undo correctness (remote edits
enter history). It also writes what the typing path writes, not what the
agent wrote: auto-close, skip-over and completion triggers apply to a
single `(` or `)` (see the Edit row in section 1), which puts the buffer
out of step with the daemon. `insert`/`replace` avoid that but act only at
the cursor or selection.

Scroll is recoverable: `scroll_to` and `set_scroll_offset` write the same
`deferred_scroll_offset` field (`state.rs:2951,3328`), so calling
`set_scroll_offset` last wins. The real loss is that a pixel offset no
longer points at the same lines after an agent inserts or deletes lines
above the viewport. Keep the top visible line where it was, not the pixel
offset.

**Smallest proper fix: an upstream patch to `gpui-base`.** Add a public
`apply_external_edits(&mut self, edits: &[(Range<usize>, String)], window, cx)`:

- It runs the batch path (`replace_text_in_ranges`, `state.rs:4049`)
  inside `with_edits_allowed` (`state.rs:1149-1154`), with
  `undo_manager.set_ignoring(true)` (`undo_manager.rs:325`).
- It does not emit `InputEvent::Change`, the way `set_value` already
  suppresses it (`state.rs:1107-1112`).
- It maps every selection through the edits instead of collapsing them.
- It shifts the recorded undo entries past each edit. `Change::shifted`
  already exists for this (`base:src/input/base/change.rs:32-43`).
- It keeps the top visible line fixed, not the pixel offset.

`Change::shifted` handles only edits that lie entirely before a recorded
undo change. A remote edit that overlaps an undo entry, the recorded
selection snapshots and auto-closed pairs need a real transform, or the
undo history must be dropped. The size of the patch is **unestimated**
(an earlier "about 100 lines" was a guess); the spike should size it, with
dropping history as the simple fallback. Until it lands, carry it as a
`[patch.crates-io]` fork of `gpui-base` only.

### Reporting local edits as they happen

- `InputEvent` is `Change`, `PressEnter`, `Focus` and `Blur`. `Change`
  carries no range (`state.rs:128-134`), and it is emitted once per batch
  (`state.rs:4251-4253`).
- **Option A: diff a snapshot.** Keep a `Rope` snapshot. On `Change`,
  diff it against `text()` by common prefix and suffix, the same way as
  `main`'s `between` (`ts:code/sync.ts:62-80`).
  - Cost: O(the unchanged ends) per change. Usually small; worst case the
    file.
  - A multi-cursor edit comes out as one wide change, so conflict
    detection is coarser.
- **Option B: tap the highlighter (exact ranges).**
  - Every edit, including undo replay, reaches the installed
    `InputHighlighter` as an `InputEdit`. That holds the start, old-end and
    new-end bytes and points (`base:src/input/base/rope_ext.rs:19-28`).
    Edits go through `update` or `update_batch` (`base:src/input/editor/highlighting.rs:30-58`,
    driven from `state.rs:4125-4175` and `mode.rs:274-303`).
  - `set_highlighter_factory` is public, and the styled `Input` will not
    replace it (`state.rs:1001-1015`, `mode.rs:366-375`,
    `comp:src/input/input.rs:560`).
  - So an Argus factory can return a wrapper that records each edit with
    the inserted text (`text.byte_slice(start..new_end)`) and then
    delegates highlighting to `SyntaxHighlighter`. That type is public:
    `new` at `highlighter.rs:380`, `update` at 597, `styles` at 1181.
  - Cost: copying the default adapter's debounce
    (`comp:src/highlighter/input_adapter.rs:30-130`) is **not enough**. It
    relies on `SyntaxHighlighter::apply_background_tree`, `update_edits`,
    `injection_parse_data`, `compute_injection_layers` and
    `parse_input_bytes`, all `pub(crate)` (`comp:highlighter.rs:160,609,690,713,991`),
    and `input_highlighter_factory` is `pub(crate)` too. A wrapper outside
    `gpui-component` can use only the public synchronous `update` and
    `edit_tree`, with no background parse, unless `gpui-component` is forked
    as well. Anything copied is Apache-2.0 only (see Licences).
  - An edit that arrives with `edit: None` (a reset after `set_value`,
    `state.rs:1193-1206`) means "resync from the full text".
- **Recommendation:** start with A, because it is simple and correct. Move
  to an upstream `InputEvent::Edited { edits }` if multi-cursor conflicts
  matter; B is viable only with a `gpui-component` fork.
- Remote edits are kept out of the report either way: the patch above
  emits no `Change`, and a sequence number is held across the apply.

### Parity beyond Vim, grammars and sync

`main` also has pieces the GPUI editor must provide, not yet planned:

- the changed-lines gutter that `]c`/`[c` and Alt-]/Alt-[ step through
- Alt-c, comment on the selection
- the Mod-p and Mod-Shift-f stage keys
- the clash resolve flow (keep mine or theirs, `ts:code/editor.ts:612`),
  beyond only showing clashes

## 5. Large files and performance, as the source states them

- `EditorState::new` says it "handles large text up to about 50K lines"
  (`base:src/input/base/state.rs:11971-11973`).
- **Highlighting.**
  - A synchronous incremental parse is allowed 2 ms and then falls back to
    a background parse, debounced by 150 ms.
  - Above 256 KiB the synchronous parse is skipped entirely, and only the
    tree's edit is applied before the background parse
    (`comp:src/highlighter/input_adapter.rs:30-32,150-190`).
  - Injections are capped at 4096 ranges, 512 KiB, 512 separate parses and
    20 ms (`comp:src/highlighter/highlighter.rs:20-26`).
- The docs say decorations are queried for the visible range only and "do
  not scan every decoration per frame" (gpui-kit.com/component/editor.md).
- **`main`'s limits.**
  - The daemon highlights nothing over 512 KiB
    (`argusd:syntax/highlight.rs:18`).
  - The page parses before first paint up to 256 KiB
    (`ts:code/colours.ts:24-26`).
  - `gpui-kit`'s 256 KiB synchronous limit lines up with the second.
- **No benchmark is in the source.** The spike measures typing latency on
  a 50K-line file, and on a 5 MB one.

## Recommended plan

| # | Step | Risk |
|---|---|---|
| 1 | **Spike the tree-sitter version.** Move `argusd` to tree-sitter 0.26, turn on `gpui-kit/tree-sitter` plus `tree-sitter-rust` in `argus-gpui`, and check that `cargo tree -d` shows one `tree-sitter` and no duplicate grammar crate. Also weigh the alternatives in section 3 (separate workspace, a `gpui-component` patch, `syntect`). | Medium. Nothing yet shows `argusd`'s highlight and symbols code compiles on the 0.26 API. If 0.26 regresses the daemon, use one of the alternatives or wait for a `gpui-kit` on 0.27. |
| 2 | **Spike runtime wasm grammars.** Build with tree-sitter's `wasm` feature. Load `main`'s pinned `tree-sitter-rust@0.24.0` `.wasm` through `WasmStore` inside a `LanguageParserFactory` that caches the loaded `Language` and gives each call a fresh store, and paint a file. Check a grammar registered after an editor opens. Measure build time and binary size on Windows. | **High.** The npm `.wasm` may not load in native `WasmStore` (a different toolchain or ABI), and `wasmtime` is heavy. Fallback: compile the grammars in and drop runtime install for now. |
| 3 | **Editor surface.** Show the styled `Editor` in the Code stage with `set_value` on open. Build `HighlightTheme` from Argus tokens. Map captures (rewrite `namespace`, `module` and `escape`). Write with `ctrl-s` and the stage keys. | Low. |
| 4 | **Local edits out.** On `InputEvent::Change`, diff against a snapshot (option A), convert to UTF-16 `TextChange`, and reuse `main`'s sync rules ported to Rust (`ts:code/sync.ts`), as a pure module with property tests. | Low to medium. UTF-8/UTF-16 and CRLF conversion bugs, which the properties cover. |
| 5 | **Remote edits in.** Write `apply_external_edits` as a `gpui-base` patch (selections mapped, no history, no `Change`, scroll kept), carried as `[patch.crates-io]` and offered upstream. Light the changed ranges with a Fill range decoration. Show clashes as decorations. | Medium to high. The fork has to follow each `gpui-kit` bump (pinned `=0.8.0`, so only on purpose). The undo transform for overlapping entries is unsized; dropping history is the fallback. The risk of upstream acceptance is unknown: no contact with the maintainers is cited. Remote inserts must not go through the typing path. |
| 6 | **Vim core.** Build the Argus state machine (pure, tested) and drive it through `App::intercept_keystrokes`, filtered to the editor's focus handle. Normal mode stops every key; insert mode passes keys through except `escape`. Include the insert-entry commands (`i a I A o O`). Approximate the block caret with a Fill decoration (nothing shows on empty lines or at EOF). | Medium to high. Backward visual selections cannot be shown; `u` is asynchronous. Text suppression is verified on Windows only, and needs checking on macOS and Linux. IME in normal mode needs checking. Undo grouping is approximate until step 7. |
| 7 | **Upstream asks.** Add a public `transact` for undo grouping, a caret shape, an `InputEvent::Edited` with ranges (replacing option A) and a public read of all selections. | Unknown. No contact with or signal from `longbridge` is cited, so acceptance is a guess. Until they land, the workarounds above stand. Folding them into the step 5 fork is the fallback, and adds a backward-selection setter. |
| 8 | **Parity.** Port the Vim setting toggle, `:w`, `]c`/`[c`, the grammar offer and install prompt, and the npm search. Add the changed-lines gutter, Alt-c, the stage keys and the clash resolve flow. Add a screen scene for each. | Low to medium. |
