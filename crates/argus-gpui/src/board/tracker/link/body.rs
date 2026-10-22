//! What the link dialog draws: the head with what linking keeps, the URL and role fields with the
//! reading beneath the URL, the tickets already linked and the map (`rows`, `map`), and the foot with
//! Cancel and Link (BOARD-LINK-2, -3).

use argus_desktop_core::board::tracker::link_dialog::{LinkGesture, LinkPage};
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString, TestSupportExt as _, Window};

use super::LinkView;
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::{esc, field_label, foot, meta};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What linking keeps, under the title.
const KEPT: &str = "Argus keeps each ticket\u{2019}s key and URL. Titles and statuses stay in the tracker, so nothing shown here can go stale.";
/// What a URL that has not been typed says it will be read as.
const HINT: &str = "Linear, Jira and GitHub URLs name their ticket; any other is its host and last path segment.";
/// The foot's note, and its tooltip.
const NOTHING_WRITTEN: &str = "Nothing is written to a tracker by linking.";
const KEYS: &str = "J and K move through the tickets; O opens one, X unlinks it.";

impl LinkView {
    /// How tall the dialog is estimated to be, in the page's pixels: the page centres it, and the kit's
    /// dialog sits near the top unless told how far down to go.
    pub(super) fn height(&self, tokens: &Tokens, cx: &gpui_kit::App) -> f32 {
        let small = tokens.type_small.size * tokens.type_small.leading;
        let title = tokens.type_title.size * tokens.type_title.leading;
        let label = tokens.fs_md * 1.4;
        let Some(page) = self.page(cx) else { return 0.0 };
        let head = tokens.sp_24 + title + tokens.sp_6 + 2.0 * small + tokens.sp_16;
        let field = label + tokens.sp_6 + tokens.control_h + tokens.sp_6 + small;
        let linked = label + tokens.sp_6 + if page.linked.is_empty() { small } else { page.linked.len() as f32 * (tokens.control_h_sm + tokens.sp_12) };
        let map = match &page.editing_map {
            Some(_) => label + tokens.sp_6 + tokens.sp_24 * 12.0 + tokens.sp_6 + tokens.control_h,
            None => label + tokens.sp_6 + small * 2.0 + tokens.sp_6 + tokens.control_h_sm,
        };
        let body = tokens.sp_32 + tokens.sp_16 + field + tokens.sp_12 + (field - small) + tokens.sp_12 + linked + tokens.sp_12 + map + tokens.sp_24;
        let foot = 2.0 * tokens.sp_16 + tokens.control_h + tokens.hairline;
        head + body + foot
    }

    fn head(&self, page: &LinkPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let title = SharedString::from(format!("Link #{} to a ticket", page.number));
        h_flex()
            .flex_shrink_0()
            .items_start()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_24))
            .pt(rem(tokens.sp_24))
            .pb(rem(tokens.sp_16))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_6))
                    .child(div().id("link-title").test_support().aria_label(title.clone()).typed(tokens.type_title).text_color(tokens.fg).child(title))
                    .child(meta(KEPT, tokens)),
            )
            .child(
                icon_button("link-close", "Close", false, tokens)
                    .on_click(cx.listener(|this, _, _, _| this.send(LinkGesture::Close)))
                    .child(Glyph::Close.at(tokens.icon_sm)),
            )
    }

    /// What the URL reads as, or why it does not, or what would be read (BOARD-LINK-3).
    fn reading(page: &LinkPage, tokens: &Tokens) -> AnyElement {
        if let Some(read) = &page.read {
            return h_flex()
                .id("link-read")
                .test_support()
                .aria_label(SharedString::from(format!("{} key {}", read.tracker, read.key)))
                .gap(rem(tokens.sp_4))
                .typed(tokens.type_small)
                .text_color(tokens.fg_muted)
                .child(div().text_color(tokens.ok).child("\u{2713}"))
                .child(SharedString::from(format!("{} \u{b7} key", read.tracker)))
                .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg).child(SharedString::from(read.key.clone())))
                .child("\u{b7} read from the URL, without going online")
                .into_any_element();
        }
        match &page.refusal {
            Some(why) => meta(why.clone(), tokens).text_color(tokens.attention).into_any_element(),
            None => meta(HINT, tokens).into_any_element(),
        }
    }

    fn fields(&self, page: &LinkPage, tokens: &Tokens) -> impl IntoElement {
        let labelled = |words: &'static str, field: &crate::kit::field::Field, id: &'static str, under: Option<AnyElement>| {
            v_flex()
                .gap(rem(tokens.sp_6))
                .child(field_label(words, tokens))
                .child(div().id(id).child(Input::new(field.input()).aria_label(words).h(rem(tokens.control_h))))
                .children(under)
        };
        v_flex()
            .gap(rem(tokens.sp_12))
            .child(labelled("Ticket URL", &self.url, "link-url", Some(Self::reading(page, tokens))))
            .child(labelled("What it is to this feature", &self.role, "link-role", None))
    }

    fn foot(&self, page: &LinkPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let words = match (&page.read, page.pending) {
            (_, true) => "Linking\u{2026}".to_owned(),
            (Some(read), false) => format!("Link {}", read.key),
            (None, false) => "Link".to_owned(),
        };
        let mono = SharedString::from(tokens.font_mono);
        let enter = keycap("Enter", mono, tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(0.5));
        let linking = if page.read.is_some() && !page.pending {
            button("link-link", words.clone(), Look::Primary, false, tokens).child(SharedString::from(words)).child(enter).on_click(cx.listener(|this, _, _, cx| this.link(cx))).into_any_element()
        } else {
            off("link-link", words.clone(), false, tokens).child(SharedString::from(words)).child(keycap("Enter", SharedString::from(tokens.font_mono), tokens)).into_any_element()
        };
        foot(tokens)
            .child(
                h_flex()
                    .id("link-why")
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .tooltip(tip(KEYS))
                    .child(Glyph::Info.at(tokens.icon_sm))
                    .child(NOTHING_WRITTEN),
            )
            .child(
                button("link-cancel", "Cancel", Look::Plain, false, tokens)
                    .child("Cancel")
                    .child(esc(tokens))
                    .on_click(cx.listener(|this, _, _, _| this.send(LinkGesture::Close))),
            )
            .child(linking)
    }
}

impl Render for LinkView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.page(cx) else { return div().into_any_element() };
        v_flex()
            .id("link-dialog")
            .role(Role::Dialog)
            .test_support()
            .aria_label("Link a ticket")
            .track_focus(&self.card)
            .max_h(window.viewport_size().height * tokens.sheet_max_share)
            .child(self.head(&page, &tokens, cx))
            .child(
                v_flex()
                    .id("link-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.body)
                    .gap(rem(tokens.sp_12))
                    .px(rem(tokens.sp_24))
                    .pt(rem(tokens.sp_16))
                    .pb(rem(tokens.sp_24))
                    .child(self.fields(&page, &tokens))
                    .child(self.linked(&page, &tokens, cx))
                    .child(self.map(&page, &tokens, cx)),
            )
            .child(self.foot(&page, &tokens, cx))
            .into_any_element()
    }
}
