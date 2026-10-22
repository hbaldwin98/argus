//! What the Feature stage puts in the strip: the counts beside the title, and at its end the
//! List and Board radio, the filter toggle that shows only when the window is narrow, and
//! `New feature` (`featuresStrip` of `feature_stage.ts`). With no repository the strip carries
//! nothing, and with a feature open it is that feature's, which the open-feature surface offers.
//!
//! The radio sends `SetLayout`; which is on is the layout Rust says the repository is in. The toggle
//! is the stage's: it opens the filter field, which a narrow window folds away.

use argus_desktop_core::board::capture_note::CaptureGesture;
use argus_desktop_core::board::features::Layout;
use argus_desktop_core::board::index_words::{counts, COUNTS_TIP};
use argus_desktop_core::chrome::strip::Says;
use argus_desktop_core::connection::page::{Call, Features};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Context, Entity, Role, SharedString, WeakEntity, Window};

use super::FeatureStage;
use crate::board::ctx::Ctx;
use crate::kit::button::{button, Look};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::segment;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the strip says of the stage: the counts, held and archived left out.
pub fn says(features: Option<&Features>) -> Says {
    match features {
        Some(features) if features.open.is_none() && !features.repository.is_empty() => Says { meta: counts(features), meta_tip: Some(COUNTS_TIP), ..Says::default() },
        _ => Says::default(),
    }
}

pub struct StripTools {
    ctx: Ctx,
    stage: WeakEntity<FeatureStage>,
}

impl StripTools {
    pub fn new(ctx: Ctx, stage: &Entity<FeatureStage>) -> Self {
        Self { ctx, stage: stage.downgrade() }
    }
}

/// The radio: List and Board, the layout Rust says is on.
fn radio(layout: Layout, ctx: &Ctx, tokens: &Tokens) -> impl IntoElement {
    segment::track("feature-layout", "Layout", Role::RadioGroup, tokens).children([(Layout::List, "List"), (Layout::Board, "Board")].map(|(this, words)| {
        let ctx = ctx.clone();
        segment::pill(SharedString::from(format!("layout-{}", this.word())), Role::RadioButton, layout == this, tokens)
            .aria_label(words)
            .child(words)
            .on_click(move |_, _, cx| ctx.send(Call::SetLayout { layout: this }, cx))
    }))
}

impl Render for StripTools {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(features) = self.ctx.features(cx).filter(|features| features.open.is_none() && !features.repository.is_empty()) else { return div().into_any_element() };
        let (ctx, stage) = (self.ctx.clone(), self.stage.clone());
        let narrow = self.stage.upgrade().is_some_and(|stage| stage.read(cx).narrow);
        h_flex()
            .id("features-tools")
            .gap(rem(tokens.sp_8))
            .items_center()
            .child(radio(argus_desktop_core::board::index_words::layout_of(&features), &self.ctx, &tokens))
            .when(narrow, |tools| {
                tools.child(
                    icon_button("feature-filter-toggle", "Filter features", false, &tokens)
                        .tooltip(tip("Filter features"))
                        .on_click(move |_, window, cx| {
                            stage.update(cx, |stage, cx| stage.toggle_filter(window, cx)).ok();
                        })
                        .child(Glyph::Search.at(tokens.icon_sm)),
                )
            })
            .child(button("new-feature", "New feature", Look::Primary, false, &tokens).child("New feature").on_click(move |_, _, cx| ctx.send(Call::Capture { gesture: CaptureGesture::Open }, cx)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::chrome::strip::Part;

    #[test]
    fn the_strip_says_the_counts_and_what_hovering_them_says() {
        let features = Features { repository: "argus".into(), active: 2, queued: 0, done: 1, ..Features::default() };
        let said = says(Some(&features));
        assert_eq!(said.meta, [Part::Attention("2 active".into()), Part::Text(" \u{b7} ".into()), Part::Ok("1 done".into())]);
        assert!(said.meta_tip.is_some_and(|tip| tip.starts_with("N new")));
    }

    #[test]
    fn with_no_repository_or_a_feature_open_the_strip_says_nothing() {
        assert_eq!(says(None), Says::default());
        assert_eq!(says(Some(&Features::default())), Says::default());
        let open = Features { repository: "argus".into(), active: 1, ..Features::default() };
        assert_ne!(says(Some(&open)), Says::default());
    }
}
