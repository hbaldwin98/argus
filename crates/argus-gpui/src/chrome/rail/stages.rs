//! What a press of a stage in the strip does: which call it sends, and the press of Git that may
//! travel.
//!
//! A press of a nav item: Git asks for the review of the checkout the window is on; every item that
//! goes to another stage first closes what the session has open of Git, in the same breath
//! (`review::entry`), so the stage is the session's to say and is never left standing under
//! another. Drawing the tiles is `strip`'s; the Extensions and Views lists are `tail`'s.

use gpui_kit::{Context, MouseDownEvent};

use super::RailView;
use crate::chrome::nav::Nav;
use crate::layout::carry::Carrier;
use crate::layout::work::FromRail;
use crate::review::entry::checkout_here;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{DockGesture, Panel};

impl RailView {
    /// A press of a nav item. Git asks for the review of the checkout the
    /// window is on; every item that goes to another stage first closes what
    /// the session has open of Git, in the same breath (`review::entry`), so
    /// the stage is the session's to say and is never left standing under
    /// another.
    pub(super) fn go(&mut self, nav: Nav, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let picked = self.slice.rail.cursor.pick.map(|pick| pick.checkout);
        let calls = match nav {
            // Git beside the work, open, comes in sight where it sits: its dock opens on it.
            Nav::Git if self.heard.read(cx).changes_beside() => vec![Call::Docks { gesture: DockGesture::Show { panel: Panel::Changes } }],
            Nav::Git => self.heard.update(cx, |heard, cx| {
                let (open, here) = (heard.open(), checkout_here(&heard.rail, picked));
                let calls = heard.git.enter(open, here);
                // A Git pressed with nothing to open puts the stage up on the window's word alone.
                cx.notify();
                calls
            }),
            _ if nav.leaves_git() => self.leave_git(cx),
            _ => Vec::new(),
        };
        for call in calls.into_iter().chain(nav.call()) {
            self.caller.call(call);
        }
    }

    /// A press of Git that may travel: the changes' panel as it is while a review is open, or one
    /// its drop opens. History open, or no checkout to review, it is Git's click only.
    pub(super) fn carry_git(&self, event: &MouseDownEvent, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let heard = self.heard.read(cx);
        let open = heard.open();
        let rail = if open.review {
            Some(FromRail::There)
        } else {
            checkout_here(&heard.rail, self.slice.rail.cursor.pick.map(|pick| pick.checkout)).filter(|_| !open.history).map(|checkout| FromRail::Review { checkout })
        };
        if let Some(rail) = rail {
            Carrier::of(cx).press_rail(event.position, Panel::Changes, "Changes".into(), rail, cx);
        }
    }
}
