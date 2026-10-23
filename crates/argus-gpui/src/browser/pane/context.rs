//! Chrome's own menu, opened by a right-click the page left alone: headless Chrome draws none, so
//! the window draws it as the kit's menu at the click, with what Chrome offers for what was under
//! it. A link can be opened in a new tab or its address copied, an image's address copied,
//! selected text copied, and a field cut from, pasted into or wholly selected; Back, Forward and
//! Reload are always there.
//!
//! Copying is the window's: the words go to the person's clipboard and the page is only told the
//! menu went. Cut copies, then has the page delete the selection; Paste reads the clipboard and
//! has the page type it. As with a `<select>`'s list, a dismissal waits a turn and sends nothing
//! when a choice came, and the same menu is not opened twice.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use argus_protocol::{BrowserContext, BrowserInput, BrowserMenuAction};
use gpui_kit::{AnyElement, App, ClipboardItem, Context, Window};

use super::BrowserPane;
use crate::kit::menu::at::At;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

pub const OPEN_LINK: &str = "Open link in new tab";
pub const COPY_LINK: &str = "Copy link address";
pub const COPY_IMAGE: &str = "Copy image address";
pub const CUT: &str = "Cut";
pub const COPY: &str = "Copy";
pub const PASTE: &str = "Paste";
pub const SELECT_ALL: &str = "Select all";
pub const BACK: &str = "Back";
pub const FORWARD: &str = "Forward";
pub const RELOAD: &str = "Reload";
pub const NOTHING_SELECTED: &str = "Nothing is selected";

/// What a choice does: something the page does, or words copied by the window.
#[derive(Debug, Clone, PartialEq)]
enum Pick {
    Page(BrowserMenuAction),
    Copy(String),
    Cut(String),
    Paste,
}

/// The menu shown, or the one put away, until the daemon moves on from it.
#[derive(Default)]
pub struct Menu {
    context: Option<BrowserContext>,
    menu: Option<At>,
}

impl Menu {
    pub fn element(&self, window: &Window) -> Option<AnyElement> {
        self.menu.as_ref().map(|menu| menu.element(window))
    }
}

/// Chrome's choices for what the click was on, in Chrome's order.
fn items(context: &BrowserContext) -> Vec<MenuItem<Pick>> {
    let mut items = Vec::new();
    if let Some(link) = &context.link {
        items.push(MenuItem::choice(OPEN_LINK, Pick::Page(BrowserMenuAction::OpenLink)));
        items.push(MenuItem::choice(COPY_LINK, Pick::Copy(link.clone())));
    }
    if let Some(image) = &context.image {
        items.push(MenuItem::choice(COPY_IMAGE, Pick::Copy(image.clone())));
    }
    let selected = context.selection.clone();
    let none = selected.is_none();
    if context.editable {
        if !items.is_empty() {
            items.push(MenuItem::Rule);
        }
        items.push(MenuItem::choice(CUT, Pick::Cut(selected.clone().unwrap_or_default())).refused_when(none, NOTHING_SELECTED));
        items.push(MenuItem::choice(COPY, Pick::Copy(selected.unwrap_or_default())).refused_when(none, NOTHING_SELECTED));
        items.push(MenuItem::choice(PASTE, Pick::Paste));
        items.push(MenuItem::choice(SELECT_ALL, Pick::Page(BrowserMenuAction::SelectAll)));
    } else if let Some(selected) = selected {
        if !items.is_empty() {
            items.push(MenuItem::Rule);
        }
        items.push(MenuItem::choice(COPY, Pick::Copy(selected)));
    }
    if !items.is_empty() {
        items.push(MenuItem::Rule);
    }
    items.push(MenuItem::choice(BACK, Pick::Page(BrowserMenuAction::Back)));
    items.push(MenuItem::choice(FORWARD, Pick::Page(BrowserMenuAction::Forward)));
    items.push(MenuItem::choice(RELOAD, Pick::Page(BrowserMenuAction::Reload)));
    items
}

/// What a pick sends the page, after doing the window's part of it.
fn picked(pick: &Pick, cx: &mut App) -> Option<BrowserMenuAction> {
    match pick {
        Pick::Page(action) => Some(action.clone()),
        Pick::Copy(words) => {
            cx.write_to_clipboard(ClipboardItem::new_string(words.clone()));
            None
        }
        Pick::Cut(words) => {
            cx.write_to_clipboard(ClipboardItem::new_string(words.clone()));
            Some(BrowserMenuAction::Cut)
        }
        Pick::Paste => cx.read_from_clipboard().and_then(|item| item.text()).map(|text| BrowserMenuAction::Paste { text }),
    }
}

impl BrowserPane {
    /// Opens the menu the daemon says is open, or lets it go when the daemon says none is.
    pub(super) fn follow_context(&mut self, browser: Option<&PageBrowser>, window: &mut Window, cx: &mut Context<Self>) {
        let said = browser.and_then(|browser| browser.context.clone());
        if said == self.context.context {
            return;
        }
        self.context = Menu::default();
        let Some(context) = said else { return };
        let Some(position) = self.point_on_window(context.x, context.y, cx) else { return };
        let (pane, tokens) = (self.pane, *Tokens::global(cx));
        let chosen = Rc::new(Cell::new(false));
        let (picks, kept) = (chosen.clone(), chosen);
        let (choose, dismiss) = (self.caller.clone(), self.caller.clone());
        let items = items(&context);
        let menu = At::open(
            position,
            move |menu, window, cx| {
                popup::fill(menu, &items, tokens, |_| false, move |pick: &Pick, _, cx| {
                    picks.set(true);
                    let action = picked(pick, cx);
                    choose.call(Call::BrowserInput { pane, input: BrowserInput::Context { action } });
                }, window, cx)
            },
            window,
            cx,
            move |this, window, cx| {
                this.context.menu = None;
                let (kept, dismiss) = (kept.clone(), dismiss.clone());
                window.defer(cx, move |_, _| {
                    if !kept.get() {
                        dismiss.call(Call::BrowserInput { pane, input: BrowserInput::Context { action: None } });
                    }
                });
                cx.notify();
            },
        );
        self.context = Menu { context: Some(context), menu: Some(menu) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(link: Option<&str>, selection: Option<&str>, editable: bool) -> BrowserContext {
        BrowserContext { x: 1.0, y: 1.0, link: link.map(str::to_string), image: None, selection: selection.map(str::to_string), editable }
    }

    fn labels(items: &[MenuItem<Pick>]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Choice(choice) if choice.reason.is_some() => format!("({})", choice.label),
                MenuItem::Choice(choice) => choice.label.clone(),
                MenuItem::Rule => "-".into(),
                _ => "?".into(),
            })
            .collect()
    }

    #[test]
    fn a_link_offers_to_open_and_copy_it_above_the_page_moves() {
        let items = items(&context(Some("https://a.test/"), None, false));
        assert_eq!(labels(&items), [OPEN_LINK, COPY_LINK, "-", BACK, FORWARD, RELOAD]);
    }

    #[test]
    fn a_field_with_nothing_selected_cannot_be_cut_or_copied_from() {
        let items = items(&context(None, None, true));
        assert_eq!(labels(&items), ["(Cut)", "(Copy)", PASTE, SELECT_ALL, "-", BACK, FORWARD, RELOAD]);
    }

    #[test]
    fn selected_text_on_the_page_can_be_copied() {
        let items = items(&context(None, Some("words"), false));
        assert_eq!(labels(&items), [COPY, "-", BACK, FORWARD, RELOAD]);
        assert!(matches!(&items[0], MenuItem::Choice(choice) if choice.does == Pick::Copy("words".into())));
    }
}
