use super::*;
use crate::layout::docks::{DockPage, DockTab, FloatBounds, FloatPage};
use crate::views::saved::{PageSaved, PageScope, SavedPanel};
use argus_protocol::AgentView;

fn view(id: &str, scope: &str, here: bool, under: Option<&str>) -> NavView {
    NavView {
        id: id.into(),
        title: id.into(),
        scope: scope.into(),
        here,
        stale: false,
        under: under.map(str::to_owned),
        panel: Panel::Saved(id.into()),
    }
}

fn ids(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Group(label) => format!("# {label}"),
            Row::View { view, depth, inner, folded, .. } => {
                format!("{}{}{}", "  ".repeat(*depth), view.id, if *inner > 0 { format!(" [{inner}{}]", if *folded { "+" } else { "-" }) } else { String::new() })
            }
            Row::None => "none".into(),
        })
        .collect()
}

fn none() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn views_are_listed_in_groups_by_scope_in_the_order_rust_sent_them() {
    // CHECKOUTS-CODE-VIEWS-SAVED-13..16.
    let views = [view("a", "Global", true, None), view("b", "argus", true, None), view("c", "argus", true, None), view("d", "argus #7", true, None)];
    assert_eq!(ids(&rows(&views, false, &none())), ["# Global", "a", "# argus", "b", "c", "# argus #7", "d"]);
}

#[test]
fn here_keeps_only_the_views_about_where_the_person_is_and_all_keeps_every_one() {
    let views = [view("a", "Global", false, None), view("b", "argus", true, None)];
    assert_eq!(ids(&rows(&views, true, &none())), ["# argus", "b"]);
    assert_eq!(ids(&rows(&views, false, &none())), ["# Global", "a", "# argus", "b"]);
}

#[test]
fn nothing_about_here_says_so_rather_than_drawing_an_empty_list() {
    let views = [view("a", "Global", false, None)];
    assert_eq!(rows(&views, true, &none()), [Row::None]);
    assert_eq!(rows(&[], false, &none()), [Row::None]);
}

#[test]
fn a_view_stands_under_the_one_that_links_to_it_and_is_hidden_until_unfolded() {
    let views = [view("top", "argus", true, None), view("inner", "argus", true, Some("top")), view("deep", "argus", true, Some("inner"))];
    assert_eq!(ids(&rows(&views, false, &none())), ["# argus", "top [2+]"], "folded, the count says how many hide");
    let open: HashSet<String> = ["top".to_owned()].into();
    assert_eq!(ids(&rows(&views, false, &open)), ["# argus", "top [2-]", "  inner [1+]"]);
    let both: HashSet<String> = ["top".to_owned(), "inner".to_owned()].into();
    assert_eq!(ids(&rows(&views, false, &both)), ["# argus", "top [2-]", "  inner [1-]", "    deep"]);
}

#[test]
fn a_view_whose_parent_the_group_does_not_show_stands_at_the_top() {
    // Filtered out by Here, or listed in another scope.
    let views = [view("parent", "argus", false, None), view("child", "argus", true, Some("parent")), view("other", "Global", true, Some("parent"))];
    assert_eq!(ids(&rows(&views, true, &none())), ["# argus", "child", "# Global", "other"]);
}

#[test]
fn views_that_each_name_the_other_as_parent_are_not_drawn_and_do_not_loop() {
    let views = [view("a", "argus", true, Some("b")), view("b", "argus", true, Some("a")), view("c", "argus", true, None)];
    assert_eq!(ids(&rows(&views, false, &none())), ["# argus", "c"]);
}

#[test]
fn a_title_does_not_say_its_group_twice() {
    // The page's `underGroup`.
    assert_eq!(under_group("reliab: the change flow", "reliab"), "the change flow");
    assert_eq!(under_group("Reliab \u{2014} the change flow", "reliab"), "the change flow");
    assert_eq!(under_group("reliab | flow", "reliab"), "flow");
    assert_eq!(under_group("reliab", "reliab"), "reliab", "a title that is only the label is kept whole");
    assert_eq!(under_group("reliab:", "reliab"), "reliab:");
    assert_eq!(under_group("reliability notes", "reliab"), "reliability notes", "no separator, no strip");
    assert_eq!(under_group("flow", "reliab"), "flow");
    assert_eq!(under_group("anything", ""), "anything");
    assert_eq!(under_group("\u{e9}cole: notes", "\u{c9}cole"), "notes", "case is folded beyond ASCII");
}

#[test]
fn a_rows_label_is_its_title_under_its_group() {
    let mut v = view("a", "reliab", true, None);
    v.title = "reliab: the change flow".into();
    let rows = rows(&[v], false, &none());
    assert!(matches!(&rows[1], Row::View { label, .. } if label == "the change flow"));
}

fn page_saved(id: &str) -> PageSaved {
    PageSaved {
        id: id.into(),
        panel: Panel::Saved(id.into()),
        scope: PageScope { rank: 1, label: "argus".into() },
        here: true,
        features: Vec::new(),
        recipe: None,
        sources: Vec::new(),
        built_at: 0,
        built_commit: None,
        stale: true,
        view: AgentView { id: id.into(), title: format!("{id} title"), ..Default::default() },
        diagrams: Vec::new(),
        refreshing: false,
        pressed: Vec::new(),
        under: Some("other".into()),
    }
}

#[test]
fn the_sidebar_reads_what_it_lists_off_the_pages_views() {
    let page = SavedViewsPage { views: vec![page_saved("plan")], panels: Vec::new(), can_press: true };
    let listed = views_of(&page);
    assert_eq!(
        listed,
        [NavView {
            id: "plan".into(),
            title: "plan title".into(),
            scope: "argus".into(),
            here: true,
            stale: true,
            under: Some("other".into()),
            panel: Panel::Saved("plan".into()),
        }]
    );
}

fn dock(shown: Option<Panel>) -> DockPage {
    DockPage { tabs: shown.iter().map(|panel| DockTab { panel: panel.clone(), title: String::new() }).collect(), shown, size: 300 }
}

#[test]
fn a_view_is_lit_while_a_panel_showing_it_is_in_a_dock_or_floating() {
    // SHELL-STAGE-18.
    let page = SavedViewsPage {
        views: Vec::new(),
        panels: vec![
            SavedPanel { panel: Panel::Saved("a".into()), columns: vec!["a".into(), "b".into()] },
            SavedPanel { panel: Panel::Saved("c".into()), columns: vec!["c".into()] },
            SavedPanel { panel: Panel::Saved("d".into()), columns: vec!["d".into()] },
        ],
        can_press: true,
    };
    let docks = DocksPage {
        right: dock(Some(Panel::Saved("a".into()))),
        left: dock(None),
        floating: vec![FloatPage { panel: Panel::Saved("c".into()), title: String::new(), bounds: FloatBounds { x: 0, y: 0, width: 300, height: 200 } }],
        ..DocksPage::default()
    };
    let lit = lit(&page.panels, &docks);
    let mut lit: Vec<_> = lit.into_iter().collect();
    lit.sort();
    assert_eq!(lit, ["a", "b", "c"], "every column of a shown panel, and a floating one, but not a panel nobody shows");
}
