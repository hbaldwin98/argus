//! What the head of an agent's view says, as words a window draws: the status pill's word and look,
//! the meta line under the title, the tab's tooltip, and which view the panel and the page show.
//!
//! The page works these out each time it draws (`views/agent.ts`); the GPUI window asks here
//! instead, so the rules are written once and held by tests with no window in them.

use argus_protocol::Tone;

use crate::connection::page::{Call, PageView, ViewRef, ViewsPage};
use crate::layout::docks::{DocksPage, Panel};
use crate::system::clock::Clock;
use crate::views::values::{ago, local_time};

/// The status the strip's pill speaks for a view's tone: its one pill has the statuses, and a tone
/// picks among them (`SPILL`).
pub fn pill_status(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "idle",
        Tone::Good => "done",
        Tone::Warn => "waiting",
        Tone::Bad => "failed",
        Tone::Active => "working",
        Tone::Info => "info",
    }
}

/// The view `at` names, if the window holds it.
fn find<'a>(page: &'a ViewsPage, at: Option<&ViewRef>) -> Option<&'a PageView> {
    let at = at?;
    page.views.iter().find(|view| view.pane == at.pane && view.view.id == at.id)
}

/// The view the panel shows.
pub fn open(page: &ViewsPage) -> Option<&PageView> {
    find(page, page.open.as_ref())
}

/// The view open as a page over the stage.
pub fn popped(page: &ViewsPage) -> Option<&PageView> {
    find(page, page.popped.as_ref())
}

/// The key a view's scroll and blocks are kept under.
pub fn key(view: &PageView) -> String {
    format!("{}/{}", view.pane, view.view.id)
}

/// A view's `at` (Unix seconds) as the RFC 3339 time the age is read from.
pub fn at_text(view: &PageView) -> String {
    jiff::Timestamp::from_second(view.at).map(|at| at.to_string()).unwrap_or_default()
}

/// `read by <pane> · <age>`, or the age alone when the pane is not known, and a note while a
/// refresh is pending.
pub fn meta(view: &PageView, clock: &Clock) -> String {
    let age = ago(clock, &at_text(view));
    let said = if view.from.is_empty() { age } else { format!("read by {} · {age}", view.from) };
    if view.refreshing { format!("{said} · asked to read it again") } else { said }
}

/// The meta line's tooltip: when the daemon received it, on the person's clock.
pub fn meta_tip(view: &PageView, clock: &Clock) -> String {
    local_time(clock, &at_text(view)).unwrap_or_default()
}

/// A tab's tooltip: the title, and the pane's when it is known.
pub fn tab_tip(view: &PageView) -> String {
    if view.from.is_empty() { view.view.title.clone() } else { format!("{} · from {}", view.view.title, view.from) }
}

/// Whether a dock shows the views panel now: the launcher is lit while one does.
pub fn out(docks: &DocksPage) -> bool {
    [&docks.left, &docks.right, &docks.bottom].iter().any(|dock| dock.shown == Some(Panel::Views))
}

/// The launcher's press: hide the panel while it is out, else bring out the newest view.
pub fn launch(page: &ViewsPage, out: bool) -> Option<Call> {
    if out {
        return Some(Call::HideViews);
    }
    page.views.first().map(|newest| Call::OpenView { pane: newest.pane, id: newest.view.id.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::AgentView;
    use jiff::tz::TimeZone;

    fn view(pane: u64, id: &str, from: &str, at: i64) -> PageView {
        PageView {
            pane,
            from: from.into(),
            at,
            refreshing: false,
            pressed: Vec::new(),
            subject: None,
            view: AgentView { id: id.into(), title: format!("{id} title"), ..Default::default() },
            diagrams: Vec::new(),
            changed: Vec::new(),
            unseen: false,
        }
    }

    fn clock() -> Clock {
        Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::UTC)
    }

    #[test]
    fn each_tone_speaks_the_status_the_page_gives_it() {
        let said: Vec<_> = [Tone::Neutral, Tone::Good, Tone::Warn, Tone::Bad, Tone::Active, Tone::Info].map(pill_status).into();
        assert_eq!(said, ["idle", "done", "waiting", "failed", "working", "info"]);
    }

    #[test]
    fn the_meta_names_the_pane_and_the_age_and_notes_a_pending_refresh() {
        let at = "2026-10-21T15:26:00Z".parse::<jiff::Timestamp>().unwrap().as_second();
        let mut shown = view(1, "ci", "claude · trim", at);
        assert_eq!(meta(&shown, &clock()), "read by claude · trim · 4 min ago");
        shown.refreshing = true;
        assert_eq!(meta(&shown, &clock()), "read by claude · trim · 4 min ago · asked to read it again");
        shown.from.clear();
        assert_eq!(meta(&shown, &clock()), "4 min ago · asked to read it again");
    }

    #[test]
    fn the_age_moves_with_the_clock() {
        let at = "2026-10-21T15:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second();
        let shown = view(1, "ci", "", at);
        let later = Clock::new("2026-10-21T15:40:00Z".parse().unwrap(), TimeZone::UTC);
        assert_ne!(meta(&shown, &clock()), meta(&shown, &later));
    }

    #[test]
    fn a_tab_names_its_pane_only_when_it_is_known() {
        assert_eq!(tab_tip(&view(1, "ci", "claude", 0)), "ci title \u{b7} from claude");
        assert_eq!(tab_tip(&view(1, "ci", "", 0)), "ci title");
    }

    #[test]
    fn the_panel_and_the_page_find_their_views_by_pane_and_id() {
        let page = ViewsPage {
            views: vec![view(2, "b", "", 0), view(1, "a", "", 0)],
            open: Some(ViewRef { pane: 1, id: "a".into() }),
            popped: Some(ViewRef { pane: 9, id: "gone".into() }),
            can_press: true,
        };
        assert_eq!(open(&page).map(|v| v.view.id.as_str()), Some("a"));
        assert!(popped(&page).is_none(), "a page whose view is not held shows none");
    }

    #[test]
    fn the_launcher_hides_the_panel_while_out_else_opens_the_newest() {
        let page = ViewsPage { views: vec![view(2, "b", "", 0), view(1, "a", "", 0)], ..ViewsPage::default() };
        assert!(matches!(launch(&page, true), Some(Call::HideViews)));
        assert!(matches!(launch(&page, false), Some(Call::OpenView { pane: 2, ref id }) if id == "b"));
        assert!(launch(&ViewsPage::default(), false).is_none());
    }

    #[test]
    fn the_launcher_is_lit_while_any_dock_shows_the_views() {
        let mut docks = DocksPage::default();
        assert!(!out(&docks));
        docks.bottom.shown = Some(Panel::Console);
        assert!(!out(&docks));
        docks.left.shown = Some(Panel::Views);
        assert!(out(&docks));
    }
}
