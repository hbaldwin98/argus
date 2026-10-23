//! The toolbar's Copy: the picture the cell shows put on the clipboard as an image, to paste to an
//! agent or anywhere else, and a notice saying it was or why not.
//!
//! The picture is the latest the daemon sent, at the device pixels it is drawn at, so what is
//! copied is what the person sees. It is decoded and written off the UI thread. An agent that
//! wants a picture of its own takes one with `argus-hook browser shot`.

use argus_desktop_core::connection::page::Event;
use gpui_kit::{prelude::*, App, Entity};

use super::bar::text_button;
use crate::connection::heard::Heard;
use crate::kit::icon_button::Node;
use crate::system::clipboard::write_picture;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub const COPY: &str = "Copy";
pub const COPY_TIP: &str = "Copy a picture of the page, to paste to an agent";
pub const COPIED: &str = "Copied a picture of the page";
pub const NOTHING_TO_COPY: &str = "The browser has no picture to copy yet";

pub fn button(pane: u64, heard: &Entity<Heard>, tokens: &Tokens) -> Node {
    let heard = heard.clone();
    text_button(format!("browser-copy-{pane}"), COPY, COPY_TIP, tokens, move |_, cx| copy(pane, &heard, cx))
        .aria_label(COPY_TIP)
        .text_size(rem(tokens.fs_md))
}

fn copy(pane: u64, heard: &Entity<Heard>, cx: &mut App) {
    let source = heard.read(cx).source();
    let heard = heard.clone();
    cx.spawn(async move |cx| {
        let copied = cx
            .background_executor()
            .spawn(async move {
                let picture = source(pane).ok_or(NOTHING_TO_COPY.to_owned())?;
                write_picture(&picture.bytes).map_err(|why| format!("The picture was not copied: {why}"))
            })
            .await;
        let text = copied.map(|()| COPIED.to_owned()).unwrap_or_else(|why| why);
        heard.update(cx, |heard, cx| {
            heard.hear(Event::Notice { text });
            cx.notify();
        });
    })
    .detach();
}
