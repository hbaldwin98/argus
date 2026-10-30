//! The stage's tabs in the title bar, right after the breadcrumb: Git's Changes and History, Code's
//! All files and Changed.
//!
//! A stage's view hands the bar a [`Tabs`] (the words, whether each is the one showing and what its
//! press does) through its [`Offer`](super::slots::Offer); the bar draws them as text with a 2px
//! accent underline under the selected one. A press only calls: the tab that shows is the session's
//! to say, so nothing here keeps which is selected.

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, App, ElementId, Role, SharedString, Window};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a press of a tab does.
pub type Press = Rc<dyn Fn(&mut Window, &mut App)>;

/// One tab.
pub struct Tab {
    /// Names the tab for a test and a screen reader.
    pub id: &'static str,
    pub label: SharedString,
    /// What is behind it, faint after the label.
    pub count: Option<usize>,
    pub on: bool,
    pub press: Press,
}

impl Tab {
    pub fn new(id: &'static str, label: impl Into<SharedString>, on: bool, press: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { id, label: label.into(), count: None, on, press: Rc::new(press) }
    }

    pub fn count(mut self, count: Option<usize>) -> Self {
        self.count = count;
        self
    }
}

/// A stage's tabs, and what it says faint after them (Git's `N files · +a −r`).
pub struct Tabs {
    /// Names the group for a screen reader.
    pub label: &'static str,
    /// Names the group for a test.
    pub id: &'static str,
    pub tabs: Vec<Tab>,
    pub after: Option<AnyElement>,
}

impl Tabs {
    pub fn new(id: &'static str, label: &'static str, tabs: Vec<Tab>) -> Self {
        Self { id, label, tabs, after: None }
    }

    pub fn after(mut self, after: Option<AnyElement>) -> Self {
        self.after = after;
        self
    }
}

/// The tabs of `tabs`, 24px in from the crumb and the height of the bar so the underline sits on
/// the bar's hairline. They are not under the drag area: a press on Windows would move the window.
pub fn tabs(tabs: Tabs, tokens: &Tokens) -> impl IntoElement {
    let Tabs { label, id, tabs, after } = tabs;
    h_flex()
        .flex_none()
        .h_full()
        .items_center()
        .ml(rem(tokens.sp_24))
        .child(
            h_flex()
                .id(id)
                .role(Role::TabList)
                .test_support()
                .aria_label(SharedString::from(label))
                .h_full()
                .gap(rem(tokens.sp_16))
                .children(tabs.into_iter().map(|tab| one(tab, tokens))),
        )
        .children(after.map(|after| div().ml(rem(tokens.sp_16)).child(after)))
}

fn one(tab: Tab, tokens: &Tokens) -> impl IntoElement {
    let Tab { id, label, count, on, press } = tab;
    let (fg, hover) = (if on { tokens.fg } else { tokens.fg_muted }, tokens.fg);
    h_flex()
        .id(ElementId::from(id))
        .role(Role::Tab)
        .test_support()
        .aria_selected(on)
        .relative()
        .h_full()
        .gap(rem(tokens.sp_4))
        .items_center()
        .typed(tokens.type_body)
        .weighted(tokens.weight_medium)
        .text_color(fg)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(!on, |tab| tab.hover(move |style| style.text_color(hover)))
        .child(label)
        .children(count.map(|count| div().text_color(tokens.fg_faint).weighted(tokens.weight_regular).child(SharedString::from(count.to_string()))))
        // The selected tab's rule is drawn over the bar's hairline, as the board's inset shadow is.
        .when(on, |tab| tab.child(div().absolute().left_0().right_0().bottom_0().h(rem(tokens.sp_2)).bg(tokens.accent)))
        .on_click(move |_, window, cx| press(window, cx))
}
