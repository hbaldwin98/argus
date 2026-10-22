use super::*;
use crate::extensions::views::{ExtConsoleEnd, ExtTableConsole};
use crate::layout::docks::Panel;

fn action(id: &str, key: Option<&str>, view_wide: bool) -> ExtTableAction {
    ExtTableAction { id: id.into(), label: id.to_uppercase(), key: key.map(str::to_owned), view_wide }
}

fn view(actions: Vec<ExtTableAction>) -> ExtTable {
    ExtTable {
        panel: Panel::extension("docker", "containers"),
        extension: "docker".into(),
        view: "containers".into(),
        title: "Containers".into(),
        page: false,
        columns: Vec::new(),
        rows: Vec::new(),
        actions,
        selected: None,
        default: None,
        console: None,
    }
}

#[test]
fn a_row_shows_its_actions_while_selected_or_pointed_at() {
    assert!(shows("a", Some("a"), None));
    assert!(shows("a", None, Some("a")));
    assert!(!shows("a", Some("b"), Some("c")));
    assert!(!shows("a", None, None));
}

#[test]
fn the_actions_are_one_button_at_560_and_under_and_text_above_it() {
    assert!(is_menu(560.0));
    assert!(!is_menu(561.0));
    assert!(!is_menu(0.0), "a view not yet measured has the text buttons");
}

#[test]
fn per_row_and_view_wide_actions_are_told_apart_and_only_keyed_rows_are_listed_with_keys() {
    let v = view(vec![action("logs", Some("l"), false), action("shell", None, false), action("prune", Some("p"), true)]);
    assert_eq!(per_row(&v).len(), 2);
    assert_eq!(keyed(&v).iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["logs"]);
    assert_eq!(wide(&v, true).iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["prune"]);
}

#[test]
fn an_ended_console_keeps_the_view_wide_actions_for_its_own_strip() {
    let mut v = view(vec![action("again", None, true)]);
    v.console = Some(ExtTableConsole { ended: Some(ExtConsoleEnd { code: Some(1) }), ..ExtTableConsole::default() });
    assert!(wide(&v, true).is_empty());
    assert_eq!(wide(&v, false).len(), 1, "the strip itself lists them");
}

#[test]
fn a_keyed_action_is_refused_while_no_row_is_selected() {
    let mut v = view(Vec::new());
    assert_eq!(refused(&v), Some("no row selected"));
    v.selected = Some("a".into());
    assert_eq!(refused(&v), None);
}

#[test]
fn only_an_action_with_a_key_carries_it_in_its_tooltip() {
    assert_eq!(tooltip(&action("logs", Some("l"), false)).as_deref(), Some("LOGS (l)"));
    assert_eq!(tooltip(&action("shell", None, false)), None);
}
