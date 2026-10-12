//! How one note reads as a feature.
//!
//! Starting a feature used to be a title typed into a line, then a task at
//! a time. What a person has at that moment is a paragraph in their head,
//! so the note is the unit: its first line is the title, the prose under it
//! the brief, a `-` line a task and an indented one a subtask, a `?` line a
//! decision still open with its options indented beneath, and a `!` line a
//! decision already made with the reason beneath.
//!
//! The grammar lives here because the client previews a note as the daemon
//! will store it, and two readings of one note would be a preview that lies.

use serde::{Deserialize, Serialize};

use crate::decisions::MAX_DECISION_BYTES;
use crate::features::{FeatureWrite, MAX_FEATURE_BODY_BYTES};
use crate::tasks::MAX_TASK_TITLE_BYTES;

/// The tasks and decisions one note may hold. A note longer than this is a
/// plan somebody should read before it becomes a board.
pub const MAX_CAPTURE_ROWS: usize = 200;

/// A note, read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub title: String,
    pub body: Option<String>,
    pub tasks: Vec<CapturedTask>,
    /// The decisions raised under no task.
    pub decisions: Vec<CapturedDecision>,
    /// One-based lines the grammar found no place for. Never sent: a note
    /// with any is not ready, and says which lines.
    #[serde(skip)]
    pub unread: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedTask {
    pub title: String,
    #[serde(default)]
    pub subtasks: Vec<CapturedTask>,
    /// The decisions raised under this task.
    #[serde(default)]
    pub decisions: Vec<CapturedDecision>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedDecision {
    pub chose: String,
    /// A `?` line: a question, with `options`. Otherwise a `!` line, made
    /// already, with `because`.
    pub open: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub because: Option<String>,
}

/// What a line of the note is, before it is given a place.
enum Mark {
    Task,
    Question,
    Made,
    Prose,
}

/// A row still able to take lines indented beneath it.
enum Holder {
    /// Indexes down through `tasks` and `subtasks`.
    Task(Vec<usize>),
    /// A decision, under the task at this path or under none.
    Decision(Option<Vec<usize>>),
}

fn marked(line: &str) -> (Mark, &str) {
    for (sign, mark) in [('-', Mark::Task), ('?', Mark::Question), ('!', Mark::Made)] {
        let Some(rest) = line.strip_prefix(sign) else {
            continue;
        };
        if rest.is_empty() || rest.starts_with(' ') {
            return (mark, rest.trim());
        }
    }
    (Mark::Prose, line)
}

/// A tab is as deep as the two spaces the editor's own indent writes.
fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum()
}

impl Capture {
    /// Reads a note as a whole feature. Total: every note reads as
    /// something, and what could not be placed is in `unread` rather than
    /// dropped.
    pub fn parse(note: &str) -> Capture {
        Capture::read(note, true)
    }

    /// Reads a note that adds to a feature already written: every line is a
    /// row, since the feature has its title and its brief.
    pub fn parse_rows(note: &str) -> Capture {
        Capture::read(note, false)
    }

    fn read(note: &str, titled: bool) -> Capture {
        let mut capture = Capture::default();
        let mut lines = note.lines().enumerate().peekable();
        if titled {
            for (_, line) in lines.by_ref() {
                if !line.trim().is_empty() {
                    capture.title = line.trim().trim_start_matches('#').trim().to_string();
                    break;
                }
            }
        }

        let mut body: Vec<&str> = Vec::new();
        let mut holders: Vec<(usize, Holder)> = Vec::new();
        // Prose is the brief only while a title has been read and no row
        // has started.
        let mut marks_seen = !titled;
        for (at, line) in lines {
            let text = line.trim();
            if text.is_empty() {
                // A blank line ends a paragraph of the brief and nothing
                // else: tasks may be spaced out without losing their place.
                if !marks_seen && !body.is_empty() {
                    body.push("");
                }
                continue;
            }
            let indent = indent_of(line);
            let (mark, text) = marked(text);
            if !marks_seen && matches!(mark, Mark::Prose) {
                body.push(line.trim());
                continue;
            }
            marks_seen = true;
            while holders.last().is_some_and(|(depth, _)| *depth >= indent) {
                holders.pop();
            }
            if text.is_empty() {
                continue;
            }
            let holder = holders.last().map(|(_, holder)| holder);
            match (mark, holder) {
                (Mark::Task | Mark::Prose, Some(Holder::Decision(path))) => {
                    let decision = capture.last_decision(path.clone());
                    if decision.open {
                        decision.options.push(text.to_string());
                    } else {
                        let because = decision.because.get_or_insert_with(String::new);
                        if !because.is_empty() {
                            because.push(' ');
                        }
                        because.push_str(text);
                    }
                }
                (Mark::Task, Some(Holder::Task(path))) => {
                    let siblings = &mut capture.task_at(path).subtasks;
                    siblings.push(CapturedTask::titled(text));
                    let mut path = path.clone();
                    path.push(siblings.len() - 1);
                    holders.push((indent, Holder::Task(path)));
                }
                (Mark::Task, None) => {
                    capture.tasks.push(CapturedTask::titled(text));
                    holders.push((indent, Holder::Task(vec![capture.tasks.len() - 1])));
                }
                (mark @ (Mark::Question | Mark::Made), None | Some(Holder::Task(_))) => {
                    let path = match holder {
                        Some(Holder::Task(path)) => Some(path.clone()),
                        _ => None,
                    };
                    let decision = CapturedDecision {
                        chose: text.to_string(),
                        open: matches!(mark, Mark::Question),
                        ..CapturedDecision::default()
                    };
                    match &path {
                        Some(path) => capture.task_at(path).decisions.push(decision),
                        None => capture.decisions.push(decision),
                    }
                    holders.push((indent, Holder::Decision(path)));
                }
                // A decision under a decision, or prose under a task: the
                // note means something by it, and this grammar cannot say
                // what.
                (Mark::Question | Mark::Made, Some(Holder::Decision(_)))
                | (Mark::Prose, None | Some(Holder::Task(_))) => capture.unread.push(at + 1),
            }
        }

        while body.last() == Some(&"") {
            body.pop();
        }
        capture.body = (!body.is_empty()).then(|| join_paragraphs(&body));
        capture
    }

    fn task_at(&mut self, path: &[usize]) -> &mut CapturedTask {
        let (first, rest) = path.split_first().expect("a task path names a task");
        rest.iter()
            .fold(&mut self.tasks[*first], |task, index| &mut task.subtasks[*index])
    }

    /// The decision most recently raised under the task at `path`, or
    /// under none.
    fn last_decision(&mut self, path: Option<Vec<usize>>) -> &mut CapturedDecision {
        let decisions = match path {
            Some(path) => &mut self.task_at(&path).decisions,
            None => &mut self.decisions,
        };
        decisions.last_mut().expect("a decision holder names a decision")
    }

    /// Every task in the note, at any depth.
    pub fn task_count(&self) -> usize {
        self.tasks.iter().map(CapturedTask::count).sum()
    }

    /// Every decision in the note, wherever it was raised, in the order a
    /// reader meets them.
    pub fn all_decisions(&self) -> Vec<&CapturedDecision> {
        let mut out = Vec::new();
        for task in &self.tasks {
            task.collect_decisions(&mut out);
        }
        out.extend(self.decisions.iter());
        out
    }

    /// Whether the note can be written down as it stands, and if not, why.
    ///
    /// `titled` is false for a note added to a feature that exists, where
    /// the first line may be anything.
    pub fn checked(self, titled: bool) -> Result<Capture, &'static str> {
        if !self.unread.is_empty() {
            return Err("some lines read as nothing: start a task with -, a question with ?");
        }
        if titled {
            FeatureWrite {
                title: self.title.clone(),
                body: self.body.clone(),
            }
            .checked()?;
        }
        if self.body.as_ref().is_some_and(|b| b.len() > MAX_FEATURE_BODY_BYTES) {
            return Err("a feature document is a brief, not a design document");
        }
        let decisions = self.all_decisions();
        if self.task_count() + decisions.len() > MAX_CAPTURE_ROWS {
            return Err("a note that long is a plan; write the feature, then add to it");
        }
        if !self.tasks.iter().all(CapturedTask::fits) {
            return Err("a task is a line, not a brief — that belongs in the feature document");
        }
        let fits = |d: &&CapturedDecision| {
            d.chose.len() <= MAX_DECISION_BYTES
                && d.because.as_ref().is_none_or(|b| b.len() <= MAX_DECISION_BYTES)
                && d.options.len() <= crate::decisions::MAX_DECISION_OPTIONS
                && d.options.iter().all(|o| o.len() <= MAX_DECISION_BYTES)
        };
        if !decisions.iter().all(fits) {
            return Err("a decision is a sentence or two per field, not a document");
        }
        Ok(self)
    }
}

impl CapturedTask {
    fn titled(title: &str) -> CapturedTask {
        CapturedTask {
            title: title.to_string(),
            ..CapturedTask::default()
        }
    }

    fn count(&self) -> usize {
        1 + self.subtasks.iter().map(CapturedTask::count).sum::<usize>()
    }

    fn fits(&self) -> bool {
        self.title.len() <= MAX_TASK_TITLE_BYTES && self.subtasks.iter().all(CapturedTask::fits)
    }

    fn collect_decisions<'a>(&'a self, out: &mut Vec<&'a CapturedDecision>) {
        out.extend(self.decisions.iter());
        for task in &self.subtasks {
            task.collect_decisions(out);
        }
    }
}

/// Wrapped lines of one paragraph become one line; a blank line stays the
/// break between two.
fn join_paragraphs(lines: &[&str]) -> String {
    lines
        .split(|line| line.is_empty())
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| paragraph.join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "The ratatui 0.30 bump and visual pass

Move the workspace to ratatui 0.30, then use the pass through
every widget to tidy the theme roles.

- Bump ratatui to 0.30
  - Fix what the new API breaks
  - Re-run the TestBackend render tests
- Visual pass over the theme.rs roles
  - Focus and selection cues
? Does the visual pass ship in the same release
  - Same release
  - Follow-up release
";

    #[test]
    fn a_note_reads_as_a_title_a_brief_tasks_and_an_open_decision() {
        let capture = Capture::parse(NOTE);

        assert_eq!(capture.title, "The ratatui 0.30 bump and visual pass");
        assert_eq!(
            capture.body.as_deref(),
            Some(
                "Move the workspace to ratatui 0.30, then use the pass through every widget to tidy the theme roles."
            )
        );
        assert_eq!(capture.task_count(), 5);
        assert_eq!(capture.tasks[0].subtasks[1].title, "Re-run the TestBackend render tests");
        assert_eq!(capture.tasks[1].subtasks[0].title, "Focus and selection cues");
        let decision = &capture.decisions[0];
        assert!(decision.open);
        assert_eq!(decision.options, ["Same release", "Follow-up release"]);
        assert!(capture.unread.is_empty());
        assert!(capture.checked(true).is_ok());
    }

    #[test]
    fn a_made_decision_takes_the_indented_line_as_its_reason() {
        let capture = Capture::parse("Title\n! One contract\n  a second would drift\n  in silence\n");

        let decision = &capture.decisions[0];
        assert!(!decision.open);
        assert_eq!(decision.because.as_deref(), Some("a second would drift in silence"));
    }

    #[test]
    fn a_decision_indented_under_a_task_is_raised_under_it() {
        let capture = Capture::parse("Title\n- Serve the brief\n  ? Which route\n    - pane API\n  - Wire the hook\n");

        let task = &capture.tasks[0];
        assert_eq!(task.decisions[0].chose, "Which route");
        assert_eq!(task.decisions[0].options, ["pane API"]);
        assert_eq!(task.subtasks[0].title, "Wire the hook");
        assert!(capture.decisions.is_empty());
        assert_eq!(capture.all_decisions().len(), 1);
    }

    #[test]
    fn a_line_the_grammar_cannot_place_is_named_rather_than_dropped() {
        let capture = Capture::parse("Title\n- A task\n  some prose under it\n");

        assert_eq!(capture.unread, [3]);
        assert!(capture.checked(true).is_err());
    }

    #[test]
    fn a_note_with_no_title_is_not_ready() {
        assert!(Capture::parse("\n\n").checked(true).is_err());
        assert!(Capture::parse_rows("- only a task\n").checked(false).is_ok());
    }

    #[test]
    fn a_note_added_to_a_feature_reads_its_first_line_as_a_row() {
        let capture = Capture::parse_rows("- one more task\nstray prose\n");

        assert_eq!(capture.tasks[0].title, "one more task");
        assert!(capture.title.is_empty() && capture.body.is_none());
        assert_eq!(capture.unread, [2]);
    }

    #[test]
    fn a_dash_inside_a_word_is_prose_not_a_task() {
        let capture = Capture::parse("Title\n-not a task\n");

        assert!(capture.tasks.is_empty());
        assert_eq!(capture.body.as_deref(), Some("-not a task"));
    }
}
