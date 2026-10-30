//! The file's header: its name and folder, the version when it is not the working tree, what is
//! known of the file, and the buttons (`header`, `code/stage.ts`), drawn on the kit's file head.
//!
//! Drawn while a file is open or the stage says where `B` goes back to, so also over the start
//! panel when the stage was opened from a review or the history. What it says is core's
//! (`code::file`); a button sends the gesture its key does.

use argus_desktop_core::code::file::{facts, has_header, path_parts};
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::{prelude::*, AnyElement, Entity, SharedString};

use super::rendered::choice::{choice, Choose};
use super::rendered::mode::Mode;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::file_head::{file_head, Head};
use crate::kit::tools;
use crate::theme::Tokens;

/// What a button of the header does, sent as the gesture its key sends unless the daemon is gone.
fn press(heard: &Entity<Heard>, caller: &Caller, gesture: CodeGesture) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static {
    let (heard, caller) = (heard.clone(), caller.clone());
    move |_, _, cx| {
        if !heard.read(cx).held() {
            caller.call(Call::Code { gesture: gesture.clone() });
        }
    }
}

/// The kit's file head over the lines: the name, the folder cut from its left, `+a −r` and the
/// facts, then the tools. Nothing without a file or a way back.
/// `reading` is how a markdown file is being read and what to call when the other way is chosen; it
/// is none for any other file, which has only its text.
pub fn bar(page: &CodePage, heard: &Entity<Heard>, caller: &Caller, tokens: &Tokens, reading: Option<(Mode, Choose)>) -> Option<AnyElement> {
    if !has_header(page) {
        return None;
    }
    // This window has no editor, so the lines stand for the file and Rust's cursor is the one shown.
    let said = facts(page, false);
    let mut tools: Vec<AnyElement> = Vec::new();
    tools.extend(reading.map(|(mode, choose)| choice(mode, &choose, tokens)));
    tools.extend(page.back.as_ref().map(|back| {
        tools::ghost("code-back", format!("Back to {back}"), Some("B"), false, tokens)
            .on_click(press(heard, caller, CodeGesture::Click { target: CodeClick::Back }))
            .into_any_element()
    }));
    // The toggle is the outline's own state: pressed while the column is up, as the page says.
    tools.push(
        tools::ghost("code-outline-show", "Outline", Some("O"), page.outline, tokens)
            .on_click(press(heard, caller, CodeGesture::Click { target: CodeClick::Outline }))
            .into_any_element(),
    );
    tools.extend(page.path.as_ref().map(|_| {
        tools::ghost("code-edit", "Open in editor", Some("E"), false, tokens)
            .on_click(press(heard, caller, CodeGesture::Action { action: Action::OpenEditor }))
            .into_any_element()
    }));
    let Some(path) = page.path.as_deref() else {
        // Only the way back, over the start panel: the head's line with the tools alone.
        let head = Head { status: None, name: "", dir: "", counts: None, meta: None };
        return Some(file_head("code-head", &head, tools, tokens).into_any_element());
    };
    let (name, dir) = path_parts(path);
    let lead = said.lead.trim_end_matches(" \u{b7} ").to_owned();
    // The version names what is read when it is not the working tree, ahead of the rest.
    let meta = match (&page.version, lead.is_empty()) {
        (Some(version), true) => Some(version.clone()),
        (Some(version), false) => Some(format!("{version} \u{b7} {lead}")),
        (None, true) => None,
        (None, false) => Some(lead),
    };
    let head = Head { status: None, name, dir, counts: Some((said.added as usize, said.removed as usize)), meta: meta.map(SharedString::from) };
    Some(file_head("code-head", &head, tools, tokens).into_any_element())
}
