//! The steps a sequence diagram's source says, in the order it says them.
//!
//! A diagram is stored as Mermaid and drawn by a renderer that returns a
//! picture. A list of who asks whom for what cannot be read back off a
//! picture, so it is read off the source: the same lines, as rows.
//!
//! What binds a step is said in the source too, as a Mermaid comment on
//! its line or the line after it — `%% decision #12 task #31` — so it is
//! written with the flow, travels with it, and the renderer skips it.
//! Binding a step from a client is writing that comment into the source.

/// One line of a flow that a reader steps through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRow {
    Step(Step),
    /// Where the flow forks or repeats: `else`, `opt`, `loop`, and what it
    /// says about when.
    Branch { kind: String, when: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// One-based, in the order the source lists them.
    pub number: usize,
    pub from: String,
    /// `None` when a participant is talking to itself.
    pub to: Option<String>,
    /// A dashed arrow: an answer rather than a request.
    pub reply: bool,
    pub text: String,
    /// The decisions that bind it, by id.
    pub decisions: Vec<i64>,
    /// The tasks that carry it out, by id, beyond those its decisions name.
    pub tasks: Vec<i64>,
    /// Which line of the source says it, counted from zero.
    pub line: usize,
}

impl Step {
    /// Who it runs between, as the list says it.
    pub fn route(&self) -> String {
        match (&self.to, self.reply) {
            (None, _) => self.from.clone(),
            (Some(to), false) => format!("{} → {to}", self.from),
            (Some(to), true) => format!("{} ⇢ {to}", self.from),
        }
    }
}

/// Longest first, so `-->>` is not read as `->` with a stray dash.
const ARROWS: [&str; 8] = ["-->>", "--x", "--)", "-->", "->>", "-x", "-)", "->"];

/// The rows of a Mermaid sequence diagram. Lines it does not understand
/// are skipped: this is a reading of the source, and the renderer is what
/// says whether the source is valid.
pub fn rows(source: &str) -> Vec<FlowRow> {
    let mut rows = Vec::new();
    let mut number = 0;
    let mut under_step = false;
    for (at, line) in source.lines().map(str::trim).enumerate() {
        let (line, comment) = line.split_once("%%").unwrap_or((line, ""));
        let line = line.trim();
        let (decisions, tasks) = named_in(comment);
        if line.is_empty() {
            // A comment on a line of its own binds the step just above it,
            // and only while nothing has come between them.
            if let (true, Some(FlowRow::Step(step))) = (under_step, rows.last_mut()) {
                step.decisions.extend(decisions);
                step.tasks.extend(tasks);
            }
            continue;
        }
        under_step = false;
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match word {
            // The first arm of an `alt` is the flow as it usually goes; it
            // is the arms after it that fork.
            "else" | "opt" | "loop" | "and" | "break" | "critical" => {
                rows.push(FlowRow::Branch {
                    kind: word.to_string(),
                    when: rest.trim().to_string(),
                });
                continue;
            }
            _ => {}
        }
        let Some((arrow_at, arrow)) = ARROWS
            .iter()
            .filter_map(|arrow| line.find(arrow).map(|at| (at, *arrow)))
            .min_by_key(|(at, arrow)| (*at, std::cmp::Reverse(arrow.len())))
        else {
            continue;
        };
        let from = line[..arrow_at].trim();
        let (to, text) = line[arrow_at + arrow.len()..]
            .split_once(':')
            .unwrap_or((&line[arrow_at + arrow.len()..], ""));
        let to = to.trim().trim_start_matches(['+', '-']).trim();
        if from.is_empty() || to.is_empty() || from.contains(char::is_whitespace) {
            continue;
        }
        number += 1;
        rows.push(FlowRow::Step(Step {
            number,
            from: from.to_string(),
            to: (to != from).then(|| to.to_string()),
            reply: arrow.starts_with("--"),
            text: text.trim().to_string(),
            decisions,
            tasks,
            line: at,
        }));
        under_step = true;
    }
    rows
}

/// The source with step `number` bound to `decision`: a comment line
/// under the step's message, indented as it is. `None` when there is no
/// such step or it is already bound to that decision.
pub fn bind(source: &str, number: usize, decision: i64) -> Option<String> {
    let step = rows(source).into_iter().find_map(|row| match row {
        FlowRow::Step(step) if step.number == number => Some(step),
        _ => None,
    })?;
    if step.decisions.contains(&decision) {
        return None;
    }
    let mut lines: Vec<&str> = source.lines().collect();
    let said = lines[step.line];
    let indent = &said[..said.len() - said.trim_start().len()];
    let comment = format!("{indent}%% decision #{decision}");
    lines.insert(step.line + 1, &comment);
    Some(lines.join("\n"))
}

/// The decisions and tasks a comment names, as `decision #12 task #31
/// #32`. A comment that names neither is only a comment.
fn named_in(comment: &str) -> (Vec<i64>, Vec<i64>) {
    let mut decisions = Vec::new();
    let mut tasks = Vec::new();
    let mut naming_tasks = None;
    for word in comment.split(|c: char| c.is_whitespace() || c == ',') {
        match word {
            "decision" | "decisions" => naming_tasks = Some(false),
            "task" | "tasks" => naming_tasks = Some(true),
            _ => {
                let Some(id) = word.strip_prefix('#').and_then(|id| id.parse().ok()) else {
                    continue;
                };
                match naming_tasks {
                    Some(true) => tasks.push(id),
                    Some(false) => decisions.push(id),
                    None => {}
                }
            }
        }
    }
    (decisions, tasks)
}

/// One lifeline of a flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    /// What the steps call it.
    pub id: String,
    /// What it is drawn as: its alias when `participant x as y` gives one.
    pub label: String,
    /// Declared with `actor`: a person rather than a part of the system.
    pub actor: bool,
}

/// The lifelines a flow draws, left to right: those declared, in the order
/// the source declares them, then any a step names that was not, in the
/// order they first appear, as Mermaid places them.
pub fn participants(source: &str) -> Vec<Participant> {
    let mut all: Vec<Participant> = Vec::new();
    for line in source.lines().map(str::trim) {
        let line = line.split_once("%%").map_or(line, |(line, _)| line).trim();
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if word != "participant" && word != "actor" {
            continue;
        }
        let (id, label) = rest.split_once(" as ").unwrap_or((rest, rest));
        let (id, label) = (id.trim(), label.trim());
        if !id.is_empty() && !all.iter().any(|p| p.id == id) {
            all.push(Participant { id: id.to_string(), label: label.to_string(), actor: word == "actor" });
        }
    }
    for row in rows(source) {
        let FlowRow::Step(step) = row else { continue };
        for id in std::iter::once(&step.from).chain(step.to.as_ref()) {
            if !all.iter().any(|p| &p.id == id) {
                all.push(Participant { id: id.clone(), label: id.clone(), actor: false });
            }
        }
    }
    all
}

/// How many steps and branches a flow has, as its row in the list says.
pub fn summary(rows: &[FlowRow]) -> String {
    let steps = rows.iter().filter(|row| matches!(row, FlowRow::Step(_))).count();
    let branches = rows.len() - steps;
    let plural = |n: usize, noun: &str| match (n, noun) {
        (1, noun) => format!("1 {noun}"),
        (n, "branch") => format!("{n} branches"),
        (n, noun) => format!("{n} {noun}s"),
    };
    match branches {
        0 => plural(steps, "step"),
        _ => format!("{} · {}", plural(steps, "step"), plural(branches, "branch")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "sequenceDiagram
    participant agent
    participant hook as argus-hook
    agent->>hook: ask for the brief
    hook->>argusd: brief request
    argusd->>argusd: find the pane's feature
    alt the pane has a feature
        argusd-->>hook: the brief
        hook-->>agent: prints it
    else the pane has no feature
        argusd-->>hook: refused
    end";

    #[test]
    fn lifelines_are_those_declared_then_those_steps_name_in_the_order_they_come() {
        let lifelines: Vec<(String, String)> =
            participants(SOURCE).into_iter().map(|p| (p.id, p.label)).collect();
        assert_eq!(
            lifelines,
            [
                ("agent".to_string(), "agent".to_string()),
                ("hook".to_string(), "argus-hook".to_string()),
                ("argusd".to_string(), "argusd".to_string()),
            ]
        );
        let person = participants("sequenceDiagram
    actor you
    you->>argus: open");
        assert!(person[0].actor && !person[1].actor);
    }

    #[test]
    fn a_flow_reads_as_numbered_steps_with_its_branch_between_them() {
        let rows = rows(SOURCE);

        let said: Vec<String> = rows
            .iter()
            .map(|row| match row {
                FlowRow::Step(step) => format!("{} {} {}", step.number, step.route(), step.text),
                FlowRow::Branch { kind, when } => format!("{kind} {when}"),
            })
            .collect();
        assert_eq!(
            said,
            [
                "1 agent → hook ask for the brief",
                "2 hook → argusd brief request",
                "3 argusd find the pane's feature",
                "4 argusd ⇢ hook the brief",
                "5 hook ⇢ agent prints it",
                "else the pane has no feature",
                "6 argusd ⇢ hook refused",
            ]
        );
        assert_eq!(summary(&rows), "6 steps · 1 branch");
    }

    #[test]
    fn a_comment_binds_the_step_on_its_line_or_the_one_above() {
        let rows = rows(
            "sequenceDiagram
    a->>b: ask %% decision #12
    b-->>a: answer
    %% decisions #3, #4 task #31
    %% the renderer skips this, and so does the list
    alt it fails
    %% task #5
    b-->>a: refused
    else retried
    %% task #9
    a->>b: again",
        );

        let steps: Vec<(&str, &[i64], &[i64])> = rows
            .iter()
            .filter_map(|row| match row {
                FlowRow::Step(step) => Some((step.text.as_str(), &step.decisions[..], &step.tasks[..])),
                FlowRow::Branch { .. } => None,
            })
            .collect();
        assert_eq!(
            steps,
            [
                ("ask", &[12][..], &[][..]),
                ("answer", &[3, 4][..], &[31][..]),
                ("refused", &[][..], &[][..]),
                ("again", &[][..], &[][..]),
            ],
            "a comment under any other line binds nothing"
        );
    }

    #[test]
    fn binding_a_step_writes_a_comment_under_its_message_once() {
        let bound = bind(SOURCE, 2, 7).unwrap();

        assert!(bound.contains("    hook->>argusd: brief request\n    %% decision #7\n"), "{bound}");
        let FlowRow::Step(step) = &rows(&bound)[1] else { panic!("{bound}") };
        assert_eq!(step.decisions, [7]);
        assert_eq!(rows(&bound).len(), rows(SOURCE).len(), "the comment is not a row");
        assert_eq!(bind(&bound, 2, 7), None, "already bound");
        assert_eq!(bind(SOURCE, 9, 7), None, "no step 9");
    }

    #[test]
    fn a_line_that_is_not_a_message_is_not_a_step() {
        assert!(rows("sequenceDiagram\n    participant a\n    Note over a: thinking\n").is_empty());
        assert_eq!(summary(&rows("a->>b: hi")), "1 step");
    }
}
