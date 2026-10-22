//! Which view an agent showed of a linked ticket, and what a press on the ticket's chip does and
//! says: the chip a feature carries in its peek and header, with the match made here and not in
//! the window that draws it.
//!
//! A view is of a ticket when it names the ticket's URL, or the same tracker and key; the newest
//! such view wins, which is the first of `views` since they arrive newest first. A ticket with a
//! view opens that view, which says more than the tracker's page at a glance; one without opens
//! the ticket in the browser. Nothing here is sent to the page: the page matches in `views/agent.ts`.

use crate::connection::page::{Call, Link, PageView};

/// The view of a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketView {
    pub pane: u64,
    pub id: String,
    pub title: String,
    /// The title of the pane whose agent showed it.
    pub from: String,
}

/// A linked ticket as its chip shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketChip {
    pub link: Link,
    pub view: Option<TicketView>,
}

impl TicketChip {
    /// What hovering the chip says: the view and who read it, else what the ticket is to the
    /// feature and where it is.
    pub fn tooltip(&self) -> String {
        match &self.view {
            Some(view) => format!("{}, as {} read it", view.title, if view.from.is_empty() { "an agent" } else { &view.from }),
            None if self.link.role.is_empty() => self.link.url.clone(),
            None => format!("{}: {}", self.link.role, self.link.url),
        }
    }

    /// What pressing it asks of the session.
    pub fn press(&self) -> Call {
        match &self.view {
            Some(view) => Call::OpenView { pane: view.pane, id: view.id.clone() },
            None => Call::OpenUrl { url: self.link.url.clone() },
        }
    }
}

/// The newest view of `link`.
pub fn view_of<'a>(link: &Link, views: &'a [PageView]) -> Option<&'a PageView> {
    views.iter().find(|v| {
        v.subject.as_ref().is_some_and(|s| s.url == link.url || (s.tracker == link.tracker && s.key == link.key))
    })
}

/// A chip for each link, in the order the feature lists them.
pub fn chips(links: &[Link], views: &[PageView]) -> Vec<TicketChip> {
    links
        .iter()
        .map(|link| TicketChip {
            link: link.clone(),
            view: view_of(link, views).map(|v| TicketView { pane: v.pane, id: v.view.id.clone(), title: v.view.title.clone(), from: v.from.clone() }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::AgentView;

    fn link(tracker: &str, key: &str, role: &str) -> Link {
        Link { tracker: tracker.into(), key: key.into(), url: format!("https://t/{key}"), role: role.into() }
    }

    fn view(pane: u64, id: &str, from: &str, subject: Option<Link>) -> PageView {
        PageView {
            pane,
            from: from.into(),
            at: 0,
            refreshing: false,
            pressed: Vec::new(),
            subject,
            view: AgentView { id: id.into(), title: format!("{id} title"), ..AgentView::default() },
            diagrams: Vec::new(),
            changed: Vec::new(),
            unseen: false,
        }
    }

    #[test]
    fn a_view_is_of_the_ticket_it_names_by_url_or_by_tracker_and_key() {
        let eng = link("Linear", "ENG-88", "");
        let by_url = Link { key: "other".into(), tracker: "x".into(), ..eng.clone() };
        let by_key = Link { url: "https://elsewhere".into(), ..eng.clone() };
        let views = [view(1, "a", "", Some(by_url)), view(2, "b", "", Some(by_key)), view(3, "c", "", None)];
        assert_eq!(view_of(&eng, &views).map(|v| v.pane), Some(1), "the newest of those that match");
        assert_eq!(view_of(&eng, &views[1..]).map(|v| v.pane), Some(2));
        assert!(view_of(&eng, &views[2..]).is_none(), "a view of no ticket is of none");
        assert!(view_of(&link("Linear", "ENG-9", ""), &views[..2]).is_none());
    }

    #[test]
    fn a_ticket_with_a_view_opens_it_and_one_without_opens_the_browser() {
        let eng = link("Linear", "ENG-88", "the work");
        let views = [view(7, "plan", "pane #32", Some(eng.clone()))];
        let chips = chips(&[eng.clone(), link("Jira", "ARG-1", "")], &views);
        assert!(matches!(chips[0].press(), Call::OpenView { pane: 7, ref id } if id == "plan"));
        assert!(matches!(chips[1].press(), Call::OpenUrl { ref url } if url == "https://t/ARG-1"));
    }

    #[test]
    fn the_tooltip_names_the_view_and_who_read_it_else_the_role_and_url() {
        let eng = link("Linear", "ENG-88", "the work");
        let seen = chips(std::slice::from_ref(&eng), &[view(7, "plan", "pane #32", Some(eng.clone()))]);
        assert_eq!(seen[0].tooltip(), "plan title, as pane #32 read it");
        let anon = chips(std::slice::from_ref(&eng), &[view(7, "plan", "", Some(eng.clone()))]);
        assert_eq!(anon[0].tooltip(), "plan title, as an agent read it");
        let bare = chips(std::slice::from_ref(&eng), &[]);
        assert_eq!(bare[0].tooltip(), "the work: https://t/ENG-88");
        assert_eq!(chips(&[link("Jira", "A-1", "")], &[])[0].tooltip(), "https://t/A-1");
    }
}
