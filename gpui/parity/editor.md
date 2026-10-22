# Parity spec: the inline Code editor

The requirements a GPUI port of the Code stage's editor must meet, as `main` has it, written so a later agent can build and test it without reading the TypeScript. Group id `EDITOR`. Rule ids are `EDITOR-<area>-<n>`. Companion to `GPUI.md` §"The Code editor" (the GPUI editor keeps Vim mode on `gpui-kit`'s `Editor`, which has tree-sitter but no Vim layer, and builds the live sync for GPUI too), and to `checkouts-code-views.md` (the rest of the Code stage).

## How to read this

- **Paths.** `ts:X` is `crates/argus-desktop/page/src/X`; `rs:X` is `crates/argus-desktop-core/src/X`; `tauri:X` is `crates/argus-desktop/src/X`; `proto:X` is `crates/argus-protocol/src/X`; `d:X` is `crates/argusd/src/X`; `css:N` is `crates/argus-desktop/assets/style.css` line N; `scene:name` is a scene of `rs:screens/mod.rs`; `DESIGN.md §` names a heading. Page files are cited by their own line numbers.
- **What is Rust and what is the page.** The editor is the one thing in the window whose state Rust does not hold (`ts:code/editor.ts:1-9`, `DESIGN.md` §"Editing a file"): the buffer, the sync state, the grammar and the status are the page's, and a key is drawn in the handler that heard it. Rust says only that the editor is up (`CodePage.editing`, `rs:code/stage.rs:91-94`) and relays the editor's messages to the daemon and back **past the session** (`rs:code/edit.rs:1-6`). The port holds the buffer and the sync in a view entity; it should still keep the relay off the session's page snapshots, since a snapshot per keystroke is what the page design avoided.
- **The daemon is the referee.** The file on disk is the authority; the daemon holds a version number per open file and writes a client's change only onto the version it was made on (`proto:code/edit.rs:1-6`).
- **Two sources of ground truth for Vim.** The Vim layer is `@replit/codemirror-vim` 6.4.0 (`page/package.json:20`), whose engine is `@replit/codemirror-vim-core`. `node_modules` is not installed in this checkout, so the key tables below were read from the published npm tarballs (`@replit/codemirror-vim@6.4.0`, core `0.1.0`, `defaultKeymap`, `defaultExCommandMap`, `defineOption`) and, where the tarball is not in the checkout, from the vendored bundle `crates/argus-desktop/assets/js/vendor/codemirror.js`, which the page ships and which is the authority in-tree. The core's version is a caret range in the Vim package, so `page/package-lock.json` is the authority for which core `main` bundles; check it before trusting a row. Rows marked **(lib)** come from the library, not from Argus code.
- **Test tools the port has**: as in `shell.md`; text is asserted through the model, not pixels.

### Surfaces covered

| Id | Surface | Page module | Rust / daemon |
|---|---|---|---|
| OPEN | Opening a working file into the editor | `code/editor.ts`, `code/stage.ts` | `code/stage.rs`, `code/edit.rs` |
| SWITCH | Switching files without a blank frame | `code/editor.ts` `host` | `code/stage.rs` `editable` |
| VIM | Vim mode, motions, operators, registers, commands | `vendor/codemirror.ts`, `code/editor.ts` | none |
| KEYS | Who hears which key | `code/editor.ts`, `keys/keymap.ts` | `keys/keymap.rs` |
| GUTTER | Change marks, `]c`/`[c`, comment on selection | `code/editor.ts` | `code/stage.rs` |
| GRAM | Grammars: known, searched, installed, stored | `code/grammar/*` | none |
| COLOUR | Tree-sitter colours and the theme | `code/colours.ts`, `style.css` | none |
| SYNC | Live sync, both directions | `code/sync.ts`, `code/editor.ts` | `proto:code/edit.rs`, `d:code/edit/` |
| SAVE | Typing to writes, saving, refusals | `code/editor.ts` | `d:code/edit/mod.rs` |
| DISK | What the person sees when the file changes | `code/editor.ts`, `code/sync.ts` | `d:code/edit/mod.rs` |
| CLASH | A change that touches the same text | `code/sync.ts`, `code/editor.ts` | none |
| RECONN | Disconnect and reconnect | `code/editor.ts` | `tauri:connection/shared.rs` |
| SET | Settings that reach the editor | `settings/preferences.ts` | `settings/remembered.rs` |
| CSP | The policy the editor is styled under | `kit/dom.ts` | `tauri.conf.json` |
| STATE | Every state of the editor | `code/editor.ts` | |

---

## Surface OPEN: a working file opens into the editor

Source: `ts:code/stage.ts:635-650`, `ts:code/editor.ts:435-523`, `rs:code/stage.rs:767-784`, `rs:code/edit.rs`, `d:connection/dispatch.rs:512-540`.

### Behaviours

1. EDITOR-OPEN-1. A file is edited, not read, when the stage is on the **working tree** (not a commit or the index), has a path, the daemon listed `code-edit` (`CODE_EDIT`, `proto:connection/hello.rs:131`), and the file's read has not said it is not text. This is `CodePage.editing`. No key opens it: the older Enter and Done keys are gone. Source: `rs:code/stage.rs:767-784` (the refusal strings are "this daemon cannot edit files; restart it on this version", "only the working tree is edited; this version is history", "no file to edit", "this file is not text that can be edited here"). Tests: `only_a_working_tree_file_a_daemon_can_edit_is_edited`, `a_file_clicked_is_edited_from_the_first_draw_not_once_its_read_arrives` (`rs:code/stage.rs:1646-1668`).
2. EDITOR-OPEN-2. `editing` is true **from the first draw** after a file is chosen, not once its read arrives; it falls back to the read view only when the read comes back with a `note` (binary, too large). Source: `rs:code/stage.rs:777-783`; commit `c8cb3ead`.
3. EDITOR-OPEN-3. The first time any file is shown, the page loads the editor module (`import('./editor.js')`); until it has loaded the stage shows an empty `code-editor` box (blank, not a skeleton), then draws again. Source: `ts:code/stage.ts:31-44,635-650`.
4. EDITOR-OPEN-4. Showing a file for the first time creates a session for `(checkout, path)`, sets its bar to "Opening…", and sends `edit{gesture:{on:"open",checkout,path}}`. The CodeMirror view is created only when the daemon answers `opened`. Source: `ts:code/editor.ts:435-469,692-694`.
5. EDITOR-OPEN-5. The daemon's `OpenEdit` handling: it reads the file (a path that is not checkout-relative, leaves the checkout through a link, is not a file, is over 1 MiB, has a NUL in its first 8000 bytes, or is not UTF-8 is refused), converts each CRLF to LF for the text it sends, remembers whether the file used CRLF, and answers `EditOpened{checkout,path,version:0,text}`. A refusal is `EditClosed{reason}`. A link out of the checkout is read by the Code stage but never written through. Source: `d:code/edit/mod.rs:34-47,80-105`, `d:code/mod.rs:20-24`. Tests: `only_a_text_file_inside_the_checkout_is_opened`, `a_crlf_file_is_held_with_lf_and_written_back_with_crlf` (`d:code/edit/tests.rs:44,65`).
6. EDITOR-OPEN-6. A file already open for the same key (a second `OpenEdit`) replaces the first: the daemon holds one `Open` per `(checkout, path)` per connection, and dropping an `Open` aborts its watcher task. A `CloseEdit` drops it. Source: `d:connection/dispatch.rs:514-538`, `d:code/edit/mod.rs:198-202`.
7. EDITOR-OPEN-7. On `opened` with no sync yet, the view is mounted with the text, a `Synced` at version 0 over that text, the Vim layer per the setting (EDITOR-SET-1), line numbers, the change gutter, history, drawn selection, active-line highlights, 4-space `indentUnit`, bracket matching, auto-closing brackets, selection-match highlights, the colour compartment (already coloured if the grammar is loaded, EDITOR-GRAM-9), and the keymap of EDITOR-KEYS-3. Then it draws the change marks Rust last sent, goes to the line Rust asked for (EDITOR-OPEN-8) and, if no grammar is loaded, looks for one (EDITOR-GRAM-6). Source: `ts:code/editor.ts:471-523`. **Note.** `Synced` is created with version **0** regardless of the `version` in `opened` (`editor.ts:472`); the daemon always opens at 0 (`d:code/edit/mod.rs:95`), so the two agree today.
8. EDITOR-OPEN-8. The editor goes to the line Rust gave in `CodePage.at` (from one, clamped to the document) with the cursor at the line start and the line scrolled to the **vertical centre**; it does so on mount and again only when `c.at` differs from the last `at` it saw. The editor's own cursor moves are never reported to Rust. Source: `ts:code/editor.ts:539-547,644-649`. The header shows no `Ln N` while editing, since Rust's cursor would be stale (`ts:code/stage.ts:727`).
9. EDITOR-OPEN-9. After the view is shown the editor takes the keyboard focus, once (`s.focus`), or whenever focus has fallen to `document.body` (a box that closed let the keys go). It calls `view.requestMeasure()` first. Source: `ts:code/editor.ts:654-663`.
10. EDITOR-OPEN-10. The stage's frame around the editor is rebuilt on every Code page, but the editor's element is **kept in place** (moved, it would lose the keyboard focus and any input method's composition text) only while the section's `data-path` equals `c.path` (`ts:code/stage.ts:861-862`). On a file switch the whole stage is rebuilt with `replaceChildren` (`ts:code/stage.ts:876-887`), and the held-over or new editor element is moved into the new section. The header, comment slot, tree and outline are replaced around it. Source: `ts:code/stage.ts:845-889`. In GPUI this is a retained view, not a rule to copy.
11. EDITOR-OPEN-11. The header offers "Open in editor" (the person's external editor, `OpenEditor`) with its `E` hint hidden while editing, since `E` is a letter there. Source: `ts:code/stage.ts:757-760`.
12. EDITOR-OPEN-12. A commit's or the index's file is never put in the editor (read-only reading view, out of scope here).
13. EDITOR-OPEN-13. An `OpenEdit` for a checkout the daemon cannot resolve is answered `EditClosed{reason}` with that error (`d:connection/dispatch.rs:519-521`). A `CloseEdit` gets no answer, so a close is never confirmed to the page.
14. EDITOR-OPEN-14. **The `Synced` version comes from the daemon.** A session's `Synced` is made from the `opened` answer's version, not from a literal `0` as the page writes it (EDITOR-OPEN-7). Today the two are the same, because every `OpenEdit` answers 0 (EDITOR-RECONN-10); the rule matters on the day the daemon numbers otherwise, when a literal 0 would make every first `EditChanged` look like a mismatch and ask to open the file again (EDITOR-SYNC-12). A test opens a file with a non-zero answered version and then folds an `EditChanged` from that version, which must land without a resync.

---

## Surface SWITCH: switching files without a blank frame

Source: `ts:code/editor.ts:621-651,692-702`, `ts:code/stage.ts`, `page/test/screens/editor.mjs:192-237`; commit `c8cb3ead`.

### Behaviours

1. EDITOR-SWITCH-1. Sessions are kept per `(checkout, path)` in a map; leaving a file does not destroy its view at once. The shown one is `current`. Source: `ts:code/editor.ts:117-119`.
2. EDITOR-SWITCH-2. Choosing another file: Rust says so in the next page (`path` changes, `lines` empty, `fileLoading`), the page creates the new session and asks the daemon to open it, **and keeps the previous file's editor on screen** (returns its element, stores a `redraw` callback on the new session) until the new file's `opened` arrives. Only then is the stage drawn again with the new editor. At no frame is a loading skeleton or an empty editor shown. Source: `ts:code/editor.ts:633-639,697-701`. Verified by `editor.mjs`: records every frame of a switch and sets exit code 1 on a skeleton, a frame with no editor, or a first new-file frame that is uncoloured (`editor.mjs:229-237`).
3. EDITOR-SWITCH-3. This hold-over applies only when the new session has no view yet, its status is not `closed`, and a previous editor with a view exists. If the new file is refused (`closed` event), the stage is drawn again at once so the closed state shows. Source: `ts:code/editor.ts:636-638,730-736`.
4. EDITOR-SWITCH-4. When the previous editor is replaced it is **retired**: its typing is written first, and only once everything is written (nothing pending, no held clash) does it send `edit{on:"close"}`, destroy its view and drop the session. A retired session with an unresolved clash or a write in flight stays alive in the map until done. Source: `ts:code/editor.ts:583-593,673-678`.
5. EDITOR-SWITCH-5. Coming back to a retired-but-alive session (same key) un-retires it and shows its existing buffer, then draws its change marks and goes to Rust's line if it changed. A session whose connection was lost is re-opened here (EDITOR-RECONN-3). Source: `ts:code/editor.ts:624-650`.
6. EDITOR-SWITCH-6. While the stage is still Code and `!c.editing` (the file is no longer edited), the current session is retired. Source: `ts:code/stage.ts:860`. Leaving the Code stage does not retire it: see EDITOR-SWITCH-8.
7. EDITOR-SWITCH-7. A grammar already loaded for the file's kind colours the **first paint** (EDITOR-GRAM-9), so the first frame of the new file is coloured.
8. EDITOR-SWITCH-8. Leaving the Code stage for another stage does not retire the editor. `ts:code/stage.ts` returns at :840 (`state.rail.stage !== 'code'`) before it reaches the retire at :860, so the session stays `current`: it keeps writing, its daemon `Open` and watcher stay alive, and the re-read after a write still sends `code{gesture:{on:"written"}}` (`ts:code/editor.ts:535`) while another stage is on screen. Gap: the port should decide whether leaving Code retires the session.

---

## Surface VIM: Vim mode

Source: `ts:vendor/codemirror.ts:43` (`export { Vim, vim } from '@replit/codemirror-vim'`), `ts:code/editor.ts:269-277,479`. Everything not in the next paragraph is **(lib)** and must be reproduced, or knowingly dropped, by the GPUI editor.

### What Argus adds or changes

1. EDITOR-VIM-1. The Vim layer is the **first** extension, so Vim hears keys before any other keymap (`ts:code/editor.ts:478-479`). It is on by default and switched live (EDITOR-SET-1). The default mode on opening is **normal** (`editor.mjs:75-76`, "opens in vim's normal mode").
2. EDITOR-VIM-2. `:w` and `:write` write the buffer now (EDITOR-SAVE-6). It is defined as `Vim.defineEx('write','w', …)`; the callback finds the session from `cm.cm6`. Source: `ts:code/editor.ts:270-273`.
3. EDITOR-VIM-3. `]c` and `[c` (normal mode) move the cursor to the next / previous change mark (EDITOR-GUTTER-4). Defined as Vim actions `nextChange` and `previousChange` mapped with `Vim.mapCommand(']c','action','nextChange',{},{context:'normal'})`. Source: `ts:code/editor.ts:274-277`.
4. EDITOR-VIM-4. No other ex command, mapping or option is defined by Argus: no `:wq`, `:q`, `:x`, no leader keys, no `vimrc`. A command not in the library's table fails in the library's own command line (EDITOR-VIM-18); `:q`, `:wq` and `:x` are **not** library commands (see the table) and so do nothing useful.

### The library's modes (lib)

5. EDITOR-VIM-5. Modes: normal; insert; replace (`R`, entered as insert with `replace`); visual (`v`), visual-line (`V`), visual-block (`Ctrl-v` or `Ctrl-q`); `gv` reselects the last selection; `gi`/`gI` enter insert at the last insert position / column 0. The block cursor is drawn by the page (`.cm-fat-cursor`, an accent fill when focused, an outline when the editor is not focused; `css:6243-6250`). `Esc`, `Ctrl-[` and `Ctrl-c` leave insert and visual modes; `insertModeEscKeysTimeout` (200 ms) exists for user-mapped escape sequences, none of which Argus maps. Normal-mode `Ins` enters insert (`i`); insert-mode `Ins` toggles overwrite (vendored bundle: `{keys:"<Ins>",toKeys:"i",context:"normal"}` and `{keys:"<Ins>",action:"toggleOverwrite",context:"insert"}`).
6. EDITOR-VIM-6. Insert-mode keys the library adds: `Ctrl-w` delete word before, `Ctrl-u` delete to line start, `Ctrl-t`/`Ctrl-d` indent / outdent, `Ctrl-r <register>` insert register, `Ctrl-o` one normal command. `Ctrl-n`/`Ctrl-p` are mapped to `j`/`k`, and `Ctrl-Space`/`Shift-Space`, `Ctrl-BS`/`Shift-BS` are word moves, all with no mode context. The library skips a mapping with no context in insert mode (the bundle's `f=="insert"&&A.context!="insert"` continue), so those keys move a line or a word only in normal and visual mode; in insert mode they are not Vim's. Arrow keys, `Home`, `End`, `PageUp`, `PageDown`, `Del`, `Space`, `Backspace` and `Enter` (as `j^`) map as in Vim.

### Motions (lib, default keymap)

7. EDITOR-VIM-7. Character and line: `h j k l`, arrows, `Space`/`BS`, `gj gk` (display lines), `0`, `^`, `$`, `|` (column), `+ - _`, `Enter`. Words: `w W e E b B ge gE`. Blocks: `{ }` paragraphs, `( )` sentences, `%` matching symbol, `]<char>` and `[<char>` move to a symbol (`] [` family such as `]}`), `]'`/`['` and `` ]` ``/`` [` `` marks. Screen: `H M L`, `Ctrl-f`/`Ctrl-b` page, `Ctrl-d`/`Ctrl-u` half page scroll, `Ctrl-e`/`Ctrl-y` line scroll, `zz z. zt z<CR> zb z-`. File: `gg`, `G`, `<count>G`. Characters: `f F t T <char>`, `;` and `,` repeat. Search: `/ ?` open the library's command line, `n N`, `* # g* g#`, `gn gN` (select next match). Marks and jumps: `m<reg>` sets a mark, `` ` `` and `'` jump to it, `Ctrl-o`/`Ctrl-i` walk the jump list. In visual mode `o` and `O` move to the other end of the selection.
8. EDITOR-VIM-8. Counts apply to motions and most commands (`3w`, `5dd`, `10G`); `"<reg>` prefixes a register.

### Operators (lib)

9. EDITOR-VIM-9. `d` delete, `y` yank, `c` change, `>` `<` indent, `=` auto-indent, `g~` `gu` `gU` toggle / lower / upper case, `g?` rot13, `gq` and `gw` hard wrap (`textwidth` default 80), `gc` toggle comment. Doubled operators act on lines (`dd`, `yy`, `cc`, `>>`, `guu`, `g~~`). Shortcuts: `x X D Y C s S ~` and (visual) `u U`. Other normal commands: `r<char>`, `J` and `gJ` join, `p P` paste, `]p [p` paste matching indent, `o O` open a line, `a A i I`, `.` repeat the last edit, `u` undo, `Ctrl-r` redo, `Ctrl-a`/`Ctrl-x` increment / decrement the number at or after the cursor, `q<reg>` record a macro and `@<reg>` replay, `@@` repeat. While recording the library shows `recording @<reg>`.
10. EDITOR-VIM-10. `u` and `Ctrl-r` use the page's CodeMirror history; an agent's change from the disk is **not** in that history (EDITOR-DISK-4), so undo never undoes it.

### Text objects (lib)

11. EDITOR-VIM-11. With `a` (around) or `i` (inner) after an operator or in visual mode: `w W` words, `s` sentence, `p` paragraph, `( ) b` parentheses, `{ } B` braces, `[ ]` brackets, `< >` angle brackets, `" ' ` `` ` `` quotes, `t` tag. A pair object that finds no enclosing pair searches forward for the next opening character. Source: library `textObjectManipulation`.

### Registers (lib)

12. EDITOR-VIM-12. Registers: unnamed `"`, numbered `0` (last yank) and `1`-`9` (shifted deletes), named `a`-`z` (uppercase appends), and the library's further registers `-` (small delete), `.` (last inserted text), `:` (last command), `_` (black hole), `/` (last search), `+` (the system clipboard; the full list could not be confirmed from the minified bundle, only `+` was). The `+` register calls `navigator.clipboard.writeText` and `readText`. **Gap.** Whether the WebView grants clipboard access to `readText` under the window's CSP and permissions was not tested in this spec; the port must route `+` to the GPUI clipboard (`cx.read_from_clipboard`) and decide whether the unnamed register also syncs to it (the library does not do so on its own).

### Ex commands (lib `defaultExCommandMap`, plus Argus's `write`)

13. EDITOR-VIM-13. The library defines: `:w[rite]` (Argus's callback replaces the library's), `:u[ndo]`, `:red[o]`, `:s[ubstitute]` with flags, ranges, `%` and the usual `/` delimited pattern (PCRE style, option `pcre` true), `:g[lobal]` and `:v[global]`, `:d[elete]`, `:y[ank]`, `:pu[t]`, `:j[oin]`, `:norm[al]`, `:sor[t]`, `:noh[lsearch]`, `:start[insert]`, `:marks`, `:delm[arks]`, `:reg[isters]`, `:se[t]`, `:setl[ocal]`, `:setg[lobal]`, `:colo[rscheme]`, and the mapping commands `:map :nmap :imap :vmap :omap` with their `noremap` forms, `:unmap`, `:mapclear` and per-mode clear forms. Anything else (`:q`, `:wq`, `:x`, `:e`, `:w <file>`) is unknown to the library.
14. EDITOR-VIM-14. The options the library registers through `defineOption` are `filetype` (`ft`), `textwidth` (`tw`, default 80), `langmap` (`lmap`), `pcre` (default true) and `insertModeEscKeysTimeout` (default 200). Argus sets none. The `:set` family reads and writes these; the options a Vim user expects (`ignorecase`, `hlsearch`, `tabstop`, `number`) are **not** in the table, so `:set ignorecase` is not a supported behaviour.
15. EDITOR-VIM-15. The line comment and indent commands depend on CodeMirror's language data. The editor loads **no** CodeMirror language package (the tree-sitter colours are decorations only, `ts:code/colours.ts:1-5`), so `gc` and `=` have no comment token or indent rules to use. **Gap**, unverified here: how `gc` behaves with no language; the port should specify it rather than inherit it.
16. EDITOR-VIM-16. The ex line and the search line are the library's panel (`.cm-vim-panel` with an `input`), drawn by CodeMirror's panel mechanism, styled by `css:6252-6262`. Messages (`recording @q`, command errors) use the library's bottom panel (`{top:!1}`), not a window notice. No `-- INSERT --` message is shown: Argus calls `vim()` with no options (`ts:code/editor.ts:479`, `669`), the library's status panel appears only when `status` is set, and the bundle has no `INSERT` string. Argus draws no mode indicator of its own beyond the cursor shape.
17. EDITOR-VIM-17. Key repeat, macros, `.` and the jump list live inside the library; a GPUI Vim layer must keep per-editor state for them and share registers across editors (the library keeps `vimGlobalState`, so a yank in one file's editor is pasted in another's).
18. EDITOR-VIM-18. When the file is read-only (EDITOR-STATE-7) the library does not enter insert mode (it checks `cm.getOption('readOnly')`).

### Performance rule the page was built to

19. EDITOR-VIM-19. With Vim on, a key costs p50 1.7 ms and p95 2.6 ms from the key event to the buffer in headless Chrome (commit `32f09ffa`). The user-visible rule is that **no key waits on Rust, the daemon, a parse or a grammar load**: the text is drawn first; colours follow (EDITOR-COLOUR-3).
20. EDITOR-VIM-20. Vim commands and keys the library also takes that the rules above do not name: `:<N>`, a bare line number or line range, moves the cursor (a range with an empty command name); `g0 g^ g$ g<Up> g<Down>` are display-line motions; `<C-Esc>` is Escape; `<C-w>` in normal mode is mapped to `idle`, a no-op that swallows the key; `<Del>` is `x`; `R` in visual mode changes whole lines. Source: the vendored bundle `crates/argus-desktop/assets/js/vendor/codemirror.js` (the Vim `defaultKeymap` and `defaultExCommandMap`).
21. EDITOR-VIM-21. In normal and visual mode the library's context-free Vim keys take chords that CodeMirror and the browser would otherwise use, because Vim hears them first: `Ctrl-f`/`Ctrl-b` page, `Ctrl-d`/`Ctrl-u` half-page, `Ctrl-a`/`Ctrl-x` change a number (so no select-all), `Ctrl-y`/`Ctrl-e` scroll, `Ctrl-r` redo, `Ctrl-v` visual block (so no paste in visual mode), `Ctrl-o`/`Ctrl-i` jump list, and `Ctrl-c` as Escape (likely no copy in visual mode). Source: the vendored bundle, as EDITOR-VIM-20.
22. EDITOR-VIM-22. **Vim is untested on main, and the subset is open.** No Argus test runs a Vim binding: EDITOR-VIM-1 to 21 are read from the library and its tarball, and the only checks are `editor.mjs`'s normal-mode cursor and Escape (Gaps 2). So these rules are not a verified oracle for a port; the port's Vim engine (a windowless state machine in core, `gpui/audit.md` item 77) is tested by its own properties and example tables, and each row of VIM-5 to 21 it promises is one test. Which of the library's modes, motions, operators, text objects, registers and ex commands the port promises is open: `gpui/audit.md` §5 5.23. EDITOR-VIM-23. **The `+` register and the clipboard.** `"+y` writes and `"+p` reads the system clipboard through the one path every other copy takes (SHELL-SYSTEM-8, CROSS-30), so a failed write says `could not copy: <err>` and a paste is the host's read, never a view reaching for the platform on its own; the page's own route is `navigator.clipboard.writeText` and `readText`, whose grant under the webview was never tested (EDITOR-VIM-12). The unnamed register does **not** sync to the clipboard (the library does not do it), so a plain `yy` leaves the system clipboard alone and a `"+yy` is needed to copy out; the registers are shared by every open editor (EDITOR-VIM-17).
24. EDITOR-VIM-24. **`gc` and `=` with no language.** The editor loads no CodeMirror language package (EDITOR-VIM-15), so neither has a comment token or an indent rule from one. Main's behaviour was not checked and is **not** a rule to inherit. The port either specifies `gc` (a comment token from the file's grammar table, EDITOR-GRAM) and `=` (re-indent by the editor's 4-space unit), or leaves both out of the promised subset; that choice is the subset's, `gpui/audit.md` §5 5. A key the port leaves out does nothing and says nothing, like an unbound key in the library.

---

## Surface KEYS: how keys are split between Vim, the editor and the window

Source: `ts:code/editor.ts:477-516`, `ts:keys/keymap.ts:282-285,416-428`, `rs:keys/keymap.rs:563-565,854,883`.

### Behaviours

1. EDITOR-KEYS-1. The CodeMirror content is a `contenteditable`, so the window treats it as a **text field**: the `Field` layer is first in `LAYERS` and holds **every** key (`ts:keys/keymap.ts:282-285`, `rs:keys/keymap.rs:854`). The window's own bindings (Code stage `j k ]` `[` `E` `V` `C` `R` `O` `B`, `1`-`9` stage keys, leader chords) therefore do **not** fire while the editor has focus. In the editor `E` and `j` are letters.
2. EDITOR-KEYS-2. The one exception is the `Window` place: Zoom in, out and reset (`rs:keys/keymap.rs:563-565`) are heard in a capture-phase listener on `document` before the editor sees the event, and are `preventDefault`/`stopPropagation`'d. Source: `ts:keys/keymap.ts:416-424`. Escape is not on the Window place: it is the editor's (Vim's mode exit) while the editor has focus; the page has no Escape ladder entry for the editor.
3. EDITOR-KEYS-3. Order within the editor, first heard first: (1) Vim (EDITOR-VIM-1); (2) `closeBracketsKeymap`; (3) `defaultKeymap` (CodeMirror's standard cursor, selection, delete and line commands); (4) `searchKeymap` (CodeMirror's search panel: `Mod-f` find, `Mod-g`/`Shift-Mod-g` next/previous, `Mod-Alt-g` go to line, `Mod-d` select next occurrence and so on); (5) `historyKeymap` (`Mod-z`, `Mod-y`, `Mod-Shift-z`, and selection undo `Mod-u`); (6) `indentWithTab` (`Tab` indents the selection or the line, `Shift-Tab` outdents); (7) Argus's own keys below. Source: `ts:code/editor.ts:497-510`.
4. EDITOR-KEYS-4. Argus's own editor keys: `Mod-s` writes now (`preventDefault`) (EDITOR-SAVE-6); `Mod-p` sends `code{action:"filter"}` (go to a file) and `Mod-Shift-f` sends `code{action:"search"}` (the checkout's search; plain `Ctrl F` stays the editor's find); `Alt-c` makes the selection's lines the stage's pick and opens the comment box (EDITOR-GUTTER-6); `Alt-]` and `Alt-[` step between changes (EDITOR-GUTTER-4). `Mod` is Ctrl on Windows and Linux, Cmd on macOS. Source: `ts:code/editor.ts:503-509,525-527`. With Vim on, `Mod-s` and `Mod-Shift-f` reach these (Vim does not bind them), but the library maps `<C-p>` to `k` with no mode context and hears keys first, so **with Vim on, `Ctrl P` moves up a line instead of opening Go to file in normal and visual mode** (read from the vendored bundle `crates/argus-desktop/assets/js/vendor/codemirror.js`; not run in a browser here). In insert mode the library skips context-free mappings, so `Mod-p` opens Go to file there. Plain `Ctrl F` is Vim's page-down in normal and visual mode (EDITOR-VIM-7), not the editor's find. This is current behaviour; `DESIGN.md` §"Editing a file" says Ctrl P reaches the stage. Treat as a **Gap** for the port to decide; do not assume the docs are right.
5. EDITOR-KEYS-5. An input method's composition text and the text a pointer click places are CodeMirror's own; the port needs the IME and clipboard behaviour of a normal text editor (`GPUI.md` §The stack, the `Textarea` IME spike).
6. EDITOR-KEYS-6. The comment box that `Alt-c` opens is an in-flow block between the header and `.code-wrap` (`.ed-comment`, `ts:code/stage.ts:641-643`; the rule `.code-file .ed-comment` has padding and a bottom border, no positioning), not drawn over the editor, and is the stage's box, not the editor's. While it is open its field holds the keys; when it closes the keyboard returns to the editor (EDITOR-OPEN-9).
7. EDITOR-KEYS-7. With Vim on, the chords that EDITOR-VIM-21 lists are Vim's in normal and visual mode, so the `Mod-d` (select next occurrence), `Mod-f` (find) and `Mod-y`/`Mod-u` entries of EDITOR-KEYS-3 are not reachable there. In insert mode `Ctrl-w`, `Ctrl-u`, `Ctrl-t`, `Ctrl-d`, `Ctrl-r` and `Ctrl-o` are Vim's (EDITOR-VIM-6). Source: `ts:code/editor.ts:477-516`, the vendored bundle.
8. EDITOR-KEYS-8. **`Ctrl P` against Vim's normal mode is undecided.** With Vim on, `Ctrl-p` in normal and visual mode is Vim's `k` (the library maps it with no mode context and hears keys first), so the stage's Go to file does not open there, while in insert mode it does (EDITOR-KEYS-4); `DESIGN.md` says `Ctrl P` reaches the stage from inside the editor. Until a person decides, the port builds main's behaviour and pins it by a test in each of the three modes. Open: `gpui/audit.md` §5 6.

---

## Surface GUTTER: change marks, stepping, comment on the selection

Source: `ts:code/editor.ts:196-267,281-288`, `rs:code/stage.rs:1195-1270` (the page's `lines`).

### Behaviours

1. EDITOR-GUTTER-1. A narrow gutter (`cm-changes`, 3 px) beside the line numbers marks the lines the **working tree changed**, as Rust last read the file: a full-height green bar (`--ok`) on an added line, a 4 px red bar (`--danger`) at a line that had lines removed before it. The marks come from `CodePage.lines[i].added` / `removedBefore`, indexed by line, ignoring any past the document's end. Source: `ts:code/editor.ts:222-232`, `css:6226-6241`.
2. EDITOR-GUTTER-2. The marks move with what is typed (mapped through each change) until Rust reads the file again and sends new ones; a page whose marks are unchanged (same row count and kinds) is not redrawn. Source: `ts:code/editor.ts:213-220,235-244`.
3. EDITOR-GUTTER-3. Rust reads the file again after the editor's writes settle: 400 ms after the last `saved` or `changed`, if the file is still the one on screen, the page sends `code{gesture:{on:"written"}}`. Rust then re-reads the file and the tree (`ReadCode` and `ListCode`), keeps what is picked, the comment box and the cursor line (`land`), and so refreshes the marks, outline and the tree's counts. Source: `ts:code/editor.ts:66,531-537,719-723`, `rs:code/stage.rs:786-802`. Test: `a_working_tree_file_opens_in_the_editor_and_a_write_reads_it_again_where_the_cursor_is` (`rs:code/stage.rs:1628`).
4. EDITOR-GUTTER-4. Stepping: `]c`/`[c` in Vim normal mode and `Alt-]`/`Alt-[` anywhere in the editor move the cursor to the **first line of the next change after the cursor's line** (or the last one before it), scrolling it to the vertical centre. A run of consecutive changed lines is one change and is stepped to at its first line. With no change in that direction nothing happens. Source: `ts:code/editor.ts:248-267`.
5. EDITOR-GUTTER-5. The header shows `+N` and `−N` counts (Rust's), the language, the line count and, when the file is a search hit, `Match i of n`. The Rust `changes_here` and `ruler` still exist; whether the editor shows the ruler is the stage's concern (`checkouts-code-views.md`).
6. EDITOR-GUTTER-6. `Alt-c` takes the lines the selection covers (first and last line of the main selection, from one) and sends two calls in order: `code{gesture:{on:"click",target:{on:"range",from,to}}}` then `code{gesture:{on:"action",action:"comment"}}`. A one-line range is the cursor's, not a pick (Rust: `CodeClick::Range` with `from == to` clears the mark). Source: `ts:code/editor.ts:281-288`. Test: `the_editors_selection_picks_the_lines_a_comment_is_on` (`rs:code/stage.rs:1670`).
7. EDITOR-GUTTER-7. `Alt-c` also moves the cursor. `CodeClick::Range` puts Rust's cursor at `to` (`rs:code/stage.rs:728-732`), so the next page has a different `at` and the editor goes to that line (`host`'s `goTo`, `ts:code/editor.ts:644-649`). The editor's selection collapses to a cursor at the start of the last selected line, scrolled to the vertical centre. Gap: the port should decide whether a comment keeps the selection.
8. EDITOR-GUTTER-8. Stepping backwards (`[c`, `Alt-[`) from inside a run of changed lines goes to that run's first line, not to the line above the run: the backward test is `line < here` on run starts (`ts:code/editor.ts:259`).
9. EDITOR-GUTTER-9. A line that is both added and has lines removed before it draws only the added mark (the full-height green bar); the 4 px red bar is not drawn there (`ts:code/editor.ts:228-229`).

---

## Surface GRAM: grammars known, searched, installed and kept

Source: `ts:code/grammar/known.ts`, `fetch.ts`, `load.ts`, `store.ts`; `ts:code/editor.ts:334-427`; `DESIGN.md` §"Editing a file". No grammar ships with the window.

### The pinned list (`known.ts`)

1. EDITOR-GRAM-1. Twenty-three languages in 23 entries (TypeScript and TSX share a package). Each is an npm package's prebuilt parser plus highlight queries, pinned to a version and to the SHA-256 and size of each file, fetched from jsDelivr. The queries of a language that only adds to another's list both (TypeScript and TSX with JavaScript's, C++ with C's). Source: `ts:code/grammar/known.ts:28-499`. A grammar is found by whole file name first, then by lower-cased extension:

| name | label | extensions | whole names | package |
|---|---|---|---|---|
| rust | Rust | rs | | tree-sitter-rust@0.24.0 |
| python | Python | py pyi | | tree-sitter-python@0.25.0 |
| go | Go | go | | tree-sitter-go@0.25.0 |
| javascript | JavaScript | js mjs cjs jsx | | tree-sitter-javascript@0.25.0 |
| typescript | TypeScript | ts mts cts | | tree-sitter-typescript@0.23.2 |
| tsx | TSX | tsx | | tree-sitter-typescript@0.23.2 |
| c | C | c h | | tree-sitter-c@0.24.1 |
| cpp | C++ | cc cpp cxx hh hpp hxx | | tree-sitter-cpp@0.23.4 |
| c_sharp | C# | cs | | tree-sitter-c-sharp@0.23.5 |
| java | Java | java | | tree-sitter-java@0.23.5 |
| json | JSON | json | | tree-sitter-json@0.24.8 |
| css | CSS | css | | tree-sitter-css@0.25.0 |
| html | HTML | html htm | | tree-sitter-html@0.23.2 |
| bash | Bash | sh bash zsh | .bashrc .bash_profile .zshrc .profile | tree-sitter-bash@0.25.1 |
| ruby | Ruby | rb gemspec rake | Gemfile Rakefile | tree-sitter-ruby@0.23.1 |
| scala | Scala | scala sc | | tree-sitter-scala@0.24.0 |
| haskell | Haskell | hs | | tree-sitter-haskell@0.23.1 |
| julia | Julia | jl | | tree-sitter-julia@0.23.1 |
| toml | TOML | toml | Cargo.lock | @tree-sitter-grammars/tree-sitter-toml@0.7.0 |
| zig | Zig | zig | | @tree-sitter-grammars/tree-sitter-zig@1.1.2 |
| lua | Lua | lua | | @tree-sitter-grammars/tree-sitter-lua@0.4.1 |
| elixir | Elixir | ex exs | | tree-sitter-elixir@0.3.5 |
| php | PHP | php | | tree-sitter-php@0.24.2 |

   Files with no match, or an extension not listed (`md`, `yaml`, `kt`, ...), go to npm search (EDITOR-GRAM-3).
2. EDITOR-GRAM-2. A file's extension is the text after the last dot of its name, lower-cased, and a leading dot does not count (`.bashrc` has none). `knownFor` makes one pass over the list in list order and takes the first entry where either the whole file name matches or the extension matches. The whole-name match is case-sensitive; only the extension is lower-cased. Source: `ts:code/grammar/known.ts` `knownFor` (the last function, lines 492-498).

### Offering and searching

3. EDITOR-GRAM-3. For a file with no pinned grammar, npm is searched. The language name is the extension, mapped where npm names it differently (`kt kts`→kotlin, `yml`→yaml, `md`→markdown, `ml mli`→ocaml, `erl hrl`→erlang, `pl pm`→perl, `tf hcl`→hcl, `mk`→make, `cmake`, `fs fsx`→fsharp, `clj`→clojure, `hx`→haxe, `gql graphql`→graphql, `sol`→solidity, plus the identity names `r sql nix dart svelte vue proto dockerfile gleam odin v nim`), `Dockerfile`→dockerfile, `Makefile`→make; any other extension that matches `^[a-z][a-z0-9]{0,11}$` is its own name. Source: `ts:code/grammar/fetch.ts:25-69`.
4. EDITOR-GRAM-4. The search is `registry.npmjs.org/-/v1/search?text=tree-sitter-<language>&size=10`. Only a package named exactly `tree-sitter-<language>` or `@scope/tree-sitter-<language>` counts. For each, in npm's order, the file list is read from `data.jsdelivr.com/v1/packages/npm/<name>@<version>?structure=flat`; the first package with a top-level parser (`/tree-sitter-<language>.wasm`, or the only top-level `.wasm`) **and** `/queries/highlights.scm` wins. The offer is **not pinned** and carries the publisher's npm username when known. A failure or an offline machine offers nothing. Source: `ts:code/grammar/fetch.ts:92-130`.
5. EDITOR-GRAM-5. An offer is `{name,label,source:"package@version",wasm,queries,pinned,publisher?}`. Source: `ts:code/grammar/fetch.ts:11-22`.

### Finding it when a file opens

6. EDITOR-GRAM-6. When a view is mounted without a loaded grammar, the page looks for one: the installed grammar for the file (EDITOR-GRAM-8) colours **every open editor of the same kind** (the same extension as `extOf` gives it: the text after the last dot of the file name, case kept; a name with no dot falls back to the whole path, not the name, so two files with no extension in different folders are never the same kind, and `.bashrc` is extension `bashrc`; `ts:code/editor.ts:373-378`) and clears their offers; if loading it throws, the bar says `Could not load the grammar: <error>`. Otherwise `offerFor`: with no offer, or one the person said "Not now" to this window session, nothing is shown. With an offer: if it is pinned **and** Settings → "Install known grammars without asking" is on, it installs at once; otherwise the bar offers it. Source: `ts:code/editor.ts:362-406`.
7. EDITOR-GRAM-7. The bar's offer reads `No colours for <label> yet. Install its tree-sitter grammar? <source> from npm, <size>` for a pinned grammar, or `<source> from npm by <publisher>, <size>; found by searching, not checked by Argus` for a searched one, with `Install` (primary) and `Not now`. Size is the sum of the parser and query sizes, `N.N MB` from 1 MiB, else whole `KB` rounded up. `Not now` remembers the grammar's name until the window closes (not persisted), clears the offer and returns focus to the editor. Source: `ts:code/editor.ts:328-355,422-427`.
8. EDITOR-GRAM-8. Installing: the bar says `Installing the <label> grammar…`; every file is downloaded (`fetch` to `cdn.jsdelivr.net`); a pinned file whose SHA-256 differs, or any non-OK answer, fails with `<url> is not the file Argus checked` / `<url> answered <status>`; searched files have no hash. The query files are decoded as UTF-8 and joined by newlines. The grammar is loaded (the parser WebAssembly instantiated, the query compiled) **before** it is kept: one that will not load is never stored. On failure the bar says `Could not install <package@version>: <message>`. A searched grammar is stored under the **extension** (so the next file of the kind finds it); a pinned one under its name. On success every open editor of that kind is coloured. Source: `ts:code/grammar/fetch.ts:132-152`, `ts:code/grammar/load.ts:74-86`, `ts:code/editor.ts:408-420`.

### Where it lives

9. EDITOR-GRAM-9. A grammar loaded in this window is held in memory by name; the editor reads that map synchronously for a first paint (`loadedFor`). The tree-sitter runtime (`web-tree-sitter` 0.27.0) starts once; its own WebAssembly (`vendor/web-tree-sitter.wasm`) sits beside the bundle. Source: `ts:code/grammar/load.ts:16-72` (the runtime, the ready and done maps, `installedFor`, `loadedFor`), `ts:vendor/codemirror.ts:42`.
10. EDITOR-GRAM-10. Persistence: the installed grammar (name, label, source, parser bytes, query text) is kept in the webview's **IndexedDB**, database `argus-grammars` version 1, object store `grammars` keyed by `name`; it lives under the instance's config directory, so a dev instance has its own, and each instance downloads each grammar once. If storage will not open, grammars last only for the window. A grammar is stored only after it loaded. Source: `ts:code/grammar/store.ts`. **The port must choose its own store and a loader for tree-sitter parsers** (GPUI has no WebAssembly runtime for grammars; `GPUI.md` says `gpui-kit`'s `Editor` already has tree-sitter): the observable rules to keep are the permission rules above (ask first, hash-check known ones, say plainly that a searched one is unchecked and who published it, never run one that did not load), not the IndexedDB or WebAssembly mechanics.
11. EDITOR-GRAM-11. `node test/code/registry.mjs` re-downloads every pin, checks the hashes, and checks each loads and its queries compile in the runtime. Source: `ts:code/grammar/known.ts:6-7`.
12. EDITOR-GRAM-12. A grammar found by search for `Dockerfile` or `Makefile` is stored under `offer.name` (`dockerfile`, `make`), because `languageKey(path)` is null for those names (`ts:code/grammar/load.ts:81`). `installedFor` and `loadedFor` look it up by `knownFor(path)?.name ?? languageKey(path)`, which is null for these files (`ts:code/grammar/load.ts:56`, `64`). The install colours the editors open now, but the next file of that kind is never found and is offered again. Gap: the store key and the lookup key must be the same for a searched grammar.
13. EDITOR-GRAM-13. The label of a searched grammar is the lower-case language name (`ts:code/grammar/fetch.ts:117-118`), so the bar reads, for example, `No colours for kotlin yet`.

---

## Surface COLOUR: tree-sitter colours and the theme

Source: `ts:code/colours.ts`, `css:6985-7010`, `DESIGN.md` §"Editing a file".

### Behaviours

1. EDITOR-COLOUR-1. A capture is coloured by its **first word** (the part before the first dot), through a table of 18 words to 8 classes. A capture whose first word is not in the table, and a plain variable, are left uncoloured. The daemon colours the read view with a table of the same 18 words (`CAPTURES` in argusd's `syntax/highlight`), so the two agree on those words. They differ in nesting: the daemon uses tree-sitter-highlight, where the innermost capture wins (`d:syntax/highlight.rs:146-158`, "Only the innermost highlight applies"), while the page takes the first capture of a run (EDITOR-COLOUR-2), so nested captures are coloured differently today. Source: `ts:code/colours.ts:28-55`:

| capture's first word | class | colour token | meaning |
|---|---|---|---|
| keyword | `sx-k` | `--mauve` | keyword |
| string, escape | `sx-s` | `--green` | string |
| comment | `sx-c` | `--fg-faint`, italic | comment |
| number, boolean, constant, label | `sx-n` | `--peach` | number |
| type, constructor, module, namespace, tag | `sx-t` | `--yellow` | type |
| function | `sx-f` | `--blue` | function |
| property, attribute | `sx-p` | `--teal` | property |
| operator, punctuation | `sx-o` | `--fg-muted` | operator |

   Source of tokens: `css:6985-7010`. The theme's palette tokens (`--mauve` and so on) come from the active theme (`css:17-274`); the port must read them from its own theme the same way, so a theme change recolours open editors.
2. EDITOR-COLOUR-2. Where captures nest, the **first capture of a run wins**: a capture that starts before the end of the last coloured range, or is empty, is skipped. Source: `ts:code/colours.ts:196-201`.
3. EDITOR-COLOUR-3. Colours are parsed and drawn **after** a key, never in it. The colours already drawn move with the text at once (decorations are mapped through the change); the tree is told each change (`Tree.edit`, with start, old end and new end index and row/column points, in the order a tree takes them) and parsed again from the old tree in a task of its own queued straight after the key (a `MessageChannel` message, ahead of any timer), which lands, as a rule, before the frame that paints it (the code comment says "as a rule"; nothing guarantees it, `ts:code/colours.ts:57-58`). Source: `ts:code/colours.ts:57-71,87-105,139-168`. Rule: a key costs only its text.
4. EDITOR-COLOUR-4. Only the **lines drawn** are coloured: the query runs over the viewport's rows (whole lines, `startPosition` column 0 to the next row), not the whole file. A viewport change with a current tree redraws the colours; with a stale tree it waits for the parse. Source: `ts:code/colours.ts:146-148,181-204`.
5. EDITOR-COLOUR-5. The first paint: a file up to 256 KB (`PARSE_FIRST`) is parsed before it is first drawn, so it opens coloured, not plain for a frame; a larger one is parsed after. Source: `ts:code/colours.ts:24-26,133-137`.
6. EDITOR-COLOUR-6. Positions in the tree are UTF-16 units and queries are bounded by rows because the runtime reads a query's indexes as bytes. A port on a native tree-sitter works in bytes; the rule is that the **colours land on the same characters** as the page's, including past non-ASCII text. A seeded test checks an incrementally parsed tree equals a fresh parse, node positions included. Source: `ts:code/colours.ts:190-195`, commit `a134c5c1`, test `page/test/code/colours.test.mjs`.
7. EDITOR-COLOUR-7. The grammar only colours: it does not indent. `Enter` copies the previous line's indentation (CodeMirror's `insertNewlineAndIndent` with no language data). Source: `DESIGN.md` §"Editing a file".
8. EDITOR-COLOUR-8. The editor's own look, in theme tokens: text `--fg` on `--bg-sunken`; mono font and `--fs-md` at 20 px line height; gutters `--fg-faint`; line numbers at least 44 px wide; active line and active gutter line `--fill`; cursor a 2 px `--accent` bar; selection 28 % `--accent`; other matches of the selection 14 %; a matching bracket 22 % fill with a 50 % outline; Vim's block cursor 70 % `--accent` filled with `--bg-sunken` text (an outline only when unfocused). Source: `css:6139-6300`.

---

## Surface SYNC: live sync, both directions

Source: `proto:code/edit.rs`, `proto:connection/message.rs:205-232,735-768`, `d:code/edit/mod.rs`, `d:code/edit/text.rs`, `ts:code/sync.ts`, `ts:code/editor.ts:680-741`, `rs:code/edit.rs`.

### The protocol

1. EDITOR-SYNC-1. Messages. Client to daemon: `OpenEdit{checkout,path}`, `Edit{checkout,path,base,changes}`, `CloseEdit{checkout,path}`, only to a daemon that listed `CODE_EDIT`. Daemon to client: `EditOpened{checkout,path,version,text}`, `EditChanged{checkout,path,from,version,changes}` (the file changed by something other than this client), `EditSaved{checkout,path,base,version}`, `EditRefused{checkout,path,base,reason: Option<String>}`, `EditClosed{checkout,path,reason}`. Source: `proto:connection/message.rs:214-232,740-768`, `proto:code/edit.rs`. In the page's own types: `EditGesture` (`open`, `change`, `close`, camelCase tag `on`) and `EditEvent` (`opened`, `changed`, `saved`, `refused`, `closed`, `lost`). Source: `rs:code/edit.rs:11-47`.
2. EDITOR-SYNC-2. A change is `{from,to,insert}` with positions counting **UTF-16 code units** in the text with each CRLF read as **one** LF; a set is positioned in the text *before* any of it applies, sorted, not overlapping. The daemon converts to and from the file's own line endings. A set that is out of order, overlapping, past the end, or splits a surrogate pair is refused whole with a reason. Source: `proto:code/edit.rs:12-23`, `d:code/edit/text.rs:35-55`. Tests: `a_change_that_does_not_fit_the_text_is_refused_with_why_and_the_file_left`.
3. EDITOR-SYNC-3. The relay: a page `Call::Edit{gesture}` becomes the matching `ClientMsg` and is sent **without** the session; the daemon's edit messages (`is_edit`) are turned into `Event::Edit(EditEvent)` and sent to the page **without** the session; nothing is kept in Rust. Source: `tauri:connection/commands.rs:45`, `rs:connection/daemon.rs:213-217`, `rs:code/edit.rs:49-77`, `rs:connection/page.rs:129,465`. `crates/argus-gpui/src/connection/owner.rs:97` already sends `Call::Edit` the same way. Tests: `the_editors_answers_are_taken_and_every_other_message_left`, `a_change_from_the_page_reaches_the_daemon_as_it_was_made` (`rs:code/edit.rs:84,92`).
4. EDITOR-SYNC-4. The daemon's rule: **a client's `Edit` is written only when its `base` is the version the daemon holds and the disk still holds that text.** Before writing it re-reads the file; if something else wrote first, that write is sent as an `EditChanged` first and then the edit is refused with **no reason**, so the client can make its change again on top. A write by anything else is therefore never overwritten by something the client did not see. Source: `d:code/edit/mod.rs:142-187`. Test: `a_write_by_anything_else_is_told_before_a_change_on_the_old_version_is_refused` (`d:code/edit/tests.rs:25`).
5. EDITOR-SYNC-5. A successful write bumps the version by one (unless the text is unchanged, when it only answers `EditSaved` with the current version), writes with the file's own line ending, and answers `EditSaved{base,version}`. A write whose bytes would exceed 1 MiB, or that fails to write, is refused with a reason (`<path> would be too large to display`, `could not write <path>: <error>`), and the file is left. Source: `d:code/edit/mod.rs:160-186`.
6. EDITOR-SYNC-6. Watching: on open the daemon watches the file; each wake waits until **two reads 8 ms apart agree** (one read, then up to `SETTLE_READS`=5 more, so up to 6 reads, `d:code/edit/mod.rs:52-62`), so a writer that truncates first is never reported half done, then re-reads under the lock. A read that differs becomes an `EditChanged` at `from=version`, `version+1`, whose changes are a **line diff** (Myers, 50 ms timeout) with each run of changed lines narrowed to the characters that differ, so a cursor on an untouched part of a changed line stays. Source: `d:code/edit/mod.rs:17-62,118-140,207-267`, `d:code/edit/text.rs:11-95`.
7. EDITOR-SYNC-7. A deleted file, or a read that fails for a passing reason (held by a writer, between delete and replace), is **not** an event: the buffer is still the person's and their next change writes the file back. A read that fails for a lasting reason (too large, binary, not UTF-8) ends the edit with `EditClosed{reason}` and the daemon drops the file. Source: `d:code/edit/mod.rs:25-32,118-124,246-257`. Test: `a_deleted_file_is_written_again_by_the_next_change` (`d:code/edit/tests.rs:75`).
8. EDITOR-SYNC-8. A file with no open entry that is sent an `Edit` is refused with reason `<path> is not open`. Source: `d:connection/dispatch.rs:526-534`, `d:code/edit/mod.rs:271-279`.

### The page's model (`Synced`)

9. EDITOR-SYNC-9. `Synced` holds: `base` (the disk's text at the daemon's `version`, or while a clash is held, the text the held changes are made on); `sent` (the write in flight, `base`→what was sent, or none); `local` (typed since); `held` (the disk's changes that clash with the person's, made on `base`, or none). The buffer is always `base + sent + local` (`mine()`). Source: `ts:code/sync.ts:89-118`.
10. EDITOR-SYNC-10. `take()` returns the wire form of `local` and moves it to `sent`, but only when nothing is in flight, no clash is held, and something was typed; otherwise it returns nothing. So there is **one write in flight at a time** and nothing is written while a clash waits. Source: `ts:code/sync.ts:137-142`.
11. EDITOR-SYNC-11. `saved(base,version)`: if a write is in flight and `base` is the version held, `base` becomes `sent` applied, the version is updated and the write clears; otherwise the answer is ignored (it was for a write already folded back). `refused(base,reason)`: with a reason and a write in flight on this version, the write goes back into `local` to be written with the next; with no reason it is ignored (the `EditChanged` before it has already folded the write back). Source: `ts:code/sync.ts:145-159`.
12. EDITOR-SYNC-12. An `EditChanged{from,...}` whose `from` is not the version held cannot be applied: the page asks the daemon to **open the file again** (resync) and shows nothing. One whose `from` matches is folded in (EDITOR-DISK-1). Source: `ts:code/sync.ts:170-176`, `ts:code/editor.ts:706-718`.
13. EDITOR-SYNC-13. `Synced` splits a document into lines on `\r\n?|\n` so CodeMirror and `Synced` count a line break as one position; wire changes are built with `iterChanges` and re-read with `ChangeSet.of(changes, length)`. Source: `ts:code/sync.ts:15-25,102-108`. A change set crosses the wire and back unchanged (`sync.test.mjs`: `a change set crosses the wire and back unchanged`).
14. EDITOR-SYNC-14. The invariant, tested by random interleavings of typing, writes and agent writes: **whatever order they land in, the buffer ends as the disk**. Source: `page/test/code/sync.test.mjs:138` (`whatever order typing, writes and agent writes land in, the buffer ends as the disk`). The port's sync must keep this property, and this is the property to carry into a `proptest`.
15. EDITOR-SYNC-15. While the reopen asked for by a mismatched `EditChanged` (EDITOR-SYNC-12) is pending, typing still writes. The daemon has already replaced the `Open` with an empty slot, so an `Edit` that gets there before `Held::open` has filled it is refused with reason `<path> is not open` (`d:code/edit/mod.rs:275-278`), and the page shows `Not saved: …` and retries. If the daemon has dropped the file for good, a refusal with a reason retries every 1 s (`ts:code/editor.ts:724-729`) with no limit.
16. EDITOR-SYNC-16. The daemon's write path also closes a file: when the reread inside a write finds a lasting failure (the case of EDITOR-SYNC-7), it sends `EditClosed` and drops the slot (`d:code/edit/mod.rs:148-154`, `281-287`). EDITOR-SYNC-7 describes only the watcher's path.
17. EDITOR-SYNC-17. The `Open` that a second `OpenEdit` replaced (EDITOR-OPEN-6) does not stop writes already running in `spawn_blocking`: they still finish and send `EditSaved` for the old numbering (`d:code/edit/mod.rs:271-289`). The page checks `base` and version (EDITOR-SYNC-11), so such an answer is ignored when its base is not the one held.

---

## Surface SAVE: typing to writes, saving, refusals

Source: `ts:code/editor.ts:56-68,551-593,719-729`.

### Behaviours

1. EDITOR-SAVE-1. There is no save step in normal use: typing is written for you. What is typed is written once typing **pauses for 200 ms** (`QUIET_MS`), or at the latest **1000 ms** after typing began while it never pauses (`MOST_MS`). One burst of typing is one write, not one per letter. Source: `ts:code/editor.ts:56-61,551-561`.
2. EDITOR-SAVE-2. The bar's status while typing: from "Saved" it becomes "Unsaved" on the first change; a write in flight shows "Saving…"; the `saved` answer returns it to "Saved" (when nothing further is pending). The status is recomputed on every write from `settled()`: no sync yet → "Opening…", a clash held → clash, anything pending → "Saving…", else "Saved". The bar redraws only when what it says changes. Source: `ts:code/editor.ts:301-307,429-433,551-580`.
3. EDITOR-SAVE-3. After a `saved` answer the page writes again at once if more was typed meanwhile (the answer "writes again"), and 400 ms after writes stop it asks Rust to read the file again (EDITOR-GUTTER-3). Source: `ts:code/editor.ts:719-723`.
4. EDITOR-SAVE-4. A refusal **with a reason** sets the bar to "Not saved: <reason>" and retries the write after 1000 ms (`RETRY_MS`); the failed change goes back into what is typed, so the retry writes it together with anything new. A refusal **with no reason** shows nothing and does not retry on its own (the change was folded back by the `EditChanged` before it, which also wrote again). Source: `ts:code/editor.ts:724-729`, `ts:code/sync.ts:155-159`.
5. EDITOR-SAVE-5. Writing while a clash is held, or while a write is in flight, sends nothing; the answer to the write in flight (or the person's resolution) writes again. Source: `ts:code/editor.ts:563-566`, `ts:code/sync.ts:137-142`.
6. EDITOR-SAVE-6. **Explicit save.** `Mod-s` and Vim `:w` run the same `write` at once, cancelling the pause timer. They are the person's way to say "now", not a different path: the bytes are the same changes. Source: `ts:code/editor.ts:270-273,503`.
7. EDITOR-SAVE-7. A retiring session (the file is no longer the one shown) writes its typing first and closes only when nothing is pending (EDITOR-SWITCH-4). Source: `ts:code/editor.ts:583-593`.
8. EDITOR-SAVE-8. The daemon writes the **whole file** from the applied text with `std::fs::write`; there is no atomic rename, backup, formatting, trailing-newline fix or encoding change, and line endings are not preserved exactly (EDITOR-SAVE-9). The file's CRLF-ness is detected on every read and used on write. Source: `d:code/edit/mod.rs:118-127,167-177`.
9. EDITOR-SAVE-9. A file with any CRLF in it is written back with every LF as CRLF: `lf()` sets CRLF when the file contains `\r\n` at all, and on write every `\n` is replaced (`d:code/edit/text.rs:17-33`). A file with mixed line endings is normalised to all-CRLF on the first write. This changes bytes the person did not type; EDITOR-SAVE-8's "no encoding change" holds for the text, not for the line endings.
10. EDITOR-SAVE-10. The `MOST_MS` (1000 ms) limit is checked only when a key arrives (`ts:code/editor.ts:555`); no timer enforces it. A forced write therefore happens at the first key after 1000 ms of typing, not at 1000 ms.

---

## Surface DISK: what the person sees when the file changes under them

Source: `ts:code/editor.ts:140-158,596-610,706-718`, `ts:code/sync.ts:170-216`.

### Behaviours

1. EDITOR-DISK-1. An `EditChanged` that the person's typing does not touch lands in the buffer **at once**: the part clear of everything the person has typed or not yet written is applied, and the person's own text is moved over it (their cursor and selection stay on the text they were on). The change is shown without being typed: it carries the `fromDisk` annotation, so it does not mark the file unsaved and **is not added to undo history**. Source: `ts:code/editor.ts:596-610`, `ts:code/sync.ts:190-200`.
2. EDITOR-DISK-2. An agent's change is **lit**: every changed range gets the `cm-agent-edit` mark, which fades from a 35 % `--blue` fill to transparent over 1.6 s (`LIT_MS`); then the marks are cleared. A change that deletes only (an empty range) lights nothing. Source: `ts:code/editor.ts:62-63,140-158,606-609`, `css:6272-6282`.
3. EDITOR-DISK-3. The change is not typed by the person, so no write results from it, but the page does call `write` after folding (the person's own pending typing, if any, is rewritten on top of the new version), and after 400 ms asks Rust to read the file again for the marks. Source: `ts:code/editor.ts:714-716`.
4. EDITOR-DISK-4. Undo and redo never undo or redo the agent's change.
5. EDITOR-DISK-5. A file that is **deleted** shows nothing (EDITOR-SYNC-7); a file that becomes unreadable for good, too large, binary or not UTF-8 ends the edit (EDITOR-STATE-6).
6. EDITOR-DISK-6. The person's own writes read back from the disk are not a change (the daemon compares with what it holds), so they are never echoed or lit. Test: `a_crlf_file_is_held_with_lf_and_written_back_with_crlf` ("its own write read back is no change").
7. EDITOR-DISK-7. If the file watch cannot be set up (`file_watch::watch` returns None, `d:system/file_watch.rs:28-29`), the wake sender is dropped and the watcher loop ends. Agent writes are then not pushed live: they show up only as the `EditChanged` sent before a refusal on the next write (EDITOR-SYNC-4).
8. EDITOR-DISK-8. Watcher wakes are merged: they go through a channel of capacity 1 with `try_send` (`d:code/edit/mod.rs:231-235`), so a burst of writes produces at most one pending reread. The watcher also fires on the client's own writes, which reread as no change (EDITOR-DISK-6).
9. EDITOR-DISK-9. The unlight effect clears all lit marks at once (`ts:code/editor.ts:153`, `609`), so a second agent change that lands before the first one's 1.6 s timer fires is un-lit early, together with the first.

---

## Surface CLASH: a change that touches the same text

Source: `ts:code/sync.ts:34-63,188-239`, `ts:code/editor.ts:160-194,309-326,612-617`.

### Behaviours

1. EDITOR-CLASH-1. A disk change and a person's change **clash** when their runs touch the same characters: overlapping runs, or two insertions at one place (which goes first is a choice), or an insertion inside the other's replaced range. Runs that only meet end to end do not: one line typed, the next rewritten. Source: `ts:code/sync.ts:34-43`.
2. EDITOR-CLASH-2. The clear part of an agent's change is applied at once (EDITOR-DISK-1). The part that clashes is **held aside**: nothing is guessed, the buffer keeps what the person typed, and **nothing is written** until the person says which stands. The held changes are made on `base`, so a second agent change composes onto the held ones. Source: `ts:code/sync.ts:170-200`.
3. EDITOR-CLASH-3. What is drawn while a clash is held: the person's clashing range gets the `cm-clash` mark (16 % `--danger` fill, a wavy underline in `--danger` at 70 %, `css:6284-6287`), and **after** each clash an inline widget `cm-theirs` (tooltip "What the agent wrote here") shows the text the agent put there, italic on an 18 % `--blue` fill, or the word `removed` when the agent deleted. The marks move with typing. Source: `ts:code/editor.ts:160-194,176-194`, `ts:code/sync.ts:204-216`, `css:6284-6297`.
4. EDITOR-CLASH-4. The bar says **"An agent changed what you were editing"** in `--danger`, with two buttons on the right: **Keep mine** and **Take the agent's** (primary). While a clash is held the bar shows no grammar offer. Source: `ts:code/editor.ts:295,309-326`.
5. EDITOR-CLASH-5. **Keep mine**: the buffer stands as it is; the disk's whole change is taken as the new base and the person's text is **written over** it. No change is made to the buffer. **Take the agent's**: the person's changes that clash are given up and the disk's whole change is taken; the person's other changes are kept and moved over it. The buffer changes (without being typed or undoable, and lit). Both then write at once and return the keyboard focus to the editor. Source: `ts:code/sync.ts:222-239`, `ts:code/editor.ts:612-617`.
6. EDITOR-CLASH-6. A clash does not stop a **retiring** session: it stays alive until resolved (EDITOR-SWITCH-4), and a clash is a reason a write sends nothing.
7. EDITOR-CLASH-7. Source of truth for the clash rules is `page/test/code/sync.test.mjs` (`a clash is held until the person keeps theirs or the agents`) and the random-order test (EDITOR-SYNC-14). Port these tests, not the DOM.

---

## Surface RECONN: disconnect and reconnect

Source: `tauri:connection/shared.rs:100-103`, `ts:code/editor.ts:681-689,626-630,692-699`, `ts:code/sync.ts:161-186`.

### Behaviours

1. EDITOR-RECONN-1. When the connection to the daemon goes, or the window switches host (EDITOR-RECONN-8), Rust sends `Event::Edit(Lost)` to the page. Every session then: loses the write in flight (its changes go back into `local`, so nothing typed is lost), marks itself to be **opened again**, and shows **"Disconnected; kept here until the daemon is back"** in the attention colour. The buffer stays on screen and **stays editable**; typing goes on being tracked. Source: `ts:code/editor.ts:681-688`, `ts:code/sync.ts:162-165`, `css:6158-6160`. A session that is `closed` stays closed (EDITOR-RECONN-7).
2. EDITOR-RECONN-2. While disconnected a write is attempted as usual but cannot reach the daemon (the session's outgoing channel is cleared: `Shared::disconnected` sets it to none, `relay` then sends nothing). **Gap**: the page does not know a write failed; `sent` stays set until `Lost` clears it, and `Lost` is delivered once per disconnect and again on every host switch (EDITOR-RECONN-8). Typing made after `Lost` and before reconnect is taken by `take()` into `sent` and sent into the void; it is folded back by `reopened` (`lost()` is called first). The port should make this explicit: writes while disconnected stay unsent.
3. EDITOR-RECONN-3. The reopen happens on the **next time the stage draws that editor** (`host` with `s.reopen`), not on the connection's own event: the status goes back to "Opening…" and `open` is sent again. In practice a reconnect makes Rust send the page's snapshots again, which draws the stage. **Gap**: a reconnect that does not redraw the stage would leave the editor "offline" until something else does; the port should reopen on the connection event.
4. EDITOR-RECONN-4. `opened` for a file that already has a buffer is folded in as **what the file became while disconnected**: the page diffs the disk text it held (including a held clash's result) against the new text as **one** change (the shortest differing middle: common prefix and suffix kept, never splitting a surrogate pair), then folds it as an agent's change would be: clear parts land at once and are lit, clashing parts are held (EDITOR-CLASH). The version is the one the daemon answers, which is always 0 (EDITOR-RECONN-10). The person's own unwritten typing is then written on top. Source: `ts:code/sync.ts:65-83,178-186`, `ts:code/editor.ts:692-702`.
5. EDITOR-RECONN-5. The daemon's side of a lost connection: its open files belong to that connection and are dropped with it, with their watchers (`conn.edits`, `d:connection/mod.rs:110`).
6. EDITOR-RECONN-6. A daemon that does not list `code-edit` makes `editing` false (EDITOR-OPEN-1). A daemon replaced by an older one while the window is open therefore returns the stage to the read view on the next page.
7. EDITOR-RECONN-7. `Lost` sets every session to `offline` and `reopen`, including sessions that are `closed` (`ts:code/editor.ts:681-686`). A closed file therefore loses its closed status, `write()` stops refusing it (the guard at :571 checks `status === 'closed'`), and it is opened again on the next draw. The port must keep a closed session closed across `Lost`.
8. EDITOR-RECONN-8. `Lost` is also sent when the window switches host: `run()` aborts the old loop and calls `disconnected()` (`rs:connection/daemon.rs:118-124`), and the GPUI owner sends it too (`crates/argus-gpui/src/connection/owner.rs:201`). So `Lost` arrives once per disconnect and again on every host switch (`rs:connection/daemon.rs:124`, `220`).
9. EDITOR-RECONN-9. A retiring session (EDITOR-SWITCH-4) that loses its connection with typing still unwritten never closes. `lost()` moves `sent` back into `local`, so `pending()` stays true and `done()` keeps the session. The reopen happens only through `host()`, which runs only if the person comes back to that file, so the typing is never written and the session stays in the map. Gap: a retiring session needs a rule for reconnect.
10. EDITOR-RECONN-10. Every `OpenEdit` answers version 0 (`d:code/edit/mod.rs:95`), so a resync or a reconnect to the same daemon restarts the numbering at 0, whether or not the daemon restarted.
11. EDITOR-RECONN-11. **Reopen answers the connection, not a redraw.** The port reopens every session that `Lost` marked (RECONN-1) when the connection event says the daemon is back (the greeting of the new connection), by sending `open` again and showing `Opening…`; it does not wait for the stage to draw that editor (RECONN-3), so a reconnect with nothing else to redraw does not leave a file "offline" for ever. A session that is `closed` stays closed (RECONN-7); what a retiring session with unwritten typing does is RECONN-9's gap and is not decided here. Until reopened, writes stay **unsent** with the typing kept in `local` (RECONN-2), and the bar is computed from the connection and the last refusal (STATE-10).

---

## Surface SET: settings that reach the editor

Source: `rs:settings/remembered.rs:113-121,250-251`, `rs:settings/preferences.rs:67-68`, `rs:connection/page.rs:325-326`, `ts:settings/preferences.ts:393-398,459-464`, `ts:code/editor.ts:123-129,665-670`, `ts:main.ts:195`.

### Behaviours

1. EDITOR-SET-1. **Vim keys** (`vim`): on by default, including for a settings file from before the setting existed (test `a_file_from_before_vim_keys_has_them_on`, `rs:settings/remembered.rs:443`). Settings → Editor → "Vim keys", sub-label "In the Code stage: normal, insert and visual modes, and :w". Toggling it sends `set{set:"vim",on}`; when the settings arrive the page reconfigures the Vim compartment of **every open editor** (`vim()` or nothing) live, without reloading. Source: `ts:settings/preferences.ts:393-395,459`, `ts:code/editor.ts:666-670`. With Vim off the editor is a plain CodeMirror editor with the keymap of EDITOR-KEYS-3.
2. EDITOR-SET-2. **Install grammars without asking** (the Rust field is `install_grammars`; the page sends `set{set:'installGrammars'}`, `ts:settings/preferences.ts:396-398`): off by default. On, a **pinned** grammar installs when its file opens; a searched one is always asked about. Sub-label: "Colours for a language Argus knows download from npm when its file opens; one found by searching npm is still asked about". Source: `rs:settings/remembered.rs:117-121`, `ts:settings/preferences.ts:396-398,460-464`.
3. EDITOR-SET-3. The Settings → Editor group also holds "Files open" (the external editor mode) and "Editor command", which govern "Open in editor" and the daemon's editor panes (EDITOR-OPEN-11), not the inline editor. They are specced in `shell.md` (SETTINGS).
4. EDITOR-SET-4. The editor has **no** settings of its own for tab width, font, wrap, line numbers, theme or keymap. The font is the pane code font (`--font-mono`), the 4-space indent unit is fixed, and Tab inserts per `indentWithTab`. These are fixed behaviours, not gaps to fill silently.

---

## Surface CSP: the policy the editor is styled under

Source: `tauri.conf.json:11-14`, `ts:kit/dom.ts:129-131`, `page/test/screens/browser.mjs:45-71`, `page/test/screens/editor.mjs:50-55`; commits `32f09ffa`, `a134c5c1`.

### Behaviours

1. EDITOR-CSP-1. The window's content security policy is `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost https://cdn.jsdelivr.net https://data.jsdelivr.com https://registry.npmjs.org`, and Tauri is told **not** to add nonces to `style-src` (`dangerousDisableAssetCspModification`), because a nonce makes a browser ignore `'unsafe-inline'`.
2. EDITOR-CSP-2. Why: CodeMirror lays itself out with a `<style>` element it writes; under the old `style-src 'self'` it was blocked and the gutter drew stacked over the text. Scripts stay `'self'`; `'wasm-unsafe-eval'` is for tree-sitter's WebAssembly; the two jsDelivr hosts and the npm registry are where grammars come from (EDITOR-GRAM-3,4,8).
3. EDITOR-CSP-3. The page's own code still sets styles through the DOM style object, not the `style` attribute (a `style-src` without `'unsafe-inline'` drops the attribute but allows the object) (`ts:kit/dom.ts:129-131`).
4. EDITOR-CSP-4. The screens harness serves the same policy (`browser.mjs:45-71`), and `editor.mjs` fails when CodeMirror's layout did not apply (`.cm-scroller` is `display: flex`). **Not a GPUI rule**: GPUI has no CSP. What carries over is the *network* rule: the only hosts the editor talks to are the two jsDelivr hosts and the npm registry, and only for grammars. A GPUI port that fetches grammars should keep to those hosts and hash checks.

---

## Surface STATE: every state of the editor

Source: `ts:code/editor.ts:70-78,290-326`, `css:6139-6161`.

The bar over the editor (`.ed-bar`, 26 px minimum height, `--fg-faint`, one line, `--fs-xs`) holds one status word (`ed-state <status>`), then, depending on state, a grammar offer or the clash buttons. The status words:

| status | text | colour | when |
|---|---|---|---|
| `opening` | Opening… | faint | session created, `opened` not yet heard; also on reopen |
| `saved` | Saved | faint | nothing pending (the initial value of the field, then `Opening…` at once) |
| `unsaved` | Unsaved | muted | typed, not yet written (pause timer running) |
| `saving` | Saving… | muted | a write is in flight or something typed waits on it |
| `clash` | An agent changed what you were editing | danger | a disk change that touches the same text is held |
| `failed` | Not saved: `<reason>` | danger | a write was refused with a reason; `Saving…` again at the 1 s retry (EDITOR-STATE-10) |
| `closed` | Closed: `<reason>` | danger | the daemon let the file go |
| `offline` | Disconnected; kept here until the daemon is back | attention | connection lost; replaced by `Saving…` at the next write (EDITOR-STATE-10) |

### Behaviours

1. EDITOR-STATE-1. **Loading.** Before the editor module has loaded, a blank box (no skeleton). Between a file being chosen and its `opened`, the **previous file's editor stays up** (EDITOR-SWITCH-2); only the first ever file of a window shows the blank box until its text arrives. The bar reads "Opening…".
2. EDITOR-STATE-2. **No grammar.** The text is plain (no `sx-*` marks). The bar shows `No colours for <label> yet. Install its tree-sitter grammar? …` with Install and Not now (EDITOR-GRAM-7); after "Not now" the bar shows only the status. A file whose extension has no known and no found grammar shows only the status. While one installs the bar shows `Installing the <label> grammar…`; after a failure it shows the error (EDITOR-GRAM-8) in the same slot.
3. EDITOR-STATE-3. **Clash.** EDITOR-CLASH-3 to 5.
4. EDITOR-STATE-4. **Failed.** "Not saved: <reason>" and a retry after 1 s (EDITOR-SAVE-4). The buffer stays editable.
5. EDITOR-STATE-5. **Offline.** EDITOR-RECONN-1. The buffer stays editable.
6. EDITOR-STATE-6. **Closed.** On `EditClosed` (the file is gone for good, too large, binary, not UTF-8, or never could be opened) the bar says `Closed: <reason>` and the editor becomes **read-only**; what is typed stays on screen but nothing more is written (`write` does no work for a closed session); a retiring closed session is destroyed. If the closing was the answer to a file that never opened, the page draws the stage at once so the closed state shows in place of the held-over previous file. Source: `ts:code/editor.ts:571-574,730-739`.
7. EDITOR-STATE-7. **Read-only.** The only read-only state is `closed` (CodeMirror `readOnly` set through the `editable` compartment). Every other state is editable, including offline, failed and clash. On `opened` the editor is made editable again.
8. EDITOR-STATE-8. **Not text.** A file the read finds is binary or over 1 MiB drops the editor for the read view's note (EDITOR-OPEN-2): Rust's `note` ("binary file", "too large to display").
9. EDITOR-STATE-9. **History or a daemon without `code-edit`.** No editor; the read view (`checkouts-code-views.md`).
10. EDITOR-STATE-10. The `offline` and `failed` words last only until the next write. `write()` always calls `tell(s, settled(s))` (`ts:code/editor.ts:578`), and `settled()` knows only opening, clash, saving and saved (`ts:code/editor.ts:429-433`). So `offline` becomes `Saving…` once typing pauses while disconnected, and stays so until reconnect because `sent` is never answered; `failed` becomes `Saving…` on the 1 s retry (EDITOR-SAVE-4). The port must compute the status from the connection and the last refusal, not from the last write.
11. EDITOR-STATE-11. **Scenes the port adds for each state.** The one scene (`code-edit`) sends no edit events (Scenes and gaps), so none of these is drawn today. The port has a scene, and a test that reads the bar's words, for each: **clash** (`An agent changed what you were editing`, with the clash buttons and the "theirs" widget, CLASH-3 to 5); **offline** (`Disconnected; kept here until the daemon is back`, the buffer still editable, RECONN-1); **failed save** (`Not saved: <reason>`, then `Saving…` at the 1 s retry, SAVE-4, STATE-10); **closed file** (`Closed: <reason>`, read-only, STATE-6); **installing** (`Installing the <label> grammar…`) and a **grammar failure** (the error in the same slot, GRAM-8); **opening**, **unsaved**, **saving** and **saved** as the quiet states; and the grammar offer (`No colours for <label> yet…`, STATE-2). Each scene is data (a sequence of `EditEvent`s and a clock) so the harness replays it without a webview.

---

## Page-side logic that must move to Rust

Every item below is logic the page holds today that a GPUI editor has no webview to hold. Where `Session` or `argus-desktop-core` can own it, the port should do that; where it must be a view entity, say so.

1. **The editor session map and its lifecycle** (`ts:code/editor.ts:117-119,435-469,621-678`): one session per `(checkout,path)`, retire, hold-over, reopen. A view-entity concern, but the *rules* (EDITOR-SWITCH-2 to 5) belong in a plain model type with no GPUI types, tested without a window.
2. **`Synced`** (`ts:code/sync.ts` whole file): the version, in-flight write, local changes, held clash, `take`, `saved`, `refused`, `lost`, `changed`, `reopened`, `resolve`, `clashes`, `between`. Port it to a Rust type over a rope (for example `ropey`) and the change-set algebra it takes from CodeMirror (`compose`, `map`, `invert`, `mapPos`, `iterChanges`, `iterChangedRanges`, `ChangeSet.of`). **The algebra is the work:** `gpui-kit`'s `Editor` must expose, or the port must write, a change-set type with compose, map-through, invert and position mapping. Put it in `argus-desktop-core` (windowless, like the rest of the editor's relay), with the property of EDITOR-SYNC-14 as a `proptest` in a `mod properties` (`AGENTS.md`), drawn from `page/test/code/sync.test.mjs`.
3. **The typing timers** (`QUIET_MS` 200, `MOST_MS` 1000, `REREAD_MS` 400, `RETRY_MS` 1000, `LIT_MS` 1600) and the `typed` / `write` / `readAgain` policy (`ts:code/editor.ts:56-68,531-593`). In GPUI these are `cx.spawn` timers on the editor entity; the policy (when to write, when to re-read) is a model function that takes "now" and the pending state and returns what to send, so it can be tested without a clock.
4. **The edit-event handler** (`ts:code/editor.ts:680-741`): the mapping from `EditEvent` to the sync and the status. It should become a `fn receive(&mut self, EditEvent) -> Vec<Effect>` on the model of item 2, reusing `rs:code/edit.rs`'s types, which the GPUI crate already shares (`crates/argus-gpui/src/connection/owner.rs:97`).
5. **The status bar's state machine** (`tell`, `settled`, `SAID`, the table in STATE) as a model enum with the strings as constants; the rendering is GPUI's.
6. **Grammar policy** (`ts:code/grammar/*`, `ts:code/editor.ts:334-427`): the pinned table, extension and name matching, the npm-name map, the npm search and file-list logic, the hash check, the "ask first, unchecked, publisher" wording, the `declined` set, the auto-install setting, which open editors a new grammar colours. The **table and the matching and the rules** are data and logic for `argus-desktop-core` (or a crate beside it), shared with the daemon's read-view colouring (`syntax/highlight` `CAPTURES`) so the two agree; the download and storage are a Rust HTTP client (hash with SHA-256) and a file store under the instance's config directory in place of IndexedDB. The WebAssembly loading does not carry over; tree-sitter grammars must be built or fetched for the native runtime (`GPUI.md` says `gpui-kit`'s `Editor` has tree-sitter): **this is the largest open question**, because the pinned `.wasm` files are not native parsers. Decide it before porting GRAM.
7. **The colour class table** (`CLASSES`, `classOf`, `ts:code/colours.ts:28-55`) and "first capture of a run wins" (`:196-201`): duplicated in the daemon today. The protocol rule in `AGENTS.md` ("a contract written twice drifts in silence") says it should be one table in `argus-protocol` or one shared module. Move it out of TypeScript into the shared place that the daemon's `CAPTURES` also reads; keep the 8-class mapping to theme tokens in the GPUI theme.
8. **Change marks and stepping** (`changeMarks`, `stepChange`, `ts:code/editor.ts:222-267`): small, and already driven by Rust's `CodePage.lines`; move the "next change after the cursor line, a run is one change" rule to `rs:code/` beside `Run`/`changes_here`, so the Rust `Code` model owns it (it already has `Run::covers` and a cursor).
9. **The go-to line after a Rust-driven move** (`host`'s `c.at !== s.at` rule, `ts:code/editor.ts:644-649`): "Rust's line is gone to only when it changed; the editor's own moves are never reported" is a protocol convention. Keep it, and say it in the model.
10. **Vim**: the whole Vim layer is a library on the page. The port needs a Vim engine over its editor (see the lib rows in VIM). If a Rust Vim engine is not available, the port must write one against the lists above, or decide a subset; the **observable** requirements are EDITOR-VIM-1 to 19.
11. **Clipboard and `+` register** (EDITOR-VIM-12) and the IME (EDITOR-KEYS-5): the page gets both from the webview.
12. **`Mod-p`/`Mod-Shift-f`/`Alt-c`/`Alt-]`/`Alt-[`/`Mod-s`** (EDITOR-KEYS-4): the keymap today lives in the page's editor; the key table is in `rs:keys/keymap.rs`. Move these rows there (or add an `Editor` place) so the sheet and the palette list them and the Vim `Ctrl-p` clash (EDITOR-KEYS-4) is decided in one place.
13. **Reopen on reconnect** (EDITOR-RECONN-3): make it a response to the connection event rather than to the next draw.

---

## Scenes and gaps

### Scenes and tests that exercise the editor today

- `scene:code-edit` (`rs:screens/mod.rs:1966`) draws the Code stage with `status.rs` open at line 52. The **scene sends no edit events**: the editor's states are produced by `page/test/screens/editor.mjs`, which plays the scene and then delivers `edit` events by hand (`window.__deliver`). The scene does not by itself show any of the editor's states.
- `editor.mjs` covers: the editor opens and CodeMirror's own layout applied (EDITOR-CSP-4); the grammar offer text and Install (EDITOR-GRAM-7,8); the colours after install; Vim opens in normal mode; change marks from a Rust page; key latency (p50/p95 event to buffer and to frame); `Escape` back to normal; writes are batched (N keys sent as M writes, none answered); an agent's write lands and is lit; a clash shows the bar and the "theirs" widget; a file switch shows no skeleton, no empty editor, and a coloured first frame (exit code 1 on failure). It exits 1 for the switch check, and it also throws, so fails, when the editor does not open or CodeMirror's layout did not apply (`crates/argus-desktop/page/test/screens/editor.mjs:48-55`).
- `page/test/code/sync.test.mjs` (the sync, including the random-interleaving property) and `page/test/code/colours.test.mjs` (incremental parse equals fresh parse; the capture table) are node tests; `page/test/code/registry.mjs` is a network check of every pin.
- Rust: `rs:code/stage.rs` tests (1628-1676), `rs:code/edit.rs` tests (84, 92), `d:code/edit/tests.rs` (six tests), `d:code/edit/text.rs`, `rs:session.rs:5540` (the `edit` call's JSON shape).

### Gaps (what no test or scene covers, or where docs and source disagree)

1. No scene draws a clash, an offline bar, a failed save, a closed file, "Installing…" or a grammar failure. The port should add `scene:`s for each state in STATE.
2. No test exercises Vim's bindings. EDITOR-VIM-1 to 19 are from the library and its tarball, not from an Argus test; the only checks are `editor.mjs`'s normal-mode cursor and Escape.
3. `DESIGN.md` says `Ctrl P` reaches the stage from inside the editor; with Vim on, normal-mode `Ctrl-p` is the library's `k` and is heard first (EDITOR-KEYS-4). Needs a decision.
4. `gc` and `=` with no CodeMirror language loaded (EDITOR-VIM-15), and the `+` register under the WebView (EDITOR-VIM-12), were not checked.
5. The `Synced` version is created as 0, not from `opened.version` (EDITOR-OPEN-7).
6. Reconnect behaviour depends on a redraw (EDITOR-RECONN-3) and writes while disconnected are sent into nothing (EDITOR-RECONN-2); neither is tested.
7. The grammar offer is persisted only as installed grammars; "Not now" is not persisted (EDITOR-GRAM-7), by design.
8. The library versions: Vim core `0.1.0` was read from npm, not from `page/package-lock.json` (not checked here). Recheck the lists if the lock differs.
9. This spec describes the editor as `main` has it at commit `19aa914d` on branch `gpui-migration`; the editor's own recent commits are `c8cb3ead`, `a134c5c1`, `32f09ffa`, `738068cb`.
