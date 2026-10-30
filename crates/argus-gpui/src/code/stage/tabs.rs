//! What the Code stage puts in the title bar: All files and Changed, the choice of which files
//! the tree lists (`.code-tools`), with how many have changed behind the second.
//!
//! The files column lost its own switch when the bar took it (`chrome::header::tabs`). A press
//! sends the click and the tab that is on is the page's to say, so a refused click leaves the bar
//! showing what is true.

use argus_desktop_core::code::stage::{CodeClick, CodeGesture};
use argus_desktop_core::connection::page::Call;
use gpui_kit::{App, Entity};

use crate::chrome::header::tabs::{Tab, Tabs};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

/// The tabs while the stage has a file tree to choose for.
pub(super) fn tabs(heard: &Entity<Heard>, caller: &Caller, cx: &App) -> Option<Tabs> {
    let page = heard.read(cx).code.clone().filter(|page| page.open)?;
    let press = |only: bool| {
        let (heard, caller) = (heard.clone(), caller.clone());
        // A window whose daemon is gone sends nothing.
        move |_: &mut gpui_kit::Window, cx: &mut App| {
            if !heard.read(cx).held() {
                caller.call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::ChangedOnly { only } } });
            }
        }
    };
    Some(Tabs::new(
        "code-tabs",
        "Which files",
        vec![
            Tab::new("code-all", "All files", !page.changed_only, press(false)),
            Tab::new("code-changed-only", "Changed", page.changed_only, press(true)).count(Some(page.changed)),
        ],
    ))
}
