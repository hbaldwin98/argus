//! What a gesture on a feature's card asks of the daemon: a card dropped on a column, and the
//! `H` and `Shift A` keys on the one under the cursor.
//!
//! Which moves a card offers, and what is said of one it does not, are `index_words`'; this sends
//! the one that was chosen and never predicts it, as everything on the stage does: the card moves
//! when a pushed board says so.

use argus_protocol::{ClientMsg, ProjectInfo};

use crate::board::feature_stage::{FeatureStage, Scope};
use crate::board::index_words::{self, FeatureColumn, Landing};
use crate::keys::keymap::Action;

/// A card dropped on `on`: an archive is sent, and a column that follows the work answers with the
/// notice that says why. A hold is not made here, since it needs a reason, which the window asks
/// for; nor is a drop on the card's own column.
pub fn dropped(stage: &FeatureStage, slug: &str, on: FeatureColumn, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) -> Option<String> {
    let row = stage.row(slug, scope, tree)?;
    match index_words::drop_verdict(&row, on) {
        Landing::Archive => stage.archive(slug, true, scope, out),
        Landing::Refuse(notice) => return Some(notice),
        Landing::Hold | Landing::Nothing => {}
    }
    None
}

/// `H` or `Shift A` on the feature under the cursor: a held one has its hold lifted, an archived
/// one is reopened, and any other is archived. The hold of one that is not held needs a reason, so
/// the window asks for it and this does nothing.
pub fn card_key(stage: &FeatureStage, action: Action, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
    let Some(slug) = stage.peeked() else { return };
    let Some(row) = stage.row(slug, scope, tree) else { return };
    let offers = |to: &str| row.moves.iter().any(|m| m.to == to);
    match action {
        Action::Hold if offers("unhold") => stage.hold(slug, None, scope, out),
        Action::Archive if offers("archived") => stage.archive(slug, true, scope, out),
        Action::Archive if offers("reopen") => stage.archive(slug, false, scope, out),
        _ => {}
    }
}
