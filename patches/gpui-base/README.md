# gpui-base 0.8.0, patched for Argus

This is `gpui-base` 0.8.0 from crates.io (Apache-2.0, `LICENSE-APACHE`) with four additions to its
code editor, wired in by `[patch.crates-io]` in the workspace `Cargo.toml`. `gpui-kit` pins
`gpui-base`, so the patch applies to the whole GPUI build and to nothing else. The manifest is the
published one with the `[[test]]` and `[[bench]]` targets removed, since their files are not
carried; `Cargo.toml`'s workspace `exclude` keeps this folder from being built or linted as
Argus's own code.

Every change is small and written so it can be offered upstream (`longbridge/gpui-kit`) as it
stands. Nothing else in the crate is touched.

## What changed, and why

### 1. `InputBaseState::apply_external_edits`

`src/input/base/state.rs`, and `UndoManager::carry_over_external_edit` in
`src/input/base/undo_manager.rs`.

The editor has no way to apply text that did not come from the person. Every way in either moves
the cursor to the end of the edit, enters undo, emits `InputEvent::Change`, is refused when the
editor is read-only, or (for `replace_text_in_range`, the one that takes a range) runs auto-close,
skip-over and completion triggers, so a lone `(` can write other text than was asked. `set_value`
avoids all of that but clears undo and resets the selection and scroll. An agent writing the file
under an open editor needs none of those effects.

```rust
pub fn apply_external_edits(&mut self, edits: &[(Range<usize>, String)], window, cx)
```

`edits` are byte ranges in the current document, apart from each other. The call:

- keeps every cursor and selection on the text it was on (a position inside a replaced range goes
  to its start; one where text is only inserted stays in front of it);
- keeps the edit out of undo and redo, and rewrites the entries already there so that undo still
  reaches the person's own changes and never the edit. An entry the edit touches cannot be
  rewritten, so that entry and every one that would have to be undone (or redone) after it are
  dropped, which is what a text editor without operational transform does;
- emits no `InputEvent::Change`, so a listener that reports the person's typing does not report
  this;
- bypasses `readonly` and `disabled`, and writes exactly the text asked for;
- keeps the top visible line where it was when the edit adds or removes lines above it (exact
  without soft wrap).

It uses the batch path the editor already has (`replace_text_in_ranges`) with history ignored, so
decorations, folds, search matches and the highlighter follow the edit as they do for any other.

Tests: `external_edit_tests` in `undo_manager.rs` (the rewriting of entries), and the two
`an_external_edit_*` tests in `state.rs`. They run in a copy of this crate with a `[workspace]`
table appended, since its dev-dependencies are not resolved as a patch:
`cargo test --lib external_edit --features test-support`.

### 2. Gutter marks: `RangeDecorationStyle::GutterBar` and `GutterTick`

`src/input/editor/decorations.rs` and `src/input/base/element.rs`.

A range decoration could be a fill or a frame over text, and nothing could be drawn in the
line-number gutter. Two styles are added to the (already `non_exhaustive`) enum:

- `GutterBar`: a 3 px bar, in the decoration's colour, in the gutter beside each row the range
  touches;
- `GutterTick`: the same 3 px wide and 4 px tall at the top of the range's first row.

They are ranges like the others: they follow edits, are queried for the visible lines only, and are
set and cleared through `RangeDecorationCollection`. Nothing is drawn when the editor has no line
numbers. The bar is painted after the gutter's background and the numbers.

### 3. `InputBaseState::set_caret_color`

`src/input/base/state.rs`. `gpui-component`'s `Input` projects `theme().caret` onto every state on
every render, so one editor cannot have a caret of its own colour without recolouring every text
field. The setter keeps a colour on the state that is applied after that projection (`None` leaves
the projected one). Argus draws the code editor's caret in the accent, as the page's `.cm-cursor`
does, and its text fields in the foreground.

### 4. `InputBaseState::set_scrollbar_gutter`

`src/input/base/state.rs`, read where `editor_paddings` was in `src/input/base/element.rs`. The
editor draws its scrollbars inside its padding, over the text's right end and its last line, and
`Input` overwrites the padding from its size on every render. The gutter is added to the projected
padding, so the bars sit in room of their own, as a native scroll box's do; Argus's code editor
keeps one bar's width, as its diff view does.

## Bumping `gpui-kit`

The pin in `crates/argus-gpui/Cargo.toml` is `=0.8.0` on purpose. A newer `gpui-base` means
re-applying these four diffs to it (`git diff --no-index` the published crate against this folder),
dropping whichever upstream has taken.
