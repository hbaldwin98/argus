//! Which gesture a key is while a tab of the Git stage holds the keys: the
//! review's (Changes), the history's (History), or the jump field's.
//!
//! The table is `argus-desktop-core`'s `BINDINGS`, the one the page reads: the
//! layer of the review hears the `Review` place and holds every other key
//! without sending it (`LAYERS`), the history the `History` place the same
//! way, and the jump field its `Jump` place. The
//! comment box's send and cancel are gpui-kit's `Enter` and `Escape` actions
//! (`review/comment`), not this table. So a key added to the table is heard
//! here, and a key that is not there is typed or ignored, never invented.

use argus_desktop_core::connection::page::{Call, ReviewJumpGesture};
use argus_desktop_core::keys::keymap::{Action, Place, BINDINGS};
use argus_desktop_core::keys::page_key::PageKey;

/// What a key does in the review.
#[derive(Debug, Clone)]
pub enum Press {
    /// A call for the session.
    Ask(Call),
    /// Escape: the stage shuts, which sends the close of whatever is open
    /// (`entry::leave`).
    Close,
    /// `/` in the history: its filter field takes the keys. Nothing is sent,
    /// the page handles it itself (SHARED-9).
    Field,
}

/// The action `page` is bound to in `place`, leader-less.
fn bound(place: Place, page: &PageKey) -> Option<Action> {
    BINDINGS
        .iter()
        .filter(|binding| binding.place == place)
        .find(|binding| binding.chords.iter().any(|chord| !chord.leader && chord.matches(page)))
        .map(|binding| binding.action)
}

/// A key pressed in the review with no field holding it: its own action, or
/// Escape from `Anywhere`'s bindings. Any other is held, so it is not typed
/// into the pane under the stage.
pub fn review(page: &PageKey) -> Option<Press> {
    if let Some(action) = bound(Place::Review, page) {
        return Some(Press::Ask(Call::ReviewAction { action }));
    }
    (bound(Place::Anywhere, page) == Some(Action::CloseLayer)).then_some(Press::Close)
}

/// A key pressed in the history with no field holding it: the history's own,
/// `/` for its filter field, or Escape. Any other is held, as in the review.
pub fn history(page: &PageKey) -> Option<Press> {
    match bound(Place::History, page) {
        Some(Action::Filter) => return Some(Press::Field),
        Some(action) => return Some(Press::Ask(Call::HistoryAction { action, row: None })),
        None => {}
    }
    (bound(Place::Anywhere, page) == Some(Action::CloseLayer)).then_some(Press::Close)
}

/// What the jump field's key does: its step, its choice or its close.
pub fn jump_gesture(action: Action) -> Option<ReviewJumpGesture> {
    match action {
        Action::Next => Some(ReviewJumpGesture::Step { by: 1 }),
        Action::Previous => Some(ReviewJumpGesture::Step { by: -1 }),
        Action::Choose => Some(ReviewJumpGesture::Choose),
        Action::CloseLayer => Some(ReviewJumpGesture::Close),
        _ => None,
    }
}

/// A key in the jump field the field's own text handling would not take:
/// Ctrl-N and Ctrl-P step as in fzf, and must not reach the pane.
pub fn jump(page: &PageKey) -> Option<ReviewJumpGesture> {
    bound(Place::Jump, page).and_then(jump_gesture)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> PageKey {
        PageKey { key: name.into(), ..PageKey::default() }
    }

    fn ctrl(name: &str) -> PageKey {
        PageKey { ctrl: true, ..key(name) }
    }

    fn asks(page: &PageKey) -> Option<String> {
        match review(page)? {
            Press::Ask(call) => Some(format!("{call:?}")),
            Press::Close => Some("close".into()),
            Press::Field => Some("field".into()),
        }
    }

    fn history_asks(page: &PageKey) -> Option<String> {
        match history(page)? {
            Press::Ask(call) => Some(format!("{call:?}")),
            Press::Close => Some("close".into()),
            Press::Field => Some("field".into()),
        }
    }

    #[test]
    fn the_reviews_keys_are_the_tables_review_place() {
        assert_eq!(asks(&key("j")).as_deref(), Some("ReviewAction { action: Next }"));
        assert_eq!(asks(&key("ArrowUp")).as_deref(), Some("ReviewAction { action: Previous }"));
        assert_eq!(asks(&key("V")).as_deref(), Some("ReviewAction { action: Pick }"));
        assert_eq!(asks(&key("c")).as_deref(), Some("ReviewAction { action: Comment }"));
        assert_eq!(asks(&key("]")).as_deref(), Some("ReviewAction { action: NextFile }"));
        assert_eq!(asks(&key("Enter")).as_deref(), Some("ReviewAction { action: Unfold }"));
        assert_eq!(asks(&key("z")).as_deref(), Some("ReviewAction { action: UnfoldAll }"));
        assert_eq!(asks(&ctrl("p")).as_deref(), Some("ReviewAction { action: Filter }"));
    }

    #[test]
    fn escape_shuts_the_stage_and_other_keys_are_held_not_sent() {
        assert!(matches!(review(&key("Escape")), Some(Press::Close)));
        // The stage digits, the palette and the rest of `Anywhere` are not the review's.
        for held in [key("1"), key("`"), ctrl("k"), key("x"), key("q")] {
            assert!(review(&held).is_none(), "{held:?}");
        }
    }

    #[test]
    fn the_jump_field_steps_with_ctrl_n_and_ctrl_p_and_a_bare_n_is_typed() {
        assert!(matches!(jump(&ctrl("n")), Some(ReviewJumpGesture::Step { by: 1 })));
        assert!(matches!(jump(&ctrl("p")), Some(ReviewJumpGesture::Step { by: -1 })));
        assert!(matches!(jump(&key("ArrowDown")), Some(ReviewJumpGesture::Step { by: 1 })));
        assert!(matches!(jump(&key("Enter")), Some(ReviewJumpGesture::Choose)));
        assert!(matches!(jump(&key("Escape")), Some(ReviewJumpGesture::Close)));
        assert!(jump(&key("n")).is_none());
    }

    /// HISTORY-7: the table's History place, `H` and `h` apart, `/` unsent.
    #[test]
    fn the_historys_keys_are_the_tables_history_place() {
        for (name, call) in [
            ("j", "HistoryAction { action: Next, row: None }"),
            ("ArrowDown", "HistoryAction { action: Next, row: None }"),
            ("k", "HistoryAction { action: Previous, row: None }"),
            ("Home", "HistoryAction { action: First, row: None }"),
            ("End", "HistoryAction { action: Last, row: None }"),
            ("l", "HistoryAction { action: Unfold, row: None }"),
            ("ArrowRight", "HistoryAction { action: Unfold, row: None }"),
            ("h", "HistoryAction { action: Fold, row: None }"),
            ("ArrowLeft", "HistoryAction { action: Fold, row: None }"),
            ("Enter", "HistoryAction { action: Choose, row: None }"),
            ("o", "HistoryAction { action: AsReview, row: None }"),
            ("O", "HistoryAction { action: AsReview, row: None }"),
            ("c", "HistoryAction { action: OpenCode, row: None }"),
            ("C", "HistoryAction { action: OpenCode, row: None }"),
            ("/", "field"),
            ("Escape", "close"),
        ] {
            assert_eq!(history_asks(&key(name)).as_deref(), Some(call), "{name}");
        }
        assert_eq!(history_asks(&key("H")).as_deref(), Some("HistoryAction { action: FoldAll, row: None }"));
    }

    #[test]
    fn the_history_holds_every_key_it_does_not_bind() {
        for held in [key("1"), key("`"), ctrl("k"), key("x"), key("q"), ctrl("p")] {
            assert!(history(&held).is_none(), "{held:?}");
        }
    }
}
