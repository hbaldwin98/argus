//! What each key of an open feature does (BOARD-WORK-10 to -12, -24, BOARD-RECORD-5, -10, BOARD-FLOWS-9,
//! BOARD-SEQ-11, -19): the page's `workAction`, `recordAction`, `flowsAction`, `decisionAction` and
//! `diagramAction`, which read the feature and say what to do, as a value.
//!
//! The mapping is pure, so every key can be held in a test without a window; the view does what it
//! says. `None` is a key the lens has nothing to do with, which the layer passes on; `Act::Nothing` is
//! one it holds and does nothing with (a subtask asked of a cursor on nothing), as the page returns
//! true without acting.

use argus_desktop_core::connection::page::{Call, FeatureEdit, FeatureView, Lens};
use argus_desktop_core::keys::keymap::Action;

use crate::board::lenses::forms::Editing;

/// What a key comes to.
#[derive(Debug, Clone)]
pub enum Act {
    /// Held, and nothing done.
    Nothing,
    /// A call to the session: a cursor step, a lens, the link dialog.
    Call(Call),
    /// An edit to the open feature, sent as it stands.
    Edit(FeatureEdit),
    /// A form opened.
    Begin(Editing),
    /// A lens read; any form being written is put away first (BOARD-HEAD-5).
    Lens(Lens),
    /// A notice the window says itself, from what the snapshot shows.
    Say(String),
    /// The flow's diagram opened over the window.
    Expand,
    /// Zoom by this much (expanded).
    Zoom(f32),
    /// The expanded diagram at 100%.
    ZoomReset,
    /// The expanded diagram fitted to its canvas.
    Fit,
    /// The expanded diagram's steps shown or hidden.
    Panel,
    /// The bind menu for the lit step, opened by a key: under the lit row, or under the lit arrow when
    /// the diagram is expanded.
    Bind { expanded: bool },
    /// The lit step moved to where step `to` stands, and kept lit there (BOARD-SEQ-12).
    MoveStep { diagram: i64, step: usize, to: usize },
}

/// The one ticket's open, or the words that say there is none (BOARD-WORK-12).
fn open_ticket(view: &FeatureView) -> Act {
    match view.links.as_slice() {
        [] => Act::Say("No ticket is linked to this feature; T links one.".into()),
        [only] => Act::Call(Call::OpenUrl { url: only.url.clone() }),
        _ => Act::Call(Call::OpenLink { map: false }),
    }
}

/// The keys every lens has, and the expanded diagram too: the lenses, the brief, the agent's pane and
/// the tracker (BOARD-WORK-24).
fn shared(view: &FeatureView, action: Action) -> Option<Act> {
    Some(match action {
        Action::WorkLens => Act::Lens(Lens::Work),
        Action::RecordLens => Act::Lens(Lens::Record),
        Action::FlowsLens => Act::Lens(Lens::Flows),
        Action::Brief => Act::Begin(Editing::Brief),
        Action::GoToPane => view.agent.as_ref().map_or(Act::Nothing, |agent| Act::Call(Call::GoToPane { pane: agent.pane })),
        Action::LinkTicket => Act::Call(Call::OpenLink { map: false }),
        Action::TrackerMap => Act::Call(Call::OpenLink { map: true }),
        Action::Reconcile => Act::Call(Call::OpenReconcile),
        Action::OpenTicket => open_ticket(view),
        _ => return None,
    })
}

fn step(action: Action) -> Option<Act> {
    matches!(action, Action::Next | Action::Previous | Action::First | Action::Last).then(|| Act::Call(Call::FeatureAction { action }))
}

/// The keys a decision answers wherever it is read (BOARD-RECORD-10): `o` only for an open one, and
/// silently otherwise; `u` says a superseded one is so already.
fn decision(view: &FeatureView, action: Action) -> Option<Act> {
    let decision = view.decision.as_ref().or_else(|| view.record.as_ref().and_then(|record| record.decision.as_ref()))?;
    Some(match action {
        Action::Answer if decision.state == "open" => Act::Begin(Editing::Answer { id: decision.id }),
        Action::Answer => Act::Nothing,
        Action::Withdraw => Act::Edit(FeatureEdit::Withdraw { id: decision.id, withdrawn: !decision.withdrawn }),
        Action::Supersede if decision.superseded_by.is_some() => Act::Say(format!("{} is superseded already.", decision.label)),
        Action::Supersede => Act::Begin(Editing::Supersede { id: decision.id }),
        Action::CopyRecord => Act::Call(copy_record(decision)),
        _ => return None,
    })
}

/// A decision copied as ADR markdown, the clipboard written by the window and the notice said in the
/// words of the page (BOARD-RECORD-10).
pub fn copy_record(decision: &argus_desktop_core::connection::page::DecisionDetail) -> Call {
    Call::CopyNoting {
        text: decision.adr.clone(),
        said: format!("Copied {} as ADR markdown.", decision.label),
        refused: "The clipboard refused the record.".into(),
    }
}

/// What the cursor's row is edited as (BOARD-WORK-14): a task as its title and description, a
/// decision as its record, with nothing under it the brief.
pub fn edit_row(view: &FeatureView) -> Editing {
    match (&view.task, &view.decision) {
        (Some(task), _) => Editing::Task { id: task.id },
        (None, Some(decision)) => Editing::Record { id: decision.id },
        _ => Editing::Brief,
    }
}

/// A key of the Outline place, heard while the expanded diagram is `expanded` or not.
pub fn outline(view: &FeatureView, expanded: bool, action: Action) -> Option<Act> {
    if let Some(act) = shared(view, action) {
        return Some(act);
    }
    if expanded {
        return diagram(view, action);
    }
    match view.lens {
        Lens::Record => record(view, action),
        Lens::Flows => flows(view, action),
        Lens::Work => work(view, action),
    }
}

fn work(view: &FeatureView, action: Action) -> Option<Act> {
    if let Some(act) = step(action) {
        return Some(act);
    }
    match action {
        Action::Edit => Some(Act::Begin(edit_row(view))),
        Action::AddTask => Some(Act::Begin(Editing::AddTask { parent: None })),
        Action::Under => Some(match (&view.task, &view.decision) {
            (Some(task), _) => Act::Begin(Editing::AddTask { parent: Some(task.id) }),
            (None, Some(decision)) => Act::Begin(Editing::AddDecision { under: Some(decision.id), task: None }),
            _ => Act::Nothing,
        }),
        Action::Decide => Some(Act::Begin(Editing::AddDecision { under: None, task: view.task.as_ref().map(|task| task.id) })),
        _ => decision(view, action),
    }
}

fn record(view: &FeatureView, action: Action) -> Option<Act> {
    if let Some(act) = step(action) {
        return Some(act);
    }
    let on = view.record.as_ref().and_then(|record| record.decision.as_ref());
    match action {
        Action::Edit => Some(on.map_or(Act::Nothing, |decision| Act::Begin(Editing::Record { id: decision.id }))),
        Action::Decide => Some(Act::Begin(Editing::AddDecision { under: None, task: None })),
        Action::Under => Some(on.map_or(Act::Nothing, |decision| Act::Begin(Editing::AddDecision { under: Some(decision.id), task: None }))),
        _ => decision(view, action),
    }
}

fn flows(view: &FeatureView, action: Action) -> Option<Act> {
    match action {
        Action::AddTask => return Some(Act::Begin(Editing::AddFlow)),
        Action::NextFlow | Action::PreviousFlow => return Some(Act::Call(Call::FeatureAction { action })),
        _ => {}
    }
    if let Some(act) = step(action) {
        return Some(act);
    }
    let flow = view.flows.as_ref()?.flow.as_ref()?;
    match action {
        Action::Expand | Action::Choose => Some(Act::Expand),
        Action::Edit => Some(Act::Begin(Editing::Flow { id: flow.id })),
        Action::Decide => Some(Act::Bind { expanded: false }),
        _ => None,
    }
}

/// A key of the Diagram place, which hears its own before the Outline's (BOARD-SEQ-11). Every other
/// key of the Outline place is swallowed while it is open (BOARD-SEQ-19).
pub fn diagram(view: &FeatureView, action: Action) -> Option<Act> {
    let flow = view.flows.as_ref()?.flow.as_ref()?;
    let number = flow.step.as_ref().map_or(1, |step| step.number);
    let count = flow.rows.iter().filter(|row| row.number.is_some()).count();
    match action {
        Action::Next | Action::Previous | Action::First | Action::Last | Action::NextFlow | Action::PreviousFlow => Some(Act::Call(Call::FeatureAction { action })),
        Action::ZoomIn => Some(Act::Zoom(argus_desktop_core::board::lenses::picture::ZOOM_BY)),
        Action::ZoomOut => Some(Act::Zoom(-argus_desktop_core::board::lenses::picture::ZOOM_BY)),
        Action::ZoomReset => Some(Act::ZoomReset),
        Action::Fit => Some(Act::Fit),
        Action::StepsPanel => Some(Act::Panel),
        Action::Edit => Some(Act::Begin(Editing::Flow { id: flow.id })),
        Action::Decide => Some(Act::Bind { expanded: true }),
        Action::Earlier => Some(if number > 1 { Act::MoveStep { diagram: flow.id, step: number, to: number - 1 } } else { Act::Nothing }),
        Action::Later => Some(if number < count { Act::MoveStep { diagram: flow.id, step: number, to: number + 1 } } else { Act::Nothing }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
