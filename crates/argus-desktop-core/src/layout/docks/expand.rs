//! Where a panel put on the stage came from — its dock, or its dialog
//! where it sat and as big — kept so that sending it back returns it
//! there, and not to a dock it never sat in.

use serde::{Deserialize, Serialize};

use super::{Dock, Docks, FloatBounds, Floating, Panel, Place};

/// A panel on the stage and the place it left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub panel: Panel,
    pub from: From,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "place")]
pub enum From {
    Dock { dock: Dock },
    Float { bounds: FloatBounds },
}

impl Docks {
    /// Where the panel is now, as a panel put on the stage from there would
    /// go back to it; a panel in no dock or dialog has nowhere to go back to.
    pub(super) fn origin(&self, panel: &Panel) -> Option<Origin> {
        let from = match self.place_of(panel)? {
            Place::Dock(dock) => From::Dock { dock },
            Place::Float => {
                let bounds = self.layout.floating.iter().find(|f| &f.panel == panel)?.bounds;
                From::Float { bounds }
            }
            Place::Stage => return None,
        };
        Some(Origin { panel: panel.clone(), from })
    }

    /// Where the panel on the stage goes back to, if it came from somewhere.
    pub fn way_back(&self, panel: &Panel) -> Option<From> {
        self.layout.origins.iter().find(|o| &o.panel == panel).map(|o| o.from)
    }

    /// Sends a panel on the stage, or one never placed, back to where it
    /// was put on the stage from, shown there. One with nowhere kept goes
    /// to its home dock, else the right. A panel in a dock or a dialog is
    /// there already.
    pub fn collapse(&mut self, panel: &Panel) {
        if matches!(self.place_of(panel), Some(Place::Dock(_) | Place::Float)) {
            return;
        }
        let from = self.way_back(panel).unwrap_or(From::Dock { dock: panel.home().unwrap_or(Dock::Right) });
        self.change(|layout| {
            layout.lift(panel);
            match from {
                From::Dock { dock } => layout.dock_mut(dock).tabs.push(panel.clone()),
                From::Float { bounds } => layout.floating.push(Floating { panel: panel.clone(), bounds: bounds.held() }),
            }
        });
        self.show(panel);
    }
}

#[cfg(test)]
mod tests {
    use super::super::DocksLayout;
    use super::*;

    fn docks() -> Docks {
        let mut docks = Docks::new(DocksLayout::default());
        docks.set_present(vec![Panel::Console, Panel::Git]);
        docks
    }

    #[test]
    fn a_panel_put_on_the_stage_from_a_dock_goes_back_to_that_dock_shown() {
        let mut docks = docks();
        docks.move_to(&Panel::Git, Dock::Left);
        docks.stage(&Panel::Git);
        assert_eq!(docks.place_of(&Panel::Git), Some(Place::Stage));
        docks.collapse(&Panel::Git);
        assert_eq!(docks.place_of(&Panel::Git), Some(Place::Dock(Dock::Left)));
        assert!(docks.shows(&Panel::Git));
        assert!(docks.layout().origins.is_empty(), "the way back is used once");
    }

    #[test]
    fn a_dialog_put_on_the_stage_floats_again_where_it_was_and_as_big() {
        let mut docks = docks();
        docks.float(&Panel::Git);
        let bounds = FloatBounds { x: 300, y: 40, width: 700, height: 600 };
        docks.set_bounds(&Panel::Git, bounds);
        docks.stage(&Panel::Git);
        docks.collapse(&Panel::Git);
        assert_eq!(docks.layout().floating, [Floating { panel: Panel::Git, bounds }]);
    }

    #[test]
    fn a_panel_with_nowhere_kept_goes_to_its_home_or_the_right() {
        let mut docks = docks();
        docks.collapse(&Panel::Git);
        assert_eq!(docks.place_of(&Panel::Git), Some(Place::Dock(Dock::Right)), "never placed: Git has no home");
        assert!(docks.shows(&Panel::Git));
    }

    #[test]
    fn a_panel_in_a_dock_or_a_dialog_is_not_moved_by_a_collapse() {
        let mut docks = docks();
        docks.move_to(&Panel::Git, Dock::Bottom);
        docks.collapse(&Panel::Git);
        assert_eq!(docks.place_of(&Panel::Git), Some(Place::Dock(Dock::Bottom)));
        docks.float(&Panel::Git);
        docks.collapse(&Panel::Git);
        assert_eq!(docks.place_of(&Panel::Git), Some(Place::Float));
    }

    #[test]
    fn the_way_back_is_kept_across_runs_and_a_stale_one_is_dropped() {
        let mut docks = docks();
        docks.move_to(&Panel::Git, Dock::Left);
        docks.stage(&Panel::Git);
        let kept: DocksLayout = serde_json::from_str(&serde_json::to_string(docks.layout()).unwrap()).unwrap();
        assert_eq!(kept.origins, docks.layout().origins);
        let mut again = Docks::new(kept.clone());
        again.set_present(vec![Panel::Git]);
        again.collapse(&Panel::Git);
        assert_eq!(again.place_of(&Panel::Git), Some(Place::Dock(Dock::Left)), "across runs");
        // A file that keeps a way back for a panel not on the stage is made whole.
        let stale = DocksLayout { staged: Vec::new(), ..kept };
        assert!(Docks::new(stale).layout().origins.is_empty());
    }
}
