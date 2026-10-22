//! The two menus the index opens on a feature, and the dialog that asks why one is held: Move to
//! (at the pointer on a right click, under the peek's `...`, or beside the row on `M`) and the
//! Comes-after menu it leads to (`moveMenu`, `afterMenu` of `feature_stage.ts`).
//!
//! One menu is open at a time, and it is drawn from the row and the peek as the last snapshot says
//! them: a snapshot that takes its feature away closes it. A choice closes the menu, then runs. Its
//! keys (Up and Down, `j` and `k`, wrapping; Enter; Escape) are heard by the Feature layer while it
//! is open, and every other key of the stage is held (`feature_stage/keys`).

pub mod card;
pub mod hold;
pub mod items;
pub mod run;

use argus_desktop_core::connection::page::{AfterChoice, Call, FeatureRow};
use gpui_kit::{anchored, deferred, div, prelude::*, Context, Pixels, Point, SharedString, Window};

use super::ctx::Ctx;
use items::{comes_after, enabled, moves, stepped, Does, Entry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Move { with_after: bool },
    After,
}

struct Open {
    slug: String,
    kind: Kind,
    at: Point<Pixels>,
    cursor: Option<usize>,
}

pub struct Menu {
    ctx: Ctx,
    open: Option<Open>,
}

impl Menu {
    pub fn new(ctx: Ctx) -> Self {
        Self { ctx, open: None }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The Move menu of `slug` at `at`; `with_after` adds `Comes after...`, which only the peek's
    /// opening offers. The first choice that can be made is highlighted.
    pub fn open_move(&mut self, slug: &str, at: Point<Pixels>, with_after: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.show(slug, Kind::Move { with_after }, at, cx);
    }

    fn show(&mut self, slug: &str, kind: Kind, at: Point<Pixels>, cx: &mut Context<Self>) {
        let mut open = Open { slug: slug.to_owned(), kind, at, cursor: None };
        if let Some((row, after)) = self.row(slug, cx) {
            open.cursor = stepped(&enabled(&entries(&open, &row, &after)), None, 1);
        }
        self.open = Some(open);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open.take().is_some() {
            cx.notify();
        }
    }

    /// The row and what its feature may come after, as the last snapshot says them.
    fn row(&self, slug: &str, cx: &Context<Self>) -> Option<(FeatureRow, Vec<AfterChoice>)> {
        let features = self.ctx.features(cx)?;
        let row = features.rows.iter().find(|row| row.slug == slug)?.clone();
        let after = features.peek.as_ref().filter(|peek| peek.slug == slug).map(|peek| peek.after.clone()).unwrap_or_default();
        Some((row, after))
    }

    /// The highlight `by` choices on, wrapping.
    pub fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(open) = &self.open else { return };
        let Some((row, after)) = self.row(&open.slug, cx) else { return };
        let list = enabled(&entries(open, &row, &after));
        let cursor = stepped(&list, open.cursor, by);
        if let Some(open) = &mut self.open {
            open.cursor = cursor;
        }
        cx.notify();
    }

    /// Chooses the highlighted item.
    pub fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.open else { return };
        let Some((row, after)) = self.row(&open.slug, cx) else { return };
        let list = entries(open, &row, &after);
        if let Some(Entry::Item(item)) = open.cursor.and_then(|at| list.get(at)) {
            let does = item.does.clone();
            self.chosen(&does, window, cx);
        }
    }

    /// Closes the menu and runs `does`.
    fn chosen(&mut self, does: &Does, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Open { slug, at, .. }) = self.open.take() else { return };
        cx.notify();
        let Some((row, _)) = self.row(&slug, cx) else { return };
        match does {
            Does::GoToPane(pane) => self.ctx.send(Call::GoToPane { pane: *pane }, cx),
            Does::Move(to) => run::run_move(&self.ctx, &row, to, window, cx),
            Does::ComesAfter => self.show(&slug, Kind::After, at, cx),
            Does::Wait { on, waits } => self.ctx.send(Call::Wait { slug, on: on.clone(), waits: !waits }, cx),
        }
    }
}

fn entries(open: &Open, row: &FeatureRow, after: &[AfterChoice]) -> Vec<Entry> {
    match open.kind {
        Kind::Move { with_after } => moves(row, if with_after { after } else { &[] }),
        Kind::After => comes_after(row, after),
    }
}

impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else { return div().into_any_element() };
        // A snapshot that took the feature away takes the menu with it.
        let Some((row, after)) = self.row(&open.slug, cx) else {
            self.open = None;
            return div().into_any_element();
        };
        let tokens = *crate::theme::Tokens::global(cx);
        let list = entries(open, &row, &after);
        let label = match &list[0] {
            Entry::Label(words) => SharedString::from(words.clone()),
            _ => SharedString::default(),
        };
        let this = cx.entity();
        let (card, at) = (
            card::card(label, &list, open.cursor, move |_, does, window, cx| this.update(cx, |menu, cx| menu.chosen(does, window, cx)), &tokens),
            open.at,
        );
        let out = cx.entity();
        deferred(anchored().position(at).snap_to_window_with_margin(crate::theme::scale::pixels(tokens.menu_margin, window.rem_size())).child(card.on_mouse_down_out(move |_, _, cx| out.update(cx, |menu, cx| menu.close(cx))))).into_any_element()
    }
}

#[cfg(test)]
mod tests;
