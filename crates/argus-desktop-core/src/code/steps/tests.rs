use super::*;

fn row(added: bool, removed_before: u32) -> CodeRow {
    CodeRow { no: 0, parts: Vec::new(), added, removed_before, cursor: false, picked: false }
}

#[test]
fn a_line_added_or_after_removed_lines_is_marked_and_added_wins() {
    let lines = [row(false, 0), row(true, 0), row(false, 2), row(true, 3)];
    assert_eq!(
        marks(&lines),
        [Mark { line: 2, kind: Kind::Added }, Mark { line: 3, kind: Kind::Removed }, Mark { line: 4, kind: Kind::Added }]
    );
}

#[test]
fn stepping_forward_goes_to_the_first_line_of_the_next_run() {
    let marked = [3, 4, 5, 9, 12];
    assert_eq!(step(&marked, 1, true), Some(3));
    assert_eq!(step(&marked, 3, true), Some(9), "from a run's first line, the next run");
    assert_eq!(step(&marked, 4, true), Some(9), "from inside a run, the run after it");
    assert_eq!(step(&marked, 12, true), None, "nothing after the last change");
}

#[test]
fn stepping_back_from_inside_a_run_goes_to_that_runs_first_line() {
    let marked = [3, 4, 5, 9, 12];
    assert_eq!(step(&marked, 5, false), Some(3));
    assert_eq!(step(&marked, 3, false), None);
    assert_eq!(step(&marked, 10, false), Some(9));
    assert_eq!(step(&marked, 100, false), Some(12));
}

#[test]
fn a_file_with_no_change_has_nothing_to_step_to() {
    assert_eq!(step(&[], 5, true), None);
    assert_eq!(step(&[], 5, false), None);
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    // EDITOR-GUTTER-4: a step lands on the first line of a run, strictly after (forward) or
    // before (backward) the line it started on, and no run start lies between.
    proptest! {
        #[test]
        fn a_step_lands_on_the_nearest_run_start_in_its_direction(set in proptest::collection::btree_set(1usize..40, 0..20), here in 1usize..40, forward in any::<bool>()) {
            let marked: Vec<usize> = set.iter().copied().collect();
            let starts: Vec<usize> = marked.iter().copied().filter(|l| !set.contains(&(l - 1))).collect();
            let want = if forward { starts.iter().copied().find(|&s| s > here) } else { starts.iter().copied().rev().find(|&s| s < here) };
            prop_assert_eq!(step(&marked, here, forward), want);
        }
    }
}
