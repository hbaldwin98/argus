//! The Feature stage: the index, the three lenses over one feature, the
//! note a feature is written as, and what each key sends.

use super::*;
use crate::app::{Lens, Standing};
use argus_protocol::outline::WorkRef;
use argus_protocol::{
    ClientMsg, DecisionChange, DecisionState, FeatureState, MapChange, TaskAction, TaskState,
    TrackerLink,
};

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_in(app: &mut App, text: &str) {
    text.chars().for_each(|c| press(app, c));
}

const SCREEN_WIDTH: u16 = 170;
const SCREEN_HEIGHT: u16 = 44;

fn screen(app: &mut App) -> String {
    lines(&draw_at(app, SCREEN_WIDTH, SCREEN_HEIGHT)).join("\n")
}

/// The screen without the rail and its border. The rail is the same in
/// every dump and is a quarter of each line, which is what pushed five
/// screens past what a tool's output keeps. The status bar runs under the
/// rail, so its row is kept whole.
fn stage(app: &mut App) -> String {
    let rail = usize::from(rail_width(SCREEN_WIDTH, app.settings.rail_width));
    let frame = draw_at(app, SCREEN_WIDTH, SCREEN_HEIGHT);
    let bar = usize::from(bar_row(&frame));

    let rows: Vec<String> = lines(&frame)
        .into_iter()
        .enumerate()
        .map(|(y, line)| if y == bar { line } else { line.chars().skip(rail).collect() })
        .collect();
    rows.join("\n")
}

fn sent(rx: &mut tokio::sync::mpsc::UnboundedReceiver<ClientMsg>) -> Vec<ClientMsg> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

/// The stage inside feature #7, with the messages opening it sent cleared.
fn reading() -> (App, tokio::sync::mpsc::UnboundedReceiver<ClientMsg>) {
    let (mut app, mut rx) = feature_stage();
    key(&mut app, KeyCode::Enter);
    sent(&mut rx);
    (app, rx)
}

fn select(app: &mut App, at: WorkRef) {
    let row = app
        .outline()
        .rows
        .iter()
        .position(|row| row.at == at)
        .expect("the row is in the outline");
    app.work_sel = row;
}

/// Not an assertion — the stage's five screens, to look at while working
/// on it: `cargo test -p argus dump_feature_stage -- --ignored --nocapture`.
/// Only the stage is printed; `dump_frame` shows the rail.
#[test]
#[ignore = "prints each screen of the stage for eyeballing; asserts nothing"]
fn dump_feature_stage() {
    let (mut app, _rx) = feature_stage();
    println!("{}\n", stage(&mut app));
    key(&mut app, KeyCode::Enter);
    select(&mut app, WorkRef::Task(8));
    println!("{}\n", stage(&mut app));
    press(&mut app, '2');
    app.decision_sel = 2;
    println!("{}\n", stage(&mut app));
    press(&mut app, '3');
    press(&mut app, 'j');
    println!("{}\n", stage(&mut app));
    key(&mut app, KeyCode::Esc);
    press(&mut app, 'a');
    type_in(&mut app, "The ratatui bump");
    key(&mut app, KeyCode::Enter);
    type_in(&mut app, "- Bump ratatui");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Tab);
    type_in(&mut app, "- Fix what breaks");
    println!("{}\n", stage(&mut app));
}

// ---- the index -----------------------------------------------------------

#[test]
fn the_index_groups_features_by_how_they_stand() {
    let (mut app, _rx) = feature_stage();

    let text = screen(&mut app);

    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle}:\n{text}"));
    assert!(at("ACTIVE") < at("Trim Argus context injection"));
    assert!(at("Trim Argus context injection") < at("QUEUED"));
    assert!(at("QUEUED") < at("The ratatui 0.30 bump"));
    assert!(at("The ratatui 0.30 bump") < at("DONE"));
    assert!(text.contains("Features in orion"), "{text}");
    assert!(text.contains("1 active · 1 queued · 1 done"), "{text}");
}

#[test]
fn a_feature_row_says_its_number_progress_open_questions_branch_and_pane() {
    let (mut app, _rx) = feature_stage();

    // Where and who are the first columns a narrow list gives up.
    let text = lines(&draw_at(&mut app, 230, 44)).join("\n");
    let row = text
        .lines()
        .find(|line| line.contains("Trim Argus context injection") && line.contains("#7"))
        .unwrap_or_else(|| panic!("{text}"));

    assert!(row.contains("4/10"), "{row}");
    assert!(row.contains("1 open"), "{row}");
    assert!(row.contains("master"), "{row}");
    assert!(row.contains("#100"), "{row}");
    assert!(text.contains("after #7"), "what a queued feature waits on:\n{text}");
}

#[test]
fn how_a_feature_stands_is_read_off_its_tasks_its_panes_and_what_it_waits_on() {
    let (app, _rx) = feature_stage();

    let standing: Vec<(i64, Standing)> = app
        .feature_rows()
        .iter()
        .map(|row| (row.number, row.standing))
        .collect();

    assert_eq!(
        standing,
        [(7, Standing::Active), (8, Standing::Queued), (6, Standing::Done)]
    );
}

#[test]
fn the_peek_shows_what_needs_you_what_is_in_flight_and_the_last_decision() {
    let (mut app, _rx) = feature_stage();

    let text = screen(&mut app);

    for landmark in ["PEEK", "NEXT", "NEEDS YOU", "IN FLIGHT", "LAST DECIDED"] {
        assert!(text.contains(landmark), "missing {landmark}:\n{text}");
    }
    assert!(text.contains("D3 How much decision history"), "{text}");
    assert!(text.contains("D2.1 The route lives in the pane API"), "{text}");
}

#[test]
fn held_and_archived_features_are_folded_until_z() {
    let (mut app, _rx) = feature_stage();
    let board = app.board.as_mut().unwrap();
    board.features[0].state = FeatureState::Done;
    board.features[2].held = Some("until the release".into());
    app.rescope_feature();

    assert_eq!(app.feature_rows().len(), 1);
    assert!(screen(&mut app).contains("held and archived features are folded"));

    press(&mut app, 'z');

    let text = screen(&mut app);
    assert_eq!(app.feature_rows().len(), 3);
    assert!(text.contains("HELD") && text.contains("ARCHIVED"), "{text}");
    assert_eq!(app.feature_slug().as_deref(), Some("trim"), "the cursor stays on its feature");
}

#[test]
fn v_archives_the_feature_and_brings_an_archived_one_back() {
    let (mut app, mut rx) = feature_stage();
    sent(&mut rx);

    press(&mut app, 'v');

    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::Feature { action: argus_protocol::FeatureAction::Done { slug, .. }, .. }] if slug == "trim"
    ));
}

#[test]
fn a_repository_with_no_features_says_what_one_is_and_how_to_start_one() {
    let (mut app, _rx) = feature_stage();
    app.board.as_mut().unwrap().features.clear();
    app.rescope_feature();

    let text = screen(&mut app);

    assert!(text.contains("No features in orion yet."), "{text}");
    assert!(text.contains("Start a feature"), "{text}");
    assert!(text.contains("WHAT IT HOLDS"), "{text}");
}

#[test]
fn a_board_for_another_project_is_dropped_rather_than_drawn() {
    let (mut app, _rx) = feature_stage();
    let mut board = feature_board(None);
    board.name = "elsewhere".into();
    board.features.truncate(1);

    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(board)));

    assert_eq!(app.board.as_ref().unwrap().features.len(), 3);
}

#[test]
fn a_board_for_another_repository_of_the_project_is_dropped_too() {
    // Every board of a project carries its name; before boards named their
    // repositories, a write to one replaced the board on screen for another.
    let (mut app, _rx) = feature_stage();
    let ours = app.current_repository().unwrap().id;
    let mut board = feature_board(None);
    board.repositories = vec![argus_protocol::RepositoryId(ours.0 + 1000)];
    board.features.truncate(1);

    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(board.clone())));
    assert_eq!(app.board.as_ref().unwrap().features.len(), 3, "another repository's");

    board.repositories = vec![ours];
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(board)));
    assert_eq!(app.board.as_ref().unwrap().features.len(), 1, "this repository's is taken");
}

// ---- levels ----------------------------------------------------------------

#[test]
fn enter_opens_a_feature_and_escape_climbs_one_level_at_a_time() {
    let (mut app, _rx) = feature_stage();

    key(&mut app, KeyCode::Enter);
    assert!(app.feature_open);
    select(&mut app, WorkRef::Task(8));
    key(&mut app, KeyCode::Tab);
    assert!(app.in_detail);

    key(&mut app, KeyCode::Esc);
    assert!(!app.in_detail && app.feature_open);
    key(&mut app, KeyCode::Esc);
    assert!(!app.feature_open && app.view == View::Feature);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.view, View::Workspace);
}

#[test]
fn the_breadcrumb_names_the_feature_the_lens_and_the_row_under_the_cursor() {
    let (mut app, _rx) = reading();
    select(&mut app, WorkRef::Task(8));

    let text = screen(&mut app);

    assert!(
        text.contains("orion › #7 Trim Argus context injection › work › 2 Serve the brief on demand › 2.2"),
        "{text}"
    );
    assert!(text.contains("esc up one level"), "{text}");
}

#[test]
fn digits_switch_lenses_inside_a_feature_and_views_outside_one() {
    let (mut app, _rx) = reading();

    press(&mut app, '2');
    assert_eq!((app.view, app.lens), (View::Feature, Lens::Record));
    press(&mut app, '3');
    assert_eq!(app.lens, Lens::Flows);

    key(&mut app, KeyCode::Esc);
    press(&mut app, '3');
    assert_eq!(app.view, View::Checkouts);
}

#[test]
fn the_heading_says_how_the_feature_stands_and_how_far_along_it_is() {
    let (mut app, _rx) = reading();

    let text = screen(&mut app);

    assert!(text.contains("◐ ACTIVE · master · pane #100"), "{text}");
    assert!(text.contains("4 of 10 tasks · 4 decided · 1 open"), "{text}");
    assert!(text.contains("1 work  2 record  3 flows"), "{text}");
    assert!(text.contains("The session-start hook prints the whole brief"), "{text}");
}

// ---- tickets -----------------------------------------------------------------

fn link(tracker: &str, key: &str, url: &str) -> TrackerLink {
    TrackerLink {
        tracker: tracker.into(),
        key: key.into(),
        url: url.into(),
    }
}

/// Feature #7 open, linked to a Linear issue and a Jira story.
fn linked() -> (App, tokio::sync::mpsc::UnboundedReceiver<ClientMsg>) {
    let (mut app, rx) = reading();
    let board = app.board.as_mut().unwrap();
    let trim = board.features.iter_mut().find(|f| f.slug == "trim").unwrap();
    trim.links = vec![
        link("Linear", "ENG-88", "https://linear.app/acme/issue/ENG-88/trim"),
        link("Jira", "ARG-142", "https://acme.atlassian.net/browse/ARG-142"),
    ];
    (app, rx)
}

/// Not an assertion — the heading of a feature linked to two tickets, and
/// the picker `t` offers: `cargo test -p argus dump_linked_feature -- --ignored --nocapture`.
#[test]
#[ignore = "prints a linked feature for eyeballing; asserts nothing"]
fn dump_linked_feature() {
    let (mut app, _rx) = linked();
    println!("{}\n", stage(&mut app));
    press(&mut app, 't');
    println!("{}\n", stage(&mut app));
}

#[test]
fn the_heading_names_every_ticket_the_feature_is_linked_to_on_every_lens() {
    let (mut app, _rx) = linked();
    for lens in ['1', '2', '3'] {
        press(&mut app, lens);
        let text = screen(&mut app);
        assert!(text.contains("⇄ Linear ENG-88 · Jira ARG-142"), "lens {lens}: {text}");
    }
}

#[test]
fn t_opens_the_only_ticket_and_offers_a_choice_of_several() {
    let (mut app, _rx) = linked();
    press(&mut app, 't');
    assert!(app.picker.is_some(), "two tickets: which one");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "no browser here; copied https://acme.atlassian.net/browse/ARG-142");

    let board = app.board.as_mut().unwrap();
    let trim = board.features.iter_mut().find(|f| f.slug == "trim").unwrap();
    trim.links.truncate(1);
    press(&mut app, 't');
    assert!(app.picker.is_none(), "one ticket: no question");
    assert_eq!(app.status, "no browser here; copied https://linear.app/acme/issue/ENG-88/trim");
}

#[test]
fn t_on_a_feature_with_no_ticket_says_how_to_link_one() {
    let (mut app, _rx) = reading();
    press(&mut app, 't');
    assert!(app.status.contains("T links one"), "{}", app.status);
}

#[test]
fn a_pasted_ticket_url_links_the_feature_and_is_not_predicted() {
    let (mut app, mut rx) = reading();
    press(&mut app, 'T');
    app.on_paste("https://linear.app/acme/issue/ENG-88/trim".into());
    assert!(screen(&mut app).contains("ticket URL https://linear.app/acme/issue/ENG-88/trim"));
    key(&mut app, KeyCode::Enter);

    let links: Vec<_> = sent(&mut rx)
        .into_iter()
        .filter_map(|msg| match msg {
            ClientMsg::Feature { action: argus_protocol::FeatureAction::Link { slug, link }, .. } => Some((slug, link.url)),
            _ => None,
        })
        .collect();
    assert_eq!(links, [("trim".to_string(), "https://linear.app/acme/issue/ENG-88/trim".to_string())]);
    assert!(
        app.selected_feature().unwrap().links.is_empty(),
        "the heading waits for the daemon's board"
    );
}

#[test]
fn a_ticket_url_that_is_no_link_is_said_at_once_and_never_sent() {
    let (mut app, mut rx) = reading();
    press(&mut app, 'T');
    type_in(&mut app, "ENG-88");
    key(&mut app, KeyCode::Enter);
    assert!(app.status.contains("http or https"), "{}", app.status);
    assert!(!sent(&mut rx).iter().any(|msg| matches!(msg, ClientMsg::Feature { action: argus_protocol::FeatureAction::Link { .. }, .. })));
}

// ---- the tracker map ---------------------------------------------------------

const DRAFT: &str = "Linear: a feature is an issue\nJira: a feature is a story";

/// The index, with a draft of the tracker map waiting from pane #32.
fn drafted() -> (App, tokio::sync::mpsc::UnboundedReceiver<ClientMsg>) {
    let (mut app, mut rx) = feature_stage();
    app.board.as_mut().unwrap().tracker_map = argus_protocol::TrackerMap {
        body: "Linear: issues".into(),
        draft: Some(argus_protocol::MapDraft { body: DRAFT.into(), by: "pane #32".into(), at: 1 }),
    };
    sent(&mut rx);
    (app, rx)
}

fn map_changes(rx: &mut tokio::sync::mpsc::UnboundedReceiver<ClientMsg>) -> Vec<MapChange> {
    sent(rx)
        .into_iter()
        .filter_map(|msg| match msg {
            ClientMsg::TrackerMap { change, .. } => Some(change),
            _ => None,
        })
        .collect()
}

/// Not an assertion — the index with a draft waiting, and the draft open:
/// `cargo test -p argus dump_tracker_map_draft -- --ignored --nocapture`.
#[test]
#[ignore = "prints the tracker map draft for eyeballing; asserts nothing"]
fn dump_tracker_map_draft() {
    let (mut app, _rx) = drafted();
    println!("{}\n", stage(&mut app));
    press(&mut app, 'M');
    println!("{}\n", screen(&mut app));
}

#[test]
fn the_index_says_a_draft_of_the_map_is_waiting() {
    let (mut app, _rx) = drafted();
    let text = screen(&mut app);
    assert!(text.contains("tracker map drafted by pane #32 M review"), "{text}");
}

#[test]
fn a_draft_is_approved_as_it_reads_in_the_editor_and_not_predicted() {
    let (mut app, mut rx) = drafted();
    press(&mut app, 'M');
    assert!(screen(&mut app).contains("Jira: a feature is a story"));

    press(&mut app, 'G');
    press(&mut app, 'o');
    type_in(&mut app, "GitHub: never");
    key(&mut app, KeyCode::Esc);
    assert!(map_changes(&mut rx).is_empty(), "leaving insert saves no draft");

    press(&mut app, 'A');
    assert_eq!(map_changes(&mut rx), [MapChange::Approve(format!("{DRAFT}\nGitHub: never"))]);
    assert!(app.brief.is_none() || app.overlay.is_none(), "the editor closes");
    assert_eq!(app.board.as_ref().unwrap().tracker_map.body, "Linear: issues", "the board waits");
}

#[test]
fn a_draft_closed_unanswered_keeps_waiting_and_d_turns_it_down() {
    let (mut app, mut rx) = drafted();
    press(&mut app, 'M');
    press(&mut app, 'q');
    assert!(map_changes(&mut rx).is_empty(), "closing is not an answer");
    assert_eq!(app.mode(), crate::app::Mode::Stage(View::Feature), "and puts you back on the index");

    press(&mut app, 'M');
    press(&mut app, 'D');
    assert_eq!(map_changes(&mut rx), [MapChange::Discard]);
}

#[test]
fn with_no_draft_the_map_is_edited_like_a_brief_and_a_and_d_are_its_own_keys() {
    let (mut app, mut rx) = feature_stage();
    sent(&mut rx);
    press(&mut app, 'M');
    assert!(app.status.contains("no tracker map yet"), "{}", app.status);
    press(&mut app, 'D');
    press(&mut app, 'A');
    assert!(map_changes(&mut rx).is_empty(), "nothing to approve or turn down");

    press(&mut app, 'i');
    type_in(&mut app, "Linear: issues");
    key(&mut app, KeyCode::Esc);
    let changes = map_changes(&mut rx);
    assert!(
        matches!(changes.as_slice(), [MapChange::Write(body)] if body.ends_with("Linear: issues")),
        "{changes:?}"
    );
}

// ---- the work lens -----------------------------------------------------------

#[test]
fn the_outline_numbers_tasks_and_hangs_decisions_under_the_task_that_raised_them() {
    let (mut app, _rx) = reading();

    let text = screen(&mut app);
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle}:\n{text}"));

    assert!(at("2.3 Refuse when the pane has no feature") < at("D2   The brief travels through the daemon"));
    assert!(at("D2   The brief travels") < at("D2.1 The route lives"));
    assert!(at("D3   How much decision history") < at("3.1 Implement the cap"));
    let question = text.lines().find(|line| line.contains("D3   How much")).unwrap();
    assert!(question.contains("NEEDS YOU"), "{question}");
}

#[test]
fn a_task_under_an_open_decision_says_what_it_waits_on() {
    let (mut app, _rx) = reading();

    let text = screen(&mut app);
    let held = text.lines().find(|line| line.contains("Implement the cap")).unwrap();
    let holder = text.lines().find(|line| line.contains("Cap the decision history")).unwrap();

    assert!(held.contains("⊘") && held.contains("waits on D3"), "{held}");
    assert!(!holder.contains("waits on"), "{holder}");
}

#[test]
fn the_selected_task_is_read_whole_beside_the_outline() {
    let (mut app, _rx) = reading();
    select(&mut app, WorkRef::Task(8));

    let text = screen(&mut app);

    for landmark in [
        "2.2 · under 2 Serve the brief on demand",
        "in progress · pane #100",
        "DESCRIPTION",
        "DONE WHEN",
        "✓ The hook's request uses the route from 2.1",
        "○ No path to the brief file",
        "BOUND BY",
        "NOTES",
        "pane #100  route answers; wiring the hook",
        "you  keep the refusal case for 2.3",
    ] {
        assert!(text.contains(landmark), "missing {landmark}:\n{text}");
    }
    let bound = &text[text.find("BOUND BY").unwrap()..text.find("NOTES").unwrap()];
    assert!(bound.contains("D2 ") && bound.contains("D2.1"), "{bound}");
    assert!(!bound.contains("D2.2"), "a rejected decision binds nothing:\n{bound}");
}

#[test]
fn h_folds_a_task_and_l_opens_it_again() {
    let (mut app, _rx) = reading();
    select(&mut app, WorkRef::Task(2));
    let open = app.outline().rows.len();

    press(&mut app, 'h');

    let text = screen(&mut app);
    assert!(app.outline().rows.len() < open);
    assert!(text.contains("▸"), "{text}");
    assert!(!text.contains("Refuse when the pane has no feature"), "{text}");
    let folded = text.lines().find(|line| line.contains("Serve the brief on demand") && line.contains("▸")).unwrap();
    assert!(folded.contains("3 decisions") && folded.contains("1/3"), "{folded}");

    press(&mut app, 'l');
    assert_eq!(app.outline().rows.len(), open);
}

#[test]
fn space_marks_a_task_done_and_in_the_detail_checks_a_criterion() {
    let (mut app, mut rx) = reading();
    select(&mut app, WorkRef::Task(8));

    press(&mut app, ' ');
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::Task { action: TaskAction::Move { id: 8, state: TaskState::Done }, .. }]
    ));

    key(&mut app, KeyCode::Tab);
    press(&mut app, 'j');
    press(&mut app, ' ');
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::Task { action: TaskAction::SetCriterion { id: 8, index: 1, done: true }, .. }]
    ));
}

#[test]
fn a_task_a_criterion_and_a_note_are_each_typed_on_the_last_row() {
    let (mut app, mut rx) = reading();
    select(&mut app, WorkRef::Task(8));

    press(&mut app, 'c');
    type_in(&mut app, "a test covers it");
    assert!(screen(&mut app).contains("done when a test covers it_"));
    key(&mut app, KeyCode::Enter);
    press(&mut app, 'n');
    type_in(&mut app, "halfway");
    key(&mut app, KeyCode::Enter);
    press(&mut app, 's');
    type_in(&mut app, "wire it");
    key(&mut app, KeyCode::Enter);

    let actions: Vec<TaskAction> = sent(&mut rx)
        .into_iter()
        .filter_map(|msg| match msg {
            ClientMsg::Task { action, .. } => Some(action),
            _ => None,
        })
        .collect();
    assert!(matches!(&actions[0], TaskAction::AddCriterion { id: 8, text } if text == "a test covers it"));
    assert!(matches!(&actions[1], TaskAction::Note { id: 8, text } if text == "halfway"));
    assert!(matches!(&actions[2], TaskAction::Add(write) if write.parent == Some(8) && write.title == "wire it"));
}

#[test]
fn escape_abandons_the_line_rather_than_the_level() {
    let (mut app, mut rx) = reading();

    press(&mut app, 'a');
    type_in(&mut app, "half a thou");
    key(&mut app, KeyCode::Esc);

    assert!(app.line.is_none() && app.feature_open);
    assert!(sent(&mut rx).is_empty());
}

#[test]
fn a_decision_typed_on_a_task_is_raised_under_it_and_a_question_mark_leaves_it_open() {
    let (mut app, mut rx) = reading();
    select(&mut app, WorkRef::Task(9));

    press(&mut app, 'd');
    type_in(&mut app, "? refuse or answer empty");
    key(&mut app, KeyCode::Enter);

    let msgs = sent(&mut rx);
    let [ClientMsg::Decide { feature, write, .. }] = msgs.as_slice() else {
        panic!("{msgs:?}");
    };
    assert_eq!(feature, "trim");
    assert_eq!(write.chose, "refuse or answer empty");
    assert_eq!((write.state, write.task), (DecisionState::Open, Some(9)));
}

#[test]
fn a_tasks_changes_are_asked_of_the_daemon_and_not_predicted() {
    let (mut app, mut rx) = reading();
    select(&mut app, WorkRef::Task(8));

    press(&mut app, 'L');

    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::Task { action: TaskAction::Move { id: 8, state: TaskState::Done }, .. }]
    ));
    assert_eq!(app.selected_task().unwrap().state, TaskState::Doing);
}

#[test]
fn the_cursor_stays_on_its_row_when_the_tasks_under_it_are_replaced() {
    let (mut app, _rx) = reading();
    select(&mut app, WorkRef::Task(9));
    let mut list = feature_tasks();
    list.tasks.retain(|task| task.id != 7);

    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(list)));

    assert_eq!(app.selected_work(), Some(WorkRef::Task(9)));
}

#[test]
fn a_new_feature_offers_the_keys_that_put_the_first_thing_in_it() {
    let (mut app, _rx) = reading();
    let board = app.board.as_mut().unwrap();
    board.decisions.clear();
    board.features[1].body.clear();
    board.features[1].tasks = Default::default();
    board.features[1].checkouts.clear();
    app.tasks.as_mut().unwrap().tasks.clear();
    app.rescope_feature();

    let text = screen(&mut app);

    assert!(text.contains("○ NEW"), "{text}");
    assert!(text.contains("no description yet"), "{text}");
    assert!(text.contains("no tasks · no decisions"), "{text}");
    for offer in ["Add the first task", "Record a decision", "Write the description", "Write it all as one note"] {
        assert!(text.contains(offer), "missing {offer}:\n{text}");
    }
}

// ---- the record lens -----------------------------------------------------------

#[test]
fn the_record_lists_every_decision_as_a_tree_with_how_it_stands() {
    let (mut app, _rx) = reading();
    press(&mut app, '2');

    let text = screen(&mut app);
    let line = |needle: &str| {
        text.lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle}:\n{text}"))
            .to_string()
    };

    assert!(line("D1.1 The name line").contains("└"));
    assert!(line("D2.1 The route lives").contains("├"));
    assert!(line("D2.2 Hook reads the brief file directly").contains("rejected"));
    assert!(line("D3   How much decision history").contains("open"));
    assert!(text.contains("in the order they were made"), "{text}");
}

#[test]
fn a_decision_reads_as_a_record() {
    let (mut app, _rx) = reading();
    press(&mut app, '2');
    app.decision_sel = 2;

    let text = screen(&mut app);

    for landmark in [
        "D2 ACCEPTED",
        "raised under task 2 · by you",
        "CONTEXT",
        "DECISION",
        "WHY",
        "CONSEQUENCES",
        "+ One contract for the brief",
        "− A pane with no feature needs a refusal",
        "CONSIDERED",
        "rejected: the second contract",
        "CARRIED OUT BY",
        "2.2 argus-hook reads the brief through the daemon",
    ] {
        assert!(text.contains(landmark), "missing {landmark}:\n{text}");
    }
    assert!(text.contains("› record › D2"), "{text}");
}

#[test]
fn an_open_decision_is_answered_by_taking_one_of_its_options() {
    let (mut app, mut rx) = reading();
    press(&mut app, '2');
    app.decision_sel = 5;
    assert!(screen(&mut app).contains("OPTIONS"));

    key(&mut app, KeyCode::Tab);
    press(&mut app, 'j');
    key(&mut app, KeyCode::Enter);

    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::ChangeDecision { change: DecisionChange::Resolve { id: 6, answer, .. }, .. }]
            if answer == "The last ten"
    ));
}

#[test]
fn a_sub_decision_and_a_supersession_name_the_decision_under_the_cursor() {
    let (mut app, mut rx) = reading();
    press(&mut app, '2');
    app.decision_sel = 2;

    press(&mut app, 's');
    type_in(&mut app, "~ poll the file");
    key(&mut app, KeyCode::Enter);
    press(&mut app, 'u');
    type_in(&mut app, "the brief is pushed");
    key(&mut app, KeyCode::Enter);

    let writes: Vec<_> = sent(&mut rx)
        .into_iter()
        .filter_map(|msg| match msg {
            ClientMsg::Decide { write, .. } => Some(write),
            _ => None,
        })
        .collect();
    assert_eq!((writes[0].under, writes[0].state), (Some(3), DecisionState::Rejected));
    assert_eq!(writes[0].chose, "poll the file");
    assert_eq!((writes[1].supersedes, writes[1].under), (Some(3), None));
}

#[test]
fn a_decision_is_withdrawn_rather_than_removed_and_x_again_restores_it() {
    let (mut app, mut rx) = reading();
    press(&mut app, '2');

    press(&mut app, 'x');
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::ChangeDecision { change: DecisionChange::Withdraw { id: 1 }, .. }]
    ));

    app.board.as_mut().unwrap().decisions[0].withdrawn_at = Some(9);
    app.rescope_feature();
    sent(&mut rx);
    press(&mut app, 'x');
    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::ChangeDecision { change: DecisionChange::Restore { id: 1 }, .. }]
    ));
}

#[test]
fn e_opens_a_decision_as_its_record_and_saving_rewrites_it() {
    let (mut app, mut rx) = reading();
    press(&mut app, '2');
    app.decision_sel = 2;

    press(&mut app, 'e');
    let body = app.brief.as_ref().expect("the record is open").body();
    assert!(body.starts_with("The brief travels through the daemon\n\n## Context\n"), "{body}");
    assert!(body.contains("## Considered"), "an empty section is offered to be filled:\n{body}");
    press(&mut app, 'o');
    type_in(&mut app, "x");
    key(&mut app, KeyCode::Esc);

    let msgs = sent(&mut rx);
    assert!(
        matches!(
            msgs.as_slice(),
            [ClientMsg::ChangeDecision { change: DecisionChange::Rewrite { id: 3, text }, .. }]
                if text.chose == "The brief travels through the daemon" && text.consequences.len() == 2
        ),
        "{msgs:?}"
    );
}

#[test]
fn y_copies_the_decision_as_adr_markdown() {
    fn keep(_: &str) -> bool {
        true
    }
    let (mut app, _rx) = reading();
    app.clipboard_write = keep;
    press(&mut app, '2');

    press(&mut app, 'y');

    assert_eq!(app.status, "copied as ADR markdown");
}

// ---- the flows lens ------------------------------------------------------------

#[test]
fn a_flow_is_listed_as_steps_beside_its_diagram() {
    let (mut app, _rx) = reading();
    press(&mut app, '3');

    let text = screen(&mut app);

    assert!(text.contains("Brief on demand"), "{text}");
    assert!(text.contains("4 steps · 1 branch"), "{text}");
    assert!(text.contains("agent → hook"), "{text}");
    assert!(text.contains("else the pane has no feature"), "{text}");
    assert!(text.contains("BRIEF ON DEMAND"), "{text}");
    assert!(text.contains("› flows › Brief on demand › step 1"), "{text}");
}

#[test]
fn j_steps_through_a_flow_past_its_branch_lines() {
    let (mut app, _rx) = reading();
    press(&mut app, '3');

    for _ in 0..9 {
        press(&mut app, 'j');
    }

    assert_eq!(app.step_sel, 3, "four steps, and the branch line is not one");
    assert!(screen(&mut app).contains("› step 4"));
}

#[test]
fn a_step_names_what_binds_it_and_the_selected_one_says_what_carries_it_out() {
    let (mut app, _rx) = reading();
    press(&mut app, '3');

    let text = screen(&mut app);
    assert!(text.contains("brief request          D2.1"), "{text}");
    assert!(text.contains("refused            task 2.3"), "{text}");
    assert!(!text.contains("IS BOUND BY"), "step 1 is bound by nothing:\n{text}");

    press(&mut app, 'j');
    let text = screen(&mut app);
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle}:\n{text}"));
    assert!(at("STEP 2 IS BOUND BY") < at("D2.1 The route lives in the pane API URL"));
    assert!(at("CARRIED OUT BY") < at("2.2 argus-hook reads the brief"));
    assert!(at("2.2 argus-hook reads the brief") < at("2.3 Refuse when the pane has no feature"));
}

#[test]
fn d_binds_the_selected_step_by_the_decision_a_reader_names() {
    let (mut app, mut rx) = reading();
    press(&mut app, '3');
    assert!(screen(&mut app).contains("nothing binds step 1"));

    press(&mut app, 'd');
    assert!(screen(&mut app).contains("bound by decision (D2.1, or #id)"));
    type_in(&mut app, "D3");
    key(&mut app, KeyCode::Enter);

    let msgs = sent(&mut rx);
    let [ClientMsg::SequenceDiagram { action: argus_protocol::DiagramAction::Rewrite { id: 1, write }, .. }] =
        msgs.as_slice()
    else {
        panic!("{msgs:?}");
    };
    assert_eq!(write.title, "Brief on demand");
    assert!(write.body.contains("agent->>hook: ask for the brief\n    %% decision #6\n"), "{}", write.body);
}

#[test]
fn a_binding_that_names_no_decision_says_so_and_sends_nothing() {
    let (mut app, mut rx) = reading();
    press(&mut app, '3');
    press(&mut app, 'j');

    press(&mut app, 'd');
    type_in(&mut app, "D9");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "no decision D9 on this feature");
    press(&mut app, 'd');
    type_in(&mut app, "#4");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "step 2 is already bound by #4");

    assert!(sent(&mut rx).is_empty());
}

#[test]
fn e_opens_a_flow_as_its_mermaid_and_saving_records_it_again() {
    let (mut app, mut rx) = reading();
    press(&mut app, '3');

    press(&mut app, 'e');
    let body = app.brief.as_ref().expect("the source is open").body();
    assert!(body.starts_with("sequenceDiagram\n    participant agent"), "{body}");
    press(&mut app, 'o');
    type_in(&mut app, "agent->>agent: think");
    key(&mut app, KeyCode::Esc);

    let msgs = sent(&mut rx);
    let [ClientMsg::SequenceDiagram { action: argus_protocol::DiagramAction::Rewrite { id: 1, write }, .. }] =
        msgs.as_slice()
    else {
        panic!("{msgs:?}");
    };
    assert_eq!(write.title, "Brief on demand");
    assert!(write.body.contains("agent->>agent: think"), "{}", write.body);
}

#[test]
fn the_selected_step_is_lit_where_the_diagram_draws_it() {
    let (mut app, _rx) = reading();
    press(&mut app, '3');
    press(&mut app, 'j');
    let th = crate::theme::Theme::default();

    let buf = draw_at(&mut app, SCREEN_WIDTH, SCREEN_HEIGHT);

    assert_eq!(fg_of(&buf, "┆ brief request"), Some(th.accent));
    assert_eq!(fg_of(&buf, "┆ ask for the brief"), Some(th.dim));
}

// ---- writing a feature as one note ------------------------------------------------

#[test]
fn a_feature_is_written_as_one_note_and_sent_whole() {
    let (mut app, mut rx) = feature_stage();
    sent(&mut rx);

    press(&mut app, 'a');
    type_in(&mut app, "Ship the cap");
    key(&mut app, KeyCode::Enter);
    type_in(&mut app, "- Bump it");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Tab);
    type_in(&mut app, "- Fix what breaks");
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::BackTab);
    type_in(&mut app, "? Same release");
    let text = screen(&mut app);
    assert!(text.contains("READS AS") && text.contains("ready to create"), "{text}");
    assert!(text.contains("1.1  Fix what breaks"), "{text}");
    assert!(text.contains("orion › features › new"), "{text}");
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let msgs = sent(&mut rx);
    let [ClientMsg::Feature { action: argus_protocol::FeatureAction::Capture { into: None, capture }, .. }] = msgs.as_slice()
    else {
        panic!("{msgs:?}");
    };
    assert_eq!(capture.title, "Ship the cap");
    assert_eq!(capture.tasks[0].subtasks[0].title, "Fix what breaks");
    assert!(capture.decisions[0].open);
    assert!(app.capture.is_none());
}

#[test]
fn a_note_that_does_not_read_is_kept_open_and_says_why() {
    let (mut app, mut rx) = feature_stage();
    sent(&mut rx);
    press(&mut app, 'a');

    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert!(app.capture.is_some());
    assert!(sent(&mut rx).is_empty());
    assert_eq!(app.status, "a feature has to have a title");
}

#[test]
fn typing_a_note_does_not_work_the_stage_underneath() {
    let (mut app, mut rx) = feature_stage();
    sent(&mut rx);
    press(&mut app, 'a');

    type_in(&mut app, "vxq2");

    assert_eq!(app.capture.as_ref().unwrap().body(), "vxq2");
    assert!(sent(&mut rx).is_empty());
    assert_eq!(app.view, View::Feature);
}

#[test]
fn i_writes_more_of_an_open_feature_as_a_note() {
    let (mut app, mut rx) = reading();

    press(&mut app, 'i');
    type_in(&mut app, "- one more task");
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert!(matches!(
        sent(&mut rx).as_slice(),
        [ClientMsg::Feature { action: argus_protocol::FeatureAction::Capture { into: Some(slug), capture }, .. }]
            if slug == "trim" && capture.tasks.len() == 1
    ));
}

// ---- the bar, the mouse, and small frames -------------------------------------------

#[test]
fn the_bar_offers_the_keys_of_the_level_the_stage_is_on() {
    let (mut app, _rx) = feature_stage();
    assert!(bar(&draw_at(&mut app, 240, 30)).contains("v archive"));

    key(&mut app, KeyCode::Enter);
    let work = bar(&draw_at(&mut app, 240, 30));
    assert!(work.contains("h/l fold") && !work.contains("v archive"), "{work}");

    press(&mut app, '2');
    assert!(bar(&draw_at(&mut app, 240, 30)).contains("u supersede"));
}

#[test]
fn a_click_selects_a_feature_and_a_second_opens_it() {
    let (mut app, _rx) = feature_stage();
    let buf = draw_at(&mut app, 170, 44);
    let row = row_of(&buf, "The ratatui 0.30 bump").expect("the queued feature is drawn");
    let click = |app: &mut App| {
        app.on_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: app.layout.features.inner.x + 10,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    click(&mut app);
    assert_eq!(app.feature_slug().as_deref(), Some("ratatui"));
    assert!(!app.feature_open);

    click(&mut app);
    assert!(app.feature_open);
}

#[test]
fn a_click_on_a_lens_opens_it() {
    let (mut app, _rx) = reading();
    draw_at(&mut app, 170, 44);
    let strip = app.layout.feature_lenses.inner;

    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: strip.right() - 2,
        row: strip.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.lens, Lens::Flows);
}

#[test]
fn a_stage_too_small_for_two_columns_still_draws_without_panicking() {
    for (w, h) in [(60, 8), (80, 12), (100, 6), (30, 3)] {
        let (mut app, _rx) = reading();
        for lens in ['1', '2', '3'] {
            press(&mut app, lens);
            draw_at(&mut app, w, h);
        }
        key(&mut app, KeyCode::Esc);
        draw_at(&mut app, w, h);
        press(&mut app, 'a');
        draw_at(&mut app, w, h);
    }
}
