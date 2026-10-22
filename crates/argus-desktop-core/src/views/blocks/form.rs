//! A form block's reading: what each field shows before the person has touched it, which of what
//! they typed still stands, and the answers a press sends (FORM-1 to FORM-4).
//!
//! The page's `views/blocks/form.ts` reads these off its inputs; a window with no inputs to read
//! needs them as rules. The daemon holds what is sent to what the form could have produced and cuts
//! an answer past `MAX_ANSWER_BYTES` (`argus-protocol` `views::press`), so none of that is repeated
//! here: a client says what the person typed and the daemon says what it will take.

use argus_protocol::{Answer, AnswerValue, FormField, FormInput};

/// What the person typed in a field and has not sent: its text as typed, or a toggle's state.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Text(String),
    Flag(bool),
}

/// What a field's text box says before anything is typed: the agent's default, a choice's first
/// option when it names none, nothing for a date or a number given none.
pub fn default_text(input: &FormInput) -> String {
    match input {
        FormInput::Text { default, .. } | FormInput::Date { default } => default.clone().unwrap_or_default(),
        FormInput::Number { default, .. } => default.map(number_text).unwrap_or_default(),
        FormInput::Choice { options, default } => default.clone().or_else(|| options.first().cloned()).unwrap_or_default(),
        FormInput::Toggle { .. } => String::new(),
    }
}

/// A number as a box writes it: whole numbers without a point.
pub fn number_text(number: f64) -> String {
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        number.to_string()
    }
}

/// The text a field shows: what was typed when that still means something, else its default. A
/// choice whose typed option is no longer among the options (the agent showed the form again with
/// others) is ignored, so a stale pick is never sent.
pub fn shown_text(input: &FormInput, typed: Option<&Typed>) -> String {
    let Some(Typed::Text(text)) = typed else { return default_text(input) };
    match input {
        FormInput::Choice { options, .. } if !options.contains(text) => default_text(input),
        FormInput::Toggle { .. } => String::new(),
        _ => text.clone(),
    }
}

/// Whether a toggle is on: what the person set, else the agent's default.
pub fn shown_flag(input: &FormInput, typed: Option<&Typed>) -> bool {
    match (typed, input) {
        (Some(Typed::Flag(on)), _) => *on,
        (_, FormInput::Toggle { default }) => *default,
        _ => false,
    }
}

/// Whether a field spans both columns: a long text.
pub fn wide(field: &FormField) -> bool {
    matches!(field.input, FormInput::Text { multiline: true, .. })
}

/// Whether a field is a box then its label on one line.
pub fn inline(field: &FormField) -> bool {
    matches!(field.input, FormInput::Toggle { .. })
}

/// A number box's text read as a number: none when it is empty or is not a finite number.
pub fn read_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

/// The answers a press sends, built at the press from what each field shows now. A number left
/// empty or unreadable is left out; every other kind always answers.
pub fn answers(fields: &[FormField], typed: impl Fn(&str) -> Option<Typed>) -> Vec<Answer> {
    fields
        .iter()
        .filter_map(|field| {
            let held = typed(&field.name);
            let value = match &field.input {
                FormInput::Number { .. } => AnswerValue::Number(read_number(&shown_text(&field.input, held.as_ref()))?),
                FormInput::Toggle { .. } => AnswerValue::Bool(shown_flag(&field.input, held.as_ref())),
                _ => AnswerValue::Text(shown_text(&field.input, held.as_ref())),
            };
            Some(Answer { name: field.name.clone(), value })
        })
        .collect()
}

/// A number box's arrows: one step from `text` in `by`'s direction, held to `min` and `max`. A box
/// left empty starts from nought, so its first step up lands on the least the bounds allow.
pub fn step_number(text: &str, by: f64, min: Option<f64>, max: Option<f64>) -> String {
    let mut next = read_number(text).unwrap_or(0.0) + by;
    if let Some(min) = min {
        next = next.max(min);
    }
    if let Some(max) = max {
        next = next.min(max);
    }
    number_text(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, input: FormInput) -> FormField {
        FormField { name: name.into(), label: name.into(), input }
    }

    fn text(default: Option<&str>) -> FormInput {
        FormInput::Text { default: default.map(str::to_owned), placeholder: None, multiline: false }
    }

    fn choice(options: &[&str], default: Option<&str>) -> FormInput {
        FormInput::Choice { options: options.iter().map(|o| (*o).to_owned()).collect(), default: default.map(str::to_owned) }
    }

    fn typed(pairs: &[(&str, Typed)]) -> impl Fn(&str) -> Option<Typed> {
        let pairs: Vec<(String, Typed)> = pairs.iter().map(|(name, typed)| ((*name).to_owned(), typed.clone())).collect();
        move |name| pairs.iter().find(|(held, _)| held == name).map(|(_, typed)| typed.clone())
    }

    // FORM-1: each kind of field shows its default before anything is typed.
    #[test]
    fn a_field_shows_the_agents_default_and_a_choice_its_first_option_when_it_names_none() {
        assert_eq!(default_text(&text(Some("main"))), "main");
        assert_eq!(default_text(&text(None)), "");
        assert_eq!(default_text(&FormInput::Number { default: Some(3.0), min: None, max: None }), "3");
        assert_eq!(default_text(&FormInput::Number { default: Some(2.5), min: None, max: None }), "2.5");
        assert_eq!(default_text(&choice(&["a", "b"], None)), "a");
        assert_eq!(default_text(&choice(&["a", "b"], Some("b"))), "b");
        assert_eq!(default_text(&FormInput::Date { default: Some("2026-10-22".into()) }), "2026-10-22");
    }

    // FORM-2: typed text wins over the default; a stale choice does not.
    #[test]
    fn what_was_typed_stands_over_the_default_and_a_stored_choice_no_longer_offered_is_ignored() {
        assert_eq!(shown_text(&text(Some("main")), Some(&Typed::Text("dev".into()))), "dev");
        assert_eq!(shown_text(&text(Some("main")), Some(&Typed::Text(String::new()))), "", "an emptied field is not put back");
        let input = choice(&["a", "b"], None);
        assert_eq!(shown_text(&input, Some(&Typed::Text("b".into()))), "b");
        assert_eq!(shown_text(&input, Some(&Typed::Text("gone".into()))), "a");
        assert!(shown_flag(&FormInput::Toggle { default: true }, None));
        assert!(!shown_flag(&FormInput::Toggle { default: true }, Some(&Typed::Flag(false))), "off stays off");
    }

    // FORM-3: the answers a press sends, by kind.
    #[test]
    fn a_press_answers_each_kind_as_the_page_does_and_leaves_out_a_number_that_is_not_one() {
        let fields = vec![
            field("branch", text(Some("main"))),
            field("count", FormInput::Number { default: None, min: Some(1.0), max: Some(9.0) }),
            field("env", choice(&["dev", "prod"], None)),
            field("loud", FormInput::Toggle { default: false }),
            field("when", FormInput::Date { default: None }),
        ];
        let said = answers(&fields, typed(&[("count", Typed::Text(" 4 ".into())), ("env", Typed::Text("prod".into())), ("loud", Typed::Flag(true))]));
        assert_eq!(
            said,
            [
                Answer { name: "branch".into(), value: AnswerValue::Text("main".into()) },
                Answer { name: "count".into(), value: AnswerValue::Number(4.0) },
                Answer { name: "env".into(), value: AnswerValue::Text("prod".into()) },
                Answer { name: "loud".into(), value: AnswerValue::Bool(true) },
                Answer { name: "when".into(), value: AnswerValue::Text(String::new()) },
            ]
        );
        for bad in ["", "  ", "abc", "inf", "NaN"] {
            let said = answers(&fields, typed(&[("count", Typed::Text(bad.into()))]));
            assert!(said.iter().all(|a| a.name != "count"), "{bad:?} is no number, so it is not sent");
        }
    }

    // FORM-1: bounds hold the arrows, not what is typed (the daemon holds what is sent).
    #[test]
    fn the_arrows_step_by_one_within_the_bounds() {
        assert_eq!(step_number("3", 1.0, Some(1.0), Some(9.0)), "4");
        assert_eq!(step_number("9", 1.0, Some(1.0), Some(9.0)), "9");
        assert_eq!(step_number("1", -1.0, Some(1.0), Some(9.0)), "1");
        assert_eq!(step_number("", 1.0, Some(5.0), None), "5", "an empty box's first step up lands on the least allowed");
        assert_eq!(step_number("2.5", 1.0, None, None), "3.5");
    }

    #[test]
    fn a_long_text_spans_both_columns_and_a_toggle_is_a_box_then_its_label() {
        let long = field("n", FormInput::Text { default: None, placeholder: None, multiline: true });
        assert!(wide(&long) && !wide(&field("n", text(None))));
        assert!(inline(&field("t", FormInput::Toggle { default: false })) && !inline(&long));
    }
}
