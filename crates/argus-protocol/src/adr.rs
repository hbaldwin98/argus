//! How a decision reads as a record somebody can paste or correct.
//!
//! One document does both jobs: what `y` copies out of the view is what `e`
//! opens to be edited, so the sections a reader sees are the ones a writer
//! fills in. Written here so an agent's helper can print and read the same
//! document a person does.

use crate::decisions::{Consequence, Decision, DecisionState, DecisionText};

const CONTEXT: &str = "Context";
const DECISION: &str = "Decision";
const WHY: &str = "Why";
const CONSEQUENCES: &str = "Consequences";
const CONSIDERED: &str = "Considered";
const OPTIONS: &str = "Options";

impl DecisionText {
    /// The decision as a document: its name on the first line, then each
    /// section that has something in it. `skeleton` keeps the empty ones
    /// too, which is what somebody about to fill them in wants to see.
    pub fn markdown(&self, open: bool, skeleton: bool) -> String {
        let consequences = self
            .consequences
            .iter()
            .map(|c| format!("{} {}", c.sign(), c.text))
            .collect::<Vec<_>>()
            .join("\n");
        let options = self
            .options
            .iter()
            .map(|option| format!("- {option}"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = |field: &Option<String>| field.clone().unwrap_or_default();
        let mut sections = vec![(CONTEXT, text(&self.context))];
        if open {
            sections.push((OPTIONS, options));
        } else {
            sections.extend([
                (DECISION, text(&self.detail)),
                (WHY, text(&self.because)),
                (CONSEQUENCES, consequences),
                (CONSIDERED, text(&self.over)),
            ]);
        }

        let mut out = self.chose.clone();
        for (name, body) in sections {
            if body.is_empty() && !skeleton {
                continue;
            }
            out.push_str(&format!("\n\n## {name}\n{body}"));
        }
        out.push('\n');
        out
    }

    /// Reads a document written by [`DecisionText::markdown`], or by a
    /// person who followed its shape loosely: the first line is the name,
    /// prose before any heading is the decision itself, and a heading this
    /// does not know keeps its text in the decision rather than losing it.
    pub fn from_markdown(document: &str) -> DecisionText {
        let mut text = DecisionText::default();
        let mut lines = document.lines();
        for line in lines.by_ref() {
            if !line.trim().is_empty() {
                text.chose = line.trim().trim_start_matches('#').trim().to_string();
                break;
            }
        }

        let mut section = DECISION.to_string();
        let mut bodies: Vec<(String, Vec<&str>)> = Vec::new();
        for line in lines {
            if let Some(heading) = line.trim().strip_prefix("## ") {
                section = heading.trim().to_string();
                continue;
            }
            match bodies.iter_mut().find(|(name, _)| *name == section) {
                Some((_, body)) => body.push(line),
                None => bodies.push((section.clone(), vec![line])),
            }
        }

        let mut detail = Vec::new();
        for (name, body) in bodies {
            let prose = body.join("\n").trim().to_string();
            if prose.is_empty() {
                continue;
            }
            match name.to_ascii_lowercase().as_str() {
                "context" => text.context = Some(prose),
                "why" => text.because = Some(prose),
                "considered" => text.over = Some(prose),
                "consequences" => {
                    text.consequences = body.iter().filter_map(|l| Consequence::parse(l)).collect()
                }
                "options" => {
                    text.options = body
                        .iter()
                        .map(|l| l.trim().trim_start_matches('-').trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                }
                "decision" => detail.insert(0, prose),
                _ => detail.push(format!("{name}: {prose}")),
            }
        }
        text.detail = (!detail.is_empty()).then(|| detail.join("\n\n"));
        text
    }
}

impl Decision {
    /// The record as it is copied out: labelled, with how it stands.
    pub fn adr(&self, label: &str) -> String {
        let status = if self.withdrawn() {
            "withdrawn"
        } else if self.superseded() {
            "superseded"
        } else {
            self.state.as_str()
        };
        let body = DecisionText::from(self).markdown(self.state == DecisionState::Open, false);
        let (name, rest) = body.split_once('\n').unwrap_or((&body, ""));
        format!("# {label} {name}\n\nStatus: {status}\n{rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made() -> DecisionText {
        DecisionText {
            chose: "The brief travels through the daemon".into(),
            context: Some("Session start only names the feature.".into()),
            detail: Some("argus-hook asks the daemon for its pane's brief.".into()),
            because: Some("A second contract would drift.".into()),
            over: Some("Hook reads the brief file directly".into()),
            consequences: vec![
                Consequence { gain: true, text: "One contract".into() },
                Consequence { gain: false, text: "A refusal is needed".into() },
            ],
            options: Vec::new(),
        }
    }

    #[test]
    fn a_decision_survives_being_written_out_and_read_back() {
        let text = made();

        assert_eq!(DecisionText::from_markdown(&text.markdown(false, false)), text);
        assert_eq!(DecisionText::from_markdown(&text.markdown(false, true)), text);
    }

    #[test]
    fn an_open_decision_is_written_with_its_options() {
        let text = DecisionText {
            chose: "How much history does a pane get?".into(),
            options: vec!["All of it".into(), "The last ten".into()],
            ..DecisionText::default()
        };

        let document = text.markdown(true, true);

        assert!(document.contains("## Options\n- All of it\n- The last ten"));
        assert!(!document.contains("## Why"));
        assert_eq!(DecisionText::from_markdown(&document), text);
    }

    #[test]
    fn prose_under_no_heading_is_the_decision() {
        let text = DecisionText::from_markdown("Use sqlite\n\nOne file, no server.\n\n## Risks\nLocking\n");

        assert_eq!(text.chose, "Use sqlite");
        assert_eq!(text.detail.as_deref(), Some("One file, no server.\n\nRisks: Locking"));
    }

    #[test]
    fn the_copied_record_carries_its_label_and_status() {
        let decision = Decision {
            chose: "Use sqlite".into(),
            because: Some("one file".into()),
            ..Decision::default()
        };

        assert_eq!(decision.adr("D2"), "# D2 Use sqlite\n\nStatus: accepted\n\n## Why\none file\n");
    }
}
