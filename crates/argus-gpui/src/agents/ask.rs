//! How the agents' dialogs and composer send a call: dropped while the daemon is gone, since the
//! session holds nothing to answer it (the page sends it and the request is lost).

use argus_desktop_core::connection::page::Call;
use gpui_kit::{App, Entity};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

/// Sends `call` unless the link to the daemon is down.
pub fn ask(heard: &Entity<Heard>, caller: &Caller, call: Call, cx: &App) {
    if !heard.read(cx).held() {
        caller.call(call);
    }
}
