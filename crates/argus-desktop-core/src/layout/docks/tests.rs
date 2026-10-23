use super::*;

fn images() -> Panel {
    Panel::extension("docker", "images")
}

fn docks() -> Docks {
    let mut docks = Docks::new(DocksLayout::default());
    docks.set_present(vec![Panel::Console, Panel::Views, images()]);
    docks
}

#[test]
fn a_panel_starts_hidden_in_its_home_dock() {
    let docks = docks();
    assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom));
    assert_eq!(docks.dock_of(&Panel::Views), Some(Dock::Right));
    assert!(!docks.shows(&Panel::Console));
    assert_eq!(docks.place_of(&images()), None, "an extension's view waits to be placed");
}

#[test]
fn hiding_a_panel_and_showing_it_again_finds_it_where_it_was_put() {
    let mut docks = docks();
    docks.move_to(&Panel::Console, Dock::Right);
    assert!(docks.shows(&Panel::Console));
    assert_eq!(docks.layout().bottom.tabs, [], "it left the bottom");
    docks.hide(&Panel::Console);
    assert!(!docks.shows(&Panel::Console));
    docks.show(&Panel::Console);
    assert_eq!((docks.dock_of(&Panel::Console), docks.shows(&Panel::Console)), (Some(Dock::Right), true));
}

#[test]
fn panels_in_one_dock_are_tabs_and_one_shows_at_a_time() {
    let mut docks = docks();
    docks.move_to(&Panel::Console, Dock::Right);
    docks.show(&Panel::Views);
    assert!(docks.shows(&Panel::Views));
    assert!(!docks.shows(&Panel::Console), "the other tab");
    docks.hide(&Panel::Console);
    assert!(docks.shows(&Panel::Views), "hiding a tab not shown leaves the dock alone");
    let page = docks.page(|p| p.to_string());
    assert_eq!(page.right.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["views", "console"]);
    assert_eq!(page.right.shown, Some(Panel::Views));
}

#[test]
fn the_dock_a_panel_leaves_shows_the_tab_before_it_or_closes_once_empty() {
    let mut docks = docks();
    docks.move_to(&Panel::Console, Dock::Right);
    docks.move_to(&Panel::Console, Dock::Left);
    assert!(docks.shows(&Panel::Views), "the right dock shows the tab before it");
    docks.move_to(&Panel::Views, Dock::Left);
    assert!(!docks.layout().right.open, "nothing left in it");
    assert_eq!(docks.layout().right.active, None);
}

#[test]
fn an_extensions_view_is_placed_once_where_it_asks_and_then_where_the_person_puts_it() {
    let mut docks = docks();
    docks.place(&images(), Place::Dock(Dock::Bottom));
    assert_eq!(docks.place_of(&images()), Some(Place::Dock(Dock::Bottom)));
    assert!(!docks.shows(&images()), "placed, not shown");
    docks.stage(&images());
    docks.place(&images(), Place::Dock(Dock::Bottom));
    assert_eq!(docks.place_of(&images()), Some(Place::Stage), "the person's choice stands");
    docks.move_to(&images(), Dock::Left);
    assert_eq!(docks.layout().staged, [], "a page docked is a page no more");
    docks.stage(&Panel::Console);
    assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom), "the console is never a page");
}

#[test]
fn a_dock_shows_only_panels_that_exist_and_keeps_the_place_of_one_gone() {
    let mut docks = docks();
    docks.place(&images(), Place::Dock(Dock::Bottom));
    docks.show(&images());
    docks.set_present(vec![Panel::Console]);
    assert!(!docks.shows(&images()));
    assert!(docks.shows(&Panel::Console), "the tab still there shows");
    assert_eq!(docks.page(|p| p.to_string()).bottom.tabs.len(), 1);
    docks.set_present(vec![Panel::Console, images()]);
    assert!(docks.shows(&images()), "back where it was");
    docks.set_present(vec![]);
    assert_eq!(docks.page(|p| p.to_string()).bottom.shown, None, "a dock of nothing is closed on screen");
}

#[test]
fn a_layout_is_saved_once_per_change_and_sizes_are_held_to_their_bounds() {
    let mut docks = docks();
    docks.set_size(Dock::Bottom, 10);
    docks.set_size(Dock::Right, 10_000);
    assert_eq!(docks.layout().bottom.size, BOTTOM.min);
    assert_eq!(docks.layout().right.size, SIDE.max);
    assert!(docks.take_unsaved());
    assert!(!docks.take_unsaved());
    docks.set_size(Dock::Right, SIDE.max);
    docks.hide(&Panel::Console);
    docks.set_present(vec![]);
    assert!(!docks.take_unsaved(), "the same size, a panel hidden already, and what exists are not kept");
}

fn plan() -> Panel {
    Panel::Saved("release-plan".into())
}

#[test]
fn a_saved_views_panel_is_placed_once_docked_and_never_a_page() {
    let mut docks = docks();
    docks.set_present(vec![Panel::Console, plan()]);
    assert_eq!(docks.place_of(&plan()), None, "a saved view waits to be opened");
    docks.place(&plan(), Place::Dock(Dock::Right));
    docks.show(&plan());
    assert!(docks.shows(&plan()));
    docks.stage(&plan());
    assert_eq!(docks.place_of(&plan()), Some(Place::Dock(Dock::Right)), "it has no page");
    docks.forget(&plan());
    assert_eq!(docks.place_of(&plan()), None);
    assert_eq!(docks.layout().right.active, Some(Panel::Views), "the dock shows the tab before it");
}

#[test]
fn a_saved_views_panel_is_kept_with_its_place_in_the_file_and_one_with_no_id_is_refused() {
    let mut docks = docks();
    docks.place(&plan(), Place::Dock(Dock::Left));
    let json = serde_json::to_value(docks.layout()).unwrap();
    assert_eq!(json["left"]["tabs"], serde_json::json!(["saved:release-plan"]));
    let again = Docks::new(serde_json::from_value(json).unwrap());
    assert_eq!(again.dock_of(&plan()), Some(Dock::Left), "across runs");
    assert!(serde_json::from_str::<Panel>(r#""saved:""#).is_err());
    let staged = Docks::new(DocksLayout { staged: vec![plan()], ..DocksLayout::default() });
    assert_eq!(staged.place_of(&plan()), None, "a hand-edited page of one is dropped");
}

#[test]
fn a_hand_edited_layout_is_made_whole() {
    let docks = Docks::new(DocksLayout {
        left: DockLayout { tabs: vec![Panel::Console], active: None, open: true, size: 1 },
        right: DockLayout { tabs: vec![Panel::Console], active: Some(Panel::Console), open: true, size: 600 },
        bottom: DockLayout { open: true, ..bottom() },
        staged: vec![Panel::Views, images(), images()],
        floating: vec![
            Floating { panel: plan(), bounds: FloatBounds { x: 0, y: 0, width: 1, height: 1 } },
            Floating { panel: images(), bounds: FLOAT_FIRST },
        ],
        origins: Vec::new(),
    });
    assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Left), "the first dock keeps it");
    assert!(docks.shows(&Panel::Console), "its only tab is the one shown");
    assert_eq!(docks.layout().left.size, SIDE.min);
    assert_eq!(docks.layout().right.tabs, [Panel::Views], "the views are never a page, and go home");
    assert!(!docks.layout().bottom.open, "nothing in it");
    assert_eq!(docks.layout().staged, [images()]);
    assert_eq!(docks.place_of(&plan()), Some(Place::Float));
    assert_eq!(docks.layout().floating.len(), 1, "an extension's view placed twice, and not one that floats");
    assert_eq!(docks.layout().floating[0].bounds, FloatBounds { x: 0, y: 0, width: FLOAT_MIN.0, height: FLOAT_MIN.1 });
}

#[test]
fn a_saved_view_floats_as_a_dialog_each_one_stepped_from_the_last_and_the_one_shown_in_front() {
    let mut docks = docks();
    let other = Panel::Saved("other".into());
    docks.set_present(vec![Panel::Console, Panel::Views, plan(), other.clone()]);
    docks.place(&plan(), Place::Dock(Dock::Right));
    docks.show(&plan());
    docks.float(&plan());
    assert_eq!(docks.place_of(&plan()), Some(Place::Float));
    assert_eq!(docks.layout().right.tabs, [Panel::Views], "it left its dock");
    assert!(docks.shows(&plan()), "a dialog of a view that exists is on screen");
    docks.place(&other, Place::Float);
    let at = |d: &Docks| d.page(|p| p.to_string()).floating.iter().map(|f| (f.panel.clone(), f.bounds.x)).collect::<Vec<_>>();
    assert_eq!(at(&docks), [(plan(), FLOAT_FIRST.x), (other.clone(), FLOAT_FIRST.x + FLOAT_STEP)]);
    docks.show(&plan());
    assert_eq!(at(&docks)[1].0, plan(), "shown, it comes in front");
    docks.set_bounds(&plan(), FloatBounds { x: -5, y: 40, width: 10, height: 900 });
    assert_eq!(docks.layout().floating[1].bounds, FloatBounds { x: -5, y: 40, width: FLOAT_MIN.0, height: 900 });
    docks.move_to(&plan(), Dock::Left);
    assert_eq!((docks.dock_of(&plan()), docks.layout().floating.len()), (Some(Dock::Left), 1), "docked, a dialog no more");
    docks.float(&Panel::Console);
    assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom), "the console never floats");
    docks.forget(&other);
    assert!(docks.layout().floating.is_empty());
}

#[test]
fn a_panel_is_named_as_one_string_in_the_file() {
    let mut docks = docks();
    docks.place(&images(), Place::Dock(Dock::Right));
    docks.show(&images());
    let json = serde_json::to_value(docks.layout()).unwrap();
    assert_eq!(json["right"]["tabs"], serde_json::json!(["views", "extension:docker/images"]));
    assert_eq!(json["right"]["active"], "extension:docker/images");
    assert_eq!(serde_json::from_value::<DocksLayout>(json).unwrap(), *docks.layout());
    assert_eq!(serde_json::from_str::<DocksLayout>("{}").unwrap(), DocksLayout::default());
    assert!(serde_json::from_str::<Panel>(r#""extension:docker""#).is_err(), "a view needs its name");
    assert_eq!(images().extension_view(), Some(("docker", "images")));
}

fn abilities(page: &DocksPage) -> Vec<(Panel, PanelKind, bool, bool, bool)> {
    page.present.iter().map(|p| (p.panel.clone(), p.kind, p.floats, p.stage, p.closes)).collect()
}

#[test]
fn every_panel_that_exists_is_told_with_what_it_can_do() {
    let mut docks = docks();
    docks.set_present(vec![Panel::Console, Panel::Views, images(), plan()]);
    assert_eq!(
        abilities(&docks.page(|p| p.to_string())),
        [
            (Panel::Console, PanelKind::Console, false, true, false),
            (Panel::Views, PanelKind::Views, false, false, false),
            (images(), PanelKind::Extension, false, true, false),
            (plan(), PanelKind::Saved, true, false, true),
        ]
    );
    docks.set_present(vec![Panel::Console]);
    assert_eq!(abilities(&docks.page(|p| p.to_string())), [(Panel::Console, PanelKind::Console, false, true, false)]);
}

#[test]
fn a_panel_no_dock_lists_is_still_told_what_it_can_do() {
    let mut docks = docks();
    let staged = Panel::extension("docker", "containers");
    docks.set_present(vec![Panel::Console, images(), staged.clone()]);
    docks.place(&staged, Place::Stage);
    assert_eq!(docks.place_of(&images()), None, "never placed");
    assert_eq!(docks.place_of(&staged), Some(Place::Stage));
    let page = docks.page(|p| p.to_string());
    let mut tabs = [&page.left, &page.right, &page.bottom].into_iter().flat_map(|d| &d.tabs);
    assert!(!tabs.any(|t| t.panel == images() || t.panel == staged), "in no dock");
    assert!(page.floating.is_empty(), "and no dialog");
    let told = page.present.iter().map(|p| &p.panel).collect::<Vec<_>>();
    assert!(told.contains(&&images()) && told.contains(&&staged), "a sidebar drag of either finds what it can do");
}

/// The page draws nothing for a panel it was not told of, so a panel
/// shown and missing here would be a dock left silently empty.
#[test]
fn every_panel_a_dock_or_dialog_shows_is_among_those_told() {
    let mut docks = docks();
    let other = Panel::Saved("other".into());
    docks.set_present(vec![Panel::Console, Panel::Views, images(), plan(), other.clone()]);
    docks.place(&images(), Place::Dock(Dock::Left));
    docks.show(&images());
    docks.move_to(&Panel::Console, Dock::Right);
    docks.place(&other, Place::Dock(Dock::Bottom));
    docks.show(&other);
    docks.float(&plan());
    let page = docks.page(|p| p.to_string());
    let told = page.present.iter().map(|p| &p.panel).collect::<Vec<_>>();
    let docked = [&page.left, &page.right, &page.bottom].into_iter().flat_map(|d| d.tabs.iter().map(|t| &t.panel).chain(&d.shown));
    let shown = docked.chain(page.floating.iter().map(|f| &f.panel)).collect::<Vec<_>>();
    assert_eq!(shown.len(), 8, "four tabs, three of them shown, and a dialog");
    for panel in shown {
        assert!(told.contains(&panel), "{panel} is drawn but not told");
    }
    let mut once = told.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), told.len(), "each one told once");
}

#[test]
fn a_panel_floats_exactly_when_the_page_is_told_it_does() {
    for panel in [Panel::Console, Panel::Views, images(), plan()] {
        let mut docks = docks();
        docks.set_present(vec![Panel::Console, Panel::Views, images(), plan()]);
        docks.float(&panel);
        assert_eq!(docks.place_of(&panel) == Some(Place::Float), panel.told().floats, "{panel} floats");
        docks.stage(&panel);
        if docks.place_of(&panel) == Some(Place::Stage) {
            assert!(panel.told().stage, "{panel} has a page, so a drop on the stage takes it");
        }
    }
}

#[test]
fn a_panel_is_told_to_the_page_as_its_kind_and_abilities() {
    assert_eq!(
        serde_json::to_value(plan().told()).unwrap(),
        serde_json::json!({"panel": "saved:release-plan", "kind": "saved", "floats": true, "stage": false, "closes": true, "cell": false})
    );
    assert_eq!(serde_json::to_value(PanelKind::Extension).unwrap(), "extension");
}

/// What a panel can do is told the page (`PresentPanel`), which asks
/// `layout/panels.ts`; a name's prefix read or built again on the page
/// is the rule written twice. The modules that handle every panel may
/// not compare one with a fixed name either.
#[test]
fn the_page_never_reads_what_a_panel_can_do_from_its_name() {
    use std::path::{Path, PathBuf};
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../argus-desktop/page/src");
    let prefixes = ["'saved:", "\"saved:", "`saved:", "'extension:", "\"extension:", "`extension:"];
    let names = ["=== 'console'", "!== 'console'", "=== 'views'", "!== 'views'"];
    // Every folder but `bindings`, whose types are Rust's, each module
    // named by its path under `src` with `/` on every system.
    fn walk(dir: &Path, src: &Path, out: &mut Vec<(String, PathBuf)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if entry.file_name() != "bindings" {
                    walk(&path, src, out);
                }
            } else if path.extension().is_some_and(|e| e == "ts") {
                let name = path.strip_prefix(src).unwrap().to_string_lossy().replace('\\', "/");
                out.push((name, path));
            }
        }
    }
    let mut modules = Vec::new();
    walk(Path::new(dir), Path::new(dir), &mut modules);
    let mut read = 0;
    for (name, path) in modules {
        read += 1;
        let every_panel = ["layout/docks.ts", "layout/drag.ts", "layout/panels.ts"].contains(&name.as_str());
        let source = std::fs::read_to_string(&path).unwrap();
        for (n, line) in source.lines().enumerate() {
            let by_name = prefixes.iter().any(|p| line.contains(p)) || (every_panel && names.iter().any(|c| line.contains(c)));
            assert!(!by_name, "{name}:{} reads a panel's name; ask `layout/panels.ts` for what Rust said instead: {}", n + 1, line.trim());
        }
    }
    assert!(read > 10, "read only {read} page modules from {dir}");
}

#[test]
fn a_dock_gesture_stages_or_closes_a_panel_by_name() {
    let gesture = |json| serde_json::from_value::<DockGesture>(json).unwrap();
    assert_eq!(gesture(serde_json::json!({"on": "stage", "panel": "console"})), DockGesture::Stage { panel: Panel::Console });
    assert_eq!(gesture(serde_json::json!({"on": "close", "panel": "saved:a"})), DockGesture::Close { panel: Panel::Saved("a".into()) });
}
