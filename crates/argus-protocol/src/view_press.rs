//! What a press of an agent view's action carries back, and what of it is
//! relayed to the agent.
//!
//! A client could send anything in a [`ViewPress`], and what reaches the
//! agent is typed into its terminal, so [`relay`] reads the press against
//! the view it claims to come from and keeps only what that view could have
//! produced, or refuses it, and writes what it kept on one line with no
//! control characters in it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::agent_view::{AgentView, FormField, FormInput, Tone};
use crate::view_check::MAX_VIEW_NUMBER;
use crate::view_tree::{leaves_within, ActionPlace, Leaf};

/// The longest text one form answer carries; past it the answer is cut.
/// A form field is a line or a note, not a document.
pub const MAX_ANSWER_BYTES: usize = 4096;

/// A button the agent offers. Argus runs nothing for it: pressing it sends
/// the agent a [`ViewPress`], as a refresh is sent, and the view stays as
/// it is until the agent shows it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ViewAction {
    /// The agent's name for it, sent back on a press. Unique in its view.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub tone: Tone,
    /// Asked of the person, as a yes or no, before the press is sent.
    #[serde(default)]
    pub confirm: Option<String>,
}

/// A press of an action in a view, as the person's client sends it and the
/// agent is told it: which action, on which selected rows, with which
/// answers when it submits a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ViewPress {
    /// The view's id.
    pub view: String,
    pub action: String,
    /// The `row_keys` of the rows selected in the table the action is on.
    #[serde(default)]
    pub rows: Vec<String>,
    /// The form the action submits, by its id.
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub answers: Vec<Answer>,
}

/// One form field's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Answer {
    pub name: String,
    pub value: AnswerValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AnswerValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

/// Why a press is not relayed: its view offers no such action, or no
/// longer does. The words are the person's, who pressed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotOffered;

impl std::fmt::Display for NotOffered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("That view no longer offers that action.")
    }
}

/// What the agent is told of `press`, read against `view`, the view it
/// claims to come from: only what that view could have produced, on one
/// line. A client could send anything; the agent hears only what its own
/// view said.
pub fn relay(view: &AgentView, press: ViewPress) -> Result<String, NotOffered> {
    let (action, place) = find(view, &press.action).ok_or(NotOffered)?;
    Ok(press_request(view, action, &press.kept(place)))
}

/// The header's actions first, then the first the blocks offer in reading order.
fn find<'a>(view: &'a AgentView, id: &str) -> Option<(&'a ViewAction, ActionPlace<'a>)> {
    if let Some(action) = view.actions.iter().find(|a| a.id == id) {
        return Some((action, ActionPlace::Button));
    }
    leaves_within(&view.blocks).into_iter().find_map(|leaf| match leaf {
        Leaf::Action(action, place) if action.id == id => Some((action, place)),
        _ => None,
    })
}

impl ViewPress {
    /// The press with only what its action, sitting at `place`, offered:
    /// the row keys of the table that carries it, each once, and the
    /// answers of the form it submits, each field once, of the kind the
    /// field takes and within what it allows.
    fn kept(mut self, place: ActionPlace<'_>) -> ViewPress {
        match place {
            ActionPlace::Table { row_keys: keys } => {
                let keys: HashSet<&str> = keys.iter().map(String::as_str).collect();
                let mut seen = HashSet::new();
                self.rows.retain(|row| keys.contains(row.as_str()) && seen.insert(row.clone()));
                self.form = None;
                self.answers.clear();
            }
            ActionPlace::Form { id: form, fields } if self.form.as_deref() == Some(form) => {
                self.rows.clear();
                let mut seen = HashSet::new();
                let answers = std::mem::take(&mut self.answers);
                self.answers = answers
                    .into_iter()
                    .filter_map(|answer| {
                        if !seen.insert(answer.name.clone()) {
                            return None;
                        }
                        let field = fields.iter().find(|f| f.name == answer.name)?;
                        Some(Answer { value: fit(field, &answer.value)?, ..answer })
                    })
                    .collect();
            }
            ActionPlace::Form { .. } | ActionPlace::Button => {
                self.rows.clear();
                self.form = None;
                self.answers.clear();
            }
        }
        self
    }
}

/// The answer if the field takes it, text cut to [`MAX_ANSWER_BYTES`].
fn fit(field: &FormField, value: &AnswerValue) -> Option<AnswerValue> {
    match (&field.input, value) {
        (FormInput::Text { .. } | FormInput::Date { .. }, AnswerValue::Text(t)) => Some(AnswerValue::Text(capped(t))),
        (FormInput::Choice { options, .. }, AnswerValue::Text(t)) if options.contains(t) => {
            Some(AnswerValue::Text(t.clone()))
        }
        (FormInput::Number { min, max, .. }, AnswerValue::Number(n))
            if n.abs() <= MAX_VIEW_NUMBER
                && min.is_none_or(|min| *n >= min)
                && max.is_none_or(|max| *n <= max) =>
        {
            Some(AnswerValue::Number(*n))
        }
        (FormInput::Toggle { .. }, AnswerValue::Bool(b)) => Some(AnswerValue::Bool(*b)),
        _ => None,
    }
}

fn capped(text: &str) -> String {
    let mut end = text.len().min(MAX_ANSWER_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Control characters made spaces, since a request may be typed into a
/// terminal where a newline sends it early and an escape does worse.
pub fn plain(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The same, through every string of `value`, keys included.
fn plain_json(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(plain(&s)),
        Value::Array(items) => Value::Array(items.into_iter().map(plain_json).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (plain(&k), plain_json(v))).collect()),
        other => other,
    }
}

/// What is typed to the agent for a press, on one line. The rows and
/// answers are JSON, so the agent can parse them.
fn press_request(view: &AgentView, action: &ViewAction, press: &ViewPress) -> String {
    let mut text = format!(
        "The person pressed `{action_id}` (\"{label}\") in the view `{id}`.",
        action_id = plain(&action.id),
        label = plain(&action.label),
        id = plain(&view.id),
    );
    if !press.rows.is_empty() {
        let rows = plain_json(serde_json::json!(press.rows));
        text.push_str(&format!(" Selected rows: {rows}."));
    }
    if let Some(form) = &press.form {
        let answers: serde_json::Map<String, serde_json::Value> = press
            .answers
            .iter()
            .map(|a| {
                let value = match &a.value {
                    AnswerValue::Text(t) => serde_json::json!(t),
                    AnswerValue::Number(n) => serde_json::json!(n),
                    AnswerValue::Bool(b) => serde_json::json!(b),
                };
                (a.name.clone(), value)
            })
            .collect();
        let answers = plain_json(serde_json::Value::Object(answers));
        text.push_str(&format!(" Form `{}` answers: {answers}.", plain(form)));
    }
    text.push_str(&format!(
        " Do what it asks with your own tools, then show the view again with \
         `argus-hook view show --stdin`, same id `{}`, so the person sees it done.",
        plain(&view.id),
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_view::{Block, Column, RowItem, RowSelect, Value};

    fn action(id: &str) -> ViewAction {
        ViewAction { id: id.into(), label: format!("label {id}"), tone: Default::default(), confirm: None }
    }

    fn press(action: &str) -> ViewPress {
        ViewPress { view: "v".into(), action: action.into(), rows: vec![], form: None, answers: vec![] }
    }

    fn field(name: &str, input: FormInput) -> FormField {
        FormField { name: name.into(), label: name.into(), input }
    }

    fn text_input() -> FormInput {
        FormInput::Text { default: None, placeholder: None, multiline: false }
    }

    fn answer(name: &str, value: AnswerValue) -> Answer {
        Answer { name: name.into(), value }
    }

    /// A view with a table whose action is `rerun`, and a form `deploy`
    /// submitted by `go`, one field of each kind.
    fn view() -> AgentView {
        let table = Block::Table {
            columns: vec![Column { title: "c".into(), numeric: false, optional: false }],
            rows: vec![vec![Value::Empty], vec![Value::Empty]],
            sorted_by: None,
            total: None,
            row_keys: vec!["r1".into(), "r2".into()],
            select: RowSelect::Many,
            actions: vec![action("rerun")],
            details: Vec::new(),
        };
        let form = Block::Form {
            id: "deploy".into(),
            fields: vec![
                field("note", text_input()),
                field("count", FormInput::Number { default: None, min: Some(1.0), max: Some(10.0) }),
                field("env", FormInput::Choice { options: vec!["dev".into(), "prod".into()], default: None }),
                field("dry", FormInput::Toggle { default: false }),
                field("when", FormInput::Date { default: None }),
            ],
            submit: action("go"),
        };
        AgentView { id: "v".into(), title: "v".into(), actions: vec![action("head")], blocks: vec![table, form], ..Default::default() }
    }

    /// What a press keeps, through the seam `relay` reads it by.
    fn offered(p: ViewPress, v: &AgentView) -> Option<ViewPress> {
        let (_, place) = find(v, &p.action)?;
        Some(p.kept(place))
    }

    fn submit(answers: Vec<Answer>) -> Vec<Answer> {
        let mut p = press("go");
        p.form = Some("deploy".into());
        p.answers = answers;
        offered(p, &view()).unwrap().answers
    }

    #[test]
    fn a_press_carries_only_the_rows_and_answers_its_view_offered() {
        let press = |action: &str| ViewPress {
            view: "v".into(),
            action: action.into(),
            rows: vec!["r1".into(), "made-up".into()],
            form: Some("deploy".into()),
            answers: vec![answer("dry", AnswerValue::Bool(true)), answer("extra", AnswerValue::Bool(true))],
        };
        let rerun = offered(press("rerun"), &view()).unwrap();
        assert_eq!(rerun.rows, ["r1"]);
        assert!(rerun.form.is_none() && rerun.answers.is_empty());
        let go = offered(press("go"), &view()).unwrap();
        assert!(go.rows.is_empty());
        assert_eq!(go.answers, [answer("dry", AnswerValue::Bool(true))]);
        let head = offered(press("head"), &view()).unwrap();
        assert!(head.rows.is_empty() && head.form.is_none() && head.answers.is_empty());
    }

    #[test]
    fn an_action_the_view_does_not_offer_is_no_press() {
        assert!(offered(press("rm-rf"), &view()).is_none());
    }

    #[test]
    fn answers_for_another_form_than_the_one_submitted_are_dropped() {
        let mut p = press("go");
        p.form = Some("other".into());
        p.answers = vec![answer("dry", AnswerValue::Bool(true))];
        let p = offered(p, &view()).unwrap();
        assert!(p.form.is_none() && p.answers.is_empty());
    }

    #[test]
    fn a_row_key_or_an_answer_sent_twice_counts_once_and_the_first_wins() {
        let mut p = press("rerun");
        p.rows = vec!["r2".into(), "r1".into(), "r2".into()];
        assert_eq!(offered(p, &view()).unwrap().rows, ["r2", "r1"]);
        let kept = submit(vec![
            answer("note", AnswerValue::Text("first".into())),
            answer("note", AnswerValue::Text("second".into())),
        ]);
        assert_eq!(kept, [answer("note", AnswerValue::Text("first".into()))]);
    }

    #[test]
    fn an_answer_of_a_kind_its_field_does_not_take_is_dropped() {
        let kept = submit(vec![
            answer("note", AnswerValue::Number(1.0)),
            answer("count", AnswerValue::Text("3".into())),
            answer("env", AnswerValue::Bool(true)),
            answer("dry", AnswerValue::Text("yes".into())),
            answer("when", AnswerValue::Bool(false)),
        ]);
        assert!(kept.is_empty(), "{kept:?}");
        let kept = submit(vec![
            answer("note", AnswerValue::Text("n".into())),
            answer("count", AnswerValue::Number(3.0)),
            answer("env", AnswerValue::Text("dev".into())),
            answer("dry", AnswerValue::Bool(true)),
            answer("when", AnswerValue::Text("2026-01-02".into())),
        ]);
        assert_eq!(kept.len(), 5);
    }

    #[test]
    fn a_choice_that_is_not_an_option_is_dropped() {
        assert!(submit(vec![answer("env", AnswerValue::Text("staging".into()))]).is_empty());
    }

    #[test]
    fn a_number_outside_its_field_or_the_view_bound_is_dropped() {
        for n in [0.0, 11.0, f64::NAN, f64::INFINITY] {
            assert!(submit(vec![answer("count", AnswerValue::Number(n))]).is_empty(), "{n}");
        }
        assert_eq!(submit(vec![answer("count", AnswerValue::Number(10.0))]).len(), 1);

        let mut v = view();
        let Block::Form { fields, .. } = &mut v.blocks[1] else { panic!() };
        fields[1].input = FormInput::Number { default: None, min: None, max: None };
        let mut p = press("go");
        p.form = Some("deploy".into());
        p.answers = vec![answer("count", AnswerValue::Number(MAX_VIEW_NUMBER * 10.0))];
        assert!(offered(p, &v).unwrap().answers.is_empty());
    }

    #[test]
    fn a_text_answer_is_cut_at_the_limit_on_a_character_boundary() {
        let long = "é".repeat(MAX_ANSWER_BYTES);
        let kept = submit(vec![answer("note", AnswerValue::Text(long))]);
        let AnswerValue::Text(t) = &kept[0].value else { panic!() };
        assert!(t.len() <= MAX_ANSWER_BYTES && t.len() >= MAX_ANSWER_BYTES - 1);
        let exact = "x".repeat(MAX_ANSWER_BYTES);
        let kept = submit(vec![answer("note", AnswerValue::Text(exact.clone()))]);
        assert_eq!(kept[0].value, AnswerValue::Text(exact));
    }

    #[test]
    fn an_action_is_found_wherever_the_view_offers_it() {
        let card = Block::Card {
            title: None,
            chip: None,
            tone: Default::default(),
            blocks: vec![Block::Form { id: "f".into(), fields: vec![], submit: action("send") }],
            actions: vec![action("card")],
            opens: None,
        };
        let row = Block::Row { align: Default::default(), items: vec![RowItem::Action { action: action("inrow") }] };
        let mut v = AgentView { id: "v".into(), title: "v".into(), actions: vec![action("head")], ..Default::default() };
        v.blocks = vec![Block::Section { heading: "s".into(), blocks: vec![card, row], opens: None }];
        v.blocks.extend(view().blocks);
        for id in ["head", "card", "send", "inrow", "rerun", "go"] {
            let line = relay(&v, press(id)).unwrap();
            assert!(line.contains(&format!("`{id}`")), "{line}");
        }
        assert_eq!(relay(&v, press("rm-rf")), Err(NotOffered));
    }

    #[test]
    fn a_press_is_relayed_as_one_line_naming_only_what_the_view_offered() {
        let mut p = press("rerun");
        p.rows = vec!["r1".into(), "made-up".into()];
        let line = relay(&view(), p).unwrap();
        assert!(line.contains(r#"["r1"]"#) && !line.contains("made-up"), "{line}");
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }

    #[test]
    fn a_press_the_view_does_not_offer_is_refused_in_the_persons_words() {
        assert_eq!(relay(&view(), press("rm-rf")), Err(NotOffered));
        assert_eq!(NotOffered.to_string(), "That view no longer offers that action.");
    }

    #[test]
    fn a_press_request_stays_on_one_line_and_carries_rows_and_answers() {
        let v = view();
        let mut act = action("ship");
        act.label = "Ship\nit".into();
        let mut p = press("ship");
        p.rows = vec!["r1".into(), "r\n2".into()];
        p.form = Some("f".into());
        p.answers = vec![
            answer("note", AnswerValue::Text("a\nb\u{1b}[0m".into())),
            answer("n", AnswerValue::Number(3.0)),
            answer("ok", AnswerValue::Bool(true)),
        ];
        let text = press_request(&v, &act, &p);
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(text.contains("`ship`") && text.contains("`v`") && text.contains("Ship it"), "{text}");
        assert!(text.contains(r#"["r1","r 2"]"#), "{text}");
        assert!(text.contains(r#""n":3.0"#) && text.contains(r#""ok":true"#), "{text}");
        assert!(text.contains("view show --stdin"), "{text}");
    }

    #[test]
    fn plain_turns_every_control_character_into_a_space() {
        assert_eq!(plain("a\nb\tc\u{1b}[0m\r"), "a b c [0m ");
    }
}
