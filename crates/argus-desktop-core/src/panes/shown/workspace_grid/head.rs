//! What a cell's head names: the pane's title as its two halves, the agent's name and the task, at
//! the ` · ` an agent's title is written with (`claude · trim context`).
//!
//! The head sets the name in the body colour and the task muted, so the split is the window's rule
//! and is made here once, not by each surface that draws a title. The rail's folded tile takes its
//! initials from the same task.

/// The separator an agent's title puts between its name and its task.
pub const SEPARATOR: &str = " \u{b7} ";

/// A title in two halves. A title with no separator (a shell, `pwsh`) is a name alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named<'a> {
    pub name: &'a str,
    pub task: Option<&'a str>,
}

/// `title` split at its first separator; an empty half is no half, so `claude · ` is the name alone
/// and ` · trim context` is a task with no name to set before it.
pub fn halves(title: &str) -> Named<'_> {
    match title.split_once(SEPARATOR) {
        Some((name, task)) => {
            let (name, task) = (name.trim(), task.trim());
            match (name.is_empty(), task.is_empty()) {
                (_, true) => Named { name, task: None },
                (true, false) => Named { name: task, task: None },
                (false, false) => Named { name, task: Some(task) },
            }
        }
        None => Named { name: title, task: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_title_is_its_name_then_its_task() {
        assert_eq!(halves("claude \u{b7} trim context"), Named { name: "claude", task: Some("trim context") });
        assert_eq!(halves("codex \u{b7} ratatui 0.30"), Named { name: "codex", task: Some("ratatui 0.30") });
    }

    #[test]
    fn a_title_with_no_separator_is_a_name_alone() {
        assert_eq!(halves("pwsh"), Named { name: "pwsh", task: None });
        assert_eq!(halves("cargo test"), Named { name: "cargo test", task: None });
        assert_eq!(halves(""), Named { name: "", task: None });
        assert_eq!(halves("a\u{b7}b"), Named { name: "a\u{b7}b", task: None }, "a dot with no spaces is part of a word");
    }

    #[test]
    fn only_the_first_separator_splits_and_the_task_keeps_the_rest() {
        assert_eq!(halves("claude \u{b7} review \u{b7} split view"), Named { name: "claude", task: Some("review \u{b7} split view") });
    }

    #[test]
    fn an_empty_half_is_dropped_rather_than_drawn_as_nothing() {
        assert_eq!(halves("claude \u{b7} "), Named { name: "claude", task: None });
        assert_eq!(halves(" \u{b7} trim context"), Named { name: "trim context", task: None });
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // The head draws the name then the task: nothing of the title is lost but the separator and
            // the spaces round it (the rule of the restyle proposal's two-weight head).
            #[test]
            fn the_halves_hold_every_word_of_the_title(title in "[a-z \u{b7}]{0,24}") {
                let named = halves(&title);
                let words = |text: &str| text.split_whitespace().filter(|word| *word != "\u{b7}").map(str::to_owned).collect::<Vec<_>>();
                let mut drawn = words(named.name);
                drawn.extend(named.task.map(words).unwrap_or_default());
                prop_assert_eq!(drawn, words(&title));
            }
        }
    }
}
