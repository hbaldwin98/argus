//! The editor in the whole window: a file opened, written, changed by an agent and left, and what
//! the bar says at each step. The daemon's words are sent as events and its asks are the calls
//! the window collected, so nothing here asks a real session. Rules cited are
//! `gpui/parity/editor.md`'s (`EDITOR-...`).

use std::cell::RefCell;
use std::sync::Arc;

use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::code::grammar::load::Loader;
use argus_desktop_core::code::grammar::net::Http;
use argus_desktop_core::code::grammar::store::Store;
use argus_desktop_core::code::stage::CodePage;
use argus_desktop_core::connection::page::Event;
use gpui_kit::component::input::EditorState;
use gpui_kit::{AppContext as _, Entity, TestAppContext, WeakEntity};

use super::CodeEditor;
pub(super) use crate::code::file::tests::{reading, settle};
pub(super) use crate::code::stage::tests::{says, sent, with};
pub(super) use crate::checkouts::stage::tests::{label, present};
pub(super) use crate::keys::root::tests::{hear, Opened};

mod clash;
mod conventions;
mod grammar;
mod gutter;
mod keys;
mod open;
mod sync;
mod vim;
mod vim_look;

type Net = Arc<dyn Http + Send + Sync>;

thread_local! {
    static EDITOR: RefCell<Option<WeakEntity<CodeEditor>>> = const { RefCell::new(None) };
    static GRAMMARS: RefCell<Option<(Arc<Loader>, Net)>> = const { RefCell::new(None) };
}

/// A network that answers nothing: a grammar search finds nothing, and a pinned one is offered.
struct Offline;

impl Http for Offline {
    fn get(&self, url: &str, _: u64) -> Result<Vec<u8>, String> {
        Err(format!("{url} answered 404"))
    }
}

/// Where the window's editor keeps and finds its grammars: what a test chose, else an empty store
/// and no network.
pub(super) fn grammars() -> (Arc<Loader>, Net) {
    GRAMMARS.with(|slot| slot.borrow().clone()).unwrap_or_else(|| (Arc::new(Loader::new(Store::new(std::env::temp_dir().join(format!("argus-gpui-no-grammars-{}", std::process::id()))))), Arc::new(Offline)))
}

/// Makes the next editor made keep its grammars in `loader` and fetch them through `net`.
pub(super) fn with_grammars(loader: Arc<Loader>, net: Net) {
    GRAMMARS.with(|slot| *slot.borrow_mut() = Some((loader, net)));
}

/// The window's editor entity, recorded as it is made.
pub(super) fn remember(editor: WeakEntity<CodeEditor>) {
    EDITOR.with(|slot| *slot.borrow_mut() = Some(editor));
}

pub(super) fn editor() -> Entity<CodeEditor> {
    EDITOR.with(|slot| slot.borrow().as_ref().and_then(WeakEntity::upgrade)).expect("the window has an editor")
}

/// `src/lib.rs` edited, `lines` lines long, the cursor on `at`: the page's word that the editor is up.
pub(super) fn editing(lines: u32, at: u32) -> CodePage {
    CodePage { editing: true, ..reading(lines, at) }
}

pub(super) fn other(path: &str) -> CodePage {
    CodePage { path: Some(path.into()), ..editing(3, 1) }
}

/// The daemon's answer to opening `path` in checkout 100.
pub(super) fn opened_with(path: &str, text: &str) -> Event {
    Event::Edit(EditEvent::Opened { checkout: 100, path: path.into(), version: 0, text: text.into() })
}

pub(super) fn heard_edit(cx: &mut TestAppContext, opened: &Opened, event: EditEvent) {
    hear(cx, opened, vec![Event::Edit(event)]);
}

/// The text of the file shown, as the editor holds it.
pub(super) fn text(cx: &mut TestAppContext) -> Option<String> {
    let editor = editor();
    cx.update(|cx| editor.read(cx).text(cx))
}

/// The state of the file shown, for a test that types in it.
pub(super) fn state(cx: &mut TestAppContext) -> Entity<EditorState> {
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        editor.sessions.get(editor.shown.as_ref()?)?.state.clone()
    })
    .expect("a file is shown")
}

/// What the bar says.
pub(super) fn bar(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    label(cx, opened, "editor-state")
}

/// The lines, from one, the added marks (or the removed marks) are on.
pub(super) fn marked(cx: &mut TestAppContext, added: bool) -> Vec<usize> {
    use gpui_kit::component::input::RopeExt as _;
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        let session = editor.sessions.get(editor.shown.as_ref().unwrap()).unwrap();
        let marks = session.marks.as_ref().unwrap();
        let text = session.state.as_ref().unwrap().read(cx).text().clone();
        let collection = if added { &marks.added } else { &marks.removed };
        let mut lines: Vec<usize> = collection.get_ranges(cx).into_iter().map(|range| text.offset_to_point(range.start).row + 1).collect();
        lines.sort_unstable();
        lines
    })
}

/// A connected window on the Code stage that has heard `page`, its calls kept: the first ask of a
/// file is among them.
pub(super) fn asking(cx: &mut TestAppContext, page: CodePage) -> Opened {
    use crate::code::stage::tests::code_rail;
    use crate::keys::root::tests::open;
    let opened = open(cx);
    hear(cx, &opened, vec![crate::checkouts::stage::tests::connected(), Event::Rail(Box::new(code_rail())), Event::Code(Box::new(page))]);
    opened
}
