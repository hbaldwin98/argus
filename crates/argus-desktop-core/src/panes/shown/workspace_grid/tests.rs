//! The grid's rules: which pane or panel joins and where, what a refusal says, how cells go and
//! number down, and the page the rail carries.

use super::*;

fn names(out: &[ClientMsg]) -> Vec<String> {
    out.iter()
        .map(|m| match m {
            ClientMsg::Subscribe { pane } => format!("subscribe {pane}"),
            ClientMsg::Unsubscribe { pane } => format!("unsubscribe {pane}"),
            ClientMsg::Resize { pane, rows, cols } => format!("resize {pane} {rows}x{cols}"),
            other => format!("{other:?}"),
        })
        .collect()
}

/// Pane 1 in the Workspace's cell with `room`, and nothing beside it.
fn workspace(room: (u16, u16)) -> WorkspaceGrid {
    let mut grid = WorkspaceGrid::default();
    let cell = grid.cell_mut(Side::Workspace).unwrap();
    cell.capacity = Some(room);
    cell.subscribe(PaneId(1), &mut Vec::new());
    grid
}

fn room(grid: &mut WorkspaceGrid, side: Side, room: (u16, u16)) {
    grid.cell_mut(side).unwrap().capacity = Some(room);
}

/// `n` panes, 1 the Workspace's and from 2 on beside it, in a grid the
/// page has not measured.
fn unmeasured(n: u64) -> WorkspaceGrid {
    let mut grid = WorkspaceGrid::default();
    grid.cell_mut(Side::Workspace).unwrap().subscribe(PaneId(1), &mut Vec::new());
    for pane in 2..=n {
        grid.join(PaneId(pane), &mut Vec::new()).unwrap();
    }
    grid
}

fn rows(grid: &WorkspaceGrid) -> Vec<Vec<u8>> {
    grid.page(Side::Workspace, |_| None).grid
}

/// The Workspace's pane 1 and 4, 5 and 6 beside it, each cell's room
/// 20 rows by 30 columns and one more for each cell after the first.
fn four() -> WorkspaceGrid {
    let mut grid = workspace((20, 60));
    for pane in [4, 5, 6] {
        grid.join(PaneId(pane), &mut Vec::new()).unwrap();
    }
    for (n, (side, cell)) in grid.cells_mut().enumerate() {
        assert_eq!(side, self::side(n));
        cell.capacity = Some((20, 30 + n as u16));
        cell.send_size(&mut Vec::new());
    }
    grid
}

#[test]
fn a_pane_joins_by_the_grid_rule_at_the_room_of_the_cell_before_it() {
    let mut grid = workspace((20, 60));
    let mut out = Vec::new();
    assert_eq!(grid.join(PaneId(4), &mut out), Ok(Side::Beside(0)));
    assert_eq!(names(&out), ["subscribe 4", "resize 4 20x60"]);
    assert_eq!(rows(&grid), [vec![0, 1]]);
    grid.join(PaneId(5), &mut Vec::new()).unwrap();
    assert_eq!(rows(&grid), [vec![0, 1], vec![2]], "two over one");
    assert_eq!(grid.panes().collect::<Vec<_>>(), [PaneId(1), PaneId(4), PaneId(5)]);
}

#[test]
fn a_pane_already_in_the_row_is_not_split_again() {
    let mut grid = four();
    let mut out = Vec::new();
    assert_eq!(grid.join(PaneId(5), &mut out), Ok(Side::Beside(1)));
    assert!(out.is_empty());
    assert_eq!(grid.sides().len(), 4);
}

#[test]
fn a_grid_refuses_a_pane_that_would_leave_one_smaller_than_the_least() {
    let mut grid = workspace((20, 70));
    assert_eq!(grid.join(PaneId(4), &mut Vec::new()), Ok(Side::Beside(0)), "70 columns make two of 35");
    room(&mut grid, Side::Beside(0), (20, 35));
    assert!(grid.has_room(), "two over one of 20 rows makes 10 each");
    room(&mut grid, Side::Workspace, (20, 20));
    room(&mut grid, Side::Beside(0), (20, 19));
    let mut out = Vec::new();
    assert_eq!(grid.join(PaneId(5), &mut out), Err(Refusal::NoRoom), "39 columns cannot make two of 20 over a third");
    room(&mut grid, Side::Workspace, (11, 40));
    room(&mut grid, Side::Beside(0), (11, 40));
    assert_eq!(grid.join(PaneId(5), &mut out), Err(Refusal::NoRoom), "11 rows cannot make two of 6");
    assert!(out.is_empty(), "nothing subscribed");
    assert_eq!(grid.sides(), [Side::Workspace, Side::Beside(0)]);
}

#[test]
fn nothing_splits_off_a_workspace_with_no_pane() {
    let mut grid = WorkspaceGrid::default();
    let mut out = Vec::new();
    assert_eq!(grid.join(PaneId(4), &mut out), Err(Refusal::NoWorkspacePane));
    assert!(out.is_empty());
    assert_eq!(grid.sides(), [Side::Workspace]);
}

#[test]
fn the_grids_page_reads_as_the_page_expects() {
    let page = unmeasured(2).page(Side::Workspace, |_| None);
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        serde_json::json!({
            "beside": [null],
            "panels": [null],
            "grid": [[0, 1]],
            "heights": [1000],
            "widths": [[500, 500]],
            "keyed": 0,
            "minCols": 20,
            "minRows": 6,
        })
    );
}

#[test]
fn the_page_sends_sizes_in_thousandths() {
    let page = unmeasured(5).page(Side::Workspace, |_| None);
    assert_eq!(page.grid, [vec![0, 1, 4], vec![2, 3]]);
    assert_eq!((page.widths[0].clone(), page.heights), (vec![333, 333, 333], vec![500, 500]));
}

#[test]
fn the_page_names_the_cell_with_the_keys_as_the_grid_numbers_it() {
    let grid = unmeasured(3);
    assert_eq!(grid.page(Side::Workspace, |_| None).keyed, Some(0));
    assert_eq!(grid.page(Side::Beside(1), |_| None).keyed, Some(2));
}

#[test]
fn no_cell_is_keyed_while_the_keys_are_off_the_grid() {
    let grid = unmeasured(3);
    for keys in [Side::First, Side::Second, Side::View(0)] {
        assert_eq!(grid.page(keys, |_| None).keyed, None, "{keys:?}");
    }
}

#[test]
fn a_split_the_tree_no_longer_names_keeps_its_place_without_a_header() {
    let grid = unmeasured(3);
    let describe = |pane: PaneId| {
        let shown = Shown {
            pane: pane.0,
            repository: String::new(),
            checkout: String::new(),
            branch: String::new(),
            agent: false,
            browser: false,
            title: format!("pane {pane}"),
            status: "idle",
            resumed: None,
        };
        (pane != PaneId(2)).then_some(shown)
    };
    let page = grid.page(Side::Workspace, describe);
    let titles: Vec<Option<String>> = page.beside.iter().map(|b| b.as_ref().map(|s| s.title.clone())).collect();
    assert_eq!(titles, [None, Some("pane 3".to_string())], "pane 3's header stays on its own cell");
    assert_eq!(page.beside.len(), grid.sides().len() - 1);
}

#[test]
fn dragged_sizes_that_do_not_fit_the_grid_change_nothing() {
    let mut grid = unmeasured(4);
    let page = |grid: &WorkspaceGrid| grid.page(Side::Workspace, |_| None);
    assert!(grid.share(&[250, 750], &[vec![750, 250], vec![500, 500]]));
    assert_eq!((page(&grid).heights, page(&grid).widths[0].clone()), (vec![250, 750], vec![750, 250]));
    assert!(grid.share(&[1, 3], &[vec![1, 1], vec![1, 1]]), "normalised");
    assert_eq!(page(&grid).widths[0], [500, 500]);
    assert!(!grid.share(&[1000, 0], &[vec![500, 500], vec![500, 500]]), "a zero share");
    assert!(!grid.share(&[1000], &[vec![1000]]), "the wrong shape");
    assert_eq!(page(&grid).heights, [250, 750]);
    grid.share(&[250, 750], &[vec![750, 250], vec![500, 500]]);
    grid.even_out(Some(0));
    assert_eq!(page(&grid).widths[0], [500, 500]);
    grid.even_out(None);
    assert_eq!(page(&grid).heights, [500, 500]);
}

#[test]
fn a_refusal_says_why_in_the_words_the_note_shows() {
    assert!(Refusal::NoWorkspacePane.to_string().contains("in the Workspace first"));
    assert!(Refusal::NoRoom.to_string().contains("20 columns by 6 rows"));
    assert!(Refusal::OnConsole.to_string().contains("open in the console"));
    assert!(Refusal::Full.to_string().contains("close one first"));
}

#[test]
fn the_grid_refuses_a_pane_past_the_most_its_page_can_number() {
    let mut grid = unmeasured(1);
    for pane in 2..=256 {
        assert!(grid.join(PaneId(pane), &mut Vec::new()).is_ok(), "pane {pane}");
    }
    let mut out = Vec::new();
    assert_eq!(grid.join(PaneId(257), &mut out), Err(Refusal::Full));
    assert!(out.is_empty());
    let sides = grid.sides();
    let distinct: std::collections::HashSet<_> = sides.iter().map(|side| format!("{side:?}")).collect();
    assert_eq!((sides.len(), distinct.len()), (256, 256), "no side's number repeats");
    let cells: std::collections::HashSet<u8> = rows(&grid).into_iter().flatten().collect();
    assert_eq!(cells.len(), 256, "nor does a cell's on the page");
}

#[test]
fn an_emptied_cell_goes_and_the_cells_after_it_number_down_with_the_keys() {
    let mut grid = four();
    let mut keys = Side::Beside(2);
    grid.cell_mut(Side::Beside(1)).unwrap().clear(true, &mut Vec::new());
    let mut out = Vec::new();
    grid.close_empty(&mut keys, &mut out);
    assert_eq!(keys, Side::Beside(1), "the keys follow pane 6");
    assert_eq!(grid.cell(Side::Beside(1)).and_then(|cell| cell.pane), Some(PaneId(6)));
    assert_eq!(grid.cell(Side::Beside(1)).and_then(|cell| cell.capacity), Some((20, 32)), "the room stays with the number");
    assert_eq!(names(&out), ["resize 6 20x32"]);
    assert_eq!(rows(&grid), [vec![0, 1], vec![2]]);

    grid.cell_mut(Side::Beside(1)).unwrap().clear(true, &mut Vec::new());
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!(keys, Side::Beside(0), "keys on the cell that went go to its left");
    assert_eq!(grid.sides(), [Side::Workspace, Side::Beside(0)]);
}

fn browsers() -> Occupant {
    Occupant::Panel(Panel::Browsers)
}

#[test]
fn a_panel_splits_off_by_the_grid_rule_with_nothing_subscribed_and_the_page_names_it() {
    let mut grid = workspace((20, 60));
    assert_eq!(grid.join_panel(&Panel::Browsers), Ok(Side::Beside(0)));
    assert_eq!(grid.join_panel(&Panel::Browsers), Ok(Side::Beside(0)), "not split again");
    grid.join(PaneId(4), &mut Vec::new()).unwrap();
    let page = grid.page(Side::Workspace, |_| None);
    assert_eq!((page.grid, page.panels), (vec![vec![0, 1], vec![2]], vec![Some(Panel::Browsers), None]));
    assert_eq!(grid.panes().collect::<Vec<_>>(), [PaneId(1), PaneId(4)], "a panel is no pane");
    assert!(grid.place(&browsers(), &Occupant::Pane(4), Zone::Left));
    assert_eq!(rows(&grid), [vec![0], vec![1, 2]]);
}

#[test]
fn a_panel_needs_a_workspace_pane_to_split_from_and_room() {
    assert_eq!(WorkspaceGrid::default().join_panel(&Panel::Browsers), Err(Refusal::NoWorkspacePane));
    let mut grid = workspace((6, 20));
    assert_eq!(grid.join_panel(&Panel::Browsers), Err(Refusal::NoRoom));
    assert!(!grid.holds_panel(&Panel::Browsers));
}

#[test]
fn a_panel_never_swaps_into_the_workspaces_own_cell() {
    let mut grid = unmeasured(2);
    grid.join_panel(&Panel::Browsers).unwrap();
    assert!(!grid.place(&browsers(), &Occupant::Pane(1), Zone::Centre));
    assert!(!grid.place(&Occupant::Pane(1), &browsers(), Zone::Centre));
    assert!(grid.place(&browsers(), &Occupant::Pane(2), Zone::Centre), "a split's it may");
}

#[test]
fn the_workspaces_pane_going_hands_its_number_to_the_next_pane_and_the_panel_keeps_its_place() {
    let mut grid = workspace((20, 60));
    grid.join_panel(&Panel::Browsers).unwrap();
    grid.join(PaneId(4), &mut Vec::new()).unwrap();
    let mut keys = Side::Beside(1);
    grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!(grid.side_of(PaneId(4)), Some(Side::Workspace));
    assert_eq!(keys, Side::Workspace, "the keys follow pane 4");
    let page = grid.page(keys, |_| None);
    assert_eq!(page.panels, [Some(Panel::Browsers)]);
    assert_eq!(page.grid, [vec![1], vec![0]], "pane 4 is still drawn under the panel");
}

#[test]
fn with_no_pane_left_a_panel_leaves_the_grid() {
    let mut grid = workspace((20, 60));
    grid.join_panel(&Panel::Browsers).unwrap();
    let mut keys = Side::Workspace;
    grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!((grid.sides(), grid.holds_panel(&Panel::Browsers)), (vec![Side::Workspace], false));
}

#[test]
fn a_released_panel_goes_and_the_cells_after_it_number_down() {
    let mut grid = unmeasured(2);
    grid.join_panel(&Panel::Browsers).unwrap();
    grid.join(PaneId(3), &mut Vec::new()).unwrap();
    assert!(grid.release(&Panel::Browsers));
    assert!(!grid.release(&Panel::Browsers), "it has no cell now");
    let mut keys = Side::Beside(2);
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!((grid.side_of(PaneId(3)), keys), (Some(Side::Beside(1)), Side::Beside(1)));
    assert!(grid.page(keys, |_| None).panels.iter().all(Option::is_none));
}

#[test]
fn a_drop_names_a_pane_by_its_number_and_a_panel_by_its_name() {
    let named: Vec<Occupant> = serde_json::from_value(serde_json::json!([7, "browsers"])).unwrap();
    assert_eq!(named, [Occupant::Pane(7), browsers()]);
}

#[test]
fn the_workspaces_cell_alone_stays_when_emptied() {
    let mut grid = workspace((20, 60));
    grid.join(PaneId(4), &mut Vec::new()).unwrap();
    let mut keys = Side::Workspace;
    grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!((grid.side_of(PaneId(4)), keys), (Some(Side::Workspace), Side::Workspace), "the split takes its place");
    grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
    grid.close_empty(&mut keys, &mut Vec::new());
    assert_eq!(grid.sides(), [Side::Workspace]);
    assert_eq!(grid.panes().count(), 0);
}
