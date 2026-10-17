//! What the reconcile dialog holds — the feature it reconciles, the line
//! under the cursor, the lines ticked and the words each was rewritten in,
//! and the agent an ask goes to — and what its gestures ask of the daemon.
//!
//! Every line starts unticked, and ticks are the dialog's alone until
//! Carry out sends them: what the agent may do changes only when the
//! daemon's board says the ticked lines are being carried out. A new
//! proposal starts the ticks again, since a tick was given to a line as it
//! read.

use std::collections::{BTreeMap, BTreeSet};

use argus_protocol::board::reconcile::Stage;
use argus_protocol::{
    ClientMsg, Feature, PaneId, PaneKind, PaneStatus, ProjectInfo, Reconcile, ReconcileAction, RepositoryId, Ticked,
};
use serde::{Deserialize, Serialize};

use crate::feature_stage::Scope;
use crate::keymap::Action;
use crate::connection::page::Line;

/// What was done in the reconcile dialog. A line is its place among the
/// proposal's lines.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ReconcileGesture {
    Select { line: usize },
    Tick { line: usize },
    /// The selected line's text as the person rewrote it.
    Text { line: usize, text: String },
    /// The agent to ask.
    Agent { pane: u64 },
    Ask { ask: String },
    CarryOut,
    Discard,
    /// A key of the dialog's own.
    Action { action: Action },
    Close,
}

/// The reconcile dialog: what was asked and of whom, the proposal grouped
/// by where each line writes, the line under the cursor in full, and the
/// agents an ask can go to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReconcilePage {
    pub open: bool,
    pub number: i64,
    pub title: String,
    /// The tickets, as the heading names them: `Linear ENG-88 and Jira ARG-142`.
    pub tickets: String,
    /// `none` with nothing under way, else `asked`, `proposed` or `carrying`.
    #[cfg_attr(test, ts(type = "\"none\" | \"asked\" | \"proposed\" | \"carrying\""))]
    pub stage: &'static str,
    /// The agent asked, or that proposed: `pane #32`.
    pub by: Option<String>,
    pub ago: Option<String>,
    pub asked: String,
    pub groups: Vec<LineGroup>,
    pub noticed: Vec<Line>,
    pub selected: Option<SelectedLine>,
    pub ticked: usize,
    pub total: usize,
    /// The live agents of the repository an ask can go to.
    pub agents: Vec<AgentChoice>,
    /// When the last one was carried out: `reconciled 2h ago by pane #32`.
    pub reconciled: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LineGroup {
    /// `Into Argus`, `Out to Linear`.
    pub heading: String,
    pub into_argus: bool,
    pub lines: Vec<ProposedRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProposedRow {
    pub line: usize,
    pub key: String,
    pub does: String,
    pub why: String,
    pub ticked: bool,
    pub on: bool,
    /// What the tracker said when it was carried out and failed.
    pub failed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SelectedLine {
    pub line: usize,
    pub heading: String,
    pub key: String,
    pub does: String,
    /// The text it writes, as the person last left it.
    pub text: String,
    /// The person rewrote it.
    pub edited: bool,
    /// The call the agent will make.
    pub call: String,
    pub ticked: bool,
    pub failed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AgentChoice {
    pub pane: u64,
    pub title: String,
    /// It did the last reconcile, or the one under way.
    pub last: bool,
    pub chosen: bool,
}

pub struct ReconcileDialog {
    slug: String,
    /// The line under the cursor, by its place among the proposal's lines.
    cursor: usize,
    ticked: BTreeSet<usize>,
    /// Lines rewritten before ticking, in the words the person left them.
    texts: BTreeMap<usize, String>,
    agent: Option<PaneId>,
    /// The proposal the ticks were given to, by when it was made.
    proposed_at: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    Close,
    GoTo(PaneId),
    Say(String),
}

impl ReconcileDialog {
    pub fn new(slug: &str) -> ReconcileDialog {
        ReconcileDialog {
            slug: slug.to_string(),
            cursor: 0,
            ticked: BTreeSet::new(),
            texts: BTreeMap::new(),
            agent: None,
            proposed_at: None,
        }
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// The board changed: ticks given to another proposal, or to one now
    /// being carried out, are let go.
    pub fn settle(&mut self, feature: &Feature) {
        let proposed = feature.reconcile.as_ref().filter(|r| r.stage() == Stage::Proposed);
        let at = proposed.map(|r| r.at);
        if at != self.proposed_at {
            self.proposed_at = at;
            self.ticked.clear();
            self.texts.clear();
            self.cursor = proposed.and_then(|r| display_order(r).first().copied()).unwrap_or(0);
        }
    }

    pub fn gesture(
        &mut self,
        gesture: ReconcileGesture,
        feature: &Feature,
        tree: &[ProjectInfo],
        scope: &Scope,
        out: &mut Vec<ClientMsg>,
    ) -> Then {
        let proposed = feature.reconcile.as_ref().filter(|r| r.stage() == Stage::Proposed);
        match gesture {
            ReconcileGesture::Select { line } => self.cursor = line,
            ReconcileGesture::Tick { line } => return self.tick(proposed, line),
            ReconcileGesture::Text { line, text } => {
                if proposed.is_some_and(|r| r.lines.get(line).is_some_and(|l| l.text != text)) {
                    self.texts.insert(line, text);
                } else {
                    self.texts.remove(&line);
                }
            }
            ReconcileGesture::Agent { pane } => self.agent = Some(PaneId(pane)),
            ReconcileGesture::Ask { ask } => return self.ask(ask, feature, tree, scope, out),
            ReconcileGesture::CarryOut => return self.carry_out(proposed, scope, out),
            ReconcileGesture::Discard => return self.discard(feature, scope, out),
            ReconcileGesture::Action { action } => return self.action(action, feature, scope, out),
            ReconcileGesture::Close => return Then::Close,
        }
        Then::Nothing
    }

    fn action(&mut self, action: Action, feature: &Feature, scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        let proposed = feature.reconcile.as_ref().filter(|r| r.stage() == Stage::Proposed);
        match action {
            Action::Next | Action::Previous => {
                let order = feature.reconcile.as_ref().map(display_order).unwrap_or_default();
                let at = order.iter().position(|&l| l == self.cursor).unwrap_or(0);
                let to = if action == Action::Next { (at + 1).min(order.len().saturating_sub(1)) } else { at.saturating_sub(1) };
                if let Some(&line) = order.get(to) {
                    self.cursor = line;
                }
            }
            Action::Tick => return self.tick(proposed, self.cursor),
            Action::CarryOut => return self.carry_out(proposed, scope, out),
            Action::Discard => return self.discard(feature, scope, out),
            Action::GoToPane => {
                if let Some(reconcile) = &feature.reconcile {
                    return Then::GoTo(reconcile.pane);
                }
            }
            Action::CloseLayer => return Then::Close,
            _ => {}
        }
        Then::Nothing
    }

    fn tick(&mut self, proposed: Option<&Reconcile>, line: usize) -> Then {
        if !proposed.is_some_and(|r| line < r.lines.len()) {
            return Then::Nothing;
        }
        if !self.ticked.remove(&line) {
            self.ticked.insert(line);
        }
        Then::Nothing
    }

    fn ask(&mut self, ask: String, feature: &Feature, tree: &[ProjectInfo], scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        let Some(pane) = self.chosen(feature, tree, scope.repository) else {
            return Then::Say("no agent runs in this repository to ask; start one first".into());
        };
        if let Err(why) = argus_protocol::board::reconcile::checked_ask(&ask) {
            return Then::Say(why.into());
        }
        out.push(self.send(scope, ReconcileAction::Ask { pane, ask }));
        Then::Nothing
    }

    fn carry_out(&mut self, proposed: Option<&Reconcile>, scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        let Some(reconcile) = proposed else {
            return Then::Say("no proposal waits to be carried out".into());
        };
        if self.ticked.is_empty() {
            return Then::Say("tick the lines to carry out first; every line starts unticked".into());
        }
        let ticked = self
            .ticked
            .iter()
            .filter_map(|&line| {
                let proposed = reconcile.lines.get(line)?;
                Some(Ticked { line, text: self.texts.get(&line).unwrap_or(&proposed.text).clone() })
            })
            .collect();
        out.push(self.send(scope, ReconcileAction::CarryOut { ticked }));
        Then::Nothing
    }

    fn discard(&mut self, feature: &Feature, scope: &Scope, out: &mut Vec<ClientMsg>) -> Then {
        if feature.reconcile.is_none() {
            return Then::Say("no reconcile is under way to discard".into());
        }
        out.push(self.send(scope, ReconcileAction::Discard));
        Then::Nothing
    }

    fn send(&self, scope: &Scope, action: ReconcileAction) -> ClientMsg {
        ClientMsg::Reconcile { project: scope.project, checkout: scope.checkout, slug: self.slug.clone(), action }
    }

    /// The agent an ask goes to: the one chosen while it runs, else the
    /// one that did the last reconcile, else the first.
    fn chosen(&self, feature: &Feature, tree: &[ProjectInfo], repository: RepositoryId) -> Option<PaneId> {
        let agents = agents(tree, repository);
        let last = last_by(feature);
        self.agent
            .filter(|pane| agents.iter().any(|(id, _)| id == pane))
            .or_else(|| agents.iter().find(|(id, _)| last.as_deref() == Some(by(*id).as_str())).map(|(id, _)| *id))
            .or_else(|| agents.first().map(|(id, _)| *id))
    }

    pub fn page(&self, feature: &Feature, tree: &[ProjectInfo], repository: RepositoryId, now: u64) -> ReconcilePage {
        let reconcile = feature.reconcile.as_ref();
        let stage = reconcile.map(Reconcile::stage);
        let carrying: BTreeMap<usize, &str> =
            reconcile.map(|r| r.carrying.iter().map(|t| (t.line, t.text.as_str())).collect()).unwrap_or_default();
        let is_ticked = |line: usize| match stage {
            Some(Stage::Carrying) => carrying.contains_key(&line),
            _ => self.ticked.contains(&line),
        };
        let mut groups: Vec<LineGroup> = Vec::new();
        if let Some(r) = reconcile {
            for line in display_order(r) {
                let proposed = &r.lines[line];
                let row = ProposedRow {
                    line,
                    key: proposed.key.clone(),
                    does: proposed.does.clone(),
                    why: proposed.why.clone(),
                    ticked: is_ticked(line),
                    on: line == self.cursor,
                    failed: proposed.failed.clone(),
                };
                let heading = proposed.heading();
                match groups.iter_mut().find(|g| g.heading == heading) {
                    Some(group) => group.lines.push(row),
                    None => groups.push(LineGroup { heading, into_argus: proposed.into_argus(), lines: vec![row] }),
                }
            }
        }
        let selected = reconcile.and_then(|r| {
            let line = r.lines.get(self.cursor)?;
            let text = match stage {
                Some(Stage::Carrying) => carrying.get(&self.cursor).map_or(line.text.as_str(), |t| t),
                _ => self.texts.get(&self.cursor).map_or(line.text.as_str(), String::as_str),
            };
            Some(SelectedLine {
                line: self.cursor,
                heading: line.heading(),
                key: line.key.clone(),
                does: line.does.clone(),
                text: text.to_string(),
                edited: text != line.text,
                call: line.call.clone(),
                ticked: is_ticked(self.cursor),
                failed: line.failed.clone(),
            })
        });
        let chosen = self.chosen(feature, tree, repository);
        let last = last_by(feature);
        let tickets: Vec<String> = feature.links.iter().map(|l| format!("{} {}", l.tracker, l.key)).collect();
        ReconcilePage {
            open: true,
            number: feature.number,
            title: feature.title.clone(),
            tickets: match tickets.as_slice() {
                [] => String::new(),
                [only] => only.clone(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            },
            stage: stage.map_or("none", Stage::as_str),
            by: reconcile.map(|r| r.by.clone()),
            ago: reconcile.map(|r| argus_protocol::ago(Some(r.at.max(0) as u64), now)),
            asked: reconcile.map(|r| r.asked.clone()).unwrap_or_default(),
            groups,
            noticed: reconcile
                .map(|r| r.noticed.iter().map(|n| Line { label: n.key.clone(), text: n.text.clone(), aside: None }).collect())
                .unwrap_or_default(),
            selected,
            ticked: reconcile.map_or(0, |r| (0..r.lines.len()).filter(|&l| is_ticked(l)).count()),
            total: reconcile.map_or(0, |r| r.lines.len()),
            agents: agents(tree, repository)
                .into_iter()
                .map(|(pane, title)| AgentChoice {
                    pane: pane.0,
                    title,
                    last: last.as_deref() == Some(by(pane).as_str()),
                    chosen: Some(pane) == chosen,
                })
                .collect(),
            reconciled: feature
                .reconciled
                .as_ref()
                .map(|r| format!("reconciled {} ago by {}", argus_protocol::ago(Some(r.at.max(0) as u64), now), r.by)),
        }
    }
}

/// The order the lines are listed in: grouped by where they write, each
/// group where its first line was proposed.
fn display_order(reconcile: &Reconcile) -> Vec<usize> {
    let mut headings: Vec<String> = Vec::new();
    for line in &reconcile.lines {
        if !headings.contains(&line.heading()) {
            headings.push(line.heading());
        }
    }
    headings
        .iter()
        .flat_map(|heading| (0..reconcile.lines.len()).filter(move |&l| &reconcile.lines[l].heading() == heading))
        .collect()
}

/// The live agents in the repository's checkouts, as the dialog names them.
fn agents(tree: &[ProjectInfo], repository: RepositoryId) -> Vec<(PaneId, String)> {
    tree.iter()
        .flat_map(|p| &p.repositories)
        .filter(|r| r.id == repository)
        .flat_map(|r| &r.checkouts)
        .flat_map(|c| c.panes.iter().map(move |pane| (c, pane)))
        .filter(|(_, pane)| pane.kind == PaneKind::Agent && !matches!(pane.status, PaneStatus::Exited { .. }))
        .map(|(c, pane)| (pane.id, format!("{} · {} #{}", c.name, pane.title, pane.id.0)))
        .collect()
}

fn by(pane: PaneId) -> String {
    format!("pane #{}", pane.0)
}

/// Who did the reconcile under way, or the last one carried out.
fn last_by(feature: &Feature) -> Option<String> {
    feature.reconcile.as_ref().map(|r| r.by.clone()).or_else(|| feature.reconciled.as_ref().map(|r| r.by.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{CheckoutId, ProjectId, ProposedLine, Reconciled};

    fn scope() -> Scope {
        Scope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(10),
            repository_name: "argus".into(),
            checkout: CheckoutId(100),
            key: "/src/main".into(),
        }
    }

    fn tree() -> Vec<ProjectInfo> {
        let agents = vec![
            pane(31, PaneKind::Agent, "claude", PaneStatus::Idle),
            pane(32, PaneKind::Agent, "codex", PaneStatus::Idle),
            pane(33, PaneKind::Shell, "zsh", PaneStatus::Idle),
        ];
        vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(100, "main", true, agents)])])]
    }

    fn line(to: &str, key: &str, text: &str) -> ProposedLine {
        ProposedLine { to: to.into(), key: key.into(), does: "do".into(), text: text.into(), ..ProposedLine::default() }
    }

    fn proposed(at: i64) -> Feature {
        Feature {
            slug: "trim".into(),
            number: 7,
            reconcile: Some(Reconcile {
                asked: "pull and push".into(),
                pane: PaneId(32),
                by: "pane #32".into(),
                at,
                proposed: true,
                lines: vec![line("Jira", "ARG-142", "comment"), line("Argus", "ENG-94", "task"), line("Linear", "ENG-91", "")],
                noticed: Vec::new(),
                carrying: Vec::new(),
            }),
            ..Feature::default()
        }
    }

    fn dialog(feature: &Feature) -> ReconcileDialog {
        let mut dialog = ReconcileDialog::new("trim");
        dialog.settle(feature);
        dialog
    }

    fn act(d: &mut ReconcileDialog, f: &Feature, gesture: ReconcileGesture, out: &mut Vec<ClientMsg>) -> Then {
        d.gesture(gesture, f, &tree(), &scope(), out)
    }

    #[test]
    fn lines_are_grouped_by_where_they_write_and_start_unticked() {
        let feature = proposed(1);
        let page = dialog(&feature).page(&feature, &tree(), RepositoryId(10), 60);
        let headings: Vec<_> = page.groups.iter().map(|g| g.heading.as_str()).collect();
        assert_eq!(headings, ["Out to Jira", "Into Argus", "Out to Linear"]);
        assert_eq!((page.ticked, page.total, page.stage), (0, 3, "proposed"));
        assert!(page.groups.iter().flat_map(|g| &g.lines).all(|l| !l.ticked));
    }

    #[test]
    fn only_the_ticked_lines_are_carried_out_in_the_words_they_were_left_in() {
        let feature = proposed(1);
        let mut d = dialog(&feature);
        let mut out = Vec::new();
        let then = act(&mut d, &feature, ReconcileGesture::CarryOut, &mut out);
        assert!(matches!(then, Then::Say(_)) && out.is_empty(), "nothing ticked, nothing sent");

        act(&mut d, &feature, ReconcileGesture::Text { line: 0, text: "Decision D2, reworded".into() }, &mut out);
        act(&mut d, &feature, ReconcileGesture::Tick { line: 0 }, &mut out);
        act(&mut d, &feature, ReconcileGesture::Tick { line: 2 }, &mut out);
        act(&mut d, &feature, ReconcileGesture::Tick { line: 2 }, &mut out);
        let page = d.page(&feature, &tree(), RepositoryId(10), 60);
        assert!(page.selected.as_ref().unwrap().edited);
        act(&mut d, &feature, ReconcileGesture::CarryOut, &mut out);
        assert!(matches!(&out[..], [ClientMsg::Reconcile { action: ReconcileAction::CarryOut { ticked }, .. }]
            if ticked == &[Ticked { line: 0, text: "Decision D2, reworded".into() }]));
        assert_eq!(d.page(&feature, &tree(), RepositoryId(10), 60).ticked, 1, "ticks stay until the board moves on");
    }

    #[test]
    fn a_new_proposal_starts_the_ticks_again() {
        let mut d = dialog(&proposed(1));
        let mut out = Vec::new();
        act(&mut d, &proposed(1), ReconcileGesture::Tick { line: 1 }, &mut out);
        d.settle(&proposed(1));
        assert_eq!(d.page(&proposed(1), &tree(), RepositoryId(10), 60).ticked, 1);
        d.settle(&proposed(2));
        assert_eq!(d.page(&proposed(2), &tree(), RepositoryId(10), 60).ticked, 0);
    }

    #[test]
    fn the_keys_move_in_the_order_the_lines_are_listed_and_tick_the_one_under_the_cursor() {
        let feature = proposed(1);
        let mut d = dialog(&feature);
        let mut out = Vec::new();
        for _ in 0..2 {
            act(&mut d, &feature, ReconcileGesture::Action { action: Action::Next }, &mut out);
        }
        act(&mut d, &feature, ReconcileGesture::Action { action: Action::Tick }, &mut out);
        let page = d.page(&feature, &tree(), RepositoryId(10), 60);
        assert_eq!(page.selected.unwrap().key, "ENG-91", "Jira, Argus, then Linear");
        assert_eq!(page.ticked, 1);
        assert_eq!(act(&mut d, &feature, ReconcileGesture::Action { action: Action::GoToPane }, &mut out), Then::GoTo(PaneId(32)));
    }

    #[test]
    fn while_lines_are_carried_out_the_page_shows_what_was_sent() {
        let mut feature = proposed(1);
        feature.reconcile.as_mut().unwrap().carrying = vec![Ticked { line: 1, text: "as sent".into() }];
        let mut d = dialog(&feature);
        let mut out = Vec::new();
        act(&mut d, &feature, ReconcileGesture::Tick { line: 0 }, &mut out);
        act(&mut d, &feature, ReconcileGesture::Select { line: 1 }, &mut out);
        let page = d.page(&feature, &tree(), RepositoryId(10), 60);
        assert_eq!((page.stage, page.ticked), ("carrying", 1), "nothing more is ticked");
        assert_eq!(page.selected.unwrap().text, "as sent");
    }

    #[test]
    fn an_ask_goes_to_the_agent_that_did_the_last_one_unless_another_is_chosen() {
        let mut feature = Feature { slug: "trim".into(), ..Feature::default() };
        feature.reconciled = Some(Reconciled { at: 0, by: "pane #32".into(), seen: Vec::new() });
        let mut d = dialog(&feature);
        let page = d.page(&feature, &tree(), RepositoryId(10), 60);
        assert_eq!(page.agents.len(), 2, "a shell is not asked");
        assert!(page.agents.iter().any(|a| a.pane == 32 && a.last && a.chosen));
        let mut out = Vec::new();
        act(&mut d, &feature, ReconcileGesture::Ask { ask: "pull ENG-88".into() }, &mut out);
        assert!(matches!(&out[..], [ClientMsg::Reconcile { action: ReconcileAction::Ask { pane: PaneId(32), .. }, .. }]));
        act(&mut d, &feature, ReconcileGesture::Agent { pane: 31 }, &mut out);
        act(&mut d, &feature, ReconcileGesture::Ask { ask: "pull ENG-88".into() }, &mut out);
        assert!(matches!(&out[1], ClientMsg::Reconcile { action: ReconcileAction::Ask { pane: PaneId(31), .. }, .. }));
        let then = act(&mut d, &feature, ReconcileGesture::Ask { ask: " ".into() }, &mut out);
        assert!(matches!(then, Then::Say(_)) && out.len() == 2);
    }
}
