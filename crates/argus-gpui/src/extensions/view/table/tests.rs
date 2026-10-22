use argus_desktop_core::connection::page::Event;
use argus_desktop_core::extensions::views::ExtTable;

use super::*;
use crate::extensions::view::tests::scene;

/// The containers as the docker scene's session sends them: six columns, the third a state that only
/// repeats each row's status.
fn containers() -> ExtTable {
    scene("extension-stage")
        .into_iter()
        .find_map(|event| match event {
            Event::Extensions(page) => page.tables.into_iter().next(),
            _ => None,
        })
        .expect("the scene sends the view on screen")
}

#[test]
fn the_status_leads_and_the_column_that_only_repeats_it_is_gone() {
    let view = containers();
    let plan = Plan::of(&view, None, 0.0);
    let titles: Vec<&str> = plan.shaped.columns.iter().map(|column| column.title.as_str()).collect();
    assert_eq!(titles, ["Status", "Name", "Image", "Ports", "CPU", "Memory"]);
}

#[test]
fn an_unmeasured_box_drops_nothing_and_a_narrow_one_gives_up_the_last_columns_first() {
    let shown = |across| Plan::of(&containers(), None, across).visible;
    assert_eq!(shown(0.0), [true; 6], "not yet measured");
    assert_eq!(shown(800.0), [true; 6]);
    assert_eq!(shown(677.0), [true, true, true, true, true, false], "the last at 700 and under");
    assert_eq!(shown(600.0), [true, true, true, true, false, false]);
    assert_eq!(shown(500.0), [true, true, true, false, false, false]);
    assert_eq!(shown(450.0), [true, true, false, false, false, false], "the status and the first view column stay");
}

#[test]
fn a_header_press_orders_the_rows_by_its_column_and_none_leaves_the_order_given() {
    let view = containers();
    assert_eq!(Plan::of(&view, None, 0.0).order, (0..view.rows.len()).collect::<Vec<_>>());
    let by_name = Plan::of(&view, Some((1, true)), 0.0).order;
    let names: Vec<&str> = by_name.iter().map(|&r| view.rows[r].key.as_str()).collect();
    assert_eq!(names, ["api-web", "api-worker", "jaeger", "minio", "postgres", "redis"]);
    let down = Plan::of(&view, Some((1, false)), 0.0).order;
    assert_eq!(down.first().map(|&r| view.rows[r].key.as_str()), Some("redis"));
}
