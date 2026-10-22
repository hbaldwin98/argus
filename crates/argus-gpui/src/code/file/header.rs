//! The file's header: its name and folder, the version when it is not the working tree, what is
//! known of the file, and the buttons (`header`, `code/stage.ts`).
//!
//! Drawn while a file is open or the stage says where `B` goes back to, so also over the start
//! panel when the stage was opened from a review or the history. What it says is core's
//! (`code::file`); a button sends the gesture its key does.

use argus_desktop_core::code::file::{facts, has_header, path_parts};
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, Look};
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use argus_desktop_core::connection::page::Call;

/// What a button of the header does, sent as the gesture its key sends unless the daemon is gone.
fn press(heard: &Entity<Heard>, caller: &Caller, gesture: CodeGesture) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static {
    let (heard, caller) = (heard.clone(), caller.clone());
    move |_, _, cx| {
        if !heard.read(cx).held() {
            caller.call(Call::Code { gesture: gesture.clone() });
        }
    }
}

/// `.fhdr`: the chrome colour over a hairline, 12px about its parts. Nothing without a file or a way
/// back.
pub fn bar(page: &CodePage, heard: &Entity<Heard>, caller: &Caller, tokens: &Tokens) -> Option<AnyElement> {
    if !has_header(page) {
        return None;
    }
    let mono = SharedString::from(tokens.font_mono);
    // This window has no editor, so the lines stand for the file and Rust's cursor is the one shown.
    let said = facts(page, false);
    let key = |text: &'static str| keycap(text, mono.clone(), tokens);
    let path = page.path.as_deref().map(|path| {
        let (name, dir) = path_parts(path);
        // The name first and whole; only the folder after it gives way.
        h_flex()
            .id("code-path")
            .test_support()
            .aria_label(SharedString::from(path.to_owned()))
            .min_w_0()
            .gap(rem(tokens.sp_8))
            .items_baseline()
            .whitespace_nowrap()
            .font_family(mono.clone())
            .child(div().flex_none().weighted(tokens.weight_semibold).text_color(tokens.fg).child(SharedString::from(name.to_owned())))
            .when(!dir.is_empty(), |row| row.child(div().min_w_0().truncate().text_color(tokens.fg_faint).child(SharedString::from(dir.to_owned()))))
    });
    let version = page.version.as_ref().map(|version| {
        div()
            .id("code-version")
            .test_support()
            .aria_label(SharedString::from(version.clone()))
            .flex_shrink_0()
            .px(rem(tokens.sp_8))
            .py(rem(tokens.hairline))
            .rounded(rem(tokens.r_pill))
            .bg(tokens.fill)
            .font_family(mono.clone())
            .text_size(rem(tokens.fs_xs))
            .weighted(tokens.weight_semibold)
            .text_color(tokens.fg)
            .child(SharedString::from(version.clone()))
    });
    let counts = (said.added > 0 || said.removed > 0).then(|| {
        h_flex()
            .gap(rem(tokens.sp_4))
            .when(said.added > 0, |row| row.child(div().text_color(tokens.ok).child(SharedString::from(format!("+{}", said.added)))))
            .when(said.removed > 0, |row| row.child(div().text_color(tokens.danger).child(SharedString::from(format!("\u{2212}{}", said.removed)))))
    });
    let meta = h_flex()
        .id("code-facts")
        .test_support()
        .aria_label(SharedString::from(format!("{}{}", said.lead, counts_text(&said))))
        .flex_shrink_0()
        .whitespace_nowrap()
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .child(SharedString::from(said.lead.clone()))
        .children(counts);
    Some(
        h_flex()
            .id("code-head")
            .test_support()
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_8))
            .gap(rem(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .border_b_1()
            .border_color(tokens.line)
            .typed(tokens.type_body)
            .children(path)
            .children(version)
            .child(div().flex_1())
            .child(meta)
            .children(page.back.as_ref().map(|back| {
                button("code-back", format!("Back to {back}"), Look::Plain, true, tokens)
                    .child(SharedString::from(format!("Back to {back}")))
                    .child(key("B"))
                    .on_click(press(heard, caller, CodeGesture::Click { target: CodeClick::Back }))
            }))
            .children(page.path.as_ref().map(|_| {
                button("code-edit", "Open in editor", Look::Plain, true, tokens)
                    .child("Open in editor")
                    .child(key("E"))
                    .on_click(press(heard, caller, CodeGesture::Action { action: Action::OpenEditor }))
            }))
            .children((!page.outline).then(|| {
                button("code-outline-show", "Outline", Look::Plain, true, tokens)
                    .child("Outline")
                    .child(key("O"))
                    .on_click(press(heard, caller, CodeGesture::Click { target: CodeClick::Outline }))
            }))
            .into_any_element(),
    )
}

/// The counts as the page's text reads them, for the facts' label.
fn counts_text(said: &argus_desktop_core::code::file::Facts) -> String {
    let mut text = String::new();
    if said.added > 0 {
        text.push_str(&format!("+{}", said.added));
    }
    if said.added > 0 && said.removed > 0 {
        text.push(' ');
    }
    if said.removed > 0 {
        text.push_str(&format!("\u{2212}{}", said.removed));
    }
    text
}
