//! The commit box under the Changes pane's lists: the message, and the button that commits what is
//! staged with the person's own `git commit`.
//!
//! The message is the box's own until a commit lands: the session counts the commits made from the
//! review (`StagingPage::commits`), and the box empties when that count moves, so a commit refused
//! by a hook keeps every word. Ctrl Enter (Cmd Enter on macOS) commits from the box.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::review::staging::StagingPage;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, Focusable as _, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::button::{button, inert, Look};
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct CommitBox {
    heard: Entity<Heard>,
    caller: Caller,
    text: Entity<TextareaState>,
    /// The commits the session had counted at the last update.
    commits: u64,
    _field: Registered,
}

/// What the button says: what a press would commit, or that one is being made.
pub fn words(page: &StagingPage, staged: usize) -> String {
    match (page.committing, staged) {
        (true, _) => "Committing\u{2026}".to_owned(),
        (false, 0) => "Stage a file to commit".to_owned(),
        (false, 1) => "Commit 1 file".to_owned(),
        (false, n) => format!("Commit {n} files"),
    }
}

impl CommitBox {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).rows(3).placeholder("Commit message"));
        let asking = cx.weak_entity();
        let field = Fields::of(cx).register_place(window, text.focus_handle(cx), Place::Writing, move |action, _, cx| match action {
            Action::Save => asking.update(cx, |commit, cx| commit.commit(cx)).is_ok(),
            _ => false,
        });
        Self { heard, caller, text, commits: 0, _field: field }
    }

    /// Follows the session: a commit counted since the last update empties the message.
    pub fn follow(&mut self, page: Option<&StagingPage>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = page else { return };
        if page.commits > self.commits {
            self.text.update(cx, |text, cx| text.set_value("", window, cx));
        }
        self.commits = page.commits;
        cx.notify();
    }

    fn commit(&self, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Commit { message: self.text.read(cx).value().to_string() });
        }
    }
}

impl Render for CommitBox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let page = self.heard.read(cx).review.clone();
        let Some((staging, staged)) = page.as_ref().and_then(|page| {
            let staged = page.sections.iter().find(|s| s.side == "staged").map_or(0, |s| s.files.len());
            page.staging.clone().map(|staging| (staging, staged))
        }) else {
            return div().into_any_element();
        };
        let said = words(&staging, staged);
        let ready = staging.can_commit && !staging.committing;
        // `.commit` (Git.dc.html): one bordered box holding the message and, under a hairline, the
        // key that commits at the left and the compact button at the right.
        let hint = if cfg!(target_os = "macos") { "Cmd Enter" } else { "Ctrl Enter" };
        let action = if ready {
            button("review-commit", said.clone(), Look::Primary, true, &tokens)
                .justify_center()
                .child(SharedString::from(said))
                .on_click(cx.listener(|this, _, _, cx| this.commit(cx)))
                .into_any_element()
        } else {
            inert("review-commit", said.clone(), Look::Plain, true, &tokens).bg(tokens.fill).text_color(tokens.fg_fainter).justify_center().child(SharedString::from(said)).into_any_element()
        };
        v_flex()
            .id("review-commit-box")
            .test_support()
            .flex_shrink_0()
            .m(rem(10.0))
            .border_1()
            .border_color(tokens.line)
            .rounded(rem(tokens.r_md))
            .bg(tokens.bg_sunken)
            .overflow_hidden()
            .child(Textarea::new(&self.text).appearance(false).bordered(false).w_full().aria_label("Commit message"))
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .py(rem(tokens.sp_6))
                    .pl(rem(tokens.sp_12))
                    .pr(rem(tokens.sp_6))
                    .border_t_1()
                    .border_color(tokens.line)
                    .child(div().flex_1().min_w_0().font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(hint))
                    .child(action),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(committing: bool) -> StagingPage {
        StagingPage { staged: false, busy: false, committing, commits: 0, can_commit: true }
    }

    #[test]
    fn the_button_says_what_it_commits_or_that_it_is_committing() {
        assert_eq!(words(&page(false), 0), "Stage a file to commit");
        assert_eq!(words(&page(false), 1), "Commit 1 file");
        assert_eq!(words(&page(false), 3), "Commit 3 files");
        assert_eq!(words(&page(true), 3), "Committing\u{2026}");
    }
}
