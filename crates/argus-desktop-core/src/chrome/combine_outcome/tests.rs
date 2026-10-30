use super::*;
use crate::chrome::add_page::ProjectRef;

fn row(id: u64, ticked: bool) -> CombineRow {
    CombineRow { id, name: format!("p{id}"), repositories: 1, needs_you: 0, working: 0, scans_folder: false, ticked }
}

fn page() -> CombinePage {
    CombinePage {
        open: true,
        into: Some(ProjectRef { id: 1, name: "argus".into() }),
        rows: vec![row(2, true), row(3, false)],
        holds: vec!["argus".into(), "web".into(), "dots".into()],
        repositories: 3,
        ..CombinePage::default()
    }
}

#[test]
fn the_summary_follows_the_ticks_and_says_the_agents_only_when_there_are_some() {
    let mut p = page();
    let words = |p: &CombinePage| summary(p).into_iter().map(|l| (l.mark, l.words)).collect::<Vec<_>>();
    let lines = words(&p);
    assert_eq!(lines[0], (Mark::Check, "argus will hold 3 repositories: argus, web, dots.".to_owned()));
    assert_eq!(lines.len(), 3, "no agents, no line for them");
    assert_eq!(lines[1].1, "Their feature boards come too; features keep their numbers.");
    assert_eq!(
        lines[2],
        (Mark::Dash, "Their own worktree and setup settings are dropped; the project row leaves the switcher. Nothing on disk changes.".to_owned())
    );
    p.rows[1].ticked = true;
    assert!(words(&p)[2].1.contains("the 2 project rows leave the switcher"));
    p.agents = 2;
    assert_eq!(words(&p)[1], (Mark::Check, "Running panes move with their repositories and keep running: 2 agents.".to_owned()));
    p.rows.iter_mut().for_each(|r| r.ticked = false);
    assert!(summary(&p).is_empty(), "nothing ticked, nothing to say");
}

#[test]
fn the_button_counts_the_ticks_and_is_off_with_none_or_while_asking() {
    let mut p = page();
    assert_eq!(button_label(&p), "Combine 1 into argus");
    assert!(can_combine(&p));
    p.combining = true;
    assert!(!can_combine(&p));
    p.combining = false;
    p.rows.iter_mut().for_each(|r| r.ticked = false);
    assert_eq!((button_label(&p), can_combine(&p)), ("Combine into argus".to_owned(), false));
}

#[test]
fn a_row_has_a_status_note_only_when_somebody_is_needed_or_working() {
    let mut r = row(2, false);
    assert_eq!(status_note(&r), None);
    r.needs_you = 1;
    assert_eq!(status_note(&r), Some(Tally { words: "1 need you".into(), tone: Tone::Loud }));
}
