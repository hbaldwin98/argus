//! The menus an open feature opens at a point (BOARD-FLOWS-8, BOARD-SEQ-8): the one that binds the lit
//! step to a decision, which a key opens under the lit row or arrow and a button under the pointer, and
//! the flow switcher on the expanded diagram's toolbar.

use argus_desktop_core::connection::page::Call;
use gpui_kit::{point, Context, Pixels, Point, Window};

use super::OpenFeature;
use crate::board::lenses::flows::bind;
use crate::kit::menu::at::At;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

impl OpenFeature {

    /// Opens the menu that binds the lit step to a decision (BOARD-FLOWS-8): at `at` when a button
    /// asked, else under the lit row (left and 40 across) or, expanded, under the lit arrow (20 across
    /// and 4 down); with nothing lit to measure, a third of the way down the window.
    pub fn open_bind(&mut self, at: Option<Point<Pixels>>, expanded: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = self.current(cx) else { return };
        let Some(flows) = features.open.as_ref().and_then(|view| view.flows.as_ref()) else { return };
        let Some(flow) = flows.flow.as_ref() else { return };
        let Some(items) = bind::items(flow, &flows.choices) else { return };
        let tokens = *Tokens::global(cx);
        let lit = if expanded { self.big.read(cx).lit_arrow() } else { self.lit_row.get() };
        let place = at.unwrap_or_else(|| {
            if lit.size.width > Pixels::ZERO {
                let (across, down) = if expanded { (tokens.bind_arrow_across, tokens.bind_arrow_down) } else { (tokens.bind_row_across, 0.0) };
                point(lit.origin.x + crate::theme::scale::pixels(across, window.rem_size()), lit.origin.y + lit.size.height + crate::theme::scale::pixels(down, window.rem_size()))
            } else {
                At::centred(window, &tokens)
            }
        });
        let (held, caller, acting) = (self.heard.clone(), self.caller.clone(), self.heard.clone());
        let menu = At::open(
            place,
            move |menu, window, cx| {
                popup::fill(menu, &items, tokens, move |cx| held.read(cx).held(), move |binds: &bind::Binds, _, cx| if !acting.read(cx).held() { caller.call(Call::EditFeature { edit: binds.edit() }) }, window, cx)
            },
            window,
            cx,
            |this: &mut Self, window, cx| {
                this.menu = None;
                window.focus(&this.focus, cx);
                cx.notify();
            },
        );
        self.menu = Some(menu);
        cx.notify();
    }

    /// Opens the flow switcher at `at`: each flow, the one drawn ticked; choosing one draws it from its
    /// first step (BOARD-SEQ-8).
    pub fn open_flow_menu(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(features) = self.current(cx) else { return };
        let Some(flows) = features.open.as_ref().and_then(|view| view.flows.as_ref()) else { return };
        let items: Vec<MenuItem<i64>> = flows.flows.iter().map(|flow| MenuItem::choice(flow.title.clone(), flow.id).marked(flow.on)).collect();
        let tokens = *Tokens::global(cx);
        let (held, caller, acting) = (self.heard.clone(), self.caller.clone(), self.heard.clone());
        let menu = At::open(
            at,
            move |menu, window, cx| {
                popup::fill(menu, &items, tokens, move |cx| held.read(cx).held(), move |id: &i64, _, cx| if !acting.read(cx).held() { caller.call(Call::SelectStep { diagram: *id, step: Some(0) }) }, window, cx)
            },
            window,
            cx,
            |this: &mut Self, window, cx| {
                this.menu = None;
                window.focus(&this.focus, cx);
                cx.notify();
            },
        );
        self.menu = Some(menu);
        cx.notify();
    }
}
