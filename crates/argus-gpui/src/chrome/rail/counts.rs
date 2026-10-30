//! How many each stage of the icon strip holds, where the window already knows the number.
//!
//! A badge is drawn only for a count the client was told: a stage the session has not described
//! (the Features board before it arrived, the Git review while none is open) has `None`, never a
//! guess from the rail's tree, whose closed repositories list no checkouts. Workspace's is the
//! panes that need the person and is the attention colour's; the rest are neutral.

use argus_desktop_core::connection::page::Rail;

use crate::chrome::nav::Nav;
use crate::connection::heard::Heard;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Counts {
    /// Panes that need the person, in the project the panel lists.
    pub needs_you: usize,
    /// Features in flight or queued in the repository the Features stage reads.
    pub features: Option<usize>,
    /// Checkouts of the repository the Checkouts stage reads.
    pub checkouts: Option<usize>,
    /// Files changed in the review the session holds open.
    pub git: Option<usize>,
    pub console: usize,
    pub browsers: usize,
    /// The views the Extensions and Views folds list.
    pub extensions: usize,
    pub views: usize,
}

/// What needs the person and what is at work in the project the panel lists: the numbers its tree
/// shows. A window with no project open says the window-wide ones.
pub(super) fn in_view(rail: &Rail) -> (usize, usize) {
    let mine = rail.project.as_ref().and_then(|project| rail.projects.iter().find(|choice| choice.id == project.id));
    mine.map_or((rail.needs_you, rail.working), |choice| (choice.needs_you, choice.working))
}

impl Counts {
    pub fn of(heard: &Heard, rail: &Rail) -> Counts {
        Counts {
            needs_you: in_view(rail).0,
            features: heard.features.as_ref().filter(|features| features.loaded).map(|features| features.active + features.queued),
            checkouts: heard.checkouts.as_ref().filter(|table| table.repository.is_some()).map(|table| table.checkouts),
            git: heard
                .review
                .as_ref()
                .filter(|review| review.open && review.base != "commit" && !review.sections.is_empty())
                .map(|review| review.sections.iter().map(|section| section.files.len()).sum()),
            console: heard.console.tabs.len(),
            browsers: heard.browsers.browsers.len(),
            extensions: heard.extension_views.len(),
            views: heard.saved_views.len(),
        }
    }

    /// The number a nav item's badge shows, `None` for no badge: nothing is drawn for a zero either,
    /// except that Workspace's zero would only say nobody needs the person.
    pub fn of_nav(&self, nav: Nav) -> Option<usize> {
        let count = match nav {
            Nav::Workspace => Some(self.needs_you),
            Nav::Features => self.features,
            Nav::Checkouts => self.checkouts,
            Nav::Git => self.git,
            Nav::Console => Some(self.console),
            Nav::Browsers => Some(self.browsers),
            Nav::Search | Nav::New | Nav::Code => None,
        };
        count.filter(|count| *count > 0)
    }

    /// What a badge says to a reader that cannot see it: `Features, 1 open`.
    pub fn said(&self, nav: Nav, count: usize) -> String {
        match nav {
            Nav::Workspace => format!("{} {}", nav.label(), if count == 1 { "1 needs you".to_owned() } else { format!("{count} need you") }),
            Nav::Features => format!("{}, {count} open", nav.label()),
            Nav::Git => format!("{}, {count} {}", nav.label(), if count == 1 { "change" } else { "changes" }),
            Nav::Console | Nav::Browsers => format!("{}, {count} open", nav.label()),
            _ => format!("{}, {count}", nav.label()),
        }
    }
}
