//! An extension's view: the one element that draws it, docked, floating or as the stage's page.
//!
//! A view is the agent table's renderer plus a selected row and actions (`table`), or, for a
//! console view, a terminal on the screen the session gave it under a strip (`console`). Its header
//! as a page is `head`'s; docked, the dock's own head or tab strip carries the title and tools
//! (`layout/tools`). Moving between a dock and the page is the same entity drawn in another place,
//! so the body's focus and a console's terminal keep their place (EXTVIEW-16, -25, -35).
//!
//! Everything the view says is the session's last `ExtTable` for its panel. What the window
//! alone keeps is the hovered row, the sort, which goes when the view is published again, and
//! where the keys are: the body takes them as a page appears, and keeps them across what the
//! session says afterwards. Nothing is predicted: a gesture is a `Call`, and the table changes
//! when the next snapshot says so.

pub mod actions;
pub mod console;
pub mod head;
pub mod table;

use argus_desktop_core::extensions::views::ExtTable;
use argus_desktop_core::layout::docks::Panel;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::blocks::kept::Widths;

pub struct ExtensionView {
    pub(crate) heard: Entity<Heard>,
    pub(crate) caller: Caller,
    pub(crate) panel: Panel,
    /// Where the keys rest while the view's body has them: a click in the body gives it them.
    pub(crate) focus: FocusHandle,
    pub(crate) widths: Widths,
    /// The sort a header press made: the column and whether it runs ascending.
    pub(crate) sort: Option<(usize, bool)>,
    /// The row the pointer is on.
    pub(crate) hovered: Option<String>,
    /// What the view last said, so a view published again puts the sort away.
    seen: Option<ExtTable>,
    /// The page takes the keys once, as it appears: the pane that had them is not drawn under it.
    needs_focus: bool,
    pub(crate) console: console::Held,
    _observing: Subscription,
}

impl ExtensionView {
    pub fn new(heard: Entity<Heard>, caller: Caller, panel: Panel, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe_in(&heard, window, |this, _, window, cx| {
            this.follow(window, cx);
            cx.notify();
        });
        let mut this = Self {
            heard,
            caller,
            panel,
            focus: cx.focus_handle(),
            widths: Widths::default(),
            sort: None,
            hovered: None,
            seen: None,
            needs_focus: false,
            console: console::Held::default(),
            _observing: observing,
        };
        this.needs_focus = this.table(cx).is_some_and(|table| table.page);
        this.follow(window, cx);
        this
    }

    /// The view as the session last said it, while it is on screen.
    pub fn table(&self, cx: &App) -> Option<ExtTable> {
        self.heard.read(cx).extension_tables.iter().find(|table| table.panel == self.panel).cloned()
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Whether the body has the keys: a click in it gave them.
    pub fn has_keys(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.table(cx);
        if now != self.seen {
            // A view published again is drawn afresh: its sort and the row pointed at go.
            self.sort = None;
            self.hovered = None;
            if now.as_ref().is_some_and(|table| table.page) && !self.seen.as_ref().is_some_and(|table| table.page) {
                self.needs_focus = true;
            }
        }
        self.seen = now;
        self.sync_console(window, cx);
    }
}

impl Render for ExtensionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(view) = self.table(cx) else { return div().into_any_element() };
        if std::mem::take(&mut self.needs_focus) && view.console.is_none() {
            window.focus(&self.focus, cx);
        }
        let body = match &view.console {
            Some(console) => self.console_body(&view, console, &tokens, cx),
            None => self.body(&view, &tokens, cx),
        };
        v_flex()
            .id(SharedString::from(format!("ext-view-{}", view.panel)))
            .test_support()
            .aria_label(SharedString::from(view.title.clone()))
            .size_full()
            .min_h_0()
            .bg(tokens.bg_surface)
            .children(view.page.then(|| self.page_head(&view, &tokens, cx)))
            .child(body)
            .into_any_element()
    }
}

impl ExtensionView {
    /// The table in its scrolling body, which a click gives the keys to.
    fn body(&self, view: &ExtTable, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        div()
            .id(SharedString::from(format!("ext-body-{}", view.panel)))
            .test_support()
            .key_context("Extension")
            .track_focus(&self.focus)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(rem(tokens.sp_16))
            // The width of the box inside the padding is what the columns and the row bar give way to.
            .child(div().relative().w_full().min_w_0().child(self.widths.watch(format!("ext-{}", view.panel), cx.entity_id())).child(self.grid(view, tokens, cx)))
            .into_any_element()
    }
}

#[cfg(test)]
pub(crate) mod tests;
