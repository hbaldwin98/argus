//! A markdown file read as a document: it opens rendered, the header's choice reads it as text and
//! back, no other file has the choice, and a link in it goes where its kind says (task 57).

use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::code::stage::CodeRow;
use argus_desktop_core::review::comment_box::CommentPage;
use argus_desktop_core::connection::page::Event;
use gpui_kit::test::TestEventExt as _;
use gpui_kit::{point, px, TestAppContext};

use super::*;
use crate::checkouts::stage::tests::{click_id, present};
use crate::code::stage::tests::is_selected;
use crate::code::file::rendered::last;
use crate::keys::root::tests::{hear, press, type_text};

const TEXT: &str = "# Guide\n\nSome *words* and a [sibling](other.md), a [site](https://example.test/x).\n\n## Steps\n\n- one\n- two\n\n```\ncode\n```\n";

fn markdown(path: &str) -> CodePage {
    let lines: Vec<CodeRow> = TEXT.lines().enumerate().map(|(n, text)| CodeRow { no: n as u32 + 1, parts: vec![(text.into(), "")], added: false, removed_before: 0, cursor: n == 0, picked: false }).collect();
    CodePage { path: Some(path.into()), language: Some("Markdown"), lines, at: 1, outline: true, ..page() }
}

fn rendered(cx: &mut TestAppContext, opened: &Opened) -> bool {
    settle(cx, opened);
    present(cx, opened, "code-rendered")
}

/// Task 57: a markdown file is read as a document, its headings in what is drawn, with no row of lines.
#[gpui_kit::test]
fn a_markdown_file_opens_rendered_with_its_headings(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("docs/guide.md"));
    assert!(rendered(cx, &opened) && !present(cx, &opened, ("diff-row", 0usize)));
    let source = cx.update(|cx| last().read(cx).source().to_owned());
    assert!(source.contains("# Guide") && source.contains("## Steps"), "{source}");
    assert!(bounds_of(cx, &opened, "code-rendered").size.height > gpui_kit::px(100.0), "the document was laid out");
    assert_eq!(is_selected(cx, &opened, "code-view-rendered"), Some(true));
    assert_eq!(is_selected(cx, &opened, "code-view-source"), Some(false));
}

/// Task 57: Source is the existing view, Rendered brings the document back, and neither asks the session.
#[gpui_kit::test]
fn the_choice_reads_the_file_as_text_and_back_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("docs/guide.md"));
    settle(cx, &opened);
    click_id(cx, &opened, "code-view-source");
    settle(cx, &opened);
    assert!(present(cx, &opened, ("diff-row", 0usize)) && !present(cx, &opened, "code-rendered"));
    assert_eq!(is_selected(cx, &opened, "code-view-source"), Some(true));
    click_id(cx, &opened, "code-view-rendered");
    assert!(rendered(cx, &opened) && !present(cx, &opened, ("diff-row", 0usize)));
    assert_eq!(sent(&opened), Vec::<String>::new());
}

/// Task 57: the choice is kept for each file, so another file is not read the way this one was left.
#[gpui_kit::test]
fn each_file_keeps_the_way_it_was_read(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("a.md"));
    settle(cx, &opened);
    click_id(cx, &opened, "code-view-source");
    says(cx, &opened, markdown("b.md"));
    assert!(rendered(cx, &opened), "b.md was never chosen for");
    says(cx, &opened, markdown("a.md"));
    settle(cx, &opened);
    assert!(present(cx, &opened, ("diff-row", 0usize)) && !present(cx, &opened, "code-rendered"), "a.md is as it was left");
}

/// Task 57: only a markdown file has the choice.
#[gpui_kit::test]
fn a_file_that_is_not_markdown_has_no_choice(cx: &mut TestAppContext) {
    let opened = with(cx, reading(30, 1));
    settle(cx, &opened);
    assert!(!present(cx, &opened, "code-read-as"), "no choice");
    assert!(!present(cx, &opened, "code-rendered"), "no document");
    assert!(present(cx, &opened, ("diff-row", 0usize)), "the lines");
    says(cx, &opened, markdown("notes.markdown"));
    assert!(rendered(cx, &opened) && present(cx, &opened, "code-read-as"), "by extension, either spelling");
}

/// Task 57: while the stage edits the file the document stands over the editor, and Source is the editor.
#[gpui_kit::test]
fn an_edited_markdown_file_is_read_rendered_and_source_is_the_editor(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { editing: true, ..markdown("docs/guide.md") });
    assert!(rendered(cx, &opened) && !present(cx, &opened, "code-editor"));
    click_id(cx, &opened, "code-view-source");
    settle(cx, &opened);
    assert!(present(cx, &opened, "code-editor") && !present(cx, &opened, "code-rendered"));
}

/// Task 57: a link to a file of the checkout reads it as the file tree does, from the folder of this file.
#[gpui_kit::test]
fn a_relative_link_sends_the_call_that_reads_that_file(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("docs/guide.md"));
    settle(cx, &opened);
    cx.update(|cx| last().read(cx).follow("other.md#top", cx));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"docs/other.md\" } } }"]);
    cx.update(|cx| last().read(cx).follow("../README.md", cx));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"README.md\" } } }"]);
}

/// Task 57: a web link goes to the browser and reads no file; an anchor, a mail address and a path out
/// of the checkout do nothing.
#[gpui_kit::test]
fn a_web_link_opens_the_browser_and_other_links_send_nothing(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("docs/guide.md"));
    settle(cx, &opened);
    cx.update(|cx| last().read(cx).follow("https://example.test/x", cx));
    assert_eq!(sent(&opened), ["OpenUrl { url: \"https://example.test/x\" }"]);
    for href in ["#steps", "mailto:a@b.test", "../../outside.md"] {
        cx.update(|cx| last().read(cx).follow(href, cx));
    }
    assert_eq!(sent(&opened), Vec::<String>::new());
}

/// An edited README as the stage holds it once the daemon has sent its text: the editor's field has the keys.
fn editing_readme(cx: &mut TestAppContext) -> Opened {
    let opened = with(cx, CodePage { editing: true, ..markdown("docs/guide.md") });
    hear(cx, &opened, vec![Event::Edit(EditEvent::Opened { checkout: 100, path: "docs/guide.md".into(), version: 0, text: TEXT.into() })]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

/// Task 57: the editor is not drawn while the document stands over it, so its field must not keep the
/// keys: the stage's keys work as they do on a file only read.
#[gpui_kit::test]
fn the_stage_has_its_keys_while_an_edited_file_is_read_rendered(cx: &mut TestAppContext) {
    let opened = editing_readme(cx);
    assert!(present(cx, &opened, "code-rendered") && !present(cx, &opened, "code-editor"));
    press(cx, &opened, "j");
    press(cx, &opened, "o");
    press(cx, &opened, "escape");
    assert_eq!(
        sent(&opened),
        ["Code { gesture: Action { action: Next } }", "Code { gesture: Action { action: ToggleOutline } }", "Code { gesture: Action { action: CloseLayer } }"]
    );
}

/// Task 57: choosing Source gives the keys back to the editor, so a letter is typed and is not the stage's.
#[gpui_kit::test]
fn choosing_source_gives_the_keys_back_to_the_editor(cx: &mut TestAppContext) {
    let opened = editing_readme(cx);
    click_id(cx, &opened, "code-view-source");
    settle(cx, &opened);
    assert!(present(cx, &opened, "code-editor"));
    type_text(cx, &opened, "j");
    let calls = sent(&opened);
    assert!(calls.iter().all(|call| !call.contains("Action")), "typed into the text, not a key of the stage: {calls:?}");
}

/// Task 57: a comment is on the text, so an open one shows the file as Source whatever was chosen.
#[gpui_kit::test]
fn an_open_comment_shows_the_file_as_source(cx: &mut TestAppContext) {
    let mut page = markdown("docs/guide.md");
    page.lines[2].picked = true;
    page.comment = Some(CommentPage { about: "lines 3 - 3, docs/guide.md".into(), recipients: Vec::new(), to: None });
    let opened = with(cx, page);
    settle(cx, &opened);
    assert!(present(cx, &opened, "review-comment") && present(cx, &opened, ("diff-row", 0usize)), "the lines and the box under them");
    assert!(!present(cx, &opened, "code-rendered"), "no document over the comment");
    assert_eq!(is_selected(cx, &opened, "code-view-source"), Some(true));
    assert_eq!(is_selected(cx, &opened, "code-view-rendered"), Some(false));
}

/// Task 57: a press on a link drawn in the document reaches `follow`, so the kit's link press is wired.
/// A link has no element of its own, so the pointer is walked over the document until one answers.
#[gpui_kit::test]
fn a_press_on_a_link_in_the_document_opens_the_file_it_names(cx: &mut TestAppContext) {
    let opened = with(cx, markdown("docs/guide.md"));
    settle(cx, &opened);
    let area = bounds_of(cx, &opened, "code-rendered");
    let mut y = area.origin.y + px(60.0);
    while y < area.origin.y + px(200.0) && opened.calls.borrow().is_empty() {
        let mut x = area.origin.x + px(8.0);
        while x < area.origin.x + area.size.width.min(px(420.0)) && opened.calls.borrow().is_empty() {
            let at = point(x, y);
            cx.update_window(opened.handle, |_, window, cx| {
                window.pointer_move(at, None, cx);
                window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
                window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
            })
            .unwrap();
            x += px(8.0);
        }
        y += px(6.0);
    }
    cx.run_until_parked();
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"docs/other.md\" } } }"]);
}
