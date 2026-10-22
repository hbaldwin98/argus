//! What the Code stage's files column says and sends from its one field: the text the field shows
//! for what Rust holds, which list the column is, the search's options and summary line, and what
//! a key in the field or in the include-glob field comes to (`code/stage.ts`, `find`, `option`,
//! `searchPanel`, `jumpList`, `scope`). The columns the window and the page draw are the same
//! sentences.

use super::stage::{CodeClick, CodeGesture, CodeHit, CodeJump, CodePage, CodeSearch};
use crate::checkouts::query::Query;
use crate::connection::page::ReviewJumpGesture;
use crate::keys::keymap::Action;

pub const FIND_HINT: &str = "Go to a file, or # for text";
/// The key cap in the field while it is not a search.
pub const FIND_KEY: &str = "Ctrl P";
pub const GLOB_HINT: &str = "Files to include, as globs";

/// What the column lists under the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    /// The search's options, summary and hits.
    Search,
    /// The go-to-file matches.
    Jump,
    /// The scope row and the tree.
    Tree,
}

pub fn body(page: &CodePage) -> Body {
    if page.search.is_some() {
        Body::Search
    } else if page.jump.is_some() {
        Body::Jump
    } else {
        Body::Tree
    }
}

/// What the field shows for what Rust holds: a search as `#query`, so what is typed and what is
/// drawn agree, else the go-to-file query, else nothing.
pub fn field_text(page: &CodePage) -> String {
    match (&page.search, &page.jump) {
        (Some(search), _) => format!("#{}", search.query),
        (None, Some(jump)) => jump.query.clone(),
        (None, None) => String::new(),
    }
}

/// Whether the text typed so far still means what Rust holds, so that a key pressed between a
/// gesture and its redraw is not written over: a list is open and the text is a search exactly
/// when the list is the search. A `#` that moved the field from one list to the other, or a list
/// put away, is Rust's word.
pub fn keeps_typed(page: &CodePage, typed: &str) -> bool {
    body(page) != Body::Tree && matches!(Query::of(typed), Query::Text(_)) == page.search.is_some()
}

/// One of the search's three toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOption {
    pub label: &'static str,
    pub on: bool,
    pub which: &'static str,
    pub title: &'static str,
}

pub fn options(search: &CodeSearch) -> [SearchOption; 3] {
    [
        SearchOption { label: "Aa", on: search.case, which: "case", title: "Match case  Alt C" },
        SearchOption { label: "ab", on: search.word, which: "word", title: "Whole word  Alt W" },
        SearchOption { label: ".*", on: search.regex, which: "regex", title: "Regular expression  Alt R" },
    ]
}

/// What a click on an option comes to.
pub fn option_click(which: &str) -> CodeGesture {
    CodeGesture::Click { target: CodeClick::SearchOption { option: which.to_owned() } }
}

/// What a key in the find field comes to. The Alt chords flip an option while the field is a
/// search; the jump place's keys step, choose and close, to whichever list the field is.
pub fn field_key(searching: bool, action: Action) -> Option<CodeGesture> {
    let option = match action {
        Action::MatchCase => Some("case"),
        Action::WholeWord => Some("word"),
        Action::UseRegex => Some("regex"),
        _ => None,
    };
    if let Some(which) = option {
        return searching.then(|| option_click(which));
    }
    let gesture = match action {
        Action::Next => ReviewJumpGesture::Step { by: 1 },
        Action::Previous => ReviewJumpGesture::Step { by: -1 },
        Action::Choose => ReviewJumpGesture::Choose,
        Action::CloseLayer => ReviewJumpGesture::Close,
        _ => return None,
    };
    Some(if searching { CodeGesture::Search { gesture } } else { CodeGesture::Jump { gesture } })
}

/// What Enter and Escape in the include-glob field come to: the search is run, or put away.
pub fn glob_key(action: Action) -> Option<CodeGesture> {
    match action {
        Action::Choose => Some(CodeGesture::Search { gesture: ReviewJumpGesture::Choose }),
        Action::CloseLayer => Some(CodeGesture::Search { gesture: ReviewJumpGesture::Close }),
        _ => None,
    }
}

/// The line under the search's field, by state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub text: String,
    /// The reason a search could not run, drawn in the danger colour.
    pub danger: bool,
}

pub fn summary(search: &CodeSearch) -> Summary {
    let said = |text: String, danger| Summary { text, danger };
    if let Some(error) = &search.error {
        said(error.clone(), true)
    } else if search.loading {
        said("Searching\u{2026}".into(), false)
    } else if !search.current {
        said("Enter searches the working tree.".into(), false)
    } else if search.total == 0 {
        said(format!("Nothing matches \u{201c}{}\u{201d}.", search.query), false)
    } else {
        let files = search.files.len();
        let more = if search.truncated { "+" } else { "" };
        said(format!("{}{more} matches in {files} {}", search.total, if files == 1 { "file" } else { "files" }), false)
    }
}

/// The hit's line as one text, which is what hovering it says.
pub fn hit_title(hit: &CodeHit) -> String {
    hit.parts.iter().map(|(text, _)| text.as_str()).collect()
}

/// The go-to list's line when nothing matches.
pub fn jump_none(jump: &CodeJump) -> Option<String> {
    jump.matches.is_empty().then(|| format!("No file matches \u{201c}{}\u{201d}.", jump.query))
}

/// The scope row's tooltip: the branch, else the checkout's name.
pub fn scope_tip(page: &CodePage) -> &str {
    if page.branch.is_empty() { &page.name } else { &page.branch }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::stage::CodeHitFile;

    fn search(query: &str) -> CodeSearch {
        CodeSearch { query: query.into(), ..CodeSearch::default() }
    }

    fn jump(query: &str) -> CodeJump {
        CodeJump { query: query.into(), matches: Vec::new(), at: None }
    }

    #[test]
    fn the_field_shows_a_search_with_its_hash_and_a_jump_as_typed() {
        let mut page = CodePage { jump: Some(jump("main")), ..CodePage::default() };
        assert_eq!((field_text(&page), body(&page)), ("main".into(), Body::Jump));
        page.search = Some(search("Dragged"));
        assert_eq!((field_text(&page), body(&page)), ("#Dragged".into(), Body::Search));
        assert_eq!(field_text(&CodePage::default()), "");
        assert_eq!(body(&CodePage::default()), Body::Tree);
    }

    #[test]
    fn typed_text_is_kept_while_it_still_means_the_list_that_is_open() {
        let go = CodePage { jump: Some(jump("m")), ..CodePage::default() };
        let hunt = CodePage { search: Some(search("m")), ..CodePage::default() };
        assert!(keeps_typed(&go, "ma") && keeps_typed(&hunt, "#ma"));
        assert!(!keeps_typed(&go, "#m"), "a hash typed moved the field to the search");
        assert!(!keeps_typed(&hunt, "m"), "the hash was deleted");
        assert!(!keeps_typed(&CodePage::default(), "m"), "no list is open: Rust's word");
    }

    #[test]
    fn the_alt_chords_flip_an_option_only_while_searching() {
        let case = option_click("case");
        assert!(matches!(field_key(true, Action::MatchCase), Some(CodeGesture::Click { target: CodeClick::SearchOption { option } }) if option == "case"));
        assert!(matches!(field_key(true, Action::WholeWord), Some(CodeGesture::Click { target: CodeClick::SearchOption { option } }) if option == "word"));
        assert!(matches!(field_key(true, Action::UseRegex), Some(CodeGesture::Click { target: CodeClick::SearchOption { option } }) if option == "regex"));
        assert!(field_key(false, Action::MatchCase).is_none());
        assert!(matches!(case, CodeGesture::Click { .. }));
    }

    #[test]
    fn the_jump_keys_go_to_the_list_the_field_is() {
        assert!(matches!(field_key(false, Action::Next), Some(CodeGesture::Jump { gesture: ReviewJumpGesture::Step { by: 1 } })));
        assert!(matches!(field_key(true, Action::Previous), Some(CodeGesture::Search { gesture: ReviewJumpGesture::Step { by: -1 } })));
        assert!(matches!(field_key(true, Action::Choose), Some(CodeGesture::Search { gesture: ReviewJumpGesture::Choose })));
        assert!(matches!(field_key(false, Action::CloseLayer), Some(CodeGesture::Jump { gesture: ReviewJumpGesture::Close })));
        assert!(field_key(false, Action::Leave).is_none());
    }

    #[test]
    fn the_glob_field_runs_or_puts_away_the_search() {
        assert!(matches!(glob_key(Action::Choose), Some(CodeGesture::Search { gesture: ReviewJumpGesture::Choose })));
        assert!(matches!(glob_key(Action::CloseLayer), Some(CodeGesture::Search { gesture: ReviewJumpGesture::Close })));
        assert!(glob_key(Action::Next).is_none());
    }

    #[test]
    fn the_summary_is_one_line_by_state_in_the_pages_order() {
        let mut s = search("Dragged");
        s.error = Some("bad pattern".into());
        s.loading = true;
        assert_eq!(summary(&s), Summary { text: "bad pattern".into(), danger: true });
        s.error = None;
        assert_eq!(summary(&s).text, "Searching\u{2026}");
        s.loading = false;
        assert_eq!(summary(&s).text, "Enter searches the working tree.");
        s.current = true;
        assert_eq!(summary(&s).text, "Nothing matches \u{201c}Dragged\u{201d}.");
        s.total = 22;
        let file = |n: u32| CodeHitFile { path: format!("{n}"), dir: String::new(), name: format!("{n}"), count: 1, open: true, hits: Vec::new() };
        s.files = vec![file(1), file(2), file(3)];
        assert_eq!(summary(&s).text, "22 matches in 3 files");
        s.truncated = true;
        s.files.truncate(1);
        assert_eq!(summary(&s).text, "22+ matches in 1 file");
    }

    #[test]
    fn the_options_say_which_are_on_and_what_hovering_them_says() {
        let mut s = search("x");
        s.word = true;
        let shown = options(&s);
        assert_eq!(shown.map(|o| (o.label, o.on)), [("Aa", false), ("ab", true), (".*", false)]);
        assert_eq!(shown[2].title, "Regular expression  Alt R");
    }

    #[test]
    fn a_list_with_no_match_says_so_and_a_hit_is_titled_by_its_whole_line() {
        assert_eq!(jump_none(&jump("zz")).as_deref(), Some("No file matches \u{201c}zz\u{201d}."));
        let listed = CodeJump { matches: vec!["a".into()], ..jump("a") };
        assert_eq!(jump_none(&listed), None);
        let hit = CodeHit { index: 0, line: 1, parts: vec![("pub struct ".into(), false), ("X".into(), true), (" {".into(), false)], on: false };
        assert_eq!(hit_title(&hit), "pub struct X {");
    }
}
