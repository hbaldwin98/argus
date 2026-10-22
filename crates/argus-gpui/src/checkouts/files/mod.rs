//! The Files panel beside the Checkouts table: one checkout's folder as it is on disk, a level at a
//! time (`ck/files.ts`). Core reads the folders and flattens the tree (`checkouts::files_page`),
//! and what is marked, folded and spelled is its too (`checkouts::marks`, `fold`, `query`); this
//! draws the page it is sent and says what is pressed.
//!
//! Nothing is predicted: a click sends its gesture and the rows change when the session's next
//! table says so. What the window keeps is the focus a redraw must not lose: the panel takes the
//! keys when it opens on another checkout, and the tree keeps them across the tables its own keys
//! bring back. A `#` typed in the filter is a search of the text, run on Enter, and is kept in
//! the field though Rust holds no filter for it.
//!
//! The panel's own keys are the `Files` layer of the key walk (`keys`); the filter's are the
//! field's (`tools`).

mod body;
mod crumb;
mod foot;
mod head;
pub mod keys;
pub mod menu;
mod tools;

use std::rc::Rc;

use argus_desktop_core::checkouts::files::FilesGesture;
use argus_desktop_core::checkouts::files_page::FilesPage;
use argus_desktop_core::checkouts::query::Query;
use argus_desktop_core::checkouts::table::Checkouts;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::component::v_flex;
use gpui_kit::{prelude::*, App, Context, Entity, FocusHandle, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::Registered;
use crate::keys::layer::Layers;
use crate::kit::field::Field;
use crate::theme::Tokens;

/// The filter's placeholder and its name to a reader.
pub const FILTER: &str = "Filter files, or # to search text";

pub struct FilesPanel {
    heard: Entity<Heard>,
    caller: Caller,
    filter: Field,
    /// What the session was last told the filter is, so a field put back to what the page says
    /// sends nothing, and a `#` that filters nothing is sent as the empty filter once.
    known: SharedString,
    /// Where the keys rest while the panel is up: the tree, which the page's `#cfiles-tree` is.
    tree: FocusHandle,
    scroll: ScrollHandle,
    /// Whether the ignored fold is open: the window's memory, since a redraw does not forget it.
    unfolded: bool,
    /// The checkout whose files were last drawn: a panel opening on another takes the keys, one
    /// redrawn for a listing does not.
    drawn_for: Option<u64>,
    /// The selected row when the list was last scrolled to it.
    lit: Option<String>,
    _following: Subscription,
    _typing: Subscription,
    _field: Registered,
}

/// The panel in `table`, while it is open.
fn files_of(table: &Option<Rc<Checkouts>>) -> Option<&FilesPage> {
    table.as_ref().and_then(|table| table.files.as_ref())
}

impl FilesPanel {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = Field::new(FILTER, window, cx);
        let tree = cx.focus_handle();
        let (typing, field) = tools::wire(&filter, &tree, window, cx);
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| {
            let table = heard.read(cx).checkouts.clone();
            this.follow(&table, window, cx);
        });
        Layers::of(cx).register(Layer::Files, Rc::new(keys::PanelKeys { panel: cx.weak_entity() }));
        let mut panel = Self {
            heard: heard.clone(),
            caller,
            filter,
            known: SharedString::default(),
            tree,
            scroll: ScrollHandle::new(),
            unfolded: false,
            drawn_for: None,
            lit: None,
            _following: following,
            _typing: typing,
            _field: field,
        };
        let table = heard.read(cx).checkouts.clone();
        panel.follow(&table, window, cx);
        panel
    }

    /// Sends a gesture of the panel; a held window (the daemon gone) sends nothing.
    pub(crate) fn ask(&self, gesture: FilesGesture, cx: &App) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Files { gesture });
        }
    }

    /// Whether the keys are in the panel's tree rather than a field (`filesFocused`).
    pub(crate) fn tree_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.tree.contains_focused(window, cx) && !self.filter.typing(window, cx) && files_of(&self.heard.read(cx).checkouts).is_some()
    }

    /// `/`: the filter takes the keys.
    pub(crate) fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        self.filter.focus(window, cx);
    }

    /// Follows the table: the field says what the page does unless it is being typed in or holds a
    /// search, the keys come to a panel opened on another checkout, and the tree brings the
    /// selected row into view when it moved.
    fn follow(&mut self, table: &Option<Rc<Checkouts>>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(files) = files_of(table) else {
            self.drawn_for = None;
            return;
        };
        // What was typed after a `#` is not Rust's to give back: it holds no filter for it.
        let searching = matches!(Query::of(&self.filter.text(cx)), Query::Text(_)) && files.filter.is_empty();
        if !searching {
            self.known = SharedString::from(files.filter.clone());
            self.filter.show(&files.filter, window, cx);
        }
        if self.drawn_for != Some(files.checkout) {
            self.drawn_for = Some(files.checkout);
            // After this change has been heard by everything that hears it: the stage takes the
            // keys as it appears, and the panel opening with it must have them last.
            let tree = self.tree.clone();
            window.defer(cx, move |window, cx| window.focus(&tree, cx));
        }
        if self.lit != files.selected {
            self.lit = files.selected.clone();
            if let Some(at) = body::child_of(files, files.selected.as_deref(), self.unfolded) {
                self.scroll.scroll_to_item(at);
            }
        }
        cx.notify();
    }
}

impl Render for FilesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let table = self.heard.read(cx).checkouts.clone();
        let Some(files) = files_of(&table) else { return gpui_kit::div().into_any_element() };
        v_flex()
            .id("files-panel")
            .test_support()
            .aria_label(SharedString::from(format!("Files of {}", files.name)))
            .size_full()
            .min_h_0()
            .bg(tokens.bg_chrome)
            .border_l_1()
            .border_color(tokens.line)
            .child(self.head(files, &tokens, cx))
            .child(self.tools(&tokens))
            .child(self.crumb(files, &tokens, cx))
            .child(self.tree(files, window, &tokens, cx))
            .child(self.foot(files, &tokens, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
