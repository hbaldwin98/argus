//! What an agent is typed when the person acts on one of its views: a press,
//! or a refresh of a view it showed or saved. Each is one line with no
//! control character in it, since it may be typed into a terminal where a
//! newline sends it early and an escape does worse.

use crate::agent_view::AgentView;
use crate::saved_view::ViewSource;
use crate::view_press::{AnswerValue, ViewAction, ViewPress};

/// Control characters made spaces.
fn plain(text: &str) -> String {
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

/// A press of `action` in `view`, carrying only what `press` kept: the rows
/// and answers as JSON, so the agent can parse them.
pub(crate) fn press(view: &AgentView, action: &ViewAction, press: &ViewPress) -> String {
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

/// A refresh of a view the agent showed: read its subject again and show it anew.
pub fn refresh_shown(view: &AgentView) -> String {
    let title = plain(&view.title);
    let source = match &view.subject {
        Some(url) => plain(url),
        None => "its source".to_string(),
    };
    format!(
        "Refresh the view `{id}` (\"{title}\"): read {source} again with your own tools and show \
         it anew with `argus-hook view show --stdin`, same id.",
        id = plain(&view.id),
    )
}

/// A refresh of a view an agent saved: its recipe and sources, read again,
/// and saved anew.
pub fn refresh_saved(view: &AgentView, recipe: Option<&str>, sources: &[ViewSource]) -> String {
    let mut text = format!(
        "Refresh the saved view `{id}` (\"{title}\")",
        id = plain(&view.id),
        title = plain(&view.title),
    );
    if let Some(recipe) = recipe.filter(|r| !r.trim().is_empty()) {
        text.push_str(&format!(". Recipe: {}", plain(recipe.trim())));
    }
    let sources: Vec<String> = sources
        .iter()
        .filter_map(|source| match source {
            ViewSource::File { path } => Some(format!("file {}", plain(path))),
            ViewSource::Commit { sha } => Some(format!("commit {}", plain(sha))),
            ViewSource::Url { url } => Some(format!("url {}", plain(url))),
            ViewSource::Note { text } => Some(format!("note {}", plain(text))),
            ViewSource::Unknown => None,
        })
        .collect();
    if !sources.is_empty() {
        text.push_str(&format!(". Sources: {}", sources.join("; ")));
    }
    text.push_str(&format!(
        ". Read them again with your own tools, then save it anew with \
         `argus-hook view save --stdin`, same id `{}`; `argus-hook view get {}` prints it as it is.",
        plain(&view.id),
        plain(&view.id),
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_press::Answer;

    fn view(id: &str) -> AgentView {
        AgentView { id: id.into(), title: format!("title of {id}"), ..Default::default() }
    }

    fn action(id: &str) -> ViewAction {
        ViewAction { id: id.into(), label: format!("label {id}"), tone: Default::default(), confirm: None }
    }

    fn answer(name: &str, value: AnswerValue) -> Answer {
        Answer { name: name.into(), value }
    }

    fn saved() -> (AgentView, Option<String>, Vec<ViewSource>) {
        let mut v = view("deps");
        v.title = "Deps".into();
        let sources = vec![
            ViewSource::File { path: "Cargo.lock".into() },
            ViewSource::Url { url: "https://crates.io".into() },
        ];
        (v, Some("run cargo tree\nand summarise".into()), sources)
    }

    #[test]
    fn a_press_stays_on_one_line_and_carries_rows_and_answers() {
        let v = view("v");
        let mut act = action("ship");
        act.label = "Ship\nit".into();
        let p = ViewPress {
            view: "v".into(),
            action: "ship".into(),
            rows: vec!["r1".into(), "r\n2".into()],
            form: Some("f".into()),
            answers: vec![
                answer("note", AnswerValue::Text("a\nb\u{1b}[0m".into())),
                answer("n", AnswerValue::Number(3.0)),
                answer("ok", AnswerValue::Bool(true)),
            ],
        };
        let text = press(&v, &act, &p);
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

    #[test]
    fn a_shown_views_refresh_stays_on_one_line_and_names_its_source() {
        let mut view = view("t");
        view.title = "line\none".into();
        view.subject = Some("https://example.com/browse/ENG-1".into());
        let text = refresh_shown(&view);
        assert!(!text.contains('\n'), "{text}");
        assert!(text.contains("`t`") && text.contains("https://example.com/browse/ENG-1"), "{text}");
        view.subject = None;
        assert!(refresh_shown(&view).contains("its source"));
    }

    #[test]
    fn a_shown_views_refresh_has_no_control_character_in_any_part() {
        let mut view = view("t\u{1b}x");
        view.title = "a\u{7}b".into();
        view.subject = Some("https://example.com/a\nb".into());
        let text = refresh_shown(&view);
        assert!(!text.chars().any(char::is_control), "{text:?}");
    }

    #[test]
    fn a_saved_views_refresh_is_one_line_naming_the_recipe_and_sources() {
        let (view, recipe, sources) = saved();
        let text = refresh_saved(&view, recipe.as_deref(), &sources);
        assert!(!text.chars().any(char::is_control), "{text:?}");
        for part in ["`deps`", "run cargo tree and summarise", "file Cargo.lock", "url https://crates.io"] {
            assert!(text.contains(part), "{part}: {text}");
        }
        assert!(text.contains("argus-hook view save --stdin"), "{text}");
    }

    #[test]
    fn a_saved_views_refresh_strips_control_characters_from_every_part() {
        let (mut view, _, _) = saved();
        view.id = "d\u{1b}x".into();
        view.title = "a\nb".into();
        let sources = vec![ViewSource::Note { text: "e\rf".into() }];
        let text = refresh_saved(&view, Some("c\u{7}d"), &sources);
        assert!(!text.chars().any(char::is_control), "{text:?}");
    }

    /// Every field that reaches a line, hostile at once: none of the three
    /// lines lets a control character through from any of them.
    #[test]
    fn no_line_an_agent_is_typed_holds_a_control_character() {
        let hostile = |what: &str| format!("{what}\n\r\u{1b}[2J\u{7}end");
        let view = AgentView {
            id: hostile("id"),
            title: hostile("title"),
            subject: Some(hostile("https://example.com/")),
            ..Default::default()
        };
        let act = ViewAction { id: hostile("act"), label: hostile("label"), tone: Default::default(), confirm: None };
        let p = ViewPress {
            view: hostile("view"),
            action: hostile("act"),
            rows: vec![hostile("row"), hostile("row2")],
            form: Some(hostile("form")),
            answers: vec![
                answer(&hostile("name"), AnswerValue::Text(hostile("text"))),
                answer(&hostile("n"), AnswerValue::Number(1.0)),
                answer(&hostile("b"), AnswerValue::Bool(true)),
            ],
        };
        let sources = vec![
            ViewSource::File { path: hostile("file") },
            ViewSource::Commit { sha: hostile("sha") },
            ViewSource::Url { url: hostile("url") },
            ViewSource::Note { text: hostile("note") },
            ViewSource::Unknown,
        ];
        let recipe = hostile("recipe");
        let lines = [press(&view, &act, &p), refresh_shown(&view), refresh_saved(&view, Some(&recipe), &sources)];
        for line in lines {
            assert!(!line.chars().any(char::is_control), "{line:?}");
            assert!(line.contains("end"), "the text around each control character is kept: {line}");
        }
    }
}
