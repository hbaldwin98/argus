//! What fills the panel between its crumb and its foot (`body`, `treeRow`, `ck/files.ts`): why a
//! folder could not be read, loading bars until the first listing, a line when there is nothing to
//! list, else the tree: a row for each entry core listed, the ignored top-level ones held in one
//! fold at the foot.
//!
//! A click sends `Click` and a second one on a file opens it in Code; a right-click picks the row
//! without opening it and shows its menu; the selected file carries two small buttons that open it.
//! Whether a row is selected, open or marked is core's word, so a click changes nothing here.

use argus_desktop_core::checkouts::files::{FilesGesture, OpenIn};
use argus_desktop_core::checkouts::files_page::{FileMark, FileRow, FilesPage, Mode};
use argus_desktop_core::checkouts::fold::{fold_ignored, Folded};
use argus_desktop_core::checkouts::marks;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{div, prelude::*, AnyElement, App, Context, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use super::menu::{self, Way};
use super::FilesPanel;
use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::menu::popup;
use crate::kit::tip::tip;
use crate::kit::tree::{self, Row};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Which child of the tree's scroll the row at `path` is in, to bring it into view: the rows that
/// stay come first, then the fold, then (while it is open) one child holding every row it holds.
/// A selected row folded away is in no child until the fold is open.
pub(super) fn child_of(files: &FilesPage, path: Option<&str>, unfolded: bool) -> Option<usize> {
    let at = files.rows.iter().position(|row| Some(row.path.as_str()) == path)?;
    let folded = fold_ignored(&files.rows);
    if let Some(shown) = folded.shown.iter().position(|&shown| shown == at) {
        return Some(shown);
    }
    unfolded.then_some(folded.shown.len() + 1)
}

/// The line that stands in for rows when there are none.
fn nothing(files: &FilesPage) -> String {
    if !files.filter.is_empty() {
        format!("Nothing named \u{201c}{}\u{201d} in the folders read.", files.filter)
    } else if files.mode == Mode::Changed {
        "Nothing changed in the folders read.".to_owned()
    } else {
        "This folder is empty.".to_owned()
    }
}

/// `.cf-note`: a line of the panel's own, quiet or, warning, tinted under a glyph.
fn note(id: &'static str, text: String, warn: bool, tokens: &Tokens) -> impl IntoElement {
    let words = SharedString::from(text);
    h_flex()
        .id(id)
        .test_support()
        .aria_label(words.clone())
        .flex_shrink_0()
        .items_start()
        .gap(rem(tokens.sp_8))
        .mx(rem(tokens.sp_8))
        .my(rem(tokens.sp_4))
        .p(rem(tokens.sp_8))
        .typed(tokens.type_small)
        .text_color(if warn { tokens.fg } else { tokens.fg_muted })
        .when(warn, |note| note.rounded(rem(tokens.r_md)).bg(tokens.attention.opacity(tokens.share_callout)))
        .when(warn, |note| note.child(div().mt(rem(tokens.sp_2)).flex_shrink_0().text_color(tokens.attention).child(Glyph::Warn.at(tokens.icon_sm))))
        .child(div().min_w_0().child(words))
}

impl FilesPanel {
    pub(super) fn tree(&self, files: &FilesPage, window: &mut Window, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let keys = self.tree_has_keys(window, cx);
        let folded = fold_ignored(&files.rows);
        let body: Vec<AnyElement> = if let Some(error) = &files.error {
            vec![note("files-error", format!("Could not read the folder: {error}"), true, tokens).into_any_element()]
        } else if files.loading {
            vec![div().px(rem(tokens.sp_8)).child(ghosts("files-loading", &[0.5, 0.7, 0.4, 0.6, 0.55], tokens)).into_any_element()]
        } else if files.rows.is_empty() {
            vec![note("files-nothing", nothing(files), false, tokens).into_any_element()]
        } else {
            self.rows(files, &folded, keys, tokens, cx)
        };
        // `.cf-tree`: the scroll that holds the keys, 8px in from each side and under.
        let tree = v_flex()
            .id("files-tree")
            .role(Role::Tree)
            .test_support()
            .aria_label(SharedString::from(format!("Files of {}", files.name)))
            .track_focus(&self.tree)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(rem(tokens.sp_8))
            .pb(rem(tokens.sp_8))
            .gap(rem(tokens.hairline))
            .children(body);
        v_flex()
            .flex_1()
            .min_h_0()
            .when(files.truncated, |column| {
                column.child(note("files-truncated", "A folder is too large to list whole; the first part is shown.".into(), true, tokens))
            })
            .child(tree)
            .into_any_element()
    }

    fn rows(&self, files: &FilesPage, folded: &Folded, keys: bool, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let me = cx.weak_entity();
        let toggle = move |_: &mut Window, cx: &mut App| {
            me.update(cx, |panel, cx| {
                panel.unfolded = !panel.unfolded;
                cx.notify();
            })
            .ok();
        };
        let unfolded = self.unfolded;
        let mut make = |at: usize| self.row(files, at, keys, tokens, cx);
        tree::rows(folded, unfolded, "files-fold", &mut make, toggle, tokens)
    }

    fn row(&self, files: &FilesPage, at: usize, keys: bool, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let row = &files.rows[at];
        let on = files.selected.as_deref() == Some(row.path.as_str());
        let entry = Row {
            path: &row.path,
            name: &row.name,
            depth: row.depth,
            dir: row.dir,
            open: row.open,
            on,
            ignored: row.mark == Some(FileMark::Ignored),
            ringed: keys,
            mark: marks::of_file(row),
        };
        let extra = (on && !row.dir).then(|| self.buttons(row, tokens, cx));
        let (click, select, dir) = (row.path.clone(), row.path.clone(), row.dir);
        let items = menu::items(files, row);
        let (heard, caller, held) = (self.heard.clone(), self.caller.clone(), self.heard.clone());
        let theme = *tokens;
        tree::row(("files-row", at), &entry, extra, tokens)
            .on_click(cx.listener(move |this, event: &gpui_kit::ClickEvent, _, cx| {
                this.ask(FilesGesture::Click { path: click.clone() }, cx);
                if event.click_count() >= 2 && !dir {
                    this.ask(FilesGesture::Open { path: click.clone(), how: OpenIn::Code }, cx);
                }
            }))
            // A right-click picks the row, without opening it, before its menu opens.
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| this.ask(FilesGesture::Select { path: select.clone() }, cx)))
            .context_menu(move |menu, window, cx| {
                let (heard, caller, held) = (heard.clone(), caller.clone(), held.clone());
                popup::fill(
                    menu,
                    &items,
                    theme,
                    move |cx: &App| held.read(cx).held(),
                    move |way: &Way, _, cx| {
                        // Against the page as it is when the choice lands: the rows may have moved.
                        let table = heard.read(cx).checkouts.clone();
                        if let Some(files) = table.as_ref().and_then(|table| table.files.as_ref()) {
                            for call in menu::calls(way, files) {
                                caller.call(call);
                            }
                        }
                    },
                    window,
                    cx,
                )
            })
            .into_any_element()
    }

    /// The two small buttons of the selected file: neither selects or toggles the row.
    fn buttons(&self, row: &FileRow, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let button = |id: &'static str, glyph: Glyph, label: String, tip_words: &'static str, how: OpenIn, cx: &mut Context<Self>| {
            let path = row.path.clone();
            icon_button(id, label, false, tokens)
                .w(rem(tokens.icon_lg))
                .h(rem(tokens.icon_lg))
                .tooltip(tip(tip_words))
                .child(glyph.at(tokens.icon_sm))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.ask(FilesGesture::Open { path: path.clone(), how }, cx);
                }))
        };
        h_flex()
            .id("files-row-buttons")
            .test_support()
            .flex_shrink_0()
            .gap(rem(tokens.sp_2))
            .child(button("files-open-code", Glyph::Code, format!("Open {} in Code", row.name), "Open in Code", OpenIn::Code, cx))
            .child(button("files-open-editor", Glyph::External, format!("Open {} in the editor", row.name), "Open in the editor", OpenIn::Editor, cx))
            .into_any_element()
    }
}
