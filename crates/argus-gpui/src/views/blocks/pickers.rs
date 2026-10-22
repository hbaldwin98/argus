//! The three form controls that are more than a text box, kept between draws as `kept` keeps the
//! rest: a select for a choice, a date picker and a number box with arrows (FORM-1).
//!
//! Each tells `typed` what it now says as the text the draft holds, and is written to only when it
//! is made or when what the draft says is not what it shows: what is drawn never writes into a
//! control somebody may be using. The rules (what an arrow adds, what a draft shows) are core's.

use std::collections::{HashMap, HashSet};

use argus_desktop_core::views::blocks::form::step_number;
use chrono::NaiveDate;
use gpui_kit::component::date_picker::{DatePickerEvent, DatePickerState};
use gpui_kit::component::input::{InputEvent, InputState, NumberInputEvent, StepAction};
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::IndexPath;
use gpui_kit::{prelude::*, App, Entity, Window};

/// How a date is written in a draft and sent: `<input type=date>`'s value.
const ISO: &str = "%Y-%m-%d";

type Choice = Entity<SelectState<Vec<String>>>;

#[derive(Default)]
pub struct Pickers {
    choices: HashMap<String, Choice>,
    dates: HashMap<String, Entity<DatePickerState>>,
    numbers: HashMap<String, Entity<InputState>>,
    asked: HashSet<String>,
}

impl Pickers {
    /// The select kept under `key`, showing `shown` among `options` (nothing when it is not one).
    pub fn choice(&mut self, key: &str, options: &[String], shown: &str, typed: impl Fn(&str, &mut App) + 'static, window: &mut Window, cx: &mut App) -> Choice {
        self.asked.insert(key.to_owned());
        let row = options.iter().position(|option| option == shown);
        let at = row.map(IndexPath::new);
        let state = self
            .choices
            .entry(key.to_owned())
            .or_insert_with(|| {
                let items = options.to_vec();
                let state = cx.new(|cx| SelectState::new(items, at, window, cx));
                window
                    .subscribe(&state, cx, move |_, event: &SelectEvent<Vec<String>>, _, cx| {
                        if let SelectEvent::Confirm(Some(option)) = event {
                            typed(option, cx);
                        }
                    })
                    .detach();
                state
            })
            .clone();
        // The agent showing the form again with other options, or the draft being let go of.
        if state.read(cx).selected_value() != row.map(|row| &options[row]) {
            state.update(cx, |state, cx| {
                state.set_items(options.to_vec(), window, cx);
                state.set_selected_index(at, window, cx);
            });
        }
        state
    }

    /// The date picker kept under `key`, showing `text` (`YYYY-MM-DD`, or nothing).
    pub fn date(&mut self, key: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static, window: &mut Window, cx: &mut App) -> Entity<DatePickerState> {
        self.asked.insert(key.to_owned());
        let held = parse(text);
        self.dates
            .entry(key.to_owned())
            .or_insert_with(|| {
                let state = cx.new(|cx| {
                    let mut state = DatePickerState::new(window, cx).date_format(ISO);
                    if let Some(date) = held {
                        state.set_date(date, window, cx);
                    }
                    state
                });
                window
                    .subscribe(&state, cx, move |_, event: &DatePickerEvent, _, cx| {
                        let DatePickerEvent::Change(value) = event;
                        typed(&value.start().map(|at| at.date().format(ISO).to_string()).unwrap_or_default(), cx);
                    })
                    .detach();
                state
            })
            .clone()
    }

    /// The number box kept under `key`, whose arrows (and the Up and Down keys) step by one within
    /// `bounds` as core says.
    pub fn number(&mut self, key: &str, text: &str, bounds: (Option<f64>, Option<f64>), typed: impl Fn(&str, &mut App) + 'static, window: &mut Window, cx: &mut App) -> Entity<InputState> {
        self.asked.insert(key.to_owned());
        self.numbers
            .entry(key.to_owned())
            .or_insert_with(|| {
                let input = cx.new(|cx| InputState::new(window, cx));
                // The box steps itself unless told not to, and core's rule is the one that counts.
                input.update(cx, |input, cx| input.set_step(None, window, cx));
                if !text.is_empty() {
                    input.update(cx, |input, cx| input.set_value(text, window, cx));
                }
                let typed = std::rc::Rc::new(typed);
                let edited = typed.clone();
                cx.subscribe(&input, move |input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        edited(&input.read(cx).value(), cx);
                    }
                })
                .detach();
                // A step is set into the box, which does not tell of it as an edit does.
                window
                    .subscribe(&input, cx, move |input, event: &NumberInputEvent, window, cx| {
                        let NumberInputEvent::Step(action) = event;
                        let by = if matches!(action, StepAction::Increment) { 1.0 } else { -1.0 };
                        let now = step_number(&input.read(cx).value(), by, bounds.0, bounds.1);
                        input.update(cx, |input, cx| input.set_value(now.clone(), window, cx));
                        typed(&now, cx);
                    })
                    .detach();
                input
            })
            .clone()
    }

    /// Lets go of what the draw that just ended did not ask for.
    pub fn sweep(&mut self) {
        let asked = std::mem::take(&mut self.asked);
        self.choices.retain(|key, _| asked.contains(key));
        self.dates.retain(|key, _| asked.contains(key));
        self.numbers.retain(|key, _| asked.contains(key));
    }

    /// The date picker kept under `key`, for a test to act on as the person does.
    #[cfg(test)]
    pub fn date_at(&self, key: &str) -> Option<Entity<DatePickerState>> {
        self.dates.get(key).cloned()
    }

    pub fn held(&self) -> usize {
        self.choices.len() + self.dates.len() + self.numbers.len()
    }
}

/// A draft's text as a date; nothing for what is not a calendar day.
fn parse(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, ISO).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_is_a_date_only_when_it_is_a_calendar_day() {
        assert_eq!(parse("2026-02-28"), NaiveDate::from_ymd_opt(2026, 2, 28));
        assert_eq!(parse("2026-02-30"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("28/02/2026"), None);
    }
}
