//! The panel's header: its kicker and the checkout it reads, and the buttons that show it in the
//! file manager (only for a daemon on this machine), open a shell on it, read it again and put
//! it away (`head`, `ck/files.ts`).

use argus_desktop_core::checkouts::files::FilesGesture;
use argus_desktop_core::checkouts::files_page::FilesPage;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::FilesPanel;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl FilesPanel {
    pub(super) fn head(&self, files: &FilesPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let act = |id: &'static str, words: String, glyph: Glyph, size: f32, gesture: FilesGesture| {
            icon_button(id, words.clone(), false, tokens)
                .tooltip(tip(words))
                .child(glyph.at(size))
                .on_click(cx.listener(move |this, _, _, cx| this.ask(gesture.clone(), cx)))
                .into_any_element()
        };
        let mut buttons = Vec::new();
        if files.can_reveal {
            buttons.push(act("files-reveal", files.reveal.clone(), Glyph::FolderSm, tokens.icon_xs, FilesGesture::Reveal { path: None }));
        }
        buttons.push(act("files-shell", "Open a shell here".into(), Glyph::Console, tokens.icon_sm, FilesGesture::Shell));
        buttons.push(act("files-reread", "Read the folder again".into(), Glyph::Refresh, tokens.icon_sm, FilesGesture::Reread));
        buttons.push(act("files-close", "Close the files (Esc)".into(), Glyph::Close, tokens.icon_sm, FilesGesture::Close));
        // `.cf-head`: 12px over, 12 right, 8 under, 16 left, 6 between.
        h_flex()
            .id("files-head")
            .test_support()
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_6))
            .pt(rem(tokens.sp_12))
            .pr(rem(tokens.sp_12))
            .pb(rem(tokens.sp_8))
            .pl(rem(tokens.sp_16))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_8))
                    .child(div().id("files-kicker").test_support().aria_label("Files").text_size(rem(tokens.fs_md)).weighted(tokens.weight_bold).child("Files"))
                    .child(
                        div()
                            .id("files-checkout")
                            .test_support()
                            .aria_label(SharedString::from(files.name.clone()))
                            .min_w_0()
                            .truncate()
                            .font_family(SharedString::from(tokens.font_mono))
                            .text_size(rem(tokens.fs_sm))
                            .text_color(tokens.fg_faint)
                            .tooltip(tip(files.root.clone()))
                            .child(SharedString::from(files.name.clone())),
                    ),
            )
            .children(buttons)
    }
}
