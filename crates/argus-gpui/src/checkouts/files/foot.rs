//! The panel's foot: the selected entry's name, its size and when it changed, and for a file the
//! button that opens it in Code; with nothing selected, a line asking for a file (`foot`,
//! `ck/files.ts`). What it says is core's (`FilesFooter`).

use argus_desktop_core::checkouts::files::{FilesGesture, OpenIn};
use argus_desktop_core::checkouts::files_page::FilesPage;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::FilesPanel;
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl FilesPanel {
    pub(super) fn foot(&self, files: &FilesPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = SharedString::from(tokens.font_mono);
        // `.cf-foot`: a hairline over, 52px at least, 8 between, 8 and 12 and 16 round.
        let foot = h_flex()
            .id("files-foot")
            .test_support()
            .flex_shrink_0()
            .min_h(rem(tokens.files_foot_h))
            .items_center()
            .gap(rem(tokens.sp_8))
            .border_t_1()
            .border_color(tokens.line)
            .pt(rem(tokens.sp_8))
            .pr(rem(tokens.sp_12))
            .pb(rem(tokens.sp_8))
            .pl(rem(tokens.sp_16));
        let Some(selected) = &files.footer else {
            return foot.child(div().id("files-hint").test_support().aria_label("Select a file.").flex_1().typed(tokens.type_small).text_color(tokens.fg_faint).child("Select a file."));
        };
        foot.child(div().flex_shrink_0().text_color(tokens.fg_muted).child(if selected.dir { Glyph::FolderSm } else { Glyph::FileSm }.at(tokens.icon_xs)))
            .child(
                div()
                    .id("files-fname")
                    .test_support()
                    .aria_label(SharedString::from(selected.name.clone()))
                    .min_w_0()
                    .truncate()
                    .font_family(mono.clone())
                    .text_size(rem(tokens.fs_sm))
                    .weighted(tokens.weight_semibold)
                    .child(SharedString::from(selected.name.clone())),
            )
            .child(
                div()
                    .id("files-detail")
                    .test_support()
                    .aria_label(SharedString::from(selected.detail.clone()))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(SharedString::from(selected.detail.clone())),
            )
            .when(!selected.dir, |foot| {
                let path = files.selected.clone();
                foot.child(
                    button("files-foot-open", "Open in Code", Look::Plain, true, tokens)
                        .child("Open in Code")
                        .child(keycap("Enter", mono, tokens))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(path) = path.clone() {
                                this.ask(FilesGesture::Open { path, how: OpenIn::Code }, cx);
                            }
                        })),
                )
            })
    }
}
