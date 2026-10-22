//! What the open form is about, what it starts out saying, which edit it sends and whether what it is
//! about is still on screen (BOARD-FORM-1, -5, -7, -8, -12 to -14): the page's `Editing` and `send`
//! (`lenses/work.ts`) without a field in them.
//!
//! The words in a form's fields are the fields' own, kept in the `Form` entity; this holds only
//! what the form is about, by the id an edit names, so a push that redraws the feature never has
//! anything of the person's to lose, and a form can be tested without a window.

use argus_desktop_core::connection::page::{FeatureEdit, FeatureView, WorkAt};

/// One form at a time. Opening another puts the first away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editing {
    Brief,
    Task { id: i64 },
    /// A decision as the ADR document it is edited as.
    Record { id: i64 },
    /// An open decision answered in words.
    Answer { id: i64 },
    Criterion { id: i64 },
    AddTask { parent: Option<i64> },
    AddDecision { under: Option<i64>, task: Option<i64> },
    Supersede { id: i64 },
    AddFlow,
    /// A flow's title and its Mermaid source.
    Flow { id: i64 },
    Step { diagram: i64, step: usize },
}

/// Which field of a form takes the keys when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Line,
    Area,
}

/// What the two fields of a form say and show when it opens. A form with no multi-line field has an
/// empty `area`, and one with no one-line field an empty `line`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    pub line: String,
    pub line_hint: &'static str,
    pub area: String,
    pub area_hint: &'static str,
    /// The field that is typed in first; the other (a description, a because, a source) does not take
    /// the keys until it is clicked.
    pub focus: Focus,
}

/// The one-line decision field's hint: how a typed decision says what it is.
pub const DECISION_LINE: &str = "? a question still open, ~ an answer turned down, or what was decided";

/// What `editing` starts out saying, read off the feature as it stands.
pub fn fill(editing: Editing, view: &FeatureView) -> Fill {
    let line = |text: &str, hint: &'static str| Fill { line: text.into(), line_hint: hint, area: String::new(), area_hint: "", focus: Focus::Line };
    let area = |text: &str, hint: &'static str| Fill { line: String::new(), line_hint: "", area: text.into(), area_hint: hint, focus: Focus::Area };
    match editing {
        Editing::Brief => area(&view.brief, "What is being built, and why"),
        Editing::Task { id } => {
            let task = view.task.as_ref().filter(|task| task.id == id);
            Fill {
                line: task.map(|task| task.title.clone()).unwrap_or_default(),
                line_hint: "The task, in a line",
                area: task.map(|task| task.body.clone()).unwrap_or_default(),
                area_hint: "What it is, and anything the one doing it should know",
                focus: Focus::Line,
            }
        }
        Editing::Record { id } => area(&decision_of(view, id).map(|d| d.document.clone()).unwrap_or_default(), ""),
        Editing::Answer { .. } => Fill { line: String::new(), line_hint: "Why, if it needs saying", area: String::new(), area_hint: "What was decided", focus: Focus::Area },
        Editing::Criterion { .. } => line("", "One checkable line"),
        Editing::AddTask { .. } => line("", "What is to be done, in a line"),
        Editing::AddDecision { .. } | Editing::Supersede { .. } => line("", DECISION_LINE),
        Editing::AddFlow => line("", "What it shows"),
        Editing::Flow { id } => {
            let flow = view.flows.as_ref().and_then(|flows| flows.flow.as_ref()).filter(|flow| flow.id == id);
            Fill {
                line: flow.map(|flow| flow.title.clone()).unwrap_or_default(),
                line_hint: "What the flow shows",
                area: flow.map(|flow| flow.source.clone()).unwrap_or_default(),
                area_hint: "sequenceDiagram",
                focus: Focus::Line,
            }
        }
        Editing::Step { diagram, step } => {
            let text = view
                .flows
                .as_ref()
                .and_then(|flows| flows.flow.as_ref())
                .filter(|flow| flow.id == diagram)
                .and_then(|flow| flow.step.as_ref())
                .filter(|found| found.number == step)
                .map(|found| found.text.clone())
                .unwrap_or_default();
            line(&text, "what is asked or answered")
        }
    }
}

/// The decision `id` as the open feature reads it, wherever it is read (the Work lens's cursor or the
/// Record lens's).
fn decision_of(view: &FeatureView, id: i64) -> Option<&argus_desktop_core::connection::page::DecisionDetail> {
    view.decision.iter().chain(view.record.iter().filter_map(|record| record.decision.as_ref())).find(|decision| decision.id == id)
}

/// The edit `editing` sends with what its fields say: the page's `send`, one edit for each form. Rust
/// refuses what is not sound (a blank row, a document that is not an ADR) and says why in a notice.
pub fn edit(editing: Editing, line: &str, area: &str) -> FeatureEdit {
    let (line, area) = (line.to_owned(), area.to_owned());
    match editing {
        Editing::Brief => FeatureEdit::Brief { body: area },
        Editing::Task { id } => FeatureEdit::Task { id, title: line, body: area },
        Editing::Record { id } => FeatureEdit::Rewrite { id, document: area },
        Editing::Answer { id } => FeatureEdit::Answer { id, answer: area, because: Some(line) },
        Editing::Criterion { id } => FeatureEdit::AddCriterion { id, text: line },
        Editing::AddTask { parent } => FeatureEdit::AddTask { parent, title: line },
        Editing::AddDecision { under, task } => FeatureEdit::AddDecision { under, task, line },
        Editing::Supersede { id } => FeatureEdit::Supersede { id, line },
        Editing::AddFlow => FeatureEdit::AddFlow { title: line },
        Editing::Flow { id } => FeatureEdit::Flow { id, title: line, source: area },
        Editing::Step { diagram, step } => FeatureEdit::StepText { diagram, step, text: line },
    }
}

/// Whether the form is still about something on screen: a form on a row is dropped when its row
/// vanished from the pushed outline, a flow's or step's when its flow is gone; the brief, a new
/// task and a new flow stay whatever is pushed.
pub fn still_there(editing: Editing, view: &FeatureView) -> bool {
    // A task and a decision are numbered apart, so a row is the one the form is about only if it is of
    // the same kind (the page compared the numbers alone).
    let task = |id: i64| view.rows.iter().any(|row| row.at == WorkAt::Task { id });
    let decision = |id: i64| view.rows.iter().any(|row| row.at == WorkAt::Decision { id });
    let in_flows = |id: i64| view.flows.as_ref().is_some_and(|flows| flows.flows.iter().any(|flow| flow.id == id));
    match editing {
        Editing::Brief | Editing::AddTask { .. } | Editing::AddFlow => true,
        Editing::AddDecision { under: Some(under), .. } => decision(under),
        Editing::AddDecision { under: None, task: Some(id) } => task(id),
        Editing::AddDecision { under: None, task: None } => true,
        Editing::Flow { id } => in_flows(id),
        Editing::Step { diagram, .. } => in_flows(diagram),
        Editing::Task { id } | Editing::Criterion { id } => task(id),
        Editing::Record { id } | Editing::Answer { id } | Editing::Supersede { id } => decision(id),
    }
}

/// What a form's primary button says (BOARD-FORM-9).
pub fn save_label(editing: Editing) -> &'static str {
    match editing {
        Editing::Brief => "Save the brief",
        Editing::Task { .. } => "Save the task",
        Editing::Record { .. } => "Save the record",
        Editing::Answer { .. } => "Answer it",
        Editing::Supersede { .. } => "Supersede",
        Editing::Step { .. } => "Save the step",
        Editing::Flow { .. } => "Save the flow",
        Editing::Criterion { .. } | Editing::AddTask { .. } | Editing::AddDecision { .. } | Editing::AddFlow => "Add",
    }
}

#[cfg(test)]
mod tests;
