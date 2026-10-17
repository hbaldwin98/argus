//! A flow's Mermaid source as the sequence diagram the page draws: its
//! lifelines left to right, each step as an arrow between two of them, and
//! the frames an `alt`, `opt` or `loop` draws around its steps.
//!
//! The terminal client hands the source to a renderer that returns text;
//! the page draws its own so a step can be lit, clicked and dragged. Which
//! lines are steps, and in what order the lifelines stand, is
//! `argus_protocol::board::diagrams::flow`'s to say, so a step here is the
//! step of the same number in the list beside it, and a drag rewrites the
//! lines it read.

use std::collections::HashMap;

use argus_protocol::board::diagrams::flow::{self, FlowRow};

use crate::page::{Lifeline, Sequence, SequenceItem};

/// The frames Mermaid opens with a keyword, and the keywords that start
/// their next arm.
const OPENS: [&str; 7] = ["alt", "opt", "loop", "par", "critical", "break", "rect"];
const ARMS: [&str; 3] = ["else", "and", "option"];

/// `source` as drawn, with step `on` (numbered from one) lit.
pub fn read(source: &str, on: Option<usize>) -> Sequence {
    let lifelines: Vec<Lifeline> = flow::participants(source)
        .into_iter()
        .map(|p| Lifeline { name: p.id, label: p.label })
        .collect();
    let place: HashMap<&str, usize> = lifelines.iter().enumerate().map(|(at, l)| (l.name.as_str(), at)).collect();
    let steps: HashMap<usize, flow::Step> = flow::rows(source)
        .into_iter()
        .filter_map(|row| match row {
            FlowRow::Step(step) => Some((step.line, step)),
            FlowRow::Branch { .. } => None,
        })
        .collect();

    let mut items = Vec::new();
    let mut open = 0usize;
    for (at, line) in source.lines().enumerate() {
        if let Some(step) = steps.get(&at) {
            let (Some(&from), to) = (place.get(step.from.as_str()), step.to.as_deref().and_then(|to| place.get(to)))
            else {
                continue;
            };
            items.push(SequenceItem::Message {
                step: step.number,
                from,
                to: to.copied().unwrap_or(from),
                reply: step.reply,
                text: step.text.clone(),
                on: on == Some(step.number),
            });
            continue;
        }
        let said = line.split_once("%%").map_or(line, |(said, _)| said).trim();
        let (word, rest) = said.split_once(char::is_whitespace).unwrap_or((said, ""));
        if OPENS.contains(&word) {
            open += 1;
            items.push(SequenceItem::Open { frame: word.to_string(), label: rest.trim().to_string() });
        } else if ARMS.contains(&word) && open > 0 {
            items.push(SequenceItem::Arm { label: rest.trim().to_string() });
        } else if word == "end" && open > 0 {
            open -= 1;
            items.push(SequenceItem::Close);
        }
    }
    // A frame the source never closed is closed at the foot, so the page
    // always draws a box rather than a frame running off the diagram.
    items.extend(std::iter::repeat_n(SequenceItem::Close, open));
    Sequence { lifelines, items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn drawn(sequence: &Sequence) -> Vec<String> {
        sequence
            .items
            .iter()
            .map(|item| match item {
                SequenceItem::Message { step, from, to, reply, on, .. } => {
                    let arrow = if *reply { "-->" } else { "->" };
                    format!("{step} {from}{arrow}{to}{}", if *on { " lit" } else { "" })
                }
                SequenceItem::Open { frame, label } => format!("{frame} {label}"),
                SequenceItem::Arm { label } => format!("else {label}"),
                SequenceItem::Close => "end".into(),
            })
            .collect()
    }

    #[test]
    fn a_flow_draws_its_lifelines_in_order_and_its_steps_between_them() {
        let sequence = read(fixtures::FLOW, Some(2));
        let labels: Vec<_> = sequence.lifelines.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(labels, ["agent", "argus-hook", "argusd"]);
        assert_eq!(
            drawn(&sequence),
            [
                "1 0->1",
                "2 1->2 lit",
                "3 2->2",
                "alt the pane has a feature",
                "4 2-->1",
                "else it has none",
                "5 2-->1",
                "end",
            ]
        );
    }

    #[test]
    fn a_frame_left_open_is_closed_at_the_foot() {
        let sequence = read("sequenceDiagram\n    loop every poll\n    a->>b: ask", None);
        assert_eq!(drawn(&sequence), ["loop every poll", "1 0->1", "end"]);
    }
}
