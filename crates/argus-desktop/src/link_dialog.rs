//! What the link dialog holds — the feature it links, the URL and role
//! being typed, the linked ticket under the cursor, the tracker map being
//! edited, and a link sent and not yet on the board — and what its gestures
//! ask of the daemon.
//!
//! The URL is read as the daemon will read it, by `LinkWrite::checked`, so
//! the tracker and key it shows are the ones the link gets. Nothing is
//! written to a tracker: a link is a key and a URL in Argus. The fields
//! clear only once the pushed board holds the link, so a refused one keeps
//! what was typed.

use argus_protocol::{ClientMsg, Feature, FeatureAction, LinkWrite, MapChange, RequestRefusal, TrackerMap};
use serde::{Deserialize, Serialize};

use crate::feature_stage::Scope;
use crate::keymap::Action;
use crate::page::Link;

/// What was done in the link dialog.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum LinkGesture {
    /// What the fields say now, read as the daemon will read them.
    Fields { url: String, role: String },
    /// Link, with what the fields say as it is pressed.
    Link { url: String, role: String },
    /// A ticket already linked, by its place in the list.
    Select { index: usize },
    Unlink { key: String },
    /// The tracker map's editor, on the draft when one waits.
    EditMap,
    MapText { text: String },
    /// The map written as `text`; with a draft waiting, the draft approved
    /// as it reads there.
    SaveMap { text: String },
    /// The draft turned down.
    DiscardDraft,
    CancelMap,
    /// A key of the dialog's own.
    Action { action: Action },
    Close,
}

/// The link dialog: the URL being linked and what it reads as, the tickets
/// already linked, and the repository's tracker map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LinkPage {
    pub open: bool,
    pub number: i64,
    pub title: String,
    pub url: String,
    pub role: String,
    /// The tracker and key the URL reads as, without going online.
    pub read: Option<Link>,
    /// Why the URL is not a link yet, once something is typed.
    pub refusal: Option<String>,
    /// A link was sent and the board does not hold it yet.
    pub pending: bool,
    pub linked: Vec<LinkedRow>,
    pub map: MapPage,
    /// The map's editor, with what it holds.
    pub editing_map: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LinkedRow {
    pub link: Link,
    pub on: bool,
}

/// The repository's tracker map, and the draft an agent left for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MapPage {
    pub body: String,
    pub draft: Option<String>,
    pub draft_by: Option<String>,
}

pub struct LinkDialog {
    slug: String,
    url: String,
    role: String,
    /// The linked ticket under the cursor, by its place in the list.
    cursor: usize,
    /// The id a link was sent with, and its key, until the board holds it
    /// or it is refused.
    pending: Option<(u64, String)>,
    /// The tracker map's editor, holding what it says now.
    editing_map: Option<String>,
}

/// What a gesture leaves for the session to do beyond the daemon's
/// messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    Close,
    /// Open a URL in the browser.
    Open(String),
    Say(String),
}

impl LinkDialog {
    /// The dialog on `slug`, with `map` on the map's editor.
    pub fn new(slug: &str, map: Option<&TrackerMap>) -> LinkDialog {
        let mut dialog = LinkDialog {
            slug: slug.to_string(),
            url: String::new(),
            role: String::new(),
            cursor: 0,
            pending: None,
            editing_map: None,
        };
        if let Some(map) = map {
            dialog.edit_map(map);
        }
        dialog
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// A gesture, with the feature and map as the board has them, what
    /// names a request, and where to send what it asks.
    pub fn gesture(
        &mut self,
        gesture: LinkGesture,
        feature: &Feature,
        map: &TrackerMap,
        scope: &Scope,
        id: impl FnMut() -> u64,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        match gesture {
            LinkGesture::Fields { url, role } => {
                self.url = url;
                self.role = role;
            }
            LinkGesture::Link { url, role } => {
                self.url = url;
                self.role = role;
                return self.link(scope, id, out);
            }
            LinkGesture::Select { index } => self.cursor = index.min(feature.links.len().saturating_sub(1)),
            LinkGesture::Unlink { key } => self.unlink(key, scope, out),
            LinkGesture::EditMap => self.edit_map(map),
            LinkGesture::MapText { text } => {
                if let Some(editing) = &mut self.editing_map {
                    *editing = text;
                }
            }
            LinkGesture::SaveMap { text } => {
                let change = if map.draft.is_some() { MapChange::Approve(text) } else { MapChange::Write(text) };
                return self.change_map(change, scope, out);
            }
            LinkGesture::DiscardDraft => return self.discard_draft(map, scope, out),
            LinkGesture::CancelMap => self.editing_map = None,
            LinkGesture::Action { action } => return self.action(action, feature, map, scope, id, out),
            LinkGesture::Close => return Then::Close,
        }
        Then::Nothing
    }

    fn action(
        &mut self,
        action: Action,
        feature: &Feature,
        map: &TrackerMap,
        scope: &Scope,
        id: impl FnMut() -> u64,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        let last = feature.links.len().saturating_sub(1);
        match action {
            Action::Next => self.cursor = (self.cursor + 1).min(last),
            Action::Previous => self.cursor = self.cursor.saturating_sub(1),
            Action::Choose => return self.link(scope, id, out),
            Action::OpenTicket => {
                if let Some(link) = feature.links.get(self.cursor) {
                    return Then::Open(link.url.clone());
                }
            }
            Action::Unlink => {
                if let Some(link) = feature.links.get(self.cursor) {
                    self.unlink(link.key.clone(), scope, out);
                }
            }
            Action::TrackerMap => self.edit_map(map),
            Action::Approve => match &map.draft {
                Some(draft) => return self.change_map(MapChange::Approve(draft.body.clone()), scope, out),
                None => return Then::Say("no draft of the map waits".into()),
            },
            Action::Discard => return self.discard_draft(map, scope, out),
            Action::CloseLayer => return Then::Close,
            _ => {}
        }
        Then::Nothing
    }

    fn link(&mut self, scope: &Scope, mut id: impl FnMut() -> u64, out: &mut Vec<ClientMsg>) -> Then {
        let link = self.write();
        // Read here only so a typo is said at once; the daemon reads it
        // again with the same function.
        let read = match link.clone().checked() {
            Ok(read) => read,
            Err(why) => return Then::Say(why.into()),
        };
        let request_id = id();
        self.pending = Some((request_id, read.key));
        out.push(ClientMsg::Feature {
            project: scope.project,
            checkout: scope.checkout,
            action: FeatureAction::Link { slug: self.slug.clone(), link },
            request_id,
        });
        Then::Nothing
    }

    fn unlink(&mut self, key: String, scope: &Scope, out: &mut Vec<ClientMsg>) {
        out.push(ClientMsg::Feature {
            project: scope.project,
            checkout: scope.checkout,
            action: FeatureAction::Unlink { slug: self.slug.clone(), key },
            request_id: 0,
        });
    }

    fn edit_map(&mut self, map: &TrackerMap) {
        self.editing_map = Some(map.draft.as_ref().map_or(&map.body, |draft| &draft.body).clone());
    }

    fn change_map(&mut self, change: MapChange, scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        out.push(ClientMsg::TrackerMap { project: scope.project, checkout: scope.checkout, change });
        self.editing_map = None;
        Then::Nothing
    }

    fn discard_draft(&mut self, map: &TrackerMap, scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        if map.draft.is_none() {
            return Then::Say("no draft of the map waits".into());
        }
        self.change_map(MapChange::Discard, scope, out)
    }

    fn write(&self) -> LinkWrite {
        LinkWrite { url: self.url.clone(), role: Some(self.role.clone()), ..LinkWrite::default() }
    }

    /// The board changed: a link it now holds was the one sent, and the
    /// fields are free for the next.
    pub fn settle(&mut self, feature: &Feature) {
        if self.pending.as_ref().is_some_and(|(_, key)| feature.links.iter().any(|l| &l.key == key)) {
            self.pending = None;
            self.url.clear();
            self.role.clear();
        }
        self.cursor = self.cursor.min(feature.links.len().saturating_sub(1));
    }

    /// Takes the daemon's refusal of the link waiting on it, keeping what
    /// was typed to fix; false for a refusal of anything else.
    pub fn refused(&mut self, refusal: &RequestRefusal) -> bool {
        if !self.pending.as_ref().is_some_and(|(id, _)| refusal.refuses_change(*id)) {
            return false;
        }
        self.pending = None;
        true
    }

    pub fn page(&self, feature: &Feature, map: &TrackerMap) -> LinkPage {
        let typed = !self.url.trim().is_empty();
        let (read, refusal) = match self.write().checked() {
            Ok(read) => (Some(Link::of(&read)), None),
            Err(why) => (None, typed.then(|| why.to_string())),
        };
        LinkPage {
            open: true,
            number: feature.number,
            title: feature.title.clone(),
            url: self.url.clone(),
            role: self.role.clone(),
            read,
            refusal,
            pending: self.pending.is_some(),
            linked: feature
                .links
                .iter()
                .enumerate()
                .map(|(i, link)| LinkedRow { link: Link::of(link), on: i == self.cursor })
                .collect(),
            map: MapPage {
                body: map.body.clone(),
                draft: map.draft.as_ref().map(|d| d.body.clone()),
                draft_by: map.draft.as_ref().map(|d| d.by.clone()),
            },
            editing_map: self.editing_map.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{CheckoutId, MapDraft, ProjectId, RepositoryId, TrackerLink};

    fn scope() -> Scope {
        Scope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(2),
            repository_name: "argus".into(),
            checkout: CheckoutId(3),
            key: "/src/main".into(),
        }
    }

    fn feature(links: &[&str]) -> Feature {
        Feature {
            slug: "trim".into(),
            number: 7,
            title: "Trim".into(),
            links: links
                .iter()
                .map(|key| TrackerLink { tracker: "Jira".into(), key: key.to_string(), url: format!("https://acme.atlassian.net/browse/{key}"), role: String::new() })
                .collect(),
            ..Feature::default()
        }
    }

    const URL: &str = "https://acme.atlassian.net/browse/ARG-142";

    #[test]
    fn a_url_reads_as_its_tracker_and_key_before_it_is_sent() {
        let mut dialog = LinkDialog::new("trim", None);
        let mut out = Vec::new();
        let map = TrackerMap::default();
        dialog.gesture(LinkGesture::Fields { url: URL.into(), role: "the PM's ticket".into() }, &feature(&[]), &map, &scope(), || 9, &mut out);
        let page = dialog.page(&feature(&[]), &map);
        let read = page.read.unwrap();
        assert_eq!((read.tracker.as_str(), read.key.as_str(), read.role.as_str()), ("Jira", "ARG-142", "the PM's ticket"));
        assert!(out.is_empty(), "typing sends nothing");

        dialog.gesture(LinkGesture::Fields { url: "file:///etc".into(), role: String::new() }, &feature(&[]), &map, &scope(), || 9, &mut out);
        assert!(dialog.page(&feature(&[]), &map).refusal.is_some());
    }

    #[test]
    fn the_fields_clear_only_once_the_board_holds_the_link() {
        let mut dialog = LinkDialog::new("trim", None);
        let mut out = Vec::new();
        let map = TrackerMap::default();
        let then = dialog.gesture(LinkGesture::Link { url: URL.into(), role: "the story".into() }, &feature(&[]), &map, &scope(), || 9, &mut out);
        assert_eq!(then, Then::Nothing);
        assert!(matches!(&out[..], [ClientMsg::Feature { action: FeatureAction::Link { slug, link }, .. }]
            if slug == "trim" && link.role.as_deref() == Some("the story")));
        assert!(dialog.page(&feature(&[]), &map).pending);

        dialog.settle(&feature(&[]));
        assert_eq!(dialog.page(&feature(&[]), &map).url, URL, "not linked yet: the URL stays");
        dialog.refused(&RequestRefusal { request_id: 9, reason: "no".into(), read: false });
        assert!(!dialog.page(&feature(&[]), &map).pending, "a refusal keeps what was typed to fix");

        dialog.gesture(LinkGesture::Link { url: URL.into(), role: String::new() }, &feature(&[]), &map, &scope(), || 9, &mut out);
        dialog.settle(&feature(&["ARG-142"]));
        let page = dialog.page(&feature(&["ARG-142"]), &map);
        assert_eq!((page.url.as_str(), page.pending, page.linked.len()), ("", false, 1));
    }

    #[test]
    fn a_url_that_is_no_link_is_said_and_never_sent() {
        let mut dialog = LinkDialog::new("trim", None);
        let mut out = Vec::new();
        let then = dialog.gesture(LinkGesture::Link { url: "ENG-88".into(), role: String::new() }, &feature(&[]), &TrackerMap::default(), &scope(), || 9, &mut out);
        assert!(matches!(then, Then::Say(_)) && out.is_empty());
    }

    #[test]
    fn the_keys_open_and_unlink_the_ticket_under_the_cursor() {
        let mut dialog = LinkDialog::new("trim", None);
        let (mut out, map, linked) = (Vec::new(), TrackerMap::default(), feature(&["A-1", "A-2"]));
        let act = |dialog: &mut LinkDialog, action, out: &mut Vec<ClientMsg>| {
            dialog.gesture(LinkGesture::Action { action }, &linked, &map, &scope(), || 9, out)
        };
        act(&mut dialog, Action::Next, &mut out);
        act(&mut dialog, Action::Next, &mut out);
        assert_eq!(act(&mut dialog, Action::OpenTicket, &mut out), Then::Open("https://acme.atlassian.net/browse/A-2".into()));
        act(&mut dialog, Action::Unlink, &mut out);
        assert!(matches!(&out[..], [ClientMsg::Feature { action: FeatureAction::Unlink { key, .. }, .. }] if key == "A-2"));
        assert_eq!(act(&mut dialog, Action::CloseLayer, &mut out), Then::Close);
    }

    #[test]
    fn the_map_editor_opens_on_a_waiting_draft_and_saving_approves_it_as_written() {
        let map = TrackerMap {
            body: "Linear: feature is an issue".into(),
            draft: Some(MapDraft { body: "Linear: feature is a project".into(), by: "pane #32".into(), at: 1 }),
        };
        let mut dialog = LinkDialog::new("trim", Some(&map));
        assert_eq!(dialog.page(&feature(&[]), &map).editing_map.as_deref(), Some("Linear: feature is a project"));
        let mut out = Vec::new();
        dialog.gesture(LinkGesture::SaveMap { text: "edited".into() }, &feature(&[]), &map, &scope(), || 9, &mut out);
        assert!(matches!(&out[..], [ClientMsg::TrackerMap { change: MapChange::Approve(text), .. }] if text == "edited"));
        assert!(dialog.page(&feature(&[]), &map).editing_map.is_none(), "the editor closes; the board says the rest");

        let plain = TrackerMap::default();
        let mut out = Vec::new();
        dialog.gesture(LinkGesture::SaveMap { text: "Jira: story".into() }, &feature(&[]), &plain, &scope(), || 9, &mut out);
        assert!(matches!(&out[..], [ClientMsg::TrackerMap { change: MapChange::Write(_), .. }]));
        let then = dialog.gesture(LinkGesture::Action { action: Action::Discard }, &feature(&[]), &plain, &scope(), || 9, &mut out);
        assert!(matches!(then, Then::Say(_)), "no draft to turn down");
    }
}
