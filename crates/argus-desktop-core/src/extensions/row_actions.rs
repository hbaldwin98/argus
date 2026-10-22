//! Which of an extension view's actions are drawn where: on a row, in the view's own bar, in its
//! menu with the key beside them, and how the row's bar gives way to one button as the view narrows.
//!
//! The page held these in `extensions/row_actions.ts` and inline in `views.ts`; they are rules of
//! the view and not of a toolkit, so both windows read them from here.

use super::views::{ExtTable, ExtTableAction};

/// The width, in the page's pixels, at and under which a row's actions are one "..." button; above
/// it they are text buttons, never both (`.vtable-acts` container query at 560).
pub const MENU_AT: f32 = 560.0;

/// Whether the row's actions are behind one button in a view `width` wide. A width of zero is a
/// view not yet measured, which has the text buttons.
pub fn is_menu(width: f32) -> bool {
    width > 0.0 && width <= MENU_AT
}

/// Whether a row shows its actions: while it is the selected one or the pointer is on it, so a
/// keyboard user, whose selection moves with the keys, always has them.
pub fn shows(key: &str, selected: Option<&str>, hovered: Option<&str>) -> bool {
    selected == Some(key) || hovered == Some(key)
}

/// The actions a row offers: every one that is not the view's as a whole.
pub fn per_row(view: &ExtTable) -> Vec<&ExtTableAction> {
    view.actions.iter().filter(|action| !action.view_wide).collect()
}

/// The actions of the view as a whole. An ended console shows its own beside how it ended, so the
/// view's header leaves them out then.
pub fn wide(view: &ExtTable, ended_console_shows: bool) -> Vec<&ExtTableAction> {
    let ended = ended_console_shows && view.console.as_ref().is_some_and(|console| console.ended.is_some());
    view.actions.iter().filter(|action| action.view_wide && !ended).collect()
}

/// The per-row actions that answer to a key, which the view's menu lists once in place of a key cap
/// on every row.
pub fn keyed(view: &ExtTable) -> Vec<&ExtTableAction> {
    per_row(view).into_iter().filter(|action| action.key.is_some()).collect()
}

/// Why a keyed action is refused in the view's menu, when it is: it acts on the selected row.
pub fn refused(view: &ExtTable) -> Option<&'static str> {
    view.selected.is_none().then_some("no row selected")
}

/// A row action's tooltip: its label, and its key when it has one, since the button carries no cap.
pub fn tooltip(action: &ExtTableAction) -> Option<String> {
    action.key.as_ref().map(|key| format!("{} ({key})", action.label))
}

#[cfg(test)]
mod tests;
