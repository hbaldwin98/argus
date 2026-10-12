//! What the Feature stage asks the daemon to change: the line or note being
//! typed, and the message each key sends.
//!
//! Nothing here changes what the stage draws. A write goes out and the
//! pushed board is what makes it true, so a refused one leaves the stage
//! showing what is.

use argus_protocol::{
    Capture, DecisionChange, DecisionState, DecisionText, DecisionWrite, FeatureState, TaskAction,
    TaskPlace, TaskState, TaskWrite,
};

use super::*;
use crate::outline::WorkRef;

/// A line being typed at the foot of the stage.
///
/// One line and no cursor movement beyond the end: a title, a criterion and
/// a note are sentences somebody dictates. Anything that wants real editing
/// opens in the editor instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineInput {
    pub text: String,
    pub what: LineEdit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEdit {
    NewTask,
    NewSubtask(i64),
    Task(i64),
    Feature(String),
    /// Why the feature with this slug is held.
    Hold(String),
    Criterion(i64),
    Note(i64),
    /// A decision: under another, raised under a task, or replacing one.
    Decision {
        under: Option<i64>,
        task: Option<i64>,
        supersedes: Option<i64>,
    },
    /// The answer to the open decision with this id.
    Resolve(i64),
}

impl LineInput {
    /// What the line calls itself, which is the whole of the affordance:
    /// there is no other cue that the keys have changed meaning.
    pub fn label(&self) -> &'static str {
        match self.what {
            LineEdit::NewTask => "new task",
            LineEdit::NewSubtask(_) => "new subtask",
            LineEdit::Task(_) => "rewrite",
            LineEdit::Feature(_) => "rename",
            LineEdit::Hold(_) => "hold because",
            LineEdit::Criterion(_) => "done when",
            LineEdit::Note(_) => "note",
            LineEdit::Decision {
                supersedes: Some(_), ..
            } => "superseded by",
            LineEdit::Decision { under: Some(_), .. } => "sub-decision (? open, ~ rejected)",
            LineEdit::Decision { .. } => "decision (? if it is still open)",
            LineEdit::Resolve(_) => "decided",
        }
    }
}

/// How a typed decision line stands: the note grammar's `?` asks a question
/// and `!` records one made, and `~` is an answer that was turned down.
fn decision_line(line: &str) -> (DecisionState, &str) {
    let (state, rest) = match line.chars().next() {
        Some('?') => (DecisionState::Open, &line[1..]),
        Some('~') => (DecisionState::Rejected, &line[1..]),
        Some('!') => (DecisionState::Accepted, &line[1..]),
        _ => (DecisionState::Accepted, line),
    };
    (state, rest.trim())
}

impl App {
    /// The project and checkout every board write is addressed to, and the
    /// feature it is about.
    fn board_target(&self) -> Option<(ProjectId, CheckoutId, String)> {
        let project = self.board.as_ref()?.project?;
        let checkout = self.current_checkout()?.id;
        Some((project, checkout, self.feature_slug()?))
    }

    fn send_task(&mut self, action: TaskAction) {
        let Some((project, checkout, feature)) = self.board_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::Task {
            project,
            checkout,
            feature,
            action,
        });
    }

    fn send_decision_change(&mut self, change: DecisionChange) {
        let Some((project, checkout, feature)) = self.board_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::ChangeDecision {
            project,
            checkout,
            feature,
            change,
        });
    }

    // ---- the typed line -------------------------------------------------

    fn begin_line(&mut self, what: LineEdit, text: String) {
        self.line = Some(LineInput { text, what });
    }

    pub(super) fn type_into_line(&mut self, c: char) {
        if let Some(input) = self.line.as_mut() {
            input.text.push(c);
        }
    }

    pub(super) fn backspace_line(&mut self) {
        if let Some(input) = self.line.as_mut() {
            input.text.pop();
        }
    }

    /// Sends what was typed. An empty line cancels rather than writing a
    /// row with no text, which the daemon would refuse anyway.
    pub(super) fn commit_line(&mut self) {
        let Some(input) = self.line.take() else {
            return;
        };
        let text = input.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(project) = self.board.as_ref().and_then(|b| b.project) else {
            return;
        };
        let Some(checkout) = self.current_checkout().map(|checkout| checkout.id) else {
            return;
        };
        let task = |title: String, parent| {
            TaskAction::Add(TaskWrite {
                title,
                external: None,
                parent,
            })
        };
        match input.what {
            LineEdit::Feature(slug) => {
                let _ = self.out.send(ClientMsg::RenameFeature {
                    project,
                    checkout,
                    slug,
                    title: text,
                });
            }
            LineEdit::Hold(slug) => {
                let _ = self.out.send(ClientMsg::HoldFeature {
                    project,
                    checkout,
                    slug,
                    reason: Some(text),
                });
            }
            LineEdit::NewTask => self.send_task(task(text, None)),
            LineEdit::NewSubtask(parent) => self.send_task(task(text, Some(parent))),
            LineEdit::Task(id) => self.send_task(TaskAction::Retitle { id, title: text }),
            LineEdit::Criterion(id) => self.send_task(TaskAction::AddCriterion { id, text }),
            LineEdit::Note(id) => self.send_task(TaskAction::Note { id, text }),
            LineEdit::Resolve(id) => self.send_decision_change(DecisionChange::Resolve {
                id,
                answer: text,
                because: None,
            }),
            LineEdit::Decision {
                under,
                task,
                supersedes,
            } => {
                let (state, chose) = decision_line(&text);
                let Some(feature) = self.feature_slug() else {
                    return;
                };
                let _ = self.out.send(ClientMsg::Decide {
                    project,
                    checkout,
                    feature,
                    write: DecisionWrite {
                        chose: chose.to_string(),
                        under,
                        supersedes,
                        state,
                        task,
                        ..DecisionWrite::default()
                    },
                });
            }
        }
    }

    // ---- features -------------------------------------------------------

    pub(super) fn begin_feature_rename(&mut self) {
        if let Some(feature) = self.selected_feature() {
            let (slug, title) = (feature.slug.clone(), feature.title.clone());
            self.begin_line(LineEdit::Feature(slug), title);
        }
    }

    /// Starts a feature as one note. It needs no checkout and no agent:
    /// this is a person writing down work that has not begun.
    pub(super) fn begin_capture(&mut self) {
        let Some(project) = self.board.as_ref().and_then(|b| b.project) else {
            return;
        };
        self.open_capture(project, None);
    }

    /// Writes more of the open feature as one note: tasks and decisions
    /// from one buffer.
    pub(super) fn begin_capture_into(&mut self) {
        let Some((project, _, slug)) = self.board_target() else {
            return;
        };
        self.open_capture(project, Some(slug));
    }

    fn open_capture(&mut self, project: ProjectId, into: Option<String>) {
        let mut note = BriefView::new(BriefTarget::Capture { project, into }, String::new(), "");
        note.insert_mode();
        self.capture = Some(note);
    }

    /// What the note being written reads as, right now.
    pub fn captured(&self) -> Option<Capture> {
        let note = self.capture.as_ref()?;
        Some(match self.capture_is_new() {
            true => Capture::parse(&note.body()),
            false => Capture::parse_rows(&note.body()),
        })
    }

    /// Whether the note is for a new feature rather than one that exists.
    pub fn capture_is_new(&self) -> bool {
        matches!(
            self.capture.as_ref().map(|note| &note.target),
            Some(BriefTarget::Capture { into: None, .. })
        )
    }

    /// Sends the note, or says why it is not ready and keeps it open.
    pub(super) fn commit_capture(&mut self) {
        let (Some(note), Some(read), Some(checkout)) = (
            self.capture.as_ref(),
            self.captured(),
            self.current_checkout().map(|checkout| checkout.id),
        ) else {
            return;
        };
        let BriefTarget::Capture { project, into } = note.target.clone() else {
            return;
        };
        match read.checked(into.is_none()) {
            Ok(capture) => {
                let _ = self.out.send(ClientMsg::CaptureFeature {
                    project,
                    checkout,
                    into,
                    capture: Box::new(capture),
                });
                self.capture = None;
            }
            Err(why) => self.report(why),
        }
    }

    /// Removes the selected feature. Its decisions survive as unfiled; the
    /// daemon is where that rule lives, and the push is what redraws it.
    pub(super) fn drop_selected_feature(&mut self) {
        let Some((project, checkout, slug)) = self.board_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::RemoveFeature {
            project,
            checkout,
            slug,
        });
    }

    /// Archives the selected feature, or brings an archived one back.
    ///
    /// Acceptance, and the human's alone: an agent cannot accept its own
    /// work. Everything else the index says about a feature is read off
    /// the panes running on it and the state of its tasks.
    pub(super) fn toggle_selected_feature_archived(&mut self) {
        let (Some((project, checkout, slug)), Some(feature)) =
            (self.board_target(), self.selected_feature())
        else {
            return;
        };
        let state = match feature.state {
            FeatureState::Done => FeatureState::Open,
            FeatureState::Open => FeatureState::Done,
        };
        let _ = self.out.send(ClientMsg::MoveFeature {
            project,
            checkout,
            slug: slug.clone(),
            state,
            detail: None,
        });
        self.report(match state {
            FeatureState::Done => format!("{slug} archived"),
            FeatureState::Open => format!("{slug} reopened"),
        });
    }

    /// Lifts the selected feature's hold, or asks why it should be held.
    pub(super) fn toggle_selected_feature_hold(&mut self) {
        let (Some((project, checkout, slug)), Some(feature)) =
            (self.board_target(), self.selected_feature())
        else {
            return;
        };
        if feature.held.is_none() {
            return self.begin_line(LineEdit::Hold(slug), String::new());
        }
        let _ = self.out.send(ClientMsg::HoldFeature {
            project,
            checkout,
            slug: slug.clone(),
            reason: None,
        });
        self.report(format!("{slug} is no longer held"));
    }

    /// Opens the selected feature's brief in the editor, the same one a
    /// task's brief gets: both are prose a human reads and corrects.
    pub(super) fn open_feature_brief(&mut self) {
        let (Some(project), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.selected_feature(),
        ) else {
            return self.report("no feature selected");
        };
        let view = BriefView::new(
            BriefTarget::Feature {
                project,
                slug: feature.slug.clone(),
            },
            feature.title.clone(),
            &feature.body,
        );
        self.open_brief(view);
    }

    fn open_brief(&mut self, view: BriefView) {
        self.brief = Some(view);
        self.overlay = Some(Overlay::Brief);
        self.focus = Focus::Overlay;
    }

    // ---- tasks ----------------------------------------------------------

    /// Starts a new root task, typed into the line at the foot of the stage.
    pub(super) fn begin_task(&mut self) {
        if self.feature_slug().is_some() {
            self.begin_line(LineEdit::NewTask, String::new());
        }
    }

    /// Starts work under the selected task. A separate binding keeps `a`
    /// useful for a new root task even when the cursor is on a deep row.
    pub(super) fn begin_subtask(&mut self) {
        if let Some(parent) = self.selected_task().map(|task| task.id) {
            self.begin_line(LineEdit::NewSubtask(parent), String::new());
        }
    }

    /// `s`: something under what the cursor is on — a subtask under a
    /// task, a sub-decision under a decision.
    pub(super) fn begin_under(&mut self) {
        match self.selected_work() {
            Some(WorkRef::Task(_)) => self.begin_subtask(),
            Some(WorkRef::Decision(_)) => self.begin_sub_decision(),
            None => self.report("nothing selected to add under"),
        }
    }

    pub(super) fn begin_criterion(&mut self) {
        match self.selected_task().map(|task| task.id) {
            Some(id) => self.begin_line(LineEdit::Criterion(id), String::new()),
            None => self.report("a criterion belongs to a task"),
        }
    }

    pub(super) fn begin_note(&mut self) {
        match self.selected_task().map(|task| task.id) {
            Some(id) => self.begin_line(LineEdit::Note(id), String::new()),
            None => self.report("a note belongs to a task"),
        }
    }

    /// `e`: rewrites what the cursor is on. A task's title is a line; a
    /// decision is a record, and opens whole.
    pub(super) fn edit_in_feature(&mut self) {
        if !self.feature_open {
            return self.open_feature_brief();
        }
        match self.selected_work() {
            Some(WorkRef::Task(id)) => {
                let title = self.selected_task().map(|task| task.title.clone());
                self.begin_line(LineEdit::Task(id), title.unwrap_or_default());
            }
            Some(WorkRef::Decision(_)) => self.open_decision_record(),
            None => self.open_feature_brief(),
        }
    }

    pub(super) fn open_task_brief(&mut self) {
        let (Some((project, _, feature)), Some(task)) = (self.board_target(), self.selected_task())
        else {
            return self.report("no task selected");
        };
        let view = BriefView::new(
            BriefTarget::Task {
                project,
                feature,
                id: task.id,
            },
            task.title.clone(),
            task.body.as_deref().unwrap_or_default(),
        );
        self.open_brief(view);
    }

    /// `space`: checks off the criterion under the detail cursor, or marks
    /// the task under the outline's done and back.
    pub(super) fn toggle_done(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let id = task.id;
        if self.in_detail {
            let Some(criterion) = task.criteria.get(self.detail_sel) else {
                return;
            };
            let done = !criterion.done;
            return self.send_task(TaskAction::SetCriterion {
                id,
                index: self.detail_sel,
                done,
            });
        }
        let state = match task.state {
            TaskState::Done => TaskState::Todo,
            TaskState::Todo | TaskState::Doing => TaskState::Done,
        };
        self.send_task(TaskAction::Move { id, state });
    }

    /// Moves the selected task along todo → doing → done, or back.
    pub(super) fn move_selected_task(&mut self, delta: i32) {
        let Some((id, current)) = self.selected_task().map(|t| (t.id, t.state)) else {
            return;
        };
        let at = TaskState::ALL
            .iter()
            .position(|s| *s == current)
            .unwrap_or(0) as i32;
        let next = at.saturating_add(delta);
        if next < 0 || next as usize >= TaskState::ALL.len() {
            return;
        }
        self.send_task(TaskAction::Move {
            id,
            state: TaskState::ALL[next as usize],
        });
    }

    /// Moves the selected task up or down among its siblings, which is
    /// what says to do it first.
    pub(super) fn reorder_selected_task(&mut self, delta: i64) {
        let Some(selected) = self.selected_task() else {
            return;
        };
        let (id, parent) = (selected.id, selected.parent);
        let siblings: Vec<i64> = self
            .feature_tasks()
            .iter()
            .filter(|task| task.parent == parent)
            .map(|task| task.id)
            .collect();
        let Some(position) = siblings.iter().position(|sibling| *sibling == id) else {
            return;
        };
        let to = (position as i64 + delta).clamp(0, siblings.len() as i64 - 1);
        if to != position as i64 {
            self.send_task(TaskAction::Reorder { id, to });
        }
    }

    /// Makes the selected task a subtask of the one above it among its
    /// siblings (`deeper`), or lifts it out to sit after its parent.
    pub(super) fn indent_selected_task(&mut self, deeper: bool) {
        let Some(selected) = self.selected_task() else {
            return;
        };
        let (id, parent) = (selected.id, selected.parent);
        let place = if deeper {
            let siblings: Vec<i64> = self
                .feature_tasks()
                .iter()
                .filter(|task| task.parent == parent)
                .map(|task| task.id)
                .collect();
            let above = siblings
                .iter()
                .position(|sibling| *sibling == id)
                .and_then(|at| at.checked_sub(1))
                .map(|at| siblings[at]);
            match above {
                Some(above) => TaskPlace::Under(above),
                None => return self.report("no task above it to go under"),
            }
        } else {
            match parent {
                Some(parent) => TaskPlace::After(parent),
                None => return self.report("already a top-level task"),
            }
        };
        self.place_selected_task(place);
    }

    /// Sends the selected task, with its subtasks, to `place`.
    pub(super) fn place_selected_task(&mut self, place: TaskPlace) {
        if let Some(id) = self.selected_task().map(|task| task.id) {
            self.send_task(TaskAction::Place { id, place });
        }
    }

    // ---- decisions ------------------------------------------------------

    /// `d`: a decision. In the outline it is raised under the task the
    /// cursor is on; in the record it is one more at the root.
    pub(super) fn begin_decision(&mut self) {
        if self.feature_slug().is_none() {
            return;
        }
        let task = match self.selected_work() {
            Some(WorkRef::Task(id)) => Some(id),
            Some(WorkRef::Decision(id)) if self.lens == Lens::Work => {
                self.outline().raised_under(id).map(|row| row.task.id)
            }
            _ => None,
        };
        self.begin_line(
            LineEdit::Decision {
                under: None,
                task,
                supersedes: None,
            },
            String::new(),
        );
    }

    pub(super) fn begin_sub_decision(&mut self) {
        match self.selected_decision().map(|decision| decision.id) {
            Some(id) => self.begin_line(
                LineEdit::Decision {
                    under: Some(id),
                    task: None,
                    supersedes: None,
                },
                String::new(),
            ),
            None => self.report("a sub-decision hangs under a decision"),
        }
    }

    pub(super) fn begin_supersede(&mut self) {
        match self.selected_decision().map(|decision| decision.id) {
            Some(id) => self.begin_line(
                LineEdit::Decision {
                    under: None,
                    task: None,
                    supersedes: Some(id),
                },
                String::new(),
            ),
            None => self.report("no decision selected"),
        }
    }

    /// Answers an open decision in words, for one with no options listed
    /// or none that fits.
    pub(super) fn begin_resolution(&mut self) {
        match self.selected_decision().filter(|d| d.open()).map(|d| d.id) {
            Some(id) => self.begin_line(LineEdit::Resolve(id), String::new()),
            None => self.report("only an open decision is waiting on an answer"),
        }
    }

    /// `Enter` or `space` in the detail column: takes the option under the
    /// cursor as the answer, or checks the criterion off.
    pub(super) fn act_in_detail(&mut self) {
        let Some(decision) = self.selected_decision() else {
            return self.toggle_done();
        };
        let Some(answer) = decision.options.get(self.detail_sel).cloned() else {
            return;
        };
        let id = decision.id;
        self.in_detail = false;
        self.send_decision_change(DecisionChange::Resolve {
            id,
            answer,
            because: None,
        });
    }

    /// Opens the selected decision as the record it is, to be corrected.
    pub(super) fn open_decision_record(&mut self) {
        let (Some((project, _, feature)), Some(decision)) =
            (self.board_target(), self.selected_decision())
        else {
            return self.report("no decision selected");
        };
        let document = DecisionText::from(decision).markdown(decision.open(), true);
        let view = BriefView::new(
            BriefTarget::Decision {
                project,
                feature,
                id: decision.id,
            },
            decision.chose.clone(),
            document.trim_end(),
        );
        self.open_brief(view);
    }

    /// Copies the selected decision out as a record somebody can paste.
    pub(super) fn copy_decision_record(&mut self) {
        let Some(id) = self.selected_decision().map(|decision| decision.id) else {
            return self.report("no decision selected");
        };
        let record = self
            .board_rows()
            .iter()
            .find(|row| row.decision.id == id)
            .map(|row| row.decision.adr(&row.label()));
        let Some(record) = record else { return };
        let copied = (self.clipboard_write)(&record);
        self.report(if copied {
            "copied as ADR markdown"
        } else {
            "no clipboard to copy to"
        });
    }

    // ---- flows ----------------------------------------------------------

    const DEFAULT_DIAGRAM_SOURCE: &'static str = "sequenceDiagram\n    participant Client\n    participant Daemon\n    Client->>Daemon: request\n    Daemon-->>Client: response";

    pub(super) fn add_diagram(&mut self) {
        let Some((project, checkout, feature)) = self.board_target() else {
            return;
        };
        let n = self.feature_diagrams().len() + 1;
        let _ = self.out.send(ClientMsg::SequenceDiagram {
            project,
            checkout,
            feature,
            action: argus_protocol::DiagramAction::Add(argus_protocol::DiagramWrite {
                title: format!("flow {n}"),
                body: Self::DEFAULT_DIAGRAM_SOURCE.to_string(),
            }),
        });
    }

    // ---- the keys that depend on where the cursor is ---------------------

    /// `a`: a new feature on the index, a new flow on the flows lens, and a
    /// new root task anywhere else in a feature.
    pub(super) fn add_in_feature(&mut self) {
        if !self.feature_open {
            return self.begin_capture();
        }
        match self.lens {
            Lens::Flows => self.add_diagram(),
            Lens::Work | Lens::Record => self.begin_task(),
        }
    }

    /// `x`: removes what the cursor is on. A decision is never removed — it
    /// is withdrawn, which keeps it on the record, and `x` again restores
    /// it.
    pub(super) fn drop_in_feature(&mut self) {
        if !self.feature_open {
            return self.drop_selected_feature();
        }
        if self.lens == Lens::Flows {
            let Some(id) = self.selected_diagram().map(|diagram| diagram.id) else {
                return;
            };
            let Some((project, checkout, feature)) = self.board_target() else {
                return;
            };
            let _ = self.out.send(ClientMsg::SequenceDiagram {
                project,
                checkout,
                feature,
                action: argus_protocol::DiagramAction::Remove { id },
            });
            return;
        }
        match self.selected_work() {
            Some(WorkRef::Task(id)) if self.in_detail => {
                self.send_task(TaskAction::RemoveCriterion {
                    id,
                    index: self.detail_sel,
                })
            }
            Some(WorkRef::Task(id)) => self.send_task(TaskAction::Remove { id }),
            Some(WorkRef::Decision(id)) => {
                let withdrawn = self.selected_decision().is_some_and(|d| d.withdrawn());
                self.send_decision_change(match withdrawn {
                    true => DecisionChange::Restore { id },
                    false => DecisionChange::Withdraw { id },
                });
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_decision_says_how_it_stands_with_its_first_character() {
        assert_eq!(decision_line("? which route"), (DecisionState::Open, "which route"));
        assert_eq!(decision_line("~ read the file"), (DecisionState::Rejected, "read the file"));
        assert_eq!(decision_line("! one contract"), (DecisionState::Accepted, "one contract"));
        assert_eq!(decision_line("one contract"), (DecisionState::Accepted, "one contract"));
    }
}
