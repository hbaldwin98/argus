//! Rules that hold for any keys typed on any text.

use proptest::prelude::*;

use super::*;
use crate::code::vim::command::{self, Parse};
use crate::code::vim::key::Code;

/// The keys Vim has a use for, with the odd one it has none for.
fn a_key() -> impl Strategy<Value = Key> {
    let chars = "hjklwWbBeE0^$|gGfFtT;,{}%dycg~uU<>xXDYCsSrJpPoOaAiIRvV.:\"_+ ()[]'`ab12éz\u{1F600}-";
    prop_oneof![
        12 => proptest::sample::select(chars.chars().collect::<Vec<_>>()).prop_map(Key::ch),
        2 => proptest::sample::select(vec![
            Code::Esc, Code::Enter, Code::Backspace, Code::Delete, Code::Left, Code::Right,
            Code::Up, Code::Down, Code::Home, Code::End, Code::Tab, Code::Insert,
        ]).prop_map(Key::of),
        1 => proptest::sample::select(vec!['r', 'w', 'u', 't', 'd', 'v', 's']).prop_map(Key::ctrl),
    ]
}

fn some_keys(most: usize) -> impl Strategy<Value = Vec<Key>> {
    proptest::collection::vec(a_key(), 0..most)
}

/// A text of short lines with brackets, quotes and wide characters.
fn some_text() -> impl Strategy<Value = String> {
    "[ab (){}\"'é😀.\n]{0,40}"
}

/// What must be true of the machine after any key.
fn sound(ed: &Ed) -> Result<(), TestCaseError> {
    let doc = ed.vim.doc();
    let len = doc.len();
    let at = ed.vim.cursor();
    prop_assert!(at <= len, "the cursor is in the text");
    prop_assert!(doc.is_char_boundary(at), "the cursor is on a character");
    if let Some((from, to)) = ed.vim.selection() {
        prop_assert!(from <= to && to <= len, "the selection is in the text");
        prop_assert!(doc.is_char_boundary(from) && doc.is_char_boundary(to));
    }
    if matches!(ed.mode(), Mode::Normal | Mode::Visual | Mode::VisualLine) && at < len {
        let line = ed.vim.buf.text().line_of(at);
        let t = ed.vim.buf.text();
        prop_assert!(
            doc.char(at) != '\n' || t.line_start(line) == t.line_end(line),
            "a cursor rests on a character, or an empty line"
        );
    }
    Ok(())
}

proptest! {
    // Not in any one source: what any editor must hold. The host applies
    // each key's change to its own copy and the two must stay one text.
    #[test]
    fn whatever_keys_are_typed_the_machine_stays_sound_and_tells_every_change(
        text in some_text(), at in 0usize..40, keys in some_keys(40)
    ) {
        let mut ed = Ed::new(&text);
        ed.vim.place(at.min(text.len()));
        // `Ed::key` asserts the told changes build the buffer.
        for k in keys {
            ed.key(k);
            sound(&ed)?;
        }
    }

    // EDITOR-VIM-10: u takes back what Vim did, and all of it: a unit is a
    // command, or an insert from its first key to Escape.
    #[test]
    fn undoing_enough_returns_the_text_to_where_it_began(
        text in some_text(), keys in some_keys(30)
    ) {
        let mut ed = Ed::new(&text);
        for k in &keys {
            ed.key(*k);
        }
        ed.keys("<Esc><Esc>");
        for _ in 0..keys.len() + 3 {
            ed.keys("u");
        }
        prop_assert_eq!(ed.text(), text);
    }

    // EDITOR-VIM-18: a read-only file is never edited or entered.
    #[test]
    fn a_read_only_file_is_never_changed(text in some_text(), keys in some_keys(40)) {
        let mut ed = Ed::new(&text);
        ed.vim.set_read_only(true);
        for k in keys {
            ed.key(k);
            sound(&ed)?;
            prop_assert!(!matches!(ed.mode(), Mode::Insert | Mode::Replace));
        }
        prop_assert_eq!(ed.text(), text);
    }

    // A command known by its keys: nothing short of them is one, so a half
    // typed command waits (the host shows it as pending).
    #[test]
    fn a_command_is_not_known_before_its_last_key(keys in some_keys(7), visual in any::<bool>()) {
        if let Parse::Done(_) = command::parse(&keys, visual) {
            for n in 1..keys.len() {
                prop_assert_eq!(command::parse(&keys[..n], visual), Parse::Need, "after {} keys", n);
            }
        }
    }

    // `.` repeats the last change: x then . is 2x, wherever the cursor is.
    #[test]
    fn a_dot_after_x_is_a_count_of_two(text in "[ab ]{3,10}", at in 0usize..10) {
        // Short of the line's end: off the last character the cursor steps back, and `.` then acts there.
        let at = at.min(text.len() - 3);
        let mut dot = Ed::new(&text);
        dot.vim.place(at);
        dot.keys("x.");
        let mut two = Ed::new(&text);
        two.vim.place(at);
        two.keys("2x");
        prop_assert_eq!((dot.text(), dot.vim.cursor()), (two.text(), two.vim.cursor()));
    }

    // yy then p puts the line again below it, whole.
    #[test]
    fn yanking_a_line_and_putting_it_duplicates_it(text in "[ab é\n]{1,30}", at in 0usize..30) {
        let mut ed = Ed::new(&text);
        ed.vim.place(at.min(text.len()));
        let line = ed.vim.buf.text().line_of(ed.vim.cursor());
        ed.keys("yyp");
        let mut want: Vec<&str> = text.split('\n').collect();
        want.insert(line + 1, want[line]);
        prop_assert_eq!(ed.text(), want.join("\n"));
    }

    // g~~ twice is no change on letters and spaces.
    #[test]
    fn swapping_case_twice_is_no_change(text in "[a-zA-Z \n]{1,30}", at in 0usize..30) {
        let mut ed = Ed::new(&text);
        ed.vim.place(at.min(text.len()));
        ed.keys("g~~g~~");
        prop_assert_eq!(ed.text(), text);
    }

    // >> then << on a line with no indent is no change.
    #[test]
    fn indenting_and_outdenting_a_line_is_no_change(text in "[a-b]{1,5}(\n[a-b]{1,5}){0,3}", at in 0usize..20) {
        let mut ed = Ed::new(&text);
        ed.vim.place(at.min(text.len()));
        ed.keys(">><<");
        prop_assert_eq!(ed.text(), text);
    }
}
