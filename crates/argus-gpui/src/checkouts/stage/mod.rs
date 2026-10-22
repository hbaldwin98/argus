//! The Checkouts stage: the chosen repository's checkouts as a table, the branches without one
//! folded under it, a key bar under the table, and a column beside it about the selected row
//! (`ck/stage.ts`). Drawn where the Workspace's pane is while the session's stage is Checkouts.
//!
//! Core builds every row and every sentence (`argus-desktop-core::checkouts`), and the session
//! holds what is selected, filtered and asked: this draws what it says and sends what is pressed,
//! so a refused action leaves the table showing what is true. Before the first table there are
//! five loading bars; with no repository chosen a card says so; with no rows a line says why. The
//! stage's question is an overlay while the table holds one (`dialogs/question`).
//!
//! The strip's words and tools are this stage's offer to the header (`header/slots`), and its keys
//! are the Checkouts layer of the key walk (`keys`).

pub mod keys;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::checkouts::words::{body, Body};
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, relative, Bounds, Context, Entity, FocusHandle, Pixels, SharedString, Subscription, TestSupportExt as _, Window};

use super::{bar, dialogs::question, empty, files::FilesPanel, side, strip, table::Ctx, table::Table};
use crate::chrome::header::slots::{Offer, Slots};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;
use crate::kit::area;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;
use strip::StripTools;

pub struct CheckoutsStage {
    heard: Entity<Heard>,
    caller: Caller,
    table: Entity<Table>,
    /// The Files panel, drawn beside the table while the table's page says it is open.
    files: Entity<FilesPanel>,
    /// The area the stage was laid out in, which says whether the panel floats over the table
    /// (`@container (max-width: 900px)`): the stage is not the window.
    area: Rc<Cell<Bounds<Pixels>>>,
    tools: Entity<StripTools>,
    /// Where the keys rest while no field holds them: the stage itself.
    focus: FocusHandle,
    /// Whether the pointer is on `New pane`, whose press toggles the start menu a press elsewhere
    /// shuts: the button must not be the elsewhere.
    new_pane_hover: Rc<Cell<bool>>,
    /// Whether the stage was up at the last update, to take the keys once as it appears: the pane
    /// that had them is not drawn under it, and a focus that is not drawn still swallows keys.
    was_up: bool,
    _following: Subscription,
}

impl CheckoutsStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let table = cx.new(|_| Table::new(heard.clone(), caller.clone()));
        let files = cx.new(|cx| FilesPanel::new(heard.clone(), caller.clone(), window, cx));
        let tools = cx.new(|cx| StripTools::new(heard.clone(), caller.clone(), focus.clone(), window, cx));
        // The strip says the counts and carries the filter and the buttons while this stage is up.
        let (saying, offered) = (heard.clone(), tools.clone());
        Slots::of(cx).offer(
            Stage::Checkouts,
            Offer::default().says(move |cx| strip::says(saying.read(cx).checkouts.as_deref())).tools(move |_, _| offered.clone().into_any_element()),
        );
        Layers::of(cx).register(Layer::Checkouts, Rc::new(keys::StageKeys { stage: cx.weak_entity() }));
        // The question is up exactly while the table holds one.
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| {
            let confirm = heard.read(cx).checkouts.as_ref().and_then(|table| table.confirm.clone());
            let up = heard.read(cx).rail.stage == Stage::Checkouts;
            if up && !this.was_up {
                window.focus(&this.focus, cx);
            }
            this.was_up = up;
            question::follow(confirm.as_ref(), &this.caller, window, cx);
            cx.notify();
        });
        Self { heard, caller, table, files, area: Rc::default(), tools, focus, new_pane_hover: Rc::default(), was_up: false, _following: following }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }
}

impl Render for CheckoutsStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let table = self.heard.read(cx).checkouts.clone();
        let wide = css(window.viewport_size().width, window.rem_size()) > tokens.ck_side_hides_below;
        let root = div().id("checkouts").test_support().key_context("Checkouts").track_focus(&self.focus).size_full().text_color(tokens.fg_muted);
        match body(table.as_deref()) {
            Body::Loading => root.child(empty::loading(&tokens)).into_any_element(),
            Body::NoRepository => root.child(empty::none_chosen(&tokens)).into_any_element(),
            shown => {
                let table = table.expect("a body other than loading has a table");
                let main = match &shown {
                    Body::Nothing(words) => empty::nothing(words, &tokens).into_any_element(),
                    _ => self.table.clone().into_any_element(),
                };
                let ctx = Ctx { heard: self.heard.clone(), caller: self.caller.clone() };
                // The Files panel takes the side column's place; a narrow window has no side column
                // and the panel floats over the table when the stage is narrow.
                let stage_width = match self.area.get().size.width {
                    width if width > gpui_kit::px(0.0) => css(width, window.rem_size()),
                    _ => css(window.viewport_size().width, window.rem_size()),
                };
                let overlay = stage_width <= tokens.files_overlay_below;
                let column = match (&table.files, table.selected_row()) {
                    (Some(files), _) => Some(files_column(self.files.clone(), &files.name, overlay, &tokens).into_any_element()),
                    _ if !wide => None,
                    (None, Some(row)) => Some(side::column(&table, row, &ctx, &self.new_pane_hover, &tokens, cx).into_any_element()),
                    (None, None) => None,
                };
                root.child(area::measure(self.area.clone()))
                    .child(
                        h_flex()
                            .relative()
                            .size_full()
                            .child(v_flex().flex_1().min_w_0().h_full().child(div().flex_1().min_h_0().child(main)).child(bar::bar(table.files_offered, &tokens)))
                            .children(column),
                    )
                    .into_any_element()
            }
        }
    }
}

/// The place of the Files panel: `min(440px, 42%)` wide beside the table, or over it at the right
/// edge with a shadow while the stage is narrow, `min(440px, 100%)` wide (`.cfiles`).
fn files_column(panel: Entity<FilesPanel>, name: &str, overlay: bool, tokens: &Tokens) -> impl IntoElement {
    div()
        .id("checkouts-files")
        .test_support()
        .aria_label(SharedString::from(format!("Files of {name}")))
        .h_full()
        .max_w(rem(tokens.files_w))
        .when(!overlay, |column| column.flex_shrink_0().w(relative(tokens.files_share)))
        .when(overlay, |column| column.absolute().top_0().right_0().bottom_0().w(relative(1.0)).shadow(tokens.shadow_pop()))
        .child(panel)
}

#[cfg(test)]
pub(crate) mod tests;
