//! Which views agents have shown, per pane, and which go when.
//!
//! Held in memory only, and never handed to the store: a view is what an
//! agent read from a ticket a moment ago, and nothing read from a ticket
//! outlives the agent's next look at it. A restart of the daemon, or the
//! pane's removal, is the end of its views.

use argus_protocol::{
    AgentView, AnswerValue, Block, PaneId, RowItem, ServerMsg, ShownView, ViewAction, ViewChange,
    ViewPress, MAX_VIEWS_PER_PANE, MAX_VIEW_BYTES,
};
use tokio::sync::broadcast;

use super::*;

/// The views, oldest shown first across every pane, which is the order a
/// client that starts watching is replayed them in.
#[derive(Default)]
pub(super) struct Views {
    shown: Vec<ShownView>,
}

impl Views {
    /// Keeps `view`, in the place of the pane's view with its id when it has
    /// one. Past the pane's limit its oldest goes; the id that did is
    /// returned.
    fn show(&mut self, view: ShownView) -> Option<String> {
        let held = self
            .shown
            .iter()
            .position(|v| v.pane == view.pane && v.view.id == view.view.id);
        if let Some(at) = held {
            self.shown[at] = view;
            return None;
        }
        let pane = view.pane;
        self.shown.push(view);
        let on_pane = self.shown.iter().filter(|v| v.pane == pane).count();
        if on_pane <= MAX_VIEWS_PER_PANE {
            return None;
        }
        let oldest = self.shown.iter().position(|v| v.pane == pane)?;
        Some(self.shown.remove(oldest).view.id)
    }

    fn close(&mut self, pane: PaneId, id: &str) -> bool {
        let before = self.shown.len();
        self.shown.retain(|v| !(v.pane == pane && v.view.id == id));
        self.shown.len() != before
    }

    /// The ids the pane had, oldest first.
    fn drop_pane(&mut self, pane: PaneId) -> Vec<String> {
        let ids = self.shown.iter().filter(|v| v.pane == pane).map(|v| v.view.id.clone());
        let ids = ids.collect();
        self.shown.retain(|v| v.pane != pane);
        ids
    }

    fn get(&self, pane: PaneId, id: &str) -> Option<&ShownView> {
        self.shown.iter().find(|v| v.pane == pane && v.view.id == id)
    }
}

/// What the daemon tells the connections watching views: a [`ServerMsg`]
/// already, since each watcher is sent it as it is.
pub(super) type ViewEvent = ServerMsg;

impl Daemon {
    /// An agent's `view` request: shows the view, or closes one.
    ///
    /// The refusal is the limit's own words, with the path to the offending
    /// block, for the agent to fix and send again. Closing a view the pane
    /// does not have is not an error: the person may have closed it first,
    /// and the agent's aim, that it be gone, is met.
    pub fn change_view_for_agent(
        &self,
        pane: PaneId,
        change: ViewChange,
    ) -> anyhow::Result<()> {
        self.agent_scope(pane)?;
        match change {
            ViewChange::Show { view } => self.show_view(pane, view),
            ViewChange::Close { id } => {
                self.close_view(pane, id.trim());
                Ok(())
            }
            ViewChange::Patch { id, path, block } => {
                let held = self.agent_views.lock().unwrap().get(pane, id.trim()).cloned();
                let held = held.ok_or_else(|| anyhow::anyhow!("the pane shows no view {:?}", id.trim()))?;
                let view = held.view.patched(&path, block).map_err(|e| anyhow::anyhow!("{e}"))?;
                self.show_view(pane, view)
            }
        }
    }

    fn show_view(&self, pane: PaneId, view: AgentView) -> anyhow::Result<()> {
        let view = view.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        // The body limit bounds what was sent; a view that serialises
        // larger than it, from defaults filled in that the sender never
        // wrote, is as much a dump as one that was sent so.
        let size = serde_json::to_vec(&view).map_or(0, |bytes| bytes.len());
        anyhow::ensure!(size <= MAX_VIEW_BYTES, "a view is at most {MAX_VIEW_BYTES} bytes");
        let shown = ShownView {
            pane,
            at: super::decisions::now(),
            subject: view.subject_link(),
            view,
        };
        // Sent under the lock, so a watcher that subscribes in
        // `watch_views` sees each change either in its replay or after it,
        // and never in the wrong order.
        let mut views = self.agent_views.lock().unwrap();
        if let Some(id) = views.show(shown.clone()) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id });
        }
        let _ = self.agent_views_tx.send(ServerMsg::ViewShown(Box::new(shown)));
        Ok(())
    }

    /// The person's close from a client, or the agent's.
    pub fn close_view(&self, pane: PaneId, id: &str) {
        let mut views = self.agent_views.lock().unwrap();
        if views.close(pane, id) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id: id.to_string() });
        }
    }

    /// Everything a watcher is replayed, oldest first, and the receiver of
    /// what follows it.
    pub fn watch_views(&self) -> (Vec<ShownView>, broadcast::Receiver<ViewEvent>) {
        let views = self.agent_views.lock().unwrap();
        (views.shown.clone(), self.agent_views_tx.subscribe())
    }

    /// Asks the agent that showed a view to look again. The view stays as
    /// it is until the agent shows it anew, so an agent that does not
    /// answer leaves the person looking at what was true when it was read.
    /// A view that is not held is ignored.
    pub fn refresh_view(&self, pane: PaneId, id: &str) {
        let Some(shown) = self.agent_views.lock().unwrap().get(pane, id).cloned() else {
            return;
        };
        self.notify(pane, &refresh_request(&shown.view));
    }

    /// Tells the agent that showed a view which of its actions the person
    /// pressed. A view that is not held is ignored, as a refresh of one is.
    ///
    /// An action the view does not offer is dropped, not relayed: what is
    /// typed to the agent must be something its own view said, never text a
    /// client made up.
    pub fn press_view(&self, pane: PaneId, press: ViewPress) {
        let Some(shown) = self.agent_views.lock().unwrap().get(pane, &press.view).cloned() else {
            return;
        };
        let Some(action) = find_action(&shown.view, &press.action) else {
            return;
        };
        self.notify(pane, &press_request(&shown.view, action, &press));
    }

    /// Called as a pane leaves the tree: its views go with it.
    pub(super) fn drop_views_of(&self, pane: PaneId) {
        let mut views = self.agent_views.lock().unwrap();
        for id in views.drop_pane(pane) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id });
        }
    }
}

/// What is typed to the agent when it has no inbox, so it must stay one
/// line: a newline in a title would send the request early.
fn refresh_request(view: &AgentView) -> String {
    let title: String = view.title.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let source = match &view.subject {
        Some(url) => url.clone(),
        None => "its source".to_string(),
    };
    format!(
        "Refresh the view `{id}` (\"{title}\"): read {source} again with your own tools and show \
         it anew with `argus-hook view show --stdin`, same id.",
        id = view.id,
    )
}

/// The view's action with this id, wherever the view offers one.
fn find_action<'a>(view: &'a AgentView, id: &str) -> Option<&'a ViewAction> {
    let header = view.actions.iter().find(|a| a.id == id);
    header.or_else(|| find_in(&view.blocks, id))
}

fn find_in<'a>(blocks: &'a [Block], id: &str) -> Option<&'a ViewAction> {
    for block in blocks {
        let own: &[ViewAction] = match block {
            Block::Card { actions, .. } | Block::Table { actions, .. } => actions,
            Block::Form { submit, .. } => std::slice::from_ref(submit),
            _ => &[],
        };
        if let Some(found) = own.iter().find(|a| a.id == id) {
            return Some(found);
        }
        if let Block::Row { items, .. } = block {
            let mut acts = items.iter().filter_map(|i| match i {
                RowItem::Action { action } => Some(action),
                RowItem::Value { .. } => None,
            });
            if let Some(found) = acts.find(|a| a.id == id) {
                return Some(found);
            }
        }
        if let Some(found) = block.nested().into_iter().find_map(|inner| find_in(inner, id)) {
            return Some(found);
        }
    }
    None
}

/// Control characters made spaces, since the request may be typed into a
/// terminal where a newline sends it early and an escape does worse.
fn plain(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The same, through every string of `value`, keys included.
fn plain_json(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(plain(&s)),
        Value::Array(items) => Value::Array(items.into_iter().map(plain_json).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (plain(&k), plain_json(v))).collect()),
        other => other,
    }
}

/// One line, like [`refresh_request`]. The rows and answers are JSON, so
/// the agent can parse them.
fn press_request(view: &AgentView, action: &ViewAction, press: &ViewPress) -> String {
    let mut text = format!(
        "The person pressed `{action_id}` (\"{label}\") in the view `{id}`.",
        action_id = plain(&action.id),
        label = plain(&action.label),
        id = plain(&view.id),
    );
    if !press.rows.is_empty() {
        let rows = plain_json(serde_json::json!(press.rows));
        text.push_str(&format!(" Selected rows: {rows}."));
    }
    if let Some(form) = &press.form {
        let answers: serde_json::Map<String, serde_json::Value> = press
            .answers
            .iter()
            .map(|a| {
                let value = match &a.value {
                    AnswerValue::Text(t) => serde_json::json!(t),
                    AnswerValue::Number(n) => serde_json::json!(n),
                    AnswerValue::Bool(b) => serde_json::json!(b),
                };
                (a.name.clone(), value)
            })
            .collect();
        let answers = plain_json(serde_json::Value::Object(answers));
        text.push_str(&format!(" Form `{}` answers: {answers}.", plain(form)));
    }
    text.push_str(&format!(
        " Do what it asks with your own tools, then show the view again with \
         `argus-hook view show --stdin`, same id `{}`, so the person sees it done.",
        plain(&view.id),
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: &str) -> ViewAction {
        ViewAction { id: id.into(), label: format!("label {id}"), tone: Default::default(), confirm: None }
    }

    fn press(action: &str) -> ViewPress {
        ViewPress { view: "t".into(), action: action.into(), rows: vec![], form: None, answers: vec![] }
    }

    #[test]
    fn an_action_is_found_wherever_the_view_offers_it() {
        let mut view = shown(1, "t").view;
        view.actions = vec![action("head")];
        let card = Block::Card {
            title: None,
            chip: None,
            tone: Default::default(),
            blocks: vec![Block::Form { id: "f".into(), fields: vec![], submit: action("send") }],
            actions: vec![action("card")],
        };
        let row = Block::Row {
            align: Default::default(),
            items: vec![RowItem::Action { action: action("inrow") }],
        };
        view.blocks = vec![Block::Section { heading: "s".into(), blocks: vec![card, row] }];
        for id in ["head", "card", "send", "inrow"] {
            assert_eq!(find_action(&view, id).map(|a| a.id.as_str()), Some(id));
        }
        assert!(find_action(&view, "rm-rf").is_none());
    }

    #[test]
    fn a_press_request_stays_on_one_line_and_carries_rows_and_answers() {
        let view = shown(1, "t").view;
        let mut act = action("ship");
        act.label = "Ship\nit".into();
        let mut p = press("ship");
        p.rows = vec!["r1".into(), "r\n2".into()];
        p.form = Some("f".into());
        p.answers = vec![
            argus_protocol::Answer { name: "note".into(), value: AnswerValue::Text("a\nb\u{1b}[0m".into()) },
            argus_protocol::Answer { name: "n".into(), value: AnswerValue::Number(3.0) },
            argus_protocol::Answer { name: "ok".into(), value: AnswerValue::Bool(true) },
        ];
        let text = press_request(&view, &act, &p);
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(text.contains("`ship`") && text.contains("`t`") && text.contains("Ship it"), "{text}");
        assert!(text.contains(r#"["r1","r 2"]"#), "{text}");
        assert!(text.contains(r#""n":3.0"#) && text.contains(r#""ok":true"#), "{text}");
        assert!(text.contains("view show --stdin"), "{text}");
    }

    fn shown(pane: u64, id: &str) -> ShownView {
        ShownView {
            pane: PaneId(pane),
            at: 0,
            subject: None,
            view: AgentView {
                id: id.into(),
                title: format!("title of {id}"),
                ..Default::default()
            },
        }
    }

    fn ids(views: &Views, pane: u64) -> Vec<&str> {
        let on = views.shown.iter().filter(|v| v.pane == PaneId(pane));
        on.map(|v| v.view.id.as_str()).collect()
    }

    #[test]
    fn showing_an_id_the_pane_has_replaces_it_in_place() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(1, "b"));
        let mut again = shown(1, "a");
        again.view.title = "newer".into();
        assert_eq!(views.show(again), None);
        assert_eq!(ids(&views, 1), ["a", "b"]);
        assert_eq!(views.get(PaneId(1), "a").unwrap().view.title, "newer");
    }

    #[test]
    fn the_same_id_on_two_panes_is_two_views() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(2, "a"));
        assert!(views.close(PaneId(1), "a"));
        assert_eq!(ids(&views, 2), ["a"]);
    }

    #[test]
    fn a_ninth_view_drops_the_panes_oldest_and_says_which() {
        let mut views = Views::default();
        views.show(shown(2, "other"));
        for n in 0..MAX_VIEWS_PER_PANE {
            assert_eq!(views.show(shown(1, &format!("v{n}"))), None);
        }
        assert_eq!(views.show(shown(1, "v8")), Some("v0".to_string()));
        assert_eq!(ids(&views, 1).len(), MAX_VIEWS_PER_PANE);
        assert_eq!(ids(&views, 2), ["other"], "another pane's views are not counted");
    }

    #[test]
    fn closing_what_is_not_held_changes_nothing() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        assert!(!views.close(PaneId(1), "zzz"));
        assert!(!views.close(PaneId(2), "a"));
        assert!(views.close(PaneId(1), "a"));
    }

    #[test]
    fn a_pane_going_takes_its_views_and_names_them() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(2, "x"));
        views.show(shown(1, "b"));
        assert_eq!(views.drop_pane(PaneId(1)), ["a", "b"]);
        assert_eq!(ids(&views, 2), ["x"]);
        assert!(views.drop_pane(PaneId(1)).is_empty());
    }

    #[test]
    fn a_refresh_request_stays_on_one_line_and_names_the_source() {
        let mut view = shown(1, "t").view;
        view.title = "line\none".into();
        view.subject = Some("https://example.com/browse/ENG-1".into());
        let text = refresh_request(&view);
        assert!(!text.contains('\n'), "{text}");
        assert!(text.contains("`t`") && text.contains("https://example.com/browse/ENG-1"), "{text}");
        view.subject = None;
        assert!(refresh_request(&view).contains("its source"));
    }
}
