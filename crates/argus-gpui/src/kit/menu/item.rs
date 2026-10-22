//! What a menu offers, as data: the page's `MenuItem` (`kit/menu.ts`) once, for every menu.
//!
//! A menu is a list of headings, rules and choices. A choice says what it does with `A`, whatever
//! the surface sends or opens, so the rail, the strip, the docks and the console build the same
//! list and one drawing (`popup`) shows it. A choice that cannot be chosen now says why, and the
//! reason is shown where its hint would be (SHELL-MENU-1).

/// What a choice shows at its right edge when it can be chosen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Hint {
    #[default]
    None,
    /// Words, `here`.
    Text(String),
    /// The keys that do the same, one list of presses per alternative.
    Keys(Vec<Vec<String>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice<A> {
    pub label: String,
    pub does: A,
    /// Why it cannot be chosen now. Set, the choice is disabled and says this in place of its hint.
    pub reason: Option<String>,
    pub hint: Hint,
    /// Ends something that cannot be got back: drawn in the danger colour.
    pub danger: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuItem<A> {
    Heading(String),
    Rule,
    Choice(Choice<A>),
}

impl<A> MenuItem<A> {
    pub fn heading(text: impl Into<String>) -> Self {
        Self::Heading(text.into())
    }

    pub fn choice(label: impl Into<String>, does: A) -> Self {
        Self::Choice(Choice { label: label.into(), does, reason: None, hint: Hint::None, danger: false })
    }

    /// Drawn in the danger colour. A heading or a rule is not one to change.
    pub fn danger(self) -> Self {
        self.with(|choice| choice.danger = true)
    }

    /// Names the keys that do the same.
    pub fn keyed(self, keys: Vec<Vec<String>>) -> Self {
        self.with(|choice| choice.hint = if keys.is_empty() { Hint::None } else { Hint::Keys(keys) })
    }

    /// Says `words` at the right edge.
    pub fn noted(self, words: impl Into<String>) -> Self {
        self.with(|choice| choice.hint = Hint::Text(words.into()))
    }

    /// Cannot be chosen, for `reason`.
    pub fn refused(self, reason: impl Into<String>) -> Self {
        self.with(|choice| choice.reason = Some(reason.into()))
    }

    /// Cannot be chosen for `reason`, when `when`.
    pub fn refused_when(self, when: bool, reason: impl Into<String>) -> Self {
        if when {
            self.refused(reason)
        } else {
            self
        }
    }

    fn with(mut self, change: impl FnOnce(&mut Choice<A>)) -> Self {
        if let Self::Choice(choice) = &mut self {
            change(choice);
        }
        self
    }

    /// The same item doing something else: the surface that built a list for one kind of choice
    /// hands it to a menu of another.
    pub fn map<B>(self, change: impl FnOnce(A) -> B) -> MenuItem<B> {
        match self {
            Self::Heading(text) => MenuItem::Heading(text),
            Self::Rule => MenuItem::Rule,
            Self::Choice(Choice { label, does, reason, hint, danger }) => {
                MenuItem::Choice(Choice { label, does: change(does), reason, hint, danger })
            }
        }
    }

    /// The choice, if this is one.
    pub fn as_choice(&self) -> Option<&Choice<A>> {
        match self {
            Self::Choice(choice) => Some(choice),
            _ => None,
        }
    }

    /// Whether it can be chosen: a choice with no reason against it.
    pub fn enabled(&self) -> bool {
        self.as_choice().is_some_and(|choice| choice.reason.is_none())
    }
}

/// A menu as a test reads it, a line per item: `# heading`, `---`, and a choice
/// with its reason in brackets and `!` before one in danger.
#[cfg(test)]
pub(crate) fn lines<A>(items: &[MenuItem<A>]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Heading(text) => format!("# {text}"),
            MenuItem::Rule => "---".to_owned(),
            MenuItem::Choice(choice) => format!(
                "{}{}{}",
                if choice.danger { "!" } else { "" },
                choice.label,
                choice.reason.as_ref().map_or(String::new(), |reason| format!(" ({reason})"))
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_item_can_do_something_else_and_keeps_everything_it_said(){
        let item = MenuItem::choice("Close", 1).danger().refused("busy").noted("x");
        let mapped = item.clone().map(|n| n + 1);
        let (before, after) = (item.as_choice().unwrap(), mapped.as_choice().unwrap());
        assert_eq!((after.does, &after.label, &after.reason, after.danger, &after.hint), (2, &before.label, &before.reason, before.danger, &before.hint));
        assert_eq!(MenuItem::<u8>::Rule.map(|n| n + 1), MenuItem::Rule);
        assert_eq!(MenuItem::<u8>::heading("h").map(|n| n + 1), MenuItem::heading("h"));
    }

    #[test]
    fn a_choice_can_be_chosen_until_it_gives_a_reason() {
        let item = MenuItem::choice("Close", 1);
        assert!(item.enabled());
        let refused = item.refused("here");
        assert!(!refused.enabled());
        assert_eq!(refused.as_choice().and_then(|c| c.reason.as_deref()), Some("here"));
    }

    #[test]
    fn a_heading_and_a_rule_are_never_chosen() {
        assert!(!MenuItem::<u8>::heading("pwsh").enabled());
        assert!(!MenuItem::<u8>::Rule.enabled());
    }

    #[test]
    fn marking_a_heading_changes_nothing() {
        assert_eq!(MenuItem::<u8>::heading("pwsh").danger().refused("no"), MenuItem::heading("pwsh"));
    }

    #[test]
    fn keys_named_and_none_named_are_a_hint_and_no_hint() {
        let keyed = MenuItem::choice("Split", 1).keyed(vec![vec!["Ctrl Space".into(), "\\".into()]]);
        assert!(matches!(keyed.as_choice().map(|c| &c.hint), Some(Hint::Keys(keys)) if keys.len() == 1));
        let bare = MenuItem::choice("Split", 1).keyed(Vec::new());
        assert_eq!(bare.as_choice().map(|c| &c.hint), Some(&Hint::None), "no chords is no hint");
    }

    #[test]
    fn a_reason_is_only_given_when_asked() {
        assert!(MenuItem::choice("x", 1).refused_when(false, "why").enabled());
        assert!(!MenuItem::choice("x", 1).refused_when(true, "why").enabled());
    }
}
