//! A new feature being written as one note: what the note reads as, by
//! `argus_protocol::capture`'s grammar, and whether the board holds the
//! feature it was sent as yet.
//!
//! The reading is the daemon's own parser run here, never a second grammar
//! on the page, so what the preview shows is what gets stored. The note
//! stays open after Create until a pushed board holds the new feature: a
//! refused one is still there to fix.

use std::collections::HashSet;

use argus_protocol::{Capture, CapturedDecision, CapturedTask, ClientMsg, DecisionBoard};

use crate::feature_stage::Scope;
use crate::page::{CapturePage, NoteDecision, NoteRow};

pub struct CaptureNote {
    note: String,
    /// What Create sent: the title asked for and the slugs the board held
    /// then, so the feature it made is the one that was not there.
    sent: Option<(String, HashSet<String>)>,
}

impl CaptureNote {
    pub fn new() -> CaptureNote {
        CaptureNote { note: String::new(), sent: None }
    }

    pub fn write(&mut self, note: String) {
        self.note = note;
    }

    /// Sends the note as a feature on `scope`'s board, or says why it is
    /// not ready. Sent once: a second Create while the first is out would
    /// make the feature twice.
    pub fn create(&mut self, note: String, scope: &Scope, board: Option<&DecisionBoard>, out: &mut Vec<ClientMsg>) -> Result<(), String> {
        self.note = note;
        if self.sent.is_some() {
            return Ok(());
        }
        let action = Capture::write(&self.note, None).map_err(str::to_string)?;
        let before = board.map(|b| b.features.iter().map(|f| f.slug.clone()).collect()).unwrap_or_default();
        self.sent = Some((Capture::parse(&self.note).title, before));
        out.push(ClientMsg::Feature { project: scope.project, checkout: scope.checkout, action });
        Ok(())
    }

    /// The daemon refused what was sent: the note can be fixed and sent again.
    pub fn refused(&mut self) -> bool {
        self.sent.take().is_some()
    }

    /// The slug of the feature the note became, once `board` holds it.
    pub fn landed(&self, board: &DecisionBoard) -> Option<String> {
        let (title, before) = self.sent.as_ref()?;
        board
            .features
            .iter()
            .find(|f| !before.contains(&f.slug) && f.title == *title)
            .map(|f| f.slug.clone())
    }

    pub fn page(&self, repository: &str) -> CapturePage {
        let capture = Capture::parse(&self.note);
        let mut tasks = Vec::new();
        for (at, task) in capture.tasks.iter().enumerate() {
            task_rows(task, &[at + 1], &mut tasks);
        }
        let decisions = capture
            .all_decisions()
            .into_iter()
            .enumerate()
            .map(|(at, decision)| note_decision(decision, at + 1))
            .collect();
        CapturePage {
            open: true,
            repository: repository.to_string(),
            note: self.note.clone(),
            title: capture.title.clone(),
            brief: capture.body.clone(),
            tasks,
            decisions,
            unread: capture.unread.clone(),
            blocked: capture.checked(true).err().map(str::to_string),
            creating: self.sent.is_some(),
        }
    }
}

fn task_rows(task: &CapturedTask, number: &[usize], out: &mut Vec<NoteRow>) {
    out.push(NoteRow {
        label: argus_protocol::outline_number(number),
        depth: number.len() - 1,
        text: task.title.clone(),
    });
    for (at, subtask) in task.subtasks.iter().enumerate() {
        let mut number = number.to_vec();
        number.push(at + 1);
        task_rows(subtask, &number, out);
    }
}

fn note_decision(decision: &CapturedDecision, number: usize) -> NoteDecision {
    NoteDecision {
        label: format!("D{number}"),
        text: decision.chose.clone(),
        open: decision.open,
        options: decision.options.clone(),
        because: decision.because.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use argus_protocol::{CheckoutId, Feature, FeatureAction, ProjectId, RepositoryId};

    const NOTE: &str = "Bring the transcript readers back

The recent-sessions dialog previews what was said.

- Restore the readers
  - Keep only reading
- List a repository's sessions
! The readers live in argusd
  it already knows each pane's harness
? Where does the generic harness say its transcript is
  - A [[harness]] key
  - A neutral JSONL
";

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

    #[test]
    fn the_preview_reads_the_note_as_the_daemon_will_store_it() {
        let mut note = CaptureNote::new();
        note.write(NOTE.into());
        let page = note.page("argus");
        assert_eq!(page.title, "Bring the transcript readers back");
        assert_eq!(page.brief.as_deref(), Some("The recent-sessions dialog previews what was said."));
        let tasks: Vec<_> = page.tasks.iter().map(|t| (t.label.as_str(), t.depth)).collect();
        assert_eq!(tasks, [("1", 0), ("1.1", 1), ("2", 0)]);
        assert_eq!(page.decisions[0].because.as_deref(), Some("it already knows each pane's harness"));
        assert!(page.decisions[1].open);
        assert_eq!(page.decisions[1].options, ["A [[harness]] key", "A neutral JSONL"]);
        assert_eq!(page.blocked, None, "ready to create");
    }

    #[test]
    fn a_line_the_grammar_cannot_place_is_named_and_blocks_create() {
        let mut note = CaptureNote::new();
        note.write("Title\n- A task\n  prose under it\n".into());
        let page = note.page("argus");
        assert_eq!(page.unread, [3]);
        assert!(page.blocked.is_some());

        let mut out = Vec::new();
        let refused = note.create("Title\n- A task\n  prose under it\n".into(), &scope(), None, &mut out);
        assert!(refused.is_err() && out.is_empty(), "nothing is sent");
    }

    #[test]
    fn create_sends_the_note_once_and_it_lands_when_the_board_holds_it() {
        let mut note = CaptureNote::new();
        let board = fixtures::board();
        let mut out = Vec::new();
        note.create(NOTE.into(), &scope(), Some(&board), &mut out).unwrap();
        note.create(NOTE.into(), &scope(), Some(&board), &mut out).unwrap();
        assert_eq!(out.len(), 1, "a second Create while the first is out sends nothing");
        let ClientMsg::Feature { action: FeatureAction::Capture { into: None, capture }, .. } = &out[0] else {
            panic!("{:?}", out[0]);
        };
        assert_eq!(capture.task_count(), 3);
        assert!(note.page("argus").creating);

        assert_eq!(note.landed(&board), None, "not on the board yet");
        let mut pushed = fixtures::board();
        pushed.features.push(Feature {
            number: 11,
            slug: "transcripts".into(),
            title: "Bring the transcript readers back".into(),
            ..Feature::default()
        });
        assert_eq!(note.landed(&pushed).as_deref(), Some("transcripts"));
    }

    #[test]
    fn a_refused_note_can_be_sent_again() {
        let mut note = CaptureNote::new();
        let mut out = Vec::new();
        note.create(NOTE.into(), &scope(), None, &mut out).unwrap();
        assert!(note.refused());
        assert!(!note.page("argus").creating);
        note.create(NOTE.into(), &scope(), None, &mut out).unwrap();
        assert_eq!(out.len(), 2);
    }
}
