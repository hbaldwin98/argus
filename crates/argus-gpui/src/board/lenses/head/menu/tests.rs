use argus_desktop_core::connection::page::{Event, FeatureViewLink};
use argus_desktop_core::views::saved::{PageFeature, PageSaved};

use super::*;
use crate::board::snapshots;
use crate::kit::menu::item::lines;

fn saved(feature: u64, number: i64, stale: bool) -> PageSaved {
    let mut view = snapshots::events("saved-view")
        .into_iter()
        .find_map(|event| match event {
            Event::SavedViews(page) => page.views.first().cloned(),
            _ => None,
        })
        .expect("the scene has a saved view");
    view.features = vec![PageFeature { repository: feature, number }];
    view.stale = stale;
    view
}

fn hints(items: &[MenuItem<Does>]) -> Vec<String> {
    items.iter().filter_map(|item| item.as_choice()).map(|choice| format!("{:?}", choice.hint)).collect()
}

// BOARD-HEAD-7: the brief, the tickets and, with a ticket, the reconcile.
#[test]
fn the_menu_offers_the_brief_the_tickets_and_with_a_ticket_the_reconcile() {
    let mut view = snapshots::open("feature-work");
    view.views.clear();
    view.reconcile_entry = Some(argus_desktop_core::board::lenses::head::reconcile_entry(None, None, None));
    let menu = items(&view, &[]);
    assert_eq!(lines(&menu), ["Edit the brief", "Link a ticket\u{2026}", "Reconcile with its tickets\u{2026}"]);
    assert_eq!(hints(&menu)[..2], ["Keys([[\"B\"]])", "Keys([[\"T\"]])"], "the keys beside them");
    let mut none = view;
    none.links.clear();
    none.reconcile_entry = None;
    assert_eq!(lines(&items(&none, &[])), ["Edit the brief", "Link a ticket\u{2026}"], "Reconcile is not offered without a ticket");
}

// BOARD-HEAD-8: what the reconcile waits on is in its label, and its history is its hint.
#[test]
fn the_reconcile_says_what_it_waits_on_and_when_it_last_ran() {
    let mut view = snapshots::open("feature-work");
    view.reconcile = Some("proposed");
    view.reconcile_entry = Some(argus_desktop_core::board::lenses::head::reconcile_entry(Some("proposed"), Some("3d"), Some(11)));
    let menu = items(&view, &[]);
    assert_eq!(menu[2].as_choice().unwrap().label, "Reconcile \u{2014} a proposal waits");
    assert_eq!(hints(&menu)[2], "Text(\"Reconciled 3d ago \u{b7} 11 changes since\")");
    assert_eq!(button_label(&view), "Feature menu: Reconcile, a proposal waits");
    view.reconcile_entry = Some(argus_desktop_core::board::lenses::head::reconcile_entry(Some("asked"), Some("3d"), Some(11)));
    assert_eq!(button_label(&view), "Feature menu", "only a waiting proposal asks for attention");
}

// BOARD-HEAD-10: the views that name the feature, agent views first, then the saved ones.
#[test]
fn the_views_that_name_the_feature_follow_a_rule_agent_views_before_saved_ones() {
    let mut view = snapshots::open("feature-work");
    view.views = vec![FeatureViewLink { pane: 4, id: "plan".into(), title: "Plan".into() }];
    let (fresh, stale) = (saved(10, view.number, false), saved(10, view.number, true));
    let menu = items(&view, &[&fresh, &stale]);
    let rows = lines(&menu);
    assert_eq!(rows[3], "---");
    assert_eq!(rows[4], "Plan");
    assert_eq!(hints(&menu)[3..], ["Text(\"agent view\")", "Text(\"saved view\")", "Text(\"saved, out of date\")"]);
    let choices: Vec<_> = menu.iter().filter_map(|item| item.as_choice()).map(|choice| choice.does.call().map(|call| format!("{call:?}"))).collect();
    assert_eq!(choices[3].as_deref(), Some("OpenView { pane: 4, id: \"plan\" }"));
    assert_eq!(choices[4].as_deref(), Some(format!("OpenSavedView {{ id: {:?}, from: None }}", fresh.id).as_str()));
    assert_eq!(choices[0], None, "the brief is a form, not a call");
}

#[test]
fn only_the_saved_views_naming_this_feature_are_offered() {
    use argus_desktop_core::views::offered::for_feature;
    let (mine, mut other) = (saved(10, 41, false), saved(10, 42, false));
    other.id = "someone-elses".into();
    let views = [mine.clone(), other];
    let offered = for_feature(&views, Some(10), 41);
    assert_eq!(offered.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), [mine.id.as_str()]);
    assert!(for_feature(&views, None, 41).is_empty(), "no repository chosen offers none");
}
