//! A `<select>`'s list: headless Chrome draws none, so when the daemon says the person's press
//! opened one, the window draws it as the kit's menu under the select, its groups as headings, the
//! chosen option ticked and the disabled ones refused.
//!
//! Picking an option sends `Choose` with its index; Escape or a press outside sends `Choose` with
//! none. The kit runs a pick after the menu has gone, so a dismissal waits a turn and sends nothing
//! when a pick came. A list put away is not opened again for the same select: only a new one
//! opens, once the daemon has said none is open or another is.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use argus_protocol::{BrowserInput, BrowserSelect};
use gpui_kit::{AnyElement, Context, Window};

use super::BrowserPane;
use crate::kit::menu::at::At;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

pub const UNAVAILABLE: &str = "Not available";

/// The list shown, or the select whose list was put away, until the daemon moves on from it.
#[derive(Default)]
pub struct List {
    select: Option<BrowserSelect>,
    menu: Option<At>,
}

impl List {
    pub fn element(&self, window: &Window) -> Option<AnyElement> {
        self.menu.as_ref().map(|menu| menu.element(window))
    }
}

/// The select's options as a menu: a heading each time the group changes.
fn items(select: &BrowserSelect) -> Vec<MenuItem<u32>> {
    let mut items = Vec::new();
    let mut group: Option<&str> = None;
    for option in &select.options {
        if option.group.as_deref() != group {
            group = option.group.as_deref();
            if let Some(heading) = group {
                items.push(MenuItem::heading(heading));
            }
        }
        let choice = MenuItem::choice(option.label.clone(), option.index).marked(select.selected == Some(option.index));
        items.push(choice.refused_when(option.disabled, UNAVAILABLE));
    }
    items
}

impl BrowserPane {
    /// Opens the list the daemon says is open, or lets it go when the daemon says none is.
    pub(super) fn follow_select(&mut self, browser: Option<&PageBrowser>, window: &mut Window, cx: &mut Context<Self>) {
        let said = browser.and_then(|browser| browser.select.clone());
        if said == self.list.select {
            return;
        }
        self.list = List::default();
        let Some(select) = said else { return };
        // Under the select's bottom-left corner, as Chrome opens its list.
        let Some(position) = self.point_on_window(select.x, select.y + select.height, cx) else { return };
        let (caller, pane, tokens) = (self.caller.clone(), self.pane, *Tokens::global(cx));
        let chosen = Rc::new(Cell::new(false));
        let (picked, kept) = (chosen.clone(), chosen);
        let (choose, dismiss) = (caller.clone(), caller);
        let items = items(&select);
        let menu = At::open(
            position,
            move |menu, window, cx| {
                popup::fill(menu, &items, tokens, |_| false, move |index: &u32, _, _| {
                    picked.set(true);
                    choose.call(Call::BrowserInput { pane, input: BrowserInput::Choose { index: Some(*index) } });
                }, window, cx)
            },
            window,
            cx,
            move |this, window, cx| {
                // Keeps the select, so the same one is not opened again before the daemon moves on.
                this.list.menu = None;
                let (kept, dismiss) = (kept.clone(), dismiss.clone());
                window.defer(cx, move |_, _| {
                    if !kept.get() {
                        dismiss.call(Call::BrowserInput { pane, input: BrowserInput::Choose { index: None } });
                    }
                });
                cx.notify();
            },
        );
        self.list = List { select: Some(select), menu: Some(menu) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::BrowserOption;

    fn option(index: u32, label: &str, group: Option<&str>, disabled: bool) -> BrowserOption {
        BrowserOption { index, label: label.into(), group: group.map(str::to_string), disabled }
    }

    #[test]
    fn a_group_is_a_heading_over_its_options_and_the_chosen_one_is_ticked() {
        let select = BrowserSelect {
            options: vec![option(0, "Any", None, false), option(1, "Red", Some("Warm"), false), option(3, "Blue", Some("Cool"), true)],
            selected: Some(1),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        };
        let items = items(&select);
        let shape: Vec<String> = items
            .iter()
            .map(|item| match item {
                MenuItem::Heading(text) => format!("# {text}"),
                MenuItem::Choice(choice) => format!("{} {} {}", choice.label, choice.checked == Some(true), choice.reason.is_some()),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(shape, ["Any false false", "# Warm", "Red true false", "# Cool", "Blue false true"]);
    }
}
