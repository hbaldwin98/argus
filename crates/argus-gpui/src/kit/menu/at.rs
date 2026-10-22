//! A menu opened at a point by a key: the bind menu of a flow's step, which `d` opens under the lit
//! row or arrow, or under the pointer when a button is pressed (BOARD-FLOWS-8).
//!
//! The kit's menu is made for a button's click or a right-click; here a view builds the menu itself,
//! draws it over the window kept 8px inside it (SHELL-MENU-2), gives it the keys, and drops it when it
//! says it was dismissed, by Escape, a press outside or a choice.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{anchored, deferred, div, point, prelude::*, px, AnyElement, Context, DismissEvent, Entity, Focusable as _, Pixels, Point, Subscription, Window};

/// The menu being shown, and where.
pub struct At {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismissed: Subscription,
}

impl At {
    /// Builds a menu with `fill`, shows it at `position` and gives it the keys; `dismissed` is told
    /// when it goes, so the view drops this.
    pub fn open<V: 'static>(
        position: Point<Pixels>,
        fill: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
        window: &mut Window,
        cx: &mut Context<V>,
        dismissed: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let menu = PopupMenu::build(window, cx, fill);
        let focus = menu.focus_handle(cx);
        window.focus(&focus, cx);
        let subscription = cx.subscribe_in(&menu, window, move |this, _, _: &DismissEvent, window, cx| dismissed(this, window, cx));
        Self { menu, position, _dismissed: subscription }
    }

    /// The menu, over everything, at its place: a layer as big as the window with the menu anchored in it.
    pub fn element(&self, window: &Window) -> AnyElement {
        deferred(
            anchored().child(
                div().w(window.bounds().size.width).h(window.bounds().size.height).child(
                    anchored().position(self.position).snap_to_window_with_margin(px(8.0)).child(self.menu.clone()),
                ),
            ),
        )
        .with_priority(gpui_kit::base::POPUP_PRIORITY)
        .into_any_element()
    }

    /// Where it was opened.
    pub fn at(&self) -> Point<Pixels> {
        self.position
    }

    /// The place the menu has when nothing says where: a third of the way down the window, a little
    /// left of its middle, as the page's `placeMenu` is given.
    pub fn centred(window: &Window, tokens: &crate::theme::Tokens) -> Point<Pixels> {
        let size = window.bounds().size;
        point(size.width / 2.0 - crate::theme::scale::pixels(tokens.menu_inset, window.rem_size()), size.height / 3.0)
    }
}
