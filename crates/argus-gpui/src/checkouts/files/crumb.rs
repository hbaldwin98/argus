//! The folder the selection is in, and which files the tree lists: the checkout's name and the
//! folders down to the selected row's, then a two-way choice of All or Changed with how many
//! paths git says changed (`crumb`, `ck/files.ts`). Core says the crumb; a click on a mode sends
//! it and the mode changes when the page says it did.

use argus_desktop_core::checkouts::files::FilesGesture;
use argus_desktop_core::checkouts::files_page::{FilesPage, Mode};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, Role, SharedString, TestSupportExt as _};

use super::FilesPanel;
use crate::kit::segment::{count, pill, track};
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl FilesPanel {
    pub(super) fn crumb(&self, files: &FilesPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let last = files.crumb.len().saturating_sub(1);
        let mode = |id: &'static str, label: &'static str, value: Mode, shown: Option<usize>| {
            pill(id, Role::RadioButton, files.mode == value, tokens)
                .child(label)
                .when_some(shown.filter(|n| *n > 0), |pill, n| pill.child(count(n.to_string(), tokens)))
                .on_click(cx.listener(move |this, _, _, cx| this.ask(FilesGesture::Mode { mode: value }, cx)))
        };
        let all = mode("files-mode-all", "All", Mode::All, None);
        let changed = mode("files-mode-changed", "Changed", Mode::Changed, Some(files.changed));
        let parts = files.crumb.iter().enumerate().flat_map(|(at, part)| {
            let sep = (at > 0).then(|| div().child("/").into_any_element());
            let part = div()
                .id(("files-crumb-part", at))
                .test_support()
                .aria_label(SharedString::from(part.clone()))
                .when(at == last, |part| part.text_color(tokens.fg_muted))
                .child(SharedString::from(part.clone()))
                .into_any_element();
            sep.into_iter().chain([part])
        });
        // `.cf-crumbrow`: 8 between, 12 right, 8 under, 16 left.
        h_flex()
            .id("files-crumbrow")
            .test_support()
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_8))
            .pr(rem(tokens.sp_12))
            .pb(rem(tokens.sp_8))
            .pl(rem(tokens.sp_16))
            .child(
                h_flex()
                    .id("files-crumb")
                    .test_support()
                    .aria_label("Folder")
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .gap_x(rem(tokens.sp_6))
                    .font_family(SharedString::from(tokens.font_mono))
                    .text_size(rem(tokens.fs_sm))
                    .text_color(tokens.fg_faint)
                    .children(parts),
            )
            .child(track("files-mode", "Which files to list", Role::RadioGroup, tokens).child(all).child(changed))
    }
}
