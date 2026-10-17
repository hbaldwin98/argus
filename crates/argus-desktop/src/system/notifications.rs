//! The system notification an agent coming to need you raises: whether a
//! tree's changes raise one, what it says, and handing it to the system.
//!
//! The window's version of the terminal client's bell. Like the chime it
//! leaves out the pane being watched in a focused window; the person may
//! also keep it to a window in the background, where the chime is easiest
//! to miss and a notification is the only thing on screen.

use argus_protocol::{HeldTree, PaneId, PaneStatus, ProjectInfo, Transition};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::settings::Preferences;

/// One notification: the pane's name, and what came of it where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub title: String,
    pub body: String,
}

/// What the person is looking at when a tree arrives.
pub struct Looking {
    pub focused: bool,
    /// The pane on screen in a focused window, which needs no telling.
    pub watched: Option<PaneId>,
}

/// The notifications `transitions` raise, as `chosen` says.
pub fn alerts(transitions: &[Transition<'_>], tree: &[ProjectInfo], chosen: &Preferences, looking: &Looking) -> Vec<Alert> {
    if !chosen.notify || (chosen.notify_in_background && looking.focused) {
        return Vec::new();
    }
    transitions
        .iter()
        .filter(|t| t.after.status.needs_you() && Some(t.pane.id) != looking.watched)
        .map(|t| {
            let what = match t.after.status {
                PaneStatus::Waiting => "is waiting for you",
                PaneStatus::NeedsReview => "needs a review",
                _ => "failed",
            };
            // A subagent speaks for its pane: the loudest state came from it.
            let who = t.after.child.map_or(String::new(), |child| format!("{child} "));
            let body = match place(tree, t.pane.id) {
                Some(place) => format!("{who}{what} · {place}"),
                None => format!("{who}{what}"),
            };
            Alert { title: t.pane.title.clone(), body: capitalise(&body) }
        })
        .collect()
}

/// Says which machine the panes in `alerts` run on, for a window reaching a
/// daemon over ssh: a notification from a remote agent that looked like a
/// local one would send the person to the wrong terminal.
pub fn on_host(alerts: &mut [Alert], host: &str) {
    for alert in alerts {
        alert.body = format!("{} · on {host}", alert.body);
    }
}

/// `repository / checkout`, where the pane runs.
fn place(tree: &[ProjectInfo], pane: PaneId) -> Option<String> {
    let (lineage, _) = tree.lineage_of_pane(pane)?;
    Some(format!("{} / {}", lineage.repository.name, lineage.checkout.name))
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// Best effort: a system with notifications off, or none to show them,
/// costs the person the notification and nothing else.
pub fn show(app: &AppHandle, alert: &Alert) {
    if let Err(err) = app.notification().builder().title(&alert.title).body(&alert.body).show() {
        tracing::warn!("could not raise a notification: {err}");
    }
}

#[cfg(test)]
mod tests {
    use argus_protocol::{transitions, PaneKind};

    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};

    fn tree(status: PaneStatus) -> Vec<ProjectInfo> {
        let agent = pane(7, PaneKind::Agent, "Trim context", status);
        vec![project(1, "work", vec![repository(10, "argus", None, vec![checkout(100, "main", true, vec![agent])])])]
    }

    fn raised(chosen: &Preferences, looking: &Looking, after: PaneStatus) -> Vec<Alert> {
        let before = tree(PaneStatus::Working);
        let next = tree(after);
        alerts(&transitions(before.as_slice(), next.as_slice()), &next, chosen, looking)
    }

    const BACKGROUND: Looking = Looking { focused: false, watched: None };

    #[test]
    fn an_agent_turning_to_need_you_is_named_with_where_it_runs() {
        let alerts = raised(&Preferences::default(), &BACKGROUND, PaneStatus::Waiting);
        assert_eq!(
            alerts,
            [Alert { title: "Trim context".into(), body: "Is waiting for you · argus / main".into() }]
        );
        let review = raised(&Preferences::default(), &BACKGROUND, PaneStatus::NeedsReview);
        assert_eq!(review[0].body, "Needs a review · argus / main");
        let failed = raised(&Preferences::default(), &BACKGROUND, PaneStatus::Failed);
        assert_eq!(failed[0].body, "Failed · argus / main");
    }

    #[test]
    fn a_remote_agent_is_named_with_its_host() {
        let mut alerts = raised(&Preferences::default(), &BACKGROUND, PaneStatus::Waiting);
        on_host(&mut alerts, "devbox");
        assert_eq!(alerts[0].body, "Is waiting for you · argus / main · on devbox");
    }

    #[test]
    fn starting_work_or_going_idle_raises_nothing() {
        assert!(raised(&Preferences::default(), &BACKGROUND, PaneStatus::Idle).is_empty());
    }

    #[test]
    fn a_window_in_front_is_told_nothing_unless_asked_to_be() {
        let front = Looking { focused: true, watched: None };
        assert!(raised(&Preferences::default(), &front, PaneStatus::Waiting).is_empty());
        let always = Preferences { notify_in_background: false, ..Preferences::default() };
        assert_eq!(raised(&always, &front, PaneStatus::Waiting).len(), 1);
    }

    #[test]
    fn the_pane_being_watched_is_never_announced() {
        let always = Preferences { notify_in_background: false, ..Preferences::default() };
        let watching = Looking { focused: true, watched: Some(PaneId(7)) };
        assert!(raised(&always, &watching, PaneStatus::Waiting).is_empty());
    }

    #[test]
    fn turned_off_it_raises_nothing() {
        let off = Preferences { notify: false, ..Preferences::default() };
        assert!(raised(&off, &BACKGROUND, PaneStatus::Failed).is_empty());
    }
}
