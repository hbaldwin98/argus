//! The strip across a saved panel, drawn unless the panel is one column in a dock that shows tabs
//! or floating (core's `column::headed`): draggable like any panel's header, with the first
//! column's title, a stale chip when it is the only column, `<n> columns` when there are several,
//! the dock menu, and a close that takes the panel out of its dock with its columns
//! (CHECKOUTS-CODE-VIEWS-SAVED-4).

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DocksPage, Panel};
use argus_desktop_core::views::saved::SavedViewsPage;
use argus_protocol::{Chip, Tone};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ClickEvent, Entity, MouseButton, SharedString};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::layout::carry::Carrier;
use crate::layout::dock_menu;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::chip;

/// What the strip says.
pub struct Strip<'a> {
    pub panel: &'a Panel,
    /// The first column's title, else `Saved view`.
    pub title: &'a str,
    pub columns: usize,
    pub docks: &'a DocksPage,
    pub page: &'a SavedViewsPage,
    pub held: bool,
}

/// What the strip's text says of the columns: `3 columns`, nothing for one.
pub fn count_words(columns: usize) -> Option<String> {
    (columns > 1).then(|| format!("{columns} columns"))
}

fn glyph(docks: &DocksPage, panel: &Panel) -> Glyph {
    match dock_menu::dock_of(docks, panel) {
        Some(Dock::Left) => Glyph::DockLeft,
        Some(Dock::Bottom) => Glyph::DockBottom,
        _ => Glyph::DockRight,
    }
}

/// What follows a head's title: the stale chip of a lone stale view, or the column count. A floating
/// panel's head is its frame's title bar, which draws these too.
pub fn marks(panel: &Panel, page: Option<&SavedViewsPage>, tokens: &Tokens) -> Vec<AnyElement> {
    let Some((columns, first)) = page.and_then(|page| {
        let trail = page.panels.iter().find(|p| &p.panel == panel)?;
        Some((trail.columns.len(), page.views.iter().find(|view| trail.columns.first() == Some(&view.id))))
    }) else {
        return Vec::new();
    };
    let stale = Chip { label: "stale".into(), tone: Tone::Warn };
    let mut out = Vec::new();
    if columns == 1 && first.is_some_and(|view| view.stale) {
        out.push(div().id(SharedString::from(format!("saved-stale-{panel}"))).test_support().aria_label("stale").flex_shrink_0().child(chip(&stale, tokens)).into_any_element());
    }
    if let Some(words) = count_words(columns) {
        let label = SharedString::from(words);
        out.push(div().id(SharedString::from(format!("saved-count-{panel}"))).test_support().aria_label(label.clone()).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(label).into_any_element());
    }
    out
}

pub fn strip(of: &Strip, heard: &Entity<Heard>, caller: &Caller, tokens: &Tokens, cx: &mut gpui_kit::App) -> impl IntoElement {
    let panel = of.panel.clone();
    let present = of.docks.present.iter().find(|present| present.panel == panel).cloned();
    let entries = dock_menu::moves(&panel, true, of.docks, present.as_ref());
    let (menu_caller, menu_heard, menu_tokens) = (caller.clone(), heard.clone(), *tokens);
    let (pressed, title) = (panel.clone(), of.title.to_owned());
    let (closing, middle) = (panel.clone(), panel.clone());
    let (close_caller, middle_caller) = (caller.clone(), caller.clone());
    let held = of.held;
    let marks = marks(&panel, Some(of.page), tokens);
    strip::bar(tokens)
        .id(SharedString::from(format!("saved-head-{panel}")))
        .test_support()
        .aria_label("Saved view")
        .pl(rem(tokens.sp_12))
        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
            // A head is dragged, never clicked.
            if !held {
                Carrier::of(cx).press_panel(event.position, pressed.clone(), title.clone(), false, cx);
            }
        })
        .on_mouse_down(MouseButton::Middle, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_aux_click(move |event: &ClickEvent, _, cx| {
            if event.is_middle_click() {
                cx.stop_propagation();
                if !held {
                    middle_caller.call(Call::CloseSavedColumn { panel: middle.clone(), column: 0 });
                }
            }
        })
        .child(div().min_w_0().truncate().typed(tokens.type_small).weighted(tokens.weight_semibold).child(SharedString::from(of.title.to_owned())))
        .children(marks)
        .child(div().flex_1())
        .child(
            div().flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
                strip::menu_button(SharedString::from(format!("saved-dock-{panel}")), glyph(of.docks, &panel), 15.0, tokens, cx)
                    .tooltip("Dock the view")
                    .disabled(held)
                    .dropdown_menu(move |menu, window, cx| dock_menu::build(menu, &entries, &menu_caller, &menu_heard, menu_tokens, window, cx)),
            ),
        )
        .child(
            icon_button(SharedString::from(format!("saved-close-{closing}")), format!("Close {}", of.title), false, tokens)
                .tooltip(tip("Close"))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, _| {
                    if !held {
                        close_caller.call(Call::CloseSavedColumn { panel: closing.clone(), column: 0 })
                    }
                })
                .child(Glyph::Close.at(tokens.icon_sm)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_columns_are_counted_and_one_is_not() {
        // SAVED-4.
        assert_eq!(count_words(1), None);
        assert_eq!(count_words(3).as_deref(), Some("3 columns"));
    }

    fn page(columns: &[&str], stale: bool) -> SavedViewsPage {
        use argus_desktop_core::views::saved::{PageSaved, PageScope, SavedPanel};
        let view = |id: &str| PageSaved {
            id: id.into(),
            panel: Panel::Saved(id.into()),
            scope: PageScope { rank: 0, label: "Global".into() },
            here: true,
            features: vec![],
            recipe: None,
            sources: vec![],
            built_at: 0,
            built_commit: None,
            stale,
            view: argus_protocol::AgentView { id: id.into(), title: id.into(), ..Default::default() },
            diagrams: vec![],
            refreshing: false,
            pressed: vec![],
            under: None,
        };
        SavedViewsPage {
            views: columns.iter().map(|id| view(id)).collect(),
            panels: vec![SavedPanel { panel: Panel::Saved("a".into()), columns: columns.iter().map(|id| (*id).to_owned()).collect() }],
            can_press: false,
        }
    }

    #[test]
    fn a_lone_stale_view_wears_the_chip_and_a_trail_wears_its_count() {
        // SAVED-4: what a floating panel's title bar carries too.
        let tokens = crate::theme::themes::ledger::tokens();
        let panel = Panel::Saved("a".into());
        assert_eq!(marks(&panel, Some(&page(&["a"], true)), &tokens).len(), 1);
        assert_eq!(marks(&panel, Some(&page(&["a"], false)), &tokens).len(), 0);
        assert_eq!(marks(&panel, Some(&page(&["a", "b"], true)), &tokens).len(), 1, "the count only, not the chip");
        assert_eq!(marks(&Panel::Saved("z".into()), Some(&page(&["a"], true)), &tokens).len(), 0);
        assert_eq!(marks(&panel, None, &tokens).len(), 0);
    }
}
