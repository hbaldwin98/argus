//! Reconciling a feature with its tickets: what an agent proposes to carry
//! between Argus and each tracker, which lines the person ticks and in what
//! words, and what changed on Argus's side since the last one was carried
//! out.
//!
//! The person says what to reconcile; an agent reads the tracker map, the
//! feature and the tickets with its own tools, writes nothing, and proposes
//! lines. Every line starts unticked, and the agent carries out only the
//! ones the person ticked, in the words they ticked them. Argus holds the
//! proposal and nothing read from a tracker beyond the words of its lines,
//! which go when it is carried out or turned down.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::board::decisions::Decision;
use crate::tree::ids::PaneId;
use crate::board::tasks::Task;

/// Room for a reconcile that touches every task of a large feature, and no
/// more: past it the proposal is a migration, not a reconcile.
pub const MAX_PROPOSED_LINES: usize = 60;
pub const MAX_NOTICED: usize = 30;
/// A comment or a description, word for word.
pub const MAX_LINE_TEXT_BYTES: usize = 8 * 1024;
/// What a line does, why, and its key: a few words each.
pub const MAX_LINE_WORDS_BYTES: usize = 300;
/// The call the agent will make, as it would say it.
pub const MAX_CALL_BYTES: usize = 1024;
/// What the person asked for, said in a sentence or two.
pub const MAX_ASK_BYTES: usize = 2000;
pub const MAX_FAILURE_BYTES: usize = 1000;
/// A proposal's JSON with every bound reached, and room for its quoting.
pub const MAX_PROPOSAL_BYTES: usize =
    MAX_PROPOSED_LINES * 2 * (MAX_LINE_TEXT_BYTES + MAX_CALL_BYTES + 4 * MAX_LINE_WORDS_BYTES)
        + MAX_NOTICED * 4 * MAX_LINE_WORDS_BYTES
        + 2 * MAX_ASK_BYTES;

/// The side a line brings something into, rather than sends out to.
pub const INTO_ARGUS: &str = "Argus";

/// One change a proposal would make, in one direction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedLine {
    /// Where it writes: `Argus` for a change brought in, else the tracker
    /// it writes to, by the name its links use: `Linear`, `Jira`.
    pub to: String,
    /// The ticket it touches or comes from, or the task it is about.
    pub key: String,
    /// What it does, in a few words: `move to Done`.
    pub does: String,
    /// Why, in a few words: `task 2.1 is done`.
    #[serde(default)]
    pub why: String,
    /// The exact text it writes — a comment, a title — which the person may
    /// rewrite before ticking. Empty for a line that writes no text.
    #[serde(default)]
    pub text: String,
    /// The call the agent will make, laid out as it would say it.
    #[serde(default)]
    pub call: String,
    /// What the tracker answered when the line was carried out and failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

impl ProposedLine {
    pub fn into_argus(&self) -> bool {
        self.to.eq_ignore_ascii_case(INTO_ARGUS)
    }

    /// The heading it is listed under.
    pub fn heading(&self) -> String {
        if self.into_argus() {
            "Into Argus".to_string()
        } else {
            format!("Out to {}", self.to)
        }
    }
}

/// Something the agent saw that is outside what it was asked: listed for
/// the person, never acted on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Noticed {
    pub key: String,
    pub text: String,
}

/// A proposal as an agent sends it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalWrite {
    /// What the person asked for, in their words, so the proposal is read
    /// against it.
    #[serde(default)]
    pub asked: String,
    pub lines: Vec<ProposedLine>,
    #[serde(default)]
    pub noticed: Vec<Noticed>,
}

impl ProposalWrite {
    /// The proposal trimmed and held to its bounds, or what is wrong with
    /// it, naming the line.
    pub fn checked(self) -> Result<ProposalWrite, String> {
        if self.lines.is_empty() && self.noticed.is_empty() {
            return Err("a proposal has lines to tick, or says what was noticed".into());
        }
        if self.lines.len() > MAX_PROPOSED_LINES {
            return Err(format!("a proposal is at most {MAX_PROPOSED_LINES} lines"));
        }
        if self.noticed.len() > MAX_NOTICED {
            return Err(format!("at most {MAX_NOTICED} things noticed"));
        }
        let asked = bounded(&self.asked, MAX_ASK_BYTES).map_err(|e| format!("asked: {e}"))?;
        let mut lines = Vec::with_capacity(self.lines.len());
        for (i, line) in self.lines.into_iter().enumerate() {
            let at = |field: &str, e: &str| format!("lines[{i}].{field}: {e}");
            let words = |field: &str, text: &str| {
                let text = one_line(text, MAX_LINE_WORDS_BYTES).map_err(|e| at(field, e))?;
                Ok::<_, String>(text)
            };
            let line = ProposedLine {
                to: words("to", &line.to)?,
                key: words("key", &line.key)?,
                does: words("does", &line.does)?,
                why: words("why", &line.why)?,
                text: checked_text(&line.text).map_err(|e| at("text", e))?,
                call: bounded(&line.call, MAX_CALL_BYTES).map_err(|e| at("call", e))?,
                failed: None,
            };
            if line.to.is_empty() || line.does.is_empty() {
                return Err(at("to", "a line says where it writes and what it does"));
            }
            lines.push(line);
        }
        let mut noticed = Vec::with_capacity(self.noticed.len());
        for (i, item) in self.noticed.into_iter().enumerate() {
            let at = |e: &str| format!("noticed[{i}]: {e}");
            let item = Noticed {
                key: one_line(&item.key, MAX_LINE_WORDS_BYTES).map_err(at)?,
                text: one_line(&item.text, MAX_LINE_WORDS_BYTES).map_err(at)?,
            };
            if item.text.is_empty() {
                return Err(at("says nothing"));
            }
            noticed.push(item);
        }
        Ok(ProposalWrite { asked, lines, noticed })
    }
}

/// A line the person ticked, in the words they ticked it in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticked {
    /// Its place among the proposal's lines, counted from zero.
    pub line: usize,
    pub text: String,
}

/// A feature's reconcile while one is under way: asked of an agent, then
/// proposed, then being carried out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconcile {
    /// What the person asked for.
    pub asked: String,
    /// The agent asked, or the one that proposed: the one told to carry it
    /// out. A runtime id, kept beside `by` only to reach the pane.
    pub pane: PaneId,
    /// That pane as the view names it: `pane #32`.
    pub by: String,
    /// When it was asked for or proposed, in Unix seconds.
    pub at: i64,
    /// Whether the agent has proposed yet; until then it is reading.
    pub proposed: bool,
    pub lines: Vec<ProposedLine>,
    pub noticed: Vec<Noticed>,
    /// The lines the person ticked and sent, while the agent carries them
    /// out. Nothing more is ticked until it reports.
    #[serde(default)]
    pub carrying: Vec<Ticked>,
}

impl Reconcile {
    /// What it is waiting on, as the dialog and the agent's read say it.
    pub fn stage(&self) -> Stage {
        if !self.carrying.is_empty() {
            Stage::Carrying
        } else if self.proposed {
            Stage::Proposed
        } else {
            Stage::Asked
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The agent is reading; nothing to tick yet.
    Asked,
    /// Lines wait for the person.
    Proposed,
    /// The ticked lines are the agent's to carry out.
    Carrying,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Asked => "asked",
            Stage::Proposed => "proposed",
            Stage::Carrying => "carrying",
        }
    }
}

/// The last reconcile carried out: when, by whom, and how the feature's
/// tasks and decisions stood then, which is what "changed since" is
/// counted against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
    pub at: i64,
    pub by: String,
    pub seen: Vec<Seen>,
}

/// One row as it stood when a reconcile was carried out, as the words that
/// would have to change for a tracker to need telling: a task's title,
/// state and parent, a decision's answer and state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Seen {
    Task { id: i64, mark: String },
    Decision { id: i64, mark: String },
}

fn task_mark(task: &Task) -> String {
    format!("{}|{:?}|{}", task.state.as_str(), task.parent, task.title)
}

fn decision_mark(decision: &Decision) -> String {
    let standing = if decision.withdrawn() {
        "withdrawn"
    } else if decision.superseded() {
        "superseded"
    } else {
        decision.state.as_str()
    };
    format!("{standing}|{}", decision.chose)
}

/// How a feature's tasks and decisions stand now, to be kept as `seen`.
pub fn seen(tasks: &[Task], decisions: &[&Decision]) -> Vec<Seen> {
    let tasks = tasks.iter().map(|t| Seen::Task { id: t.id, mark: task_mark(t) });
    let decisions = decisions.iter().map(|d| Seen::Decision { id: d.id, mark: decision_mark(d) });
    tasks.chain(decisions).collect()
}

/// What changed on Argus's side since a reconcile: rows added or changed,
/// and how many went.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub tasks: BTreeSet<i64>,
    pub decisions: BTreeSet<i64>,
    pub removed: usize,
}

impl Changes {
    pub fn since(reconciled: &Reconciled, tasks: &[Task], decisions: &[&Decision]) -> Changes {
        let mut then: BTreeMap<(bool, i64), &str> = BTreeMap::new();
        for row in &reconciled.seen {
            match row {
                Seen::Task { id, mark } => then.insert((true, *id), mark),
                Seen::Decision { id, mark } => then.insert((false, *id), mark),
            };
        }
        let mut changes = Changes::default();
        for task in tasks {
            if then.remove(&(true, task.id)) != Some(task_mark(task).as_str()) {
                changes.tasks.insert(task.id);
            }
        }
        for decision in decisions {
            if then.remove(&(false, decision.id)) != Some(decision_mark(decision).as_str()) {
                changes.decisions.insert(decision.id);
            }
        }
        changes.removed = then.len();
        changes
    }

    pub fn count(&self) -> usize {
        self.tasks.len() + self.decisions.len() + self.removed
    }
}

/// What the person asks of a feature's reconcile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconcileAction {
    /// Asks an agent to read the feature, the map and the tickets and
    /// propose, writing nothing.
    Ask { pane: PaneId, ask: String },
    /// Sends the agent the ticked lines to carry out, in these words.
    CarryOut { ticked: Vec<Ticked> },
    /// Turns the proposal down, or takes back an ask.
    Discard,
}

/// What an agent sends about a feature's reconcile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum ReconcileChange {
    /// Reads the reconcile under way and changes nothing.
    Read,
    Propose { proposal: ProposalWrite },
    /// The ticked lines are carried out; those in `failed` were not.
    Report { failed: Vec<Failed> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failed {
    /// The line's place among the proposal's lines, counted from zero.
    pub line: usize,
    /// What the tracker answered.
    pub error: String,
}

/// A line's text as it will be written, whoever wrote it last.
pub fn checked_text(text: &str) -> Result<String, &'static str> {
    let text = text.trim();
    if text.len() > MAX_LINE_TEXT_BYTES {
        return Err("that is longer than a line's text may be");
    }
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("a line's text is plain text");
    }
    Ok(text.to_string())
}

/// What the person asked for, as it will be kept and sent on one line.
pub fn checked_ask(ask: &str) -> Result<String, &'static str> {
    let ask = one_line(ask, MAX_ASK_BYTES)?;
    if ask.is_empty() {
        return Err("say what to reconcile: what to bring in, what to send out");
    }
    Ok(ask)
}

fn bounded(text: &str, max: usize) -> Result<String, &'static str> {
    let text = text.trim();
    if text.len() > max {
        return Err("too long");
    }
    if text.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("plain text only");
    }
    Ok(text.to_string())
}

/// Text that will be said on one line: a newline would end a message typed
/// into an agent's terminal early.
fn one_line(text: &str, max: usize) -> Result<String, &'static str> {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.len() > max {
        return Err("too long for a line");
    }
    if text.chars().any(char::is_control) {
        return Err("plain text only");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::decisions::DecisionState;
    use crate::board::tasks::TaskState;

    fn line(to: &str, key: &str, does: &str) -> ProposedLine {
        ProposedLine { to: to.into(), key: key.into(), does: does.into(), ..ProposedLine::default() }
    }

    fn task(id: i64, title: &str, state: TaskState) -> Task {
        Task { id, title: title.into(), state, ..Task::default() }
    }

    fn decision(id: i64, chose: &str) -> Decision {
        Decision { id, chose: chose.into(), state: DecisionState::Accepted, ..Decision::default() }
    }

    #[test]
    fn a_line_is_listed_under_where_it_writes() {
        assert_eq!(line("argus", "ENG-94", "add as task 2.4").heading(), "Into Argus");
        assert_eq!(line("Jira", "ARG-142", "comment").heading(), "Out to Jira");
    }

    #[test]
    fn a_proposal_is_trimmed_and_its_fault_named_by_line() {
        let ok = ProposalWrite {
            asked: " pull new sub-issues ".into(),
            lines: vec![ProposedLine { text: "  Decision D2\n\nthe brief  ".into(), ..line(" Linear ", "ENG-91", "move  to\nDone") }],
            noticed: vec![Noticed { key: "ENG-93".into(), text: "title differs".into() }],
        }
        .checked()
        .unwrap();
        assert_eq!(ok.asked, "pull new sub-issues");
        assert_eq!((ok.lines[0].to.as_str(), ok.lines[0].does.as_str()), ("Linear", "move to Done"));
        assert_eq!(ok.lines[0].text, "Decision D2\n\nthe brief", "the text keeps its lines");

        let bad = ProposalWrite { lines: vec![line("Jira", "A-1", "x"), line("Jira", "A-2", "")], ..ProposalWrite::default() };
        assert!(bad.checked().unwrap_err().starts_with("lines[1]"));
        let long = ProposalWrite { lines: vec![ProposedLine { text: "x".repeat(MAX_LINE_TEXT_BYTES + 1), ..line("Jira", "A", "b") }], ..ProposalWrite::default() };
        assert!(long.checked().unwrap_err().starts_with("lines[0].text"));
        assert!(ProposalWrite::default().checked().is_err(), "an empty proposal proposes nothing");
    }

    #[test]
    fn the_stage_follows_what_the_reconcile_waits_on() {
        let mut r = Reconcile {
            asked: "pull".into(),
            pane: PaneId(3),
            by: "pane #3".into(),
            at: 0,
            proposed: false,
            lines: Vec::new(),
            noticed: Vec::new(),
            carrying: Vec::new(),
        };
        assert_eq!(r.stage(), Stage::Asked);
        r.proposed = true;
        assert_eq!(r.stage(), Stage::Proposed);
        r.carrying.push(Ticked { line: 0, text: String::new() });
        assert_eq!(r.stage(), Stage::Carrying);
    }

    #[test]
    fn changes_since_a_reconcile_are_rows_added_changed_or_gone() {
        let tasks = vec![task(1, "name it", TaskState::Done), task(2, "serve it", TaskState::Todo), task(3, "gone", TaskState::Todo)];
        let decisions = [decision(9, "the daemon")];
        let then = Reconciled { at: 1, by: "pane #3".into(), seen: seen(&tasks, &decisions.iter().collect::<Vec<_>>()) };
        assert_eq!(Changes::since(&then, &tasks, &decisions.iter().collect::<Vec<_>>()).count(), 0, "nothing moved");

        let mut now = tasks.clone();
        now[1].state = TaskState::Done;
        now.remove(2);
        now.push(task(4, "new", TaskState::Todo));
        let mut withdrawn = decision(9, "the daemon");
        withdrawn.withdrawn_at = Some(5);
        let changes = Changes::since(&then, &now, &[&withdrawn]);
        assert_eq!(changes.tasks, BTreeSet::from([2, 4]));
        assert_eq!(changes.decisions, BTreeSet::from([9]));
        assert_eq!((changes.removed, changes.count()), (1, 4));
    }

    #[test]
    fn an_ask_is_one_line_and_says_something() {
        assert_eq!(checked_ask(" pull\nENG-88 ").unwrap(), "pull ENG-88");
        assert!(checked_ask("  ").is_err());
    }

    #[test]
    fn an_agents_change_reads_as_tagged_json() {
        let report: ReconcileChange = serde_json::from_str(r#"{"action":"report","failed":[{"line":1,"error":"403"}]}"#).unwrap();
        assert_eq!(report, ReconcileChange::Report { failed: vec![Failed { line: 1, error: "403".into() }] });
        let read: ReconcileChange = serde_json::from_str(r#"{"action":"read"}"#).unwrap();
        assert_eq!(read, ReconcileChange::Read);
    }
}
