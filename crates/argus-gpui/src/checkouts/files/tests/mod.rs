//! The Files panel in the whole window: what it draws of the page core sends, what each press and
//! key sends, and that nothing changes until the session's next table says so. Rules cited are
//! `gpui/parity/checkouts-code-views.md`'s (`FILES-n`).
//!
//! The window is the shell over a caller that collects its calls (`keys::root::tests`), and the
//! session's table is sent as an event, so nothing here asks a real session.

use argus_desktop_core::checkouts::files_page::{FileMark, FileRow, FilesFooter, FilesPage, Mode};
use argus_desktop_core::checkouts::table::Checkouts;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, InputEvent, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

use crate::checkouts::stage::tests::{heard, on, table};
use crate::keys::root::tests::{drain, hear, Opened};

mod keys;
mod panel;
mod tree;

/// A row of the tree.
pub(super) fn row(path: &str, dir: bool, open: bool, mark: Option<FileMark>) -> FileRow {
    let depth = path.matches('/').count();
    FileRow { path: path.into(), name: path.rsplit('/').next().unwrap().into(), depth, dir, open, mark }
}

/// The panel of `desktop` as the page's screenshot has it: `crates` opened to a changed file, a
/// plain root file, and two ignored folders, one listed before the sources.
pub(super) fn page() -> FilesPage {
    FilesPage {
        checkout: 101,
        name: "desktop".into(),
        root: "C:\\src\\desktop".into(),
        separator: "\\".into(),
        mode: Mode::All,
        changed: 6,
        crumb: vec!["desktop".into(), "crates".into(), "argus-desktop".into()],
        rows: vec![
            row(".argus", true, false, Some(FileMark::Ignored)),
            row("crates", true, true, Some(FileMark::Changed)),
            row("crates/argus-desktop", true, true, Some(FileMark::Changed)),
            row("crates/argus-desktop/style.css", false, false, Some(FileMark::Changed)),
            row("crates/notes.txt", false, false, Some(FileMark::Untracked)),
            row("Cargo.toml", false, false, None),
            row("target", true, false, Some(FileMark::Ignored)),
        ],
        selected: Some("crates/argus-desktop/style.css".into()),
        footer: Some(FilesFooter { name: "style.css".into(), dir: false, detail: "212 KB \u{b7} changed 4m ago".into() }),
        reveal: "Show in Explorer".into(),
        can_reveal: true,
        ..FilesPage::default()
    }
}

/// A connected window on the Checkouts stage with the panel of `files` open, its calls cleared.
pub(super) fn with(cx: &mut TestAppContext, files: FilesPage) -> Opened {
    on(cx, Checkouts { files: Some(files), ..table("c101") })
}

/// The table again with `files` in it, as the session says it after something changed.
pub(super) fn says(cx: &mut TestAppContext, opened: &Opened, files: FilesPage) {
    hear(cx, opened, vec![heard(Checkouts { files: Some(files), ..table("c101") })]);
}

/// What was sent since last asked.
pub(super) fn sent(opened: &Opened) -> Vec<String> {
    drain(opened)
}

/// A right-click on the element `id`.
pub(super) fn right_click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.right_click(id, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Chooses the item of the open menu named `label`.
pub(super) fn choose(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let item = window.elements().into_iter().find(|element| {
                element.label() == Some(label) && element.path().iter().any(|id| matches!(id, gpui_kit::ElementId::Name(name) if name.as_ref() == "popup-menu"))
            });
            item.unwrap_or_else(|| panic!("the open menu has no {label}")).bounds().center()
        })
        .unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// Whether the element `id` says it is selected.
pub(super) fn is_selected(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

/// Whether the element `id` says it is expanded.
pub(super) fn is_expanded(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).expanded()).unwrap()
}

/// Presses the element `id` `times` times in a row, each counted as the next click of a multiple click.
pub(super) fn click_times(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>, times: usize) {
    let id = id.into();
    let at = cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
    for count in 1..=times {
        cx.update_window(opened.handle, |_, window, cx| {
            window.dispatch_event(InputEvent::to_platform_input(MouseDownEvent { button: MouseButton::Left, position: at, click_count: count, ..Default::default() }), cx);
            window.dispatch_event(InputEvent::to_platform_input(MouseUpEvent { button: MouseButton::Left, position: at, click_count: count, ..Default::default() }), cx);
        })
        .unwrap();
    }
    cx.run_until_parked();
}
