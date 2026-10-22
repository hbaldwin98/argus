//! Rules that hold for any run of whole commands, which reach what single
//! keys rarely do: a search and then `n`, a macro played inside a macro, `:g`
//! over lines that change, a block and then `gv`.

use proptest::prelude::*;

use super::properties::{some_text, sound};
use super::*;

/// Commands of every kind, whole.
const COMMANDS: &[&str] = &[
    "/a<CR>", "?b<CR>", "n", "N", "*", "#", "g*", "gn", "gN", "cgnX<Esc>", "dgn", "ma", "mb", "'a", "`b", "]'", "['", "<C-o>", "<C-i>",
    "qa", "q", "@a", "@@", "@:", ":s/a/x/g<CR>", ":%s/b//<CR>", ":s/(a)/$1$1/gc<CR>y", "n", "a", "q", ":g/a/d<CR>",
    ":v/a/s/b/c/<CR>", ":g/b/normal Ax<CR>", ":sor<CR>", ":sor! n<CR>", ":sor u<CR>", ":%norm Ax<CR>", ":norm! ix<CR>",
    ":2,3d<CR>", ":1,2y<CR>", ":$pu<CR>", ":j<CR>", ":'a,.d<CR>", ":set tw=6<CR>", ":set nopcre<CR>", ":set pcre<CR>",
    ":nmap x dd<CR>", ":imap jk <lt>Esc><CR>", ":unmap x<CR>", ":mapclear<CR>", ":reg<CR>", ":marks<CR>", ":delm a<CR>",
    "<C-v>", "<C-v>jld", "<C-v>jIX<Esc>", "<C-v>j$AY<Esc>", "<C-v>jlcZ<Esc>", "<C-v>jlrq", "<C-v>jly$p", "gv",
    "I", "A", "$", "gqq", "gqip", "gwip", "g?w", "g??", "=ap", "gcc", "]p", "[p", "H", "M", "L", "<C-f>", "<C-b>", "<C-d>",
    "<C-u>", "<C-e>", "<C-y>", "zz", "zt", "zb", "z.", "is", "as", "it", "at", "dis", "das", "dit", "dat", "(", ")", "d)",
    "]}", "[{", "])", "[(", "]]", "[[", "][", "[]", "]*", "[/", "]m", "]#", "<C-a>", "<C-x>", "5<C-a>", "gJ", "J", "u", "<C-r>",
    ".", "<Esc>", "v", "V", "viw", "vip", "vi(", "iab<Esc>", "ohello<Esc>", "Oyo<Esc>", "dd", "yyp", "xp", "3x", "dw", "cwZ<Esc>",
    "ciwQ<Esc>", "ci(Z<Esc>", "yiwP", "\"ayy", "\"ap", "\"_dd", ">>", "<<", "gUU", "guu", "g~~", "~", "r<CR>", "2r<CR>", "R12<Esc>",
    "i<C-o>$X<Esc>", "A<C-o>0Y<Esc>", "gi", "gI", "i<C-r>.<Esc>", "gg", "G", "5G", "%", "{", "}", "fa", ";", ",", "ta", "w", "b", "e",
    "ge", "0", "^", "|", "gj", "gk", "g0", "g$", "<C-p>", "<C-n>", "<BS>", "<CR>", "<Space>", "<S-Space>", "<C-Space>",
];

fn script() -> impl Strategy<Value = Vec<Key>> {
    proptest::collection::vec(proptest::sample::select(COMMANDS.to_vec()), 1..24).prop_map(|parts| {
        let mut keys = Vec::new();
        let mut recording = false;
        // A bare `q` names the register with the first character of the part after it.
        let mut naming = false;
        for p in parts {
            // A register played while it is recorded plays itself down to the depth limit,
            // as in the library, and over a text the macro grows that takes minutes and
            // gigabytes: it is pinned by a unit test, not left to chance here.
            if recording && matches!(p, "@a" | "@@") {
                continue;
            }
            if naming {
                recording = p.starts_with(|c: char| c.is_ascii_alphanumeric());
                naming = false;
            } else if p == "q" {
                naming = !recording;
                recording = false;
            } else if p == "qa" {
                recording = !recording;
            }
            keys.extend(key::parse(p));
        }
        keys
    })
}

/// Text with lines, words, brackets, tags, sentences and numbers in it.
fn rich_text() -> impl Strategy<Value = String> {
    proptest::collection::vec("[ab ]{0,6}|\\(a b\\) 12\\.|<p>a</p>|\\{ a \\}\\.", 1..6).prop_map(|lines| lines.join("\n"))
}

proptest! {
    // Whatever whole commands are run, the machine stays sound, and the host's copy is the buffer.
    #[test]
    fn a_script_leaves_the_machine_sound(text in rich_text(), at in 0usize..60, keys in script()) {
        let mut ed = Ed::new(&text);
        ed.vim.place(at.min(text.len()));
        ed.vim.set_view(1, 3);
        for k in keys {
            ed.key(k);
            sound(&ed)?;
        }
    }

    // The same, on text that is only the odd characters.
    #[test]
    fn a_script_on_odd_text_leaves_the_machine_sound(text in some_text(), keys in script()) {
        let mut ed = Ed::new(&text);
        for k in keys {
            ed.key(k);
            sound(&ed)?;
        }
    }

    // A read-only file is never changed by any command, ex ones and macros too.
    #[test]
    fn a_script_never_changes_a_read_only_file(text in rich_text(), keys in script()) {
        let mut ed = Ed::new(&text);
        ed.vim.set_read_only(true);
        for k in keys {
            ed.key(k);
            sound(&ed)?;
            prop_assert!(!matches!(ed.mode(), Mode::Insert | Mode::Replace));
        }
        prop_assert_eq!(ed.text(), text);
    }

    // Everything a script did to the text is undone by undoing every unit, and no more than the text.
    #[test]
    fn a_script_is_undone_by_undoing_all_of_it(text in rich_text(), keys in script()) {
        let mut ed = Ed::new(&text);
        for k in keys {
            ed.key(k);
        }
        ed.keys("<Esc><Esc><Esc>");
        while ed.vim.buf.undo().is_some() {}
        prop_assert_eq!(ed.text(), text);
    }

    // Macros end: no script, whatever it records and plays, leaves one playing or runs for ever.
    #[test]
    fn no_macro_is_left_playing(text in rich_text(), keys in script()) {
        let mut ed = Ed::new(&text);
        for k in keys {
            ed.key(k);
            prop_assert_eq!(ed.sh.macros.depth, 0);
        }
    }

    // The window's view is only ever read: Vim does not make a line of it that is not in the text.
    #[test]
    fn a_view_past_the_text_is_no_trouble(text in rich_text(), top in 0usize..40, rows in 1usize..40, keys in script()) {
        let mut ed = Ed::new(&text);
        ed.view(top, top + rows - 1);
        for k in keys {
            ed.key(k);
            sound(&ed)?;
        }
    }
}

/// Commands that change the text the same way wherever they run, without help from anything
/// they or an earlier command left in a register, a mark or the search.
const PURE: &[&str] = &[
    "x", "X", "~", "J", "dd", "D", ">>", "<<", "iab<Esc>", "Ax<Esc>", "Iy<Esc>", "ohi<Esc>", "Oyo<Esc>", "j", "k", "w", "b", "l", "h",
    "0", "$", "rz", "gUU", "guu", "g~~", "cwZ<Esc>", "ciwQ<Esc>", "2x", "dw", "g?w", "gJ",
];

proptest! {
    // A macro does what typing it did: record a run of commands, undo it all, play it from where
    // it began, and the text is the text the typing made.
    #[test]
    fn a_macro_does_what_the_typing_did(
        text in "[ab ]{0,6}(\n[ab ]{0,6}){0,3}",
        parts in proptest::collection::vec(proptest::sample::select(PURE.to_vec()), 1..12)
    ) {
        let script = parts.concat();
        let mut ed = Ed::new(&text);
        ed.keys(&format!("qa{script}q"));
        let typed = ed.text();
        ed.keys("<Esc>");
        while ed.vim.buf.undo().is_some() {}
        prop_assert_eq!(ed.text(), text.clone());
        ed.vim.place(0);
        ed.keys("@a");
        prop_assert_eq!(ed.text(), typed);
    }

    // `:normal` does what typing did, on a line.
    #[test]
    fn normal_does_what_the_typing_did(
        text in "[ab ]{0,6}(\n[ab ]{0,6}){0,3}",
        parts in proptest::collection::vec(proptest::sample::select(PURE.to_vec()), 1..8)
    ) {
        let script = parts.concat();
        let mut by_hand = Ed::new(&text);
        by_hand.keys(&script);
        by_hand.keys("<Esc>");
        let mut by_normal = Ed::new(&text);
        // What `:normal` is given is typed on the `:` line, so a `<` in it is typed as one.
        by_normal.keys(&format!(":normal {}<CR>", script.replace('<', "<lt>")));
        prop_assert_eq!(by_normal.text(), by_hand.text());
    }
}
