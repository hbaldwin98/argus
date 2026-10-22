//! What a gesture on the Checkouts stage does in the window: the calls it comes to are sent, and a
//! new worktree that wants a name opens the name dialog first (`argus-desktop-core`'s `ways`
//! decides which).
//!
//! Nothing is predicted. A held window (the daemon gone) sends nothing, and the table changes when
//! the session's next `Checkouts` says it did, so a refused action leaves the table as it was.

use argus_desktop_core::checkouts::table::CheckoutsRow;
use argus_desktop_core::checkouts::ways::{self, Then, Way};
use gpui_kit::{App, Entity, Window};

use super::dialogs::name;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

/// Sends what `then` comes to.
pub fn run(then: Then, caller: &Caller, heard: &Entity<Heard>, window: &mut Window, cx: &mut App) {
    if heard.read(cx).held() {
        return;
    }
    for call in then.calls {
        caller.call(call);
    }
    if let Some(off) = then.name_off {
        name::open(off, caller.clone(), heard.clone(), window, cx);
    }
}

/// The row with `key`, as the table heard it last, and which row it says is selected.
fn row_of(heard: &Entity<Heard>, key: &str, cx: &App) -> Option<(CheckoutsRow, Option<String>)> {
    let table = heard.read(cx).checkouts.clone()?;
    let row = table.rows.iter().find(|row| row.key == key)?.clone();
    Some((row, table.selected.clone()))
}

/// `way` on the row `key`: asked of the table as it is when the gesture lands, since a menu opened
/// before a snapshot may be chosen after it. A row that has gone since is nothing to act on.
pub fn way(way: &Way, key: &str, caller: &Caller, heard: &Entity<Heard>, window: &mut Window, cx: &mut App) {
    if let Some((row, selected)) = row_of(heard, key, cx) {
        run(ways::then(way, &row, selected.as_deref()), caller, heard, window, cx);
    }
}

/// A way of the table itself, with no row: the strip's `⋯`.
pub fn table(way: &Way, caller: &Caller, heard: &Entity<Heard>, window: &mut Window, cx: &mut App) {
    run(ways::table(way), caller, heard, window, cx);
}
