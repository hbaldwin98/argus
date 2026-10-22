//! The rows of an extension's table: the selected one lit with an edge, a click selecting and a
//! double click taking the view's default action on the row clicked, a status chip drawn as a dot
//! and a quiet word, a long text cell cut with its whole text for a tooltip, and the actions at the
//! row's end while the row is selected or pointed at, never inserted into the layout of any row
//! (EXTVIEW-3, -5, -6, -8).

use argus_desktop_core::extensions::row_actions::{is_menu, per_row, shows};
use argus_desktop_core::extensions::views::{ExtTable, ExtensionGesture};
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::blocks::table::width::LONG;
use argus_protocol::Value;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};

use super::Plan;
use crate::extensions::view::actions::{gesture, row_button, row_more};
use crate::extensions::view::ExtensionView;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::blocks::table::head::track;
use crate::views::chip::dotted;
use crate::views::ctx::Draw;
use crate::views::values::value;

impl ExtensionView {
    pub(super) fn grid_rows(&self, plan: &Plan, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if plan.shaped.rows.is_empty() {
            return div()
                .id(SharedString::from(format!("ext-empty-{}", plan.view.panel)))
                .test_support()
                .aria_label("No rows")
                .p(rem(tokens.sp_8))
                .text_color(tokens.fg_muted)
                .child("No rows")
                .into_any_element();
        }
        let clock = Clock::local();
        let saved = self.heard.read(cx).saved_views.clone();
        let draw = Draw { tokens, clock: &clock, caller: &self.caller, saved: &saved, view: None };
        let rows: Vec<AnyElement> = plan.order.iter().map(|&r| self.grid_row(plan, r, &draw, cx)).collect();
        v_flex().children(rows).into_any_element()
    }

    fn grid_row(&self, plan: &Plan, r: usize, draw: &Draw, cx: &mut Context<Self>) -> AnyElement {
        let tokens = draw.tokens;
        let view = plan.view;
        let row = &view.rows[r];
        let key = row.key.clone();
        let on = view.selected.as_deref() == Some(key.as_str());
        let id = SharedString::from(format!("ext-row-{}-{key}", view.panel));
        let cells: Vec<AnyElement> = plan
            .shaped
            .columns
            .iter()
            .enumerate()
            .filter(|(c, _)| plan.visible[*c])
            .map(|(c, column)| {
                let cell = plan.shaped.rows[r].get(c);
                let at = format!("{id}-c{c}");
                let long = match cell {
                    Some(Value::Text { text }) if text.chars().count() > LONG => Some(text.clone()),
                    _ => None,
                };
                // A chip is a dot and a quiet word in an extension's table, everywhere.
                let inner = cell.map(|cell| match cell {
                    Value::Chip { chip } => dotted(chip, tokens).into_any_element(),
                    other => value(other, &at, draw),
                });
                let body = div()
                    .id(SharedString::from(at))
                    .min_w_0()
                    .px(rem(tokens.sp_8))
                    .when(column.numeric, |cell| cell.flex().justify_end().w_full())
                    .children(inner);
                track(plan.widths[c])
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(match long {
                        Some(text) => body.truncate().tooltip(tip(text)).into_any_element(),
                        None => body.into_any_element(),
                    })
                    .into_any_element()
            })
            .collect();
        let shown = shows(&key, view.selected.as_deref(), self.hovered.as_deref());
        let (fill, selected) = (tokens.fill, tokens.bg_selected);
        let (hover, leaving, clicking) = (key.clone(), key.clone(), key.clone());
        let panel = view.panel.clone();
        let has_default = view.default.is_some();
        h_flex()
            .id(id)
            .test_support()
            .aria_selected(on)
            .aria_label(SharedString::from(key.clone()))
            .relative()
            .h(rem(tokens.table_row_h))
            .border_b_1()
            .border_color(tokens.line)
            .cursor_pointer()
            .when(on, |row| row.bg(selected))
            .when(!on, |row| row.hover(move |style| style.bg(fill)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let was = this.hovered.clone();
                if *hovered {
                    this.hovered = Some(hover.clone());
                } else if this.hovered.as_deref() == Some(leaving.as_str()) {
                    this.hovered = None;
                }
                if was != this.hovered {
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                this.caller.call(gesture(&panel, ExtensionGesture::Select { key: clicking.clone() }));
                // The row rides along, so the default is taken on the row clicked whether or not
                // its select has landed.
                if has_default && event.click_count() == 2 {
                    this.caller.call(gesture(&panel, ExtensionGesture::Default { row: clicking.clone() }));
                }
            }))
            .children(on.then(|| div().absolute().left_0().top_0().bottom_0().w(rem(tokens.sp_4 - tokens.hairline)).bg(tokens.mark_sel_edge)))
            .children(cells)
            .children(shown.then(|| self.bar(plan.view, &key, plan.across, on, tokens, cx)).flatten())
            .into_any_element()
    }

    /// The actions of a row, hung at its end over the row's own fill: text buttons while the view is
    /// wide, one "..." button once it is narrow, never both (EXTVIEW-7, -31).
    fn bar(&self, view: &ExtTable, row: &str, across: f32, on: bool, tokens: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        let actions = per_row(view);
        if actions.is_empty() {
            return None;
        }
        let ground = if on { tokens.bg_selected } else { tokens.fill };
        let buttons: Vec<AnyElement> = if is_menu(across) {
            vec![row_more(view, &actions, row, &self.caller, &self.heard, tokens, cx).into_any_element()]
        } else {
            actions.iter().map(|action| row_button(view, action, row, &self.caller, tokens).into_any_element()).collect()
        };
        Some(
            h_flex()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .items_center()
                .gap(rem(tokens.sp_4))
                .pl(rem(tokens.sp_16))
                .pr(rem(tokens.sp_8))
                .bg(ground)
                .children(buttons)
                .into_any_element(),
        )
    }
}
