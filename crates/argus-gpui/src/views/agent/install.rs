//! Puts the views agents showed where they are drawn: the panel body for every dock or float that
//! holds the views, the page over the stage, and what the page puts in the strip (its title, its
//! status pill and its buttons).
//!
//! Called once by the shell; nothing else knows which entities draw the views.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::chrome::strip::{Pill, Says};
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::head;
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, App, Entity};

use super::acts;
use super::panel::{AgentView, Place};
use crate::chrome::header::slots::{Offer, Slots};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::layout::bodies::Contents;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::blocks::kept::Kept;
use crate::views::blocks::pen::Pen;
use crate::views::ctx::Draw;

/// What the strip says while a view is the page: its title in the name's place and its status in
/// the pill's. None while the view is gone, so the strip says `View`.
pub fn says(heard: &Entity<Heard>, cx: &App) -> Says {
    let heard = heard.read(cx);
    let Some(view) = heard.views.as_deref().and_then(head::popped) else { return Says::default() };
    Says {
        title: Some(view.view.title.clone()),
        pill: view.view.status.as_ref().map(|status| Pill { status: head::pill_status(status.tone), word: status.label.clone() }),
        ..Says::default()
    }
}

pub fn install(heard: &Entity<Heard>, caller: &Caller, cx: &mut App) {
    let panel = cx.new(|cx| AgentView::new(Place::Panel, heard.clone(), caller.clone(), cx));
    let page = cx.new(|cx| AgentView::new(Place::Page, heard.clone(), caller.clone(), cx));
    let bodies = Contents::of(cx);
    bodies.offer_panel(|panel| *panel == Panel::Views, panel.into());
    bodies.offer_stage(Stage::View, page.into());
    let (saying, tooling, caller) = (heard.clone(), heard.clone(), caller.clone());
    // What the strip's buttons keep between draws (a confirmation being asked), as the panel keeps its own.
    let kept = Rc::new(RefCell::new(Kept::default()));
    Slots::of(cx).offer(
        Stage::View,
        Offer::default().says(move |cx| says(&saying, cx)).tools(move |window, cx| {
            let tokens = *Tokens::global(cx);
            let (page, memory) = {
                let heard = tooling.read(cx);
                (heard.views.clone(), heard.memory.clone())
            };
            let Some(page) = page else { return h_flex().into_any_element() };
            let Some(view) = head::popped(&page) else { return h_flex().into_any_element() };
            let clock = Clock::local();
            let draw = Draw { tokens: &tokens, clock: &clock, caller: &caller, saved: &[], view: None };
            // The strip is the shell's: a question asked in it is the shell's to draw again, and it hears the session.
            let repaint = tooling.entity_id();
            let mut kept = kept.borrow_mut();
            let pen = Pen { draw, memory: &memory, saved: None, repaint, kept: &mut kept, window, cx };
            h_flex().gap(rem(tokens.sp_8)).children(acts::acts(view, true, page.can_press, &pen, &tooling)).into_any_element()
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::{Event, PageView, ViewRef, ViewsPage};
    use argus_protocol::{AgentView, Chip, Tone};
    use gpui_kit::TestAppContext;

    fn view(tone: Tone) -> PageView {
        PageView {
            pane: 1,
            from: String::new(),
            at: 0,
            refreshing: false,
            pressed: Vec::new(),
            subject: None,
            view: AgentView { id: "trim".into(), title: "Trim context".into(), status: Some(Chip { label: "blocked".into(), tone }), ..Default::default() },
            diagrams: Vec::new(),
            changed: Vec::new(),
            unseen: false,
        }
    }

    #[gpui_kit::test]
    fn the_strip_says_the_popped_views_title_and_a_pill_of_its_tone_and_nothing_once_it_is_gone(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let heard = cx.new(|_| Heard::new());
            assert_eq!(says(&heard, cx), Says::default());
            let page = ViewsPage { views: vec![view(Tone::Warn)], open: None, popped: Some(ViewRef { pane: 1, id: "trim".into() }), can_press: true };
            heard.update(cx, |heard, _| {
                heard.hear(Event::Views(Box::new(page)));
            });
            let said = says(&heard, cx);
            assert_eq!(said.title.as_deref(), Some("Trim context"));
            assert_eq!(said.pill, Some(Pill { status: "waiting", word: "blocked".into() }));
            heard.update(cx, |heard, _| {
                heard.hear(Event::Views(Box::default()));
            });
            assert_eq!(says(&heard, cx), Says::default(), "a page whose view went says the stage's name");
        });
    }
}
