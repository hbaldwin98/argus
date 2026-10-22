//! Running one of a row's moves that a person sets, wherever it was chosen: the Move menu, a card
//! dropped on Held, or the `H` key on a feature that is not held (`runMove` of `feature_stage.ts`).
//!
//! A hold asks why first, so it is the window's; lifting a hold, archiving and reopening are
//! calls. Queued, Active and Done follow the work and are never run.

use argus_desktop_core::connection::page::{Call, FeatureRow};
use gpui_kit::{App, Window};

use super::hold;
use crate::board::ctx::Ctx;

pub fn run_move(ctx: &Ctx, row: &FeatureRow, to: &str, window: &mut Window, cx: &mut App) {
    let slug = row.slug.clone();
    match to {
        "held" => hold::open(slug, row.number, row.title.clone(), ctx.clone(), window, cx),
        "unhold" => ctx.send(Call::Hold { slug, reason: None }, cx),
        "archived" => ctx.send(Call::Archive { slug, archived: true }, cx),
        "reopen" => ctx.send(Call::Archive { slug, archived: false }, cx),
        _ => {}
    }
}
