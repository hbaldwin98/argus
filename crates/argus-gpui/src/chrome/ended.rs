//! What a pane whose program has ended offers: the screen is still there, the program is gone
//! (SHELL-STAGE-10).
//!
//! A bar over the foot of the stage: an info mark, the pane's note when it has one, a button that
//! starts another where this one was, and one that closes the pane. The start button is offered
//! only while the pane is still listed in the tree, since it starts on that pane's checkout. What
//! it starts is the page's guess, kept as it is: an agent starts the first of the templates the
//! window offers, a shell (or an agent when the daemon offers no template) starts a shell.
//! Pressing sends the call and changes nothing here; the bar goes when the pane does.

use argus_desktop_core::connection::page::{Call, Rail};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Role, SharedString};

use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::theme::Tokens;

/// What the bar offers for the pane on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub pane: u64,
    /// Why the program ended, in the pane's own words.
    pub note: Option<String>,
    /// Another where this one was, while the pane is in the tree.
    pub start: Option<Start>,
}

/// A new pane on the ended pane's checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub checkout: u64,
    /// The agent to start, or none for a shell.
    pub template: Option<String>,
}

impl Start {
    pub fn words(&self) -> String {
        self.template.as_ref().map_or_else(|| "New shell here".to_owned(), |template| format!("Start {template} here"))
    }

    pub fn call(&self) -> Call {
        match &self.template {
            Some(template) => Call::SpawnAgent { checkout: self.checkout, template: template.clone() },
            None => Call::SpawnShell { checkout: self.checkout },
        }
    }
}

/// The bar for the shown pane, if its program has ended.
pub fn of(rail: &Rail) -> Option<Ended> {
    let shown = rail.shown.as_ref().filter(|shown| shown.status == "exited")?;
    let found = rail
        .project
        .iter()
        .flat_map(|project| &project.repositories)
        .flat_map(|repository| &repository.checkouts)
        .find_map(|checkout| checkout.panes.iter().find(|pane| pane.id == shown.pane).map(|pane| (checkout.id, pane)));
    Some(Ended {
        pane: shown.pane,
        note: found.and_then(|(_, pane)| pane.note.clone()),
        start: found.map(|(checkout, pane)| Start {
            checkout,
            template: pane.agent.then(|| rail.templates.first().cloned()).flatten(),
        }),
    })
}

/// The bar, along the foot of the stage it is put over. Its buttons send nothing while the
/// daemon is gone (`held`).
pub fn bar(ended: Ended, held: bool, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let close = {
        let (caller, pane) = (caller.clone(), ended.pane);
        button("ended-close", "Close pane", Look::Plain, true, tokens).child("Close pane").on_click(move |_, _, _| {
            if !held {
                caller.call(Call::Close { pane });
            }
        })
    };
    let start = ended.start.map(|start| {
        let words = start.words();
        let caller = caller.clone();
        button("ended-start", words.clone(), Look::Plain, true, tokens).child(SharedString::from(words)).on_click(move |_, _, _| {
            if !held {
                caller.call(start.call());
            }
        })
    });
    // `.ended`: along the foot with a hand's margin, on the surface colour.
    h_flex()
        .id("ended")
        .role(Role::Status)
        .test_support()
        .absolute()
        .left(px(tokens.sp_16))
        .right(px(tokens.sp_16))
        .bottom(px(tokens.sp_16))
        .gap(px(tokens.sp_12))
        .items_center()
        .px(px(tokens.sp_12))
        .py(px(tokens.sp_8))
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(px(tokens.r_md))
        .bg(tokens.bg_surface)
        .shadow(tokens.shadow_pop())
        .text_size(px(tokens.fs_sm))
        .text_color(tokens.fg)
        .child(div().flex_shrink_0().text_color(tokens.fg_muted).child(Glyph::Info.at(tokens.icon_sm)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(tokens.fg_muted)
                // The rail's row and the header already say it ended; only its note is news.
                .children(ended.note.map(SharedString::from)),
        )
        .children(start)
        .child(close)
}

#[cfg(test)]
mod tests;
