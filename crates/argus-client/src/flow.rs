//! The steps a sequence diagram's source says, in the order it says them.
//!
//! A diagram is stored as Mermaid and drawn by a renderer that returns a
//! picture. A list of who asks whom for what cannot be read back off a
//! picture, so it is read off the source: the same lines, as rows.

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
    for line in source.lines().map(str::trim) {
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
        }));
    }
    rows
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
    fn a_line_that_is_not_a_message_is_not_a_step() {
        assert!(rows("sequenceDiagram\n    participant a\n    Note over a: thinking\n").is_empty());
        assert_eq!(summary(&rows("a->>b: hi")), "1 step");
    }
}
