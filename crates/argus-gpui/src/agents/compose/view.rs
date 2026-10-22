//! How the composer is drawn: the place line between two rules, the question, the card with the
//! message and its row of pickers and buttons, and the note and the keys under it (COMPOSE-1..3, -7, -13, -14).

use argus_desktop_core::agents::composer::{ComposerPage, Where};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::select::Select;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, Focusable as _, SharedString, TestSupportExt as _, Window};

use super::picker::Picker;
use super::ComposeView;
use crate::kit::button::{button, off, Look};
use crate::kit::icons::Glyph;
use crate::kit::keycap::{keycap, keycaps};
use crate::keys::caps;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// `.compose-rule`: the hairline either side of the place line.
fn rule(tokens: &Tokens) -> impl IntoElement {
    div().flex_shrink_0().w(rem(tokens.compose_rule_w)).h(rem(tokens.hairline)).bg(tokens.line_strong)
}

/// The words of the place line: `Starting in **argus** on `main``, as core says where.
fn place(place: &Where, tokens: &Tokens) -> AnyElement {
    let mono = SharedString::from(tokens.font_mono);
    let bold = |words: String| div().text_color(tokens.fg).weighted(tokens.weight_semibold).child(SharedString::from(words));
    let line = h_flex().id("compose-place").test_support().items_center().whitespace_nowrap();
    match place {
        Where::NoRepository => line.aria_label("Add a repository to the project to start here.").child("Add a repository to the project to start here.").into_any_element(),
        Where::Folded { repository } => {
            let words = format!("{repository} is folded on the rail; open it to pick a checkout.");
            line.aria_label(SharedString::from(words.clone())).child(SharedString::from(words)).into_any_element()
        }
        Where::Starting { worktree, name, branch } => {
            let said = format!("Starting in {}{name}{}", if *worktree { "the worktree " } else { "" }, branch.as_ref().map(|branch| format!(" on {branch}")).unwrap_or_default());
            line.aria_label(SharedString::from(said))
                .child(if *worktree { "Starting in the worktree\u{a0}" } else { "Starting in\u{a0}" })
                .child(bold(name.clone()))
                .children(branch.as_ref().map(|branch| h_flex().child("\u{a0}on\u{a0}").child(div().font_family(mono).text_color(tokens.fg).child(SharedString::from(branch.clone())))))
                .into_any_element()
        }
    }
}

/// `.pick`: a select in a small box, led by a glyph and read as `label`.
fn pick<V: Clone + PartialEq + 'static>(id: &'static str, label: &'static str, glyph: AnyElement, picker: &Picker<V>, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .aria_label(label)
        .flex_shrink_0()
        .h(rem(tokens.control_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_4))
        .items_center()
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_sm))
        .bg(tokens.bg_chrome)
        .text_color(tokens.fg)
        .hover(move |style| style.bg(tokens.fill).border_color(tokens.fg_faint))
        .child(div().flex_shrink_0().text_color(tokens.fg_faint).child(glyph))
        .child(
            div().max_w(rem(tokens.pick_max_w)).text_size(rem(tokens.fs_sm)).child(
                Select::new(&picker.state).id(SharedString::from(format!("{id}-select"))).appearance(false).accessibility_label(label).disabled(picker.is_empty()),
            ),
        )
}

impl Render for ComposeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let page: ComposerPage = self.page(cx);
        let keys = caps::presses(&self.heard.read(cx).keymap, Action::Compose, Place::Anywhere).into_iter().next();
        let typing = self.text.focus_handle(cx).contains_focused(window, cx);
        let mono = SharedString::from(tokens.font_mono);
        let chord = if cfg!(target_os = "macos") { "Cmd Enter" } else { "Ctrl Enter" };
        let (shell, start) = (cx.listener(|this, _, _, cx| this.start(false, cx)), cx.listener(|this, _, _, cx| this.start(true, cx)));
        let shell_button = if page.can_shell { button("compose-shell", "Shell", Look::Plain, false, &tokens).child("Shell").on_click(shell).into_any_element() } else { off("compose-shell", "Shell", false, &tokens).child("Shell").into_any_element() };
        let start_button = if page.can_start {
            button("compose-start", "Start agent", Look::Primary, false, &tokens)
                .child("Start agent")
                .child(keycap(chord, mono.clone(), &tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(tokens.share_edge)))
                .on_click(start)
                .into_any_element()
        } else {
            off("compose-start", "Start agent", false, &tokens).child("Start agent").child(keycap(chord, mono.clone(), &tokens)).into_any_element()
        };
        // `.composer`: the card the message and its controls sit in; the edge goes to the ring while the message has the keys.
        let card = v_flex()
            .id("composer")
            .test_support()
            .aria_label("Start an agent")
            .gap(rem(tokens.sp_12))
            .p(rem(tokens.sp_12))
            .border_1()
            .border_color(if typing { tokens.ring } else { tokens.line_strong })
            .rounded(rem(tokens.r_lg))
            .bg(tokens.bg_raised)
            .child(
                Textarea::new(&self.text)
                    .appearance(false)
                    .bordered(false)
                    .w_full()
                    .min_h(rem(tokens.area_min_h))
                    .typed(Type::new(tokens.weight_regular, tokens.fs_lg, 1.5))
                    .aria_label("What the agent should do"),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap(rem(tokens.sp_6))
                    .flex_wrap()
                    .child(pick("compose-repository", "Repository", Glyph::FolderSm.at(tokens.icon_sm).into_any_element(), &self.repository, &tokens))
                    .child(pick("compose-checkout", "Checkout", Glyph::Branch.at(tokens.icon_sm).into_any_element(), &self.checkout, &tokens))
                    .child(pick("compose-agent", "Agent", div().child("\u{25c6}").into_any_element(), &self.agent, &tokens))
                    .child(div().flex_1())
                    .child(shell_button)
                    .child(start_button),
            );
        h_flex()
            .id("compose-stage")
            .test_support()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .id("compose")
                    .w(rem(tokens.compose_w))
                    .max_w_full()
                    .gap(rem(tokens.sp_16))
                    .mt(-rem(tokens.compose_lift))
                    .child(h_flex().justify_center().items_center().gap(rem(tokens.sp_12)).typed(tokens.type_small).text_color(tokens.fg_muted).child(rule(&tokens)).child(place(&page.place, &tokens)).child(rule(&tokens)))
                    .child(div().id("compose-title").test_support().aria_label(SharedString::from(page.title.clone())).text_center().typed(tokens.type_hero).child(SharedString::from(page.title.clone())))
                    .child(card)
                    .child(
                        h_flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_x(rem(tokens.sp_16))
                            .gap_y(rem(tokens.sp_8))
                            .px(rem(tokens.sp_4))
                            .typed(tokens.type_small)
                            .text_color(tokens.fg_muted)
                            .child(
                                h_flex()
                                    .gap(rem(tokens.sp_6))
                                    .items_center()
                                    .child(Glyph::Info.at(tokens.icon_sm))
                                    .child("The message becomes the agent\u{2019}s first prompt. Shell starts a shell there and leaves it."),
                            )
                            .children(keys.map(|keys| h_flex().gap(rem(tokens.sp_6)).items_center().child(keycaps(&[keys], &tokens)).child(" opens this from anywhere"))),
                    ),
            )
    }
}
