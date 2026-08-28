//! Notes attached to a project or a checkout, and the counts a row
//! shows for them.

use super::*;
// ---- notes -------------------------------------------------------

#[test]
fn a_checkout_note_round_trips_and_its_counts_reach_the_tree() {
    let d = daemon_with_primary("/repo");
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
    let target = NoteTarget::Checkout(checkout);

    assert_eq!(d.note(target).unwrap().body, "", "an unwritten note is empty");
    assert!(!d.snapshot()[0].repositories[0].checkouts[0].has_note);

    d.set_note(target, "- [ ] one
- [x] two
- [!] three
".to_string())
        .unwrap();

    let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
    assert!(checkout.has_note);
    assert_eq!(
        checkout.notes,
        NoteCounts {
            open: 1,
            done: 1,
            pinned: 1
        }
    );
    assert_eq!(d.note(target).unwrap().todos.len(), 3);
}

#[test]
fn a_project_note_is_separate_from_its_checkouts() {
    let d = daemon_with_primary("/repo");
    let project = d.snapshot()[0].id;
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    d.set_note(NoteTarget::Project(project), "- [ ] project work".to_string())
        .unwrap();
    d.set_note(
        NoteTarget::Checkout(checkout),
        "- [ ] checkout work
- [ ] more".to_string(),
    )
    .unwrap();

    let tree = d.snapshot();
    assert_eq!(tree[0].notes.open, 1);
    assert_eq!(tree[0].repositories[0].checkouts[0].notes.open, 2);
}

#[test]
fn counts_roll_up_from_every_checkout_to_the_project() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    let tree = d.snapshot();
    let project = tree[0].id;
    for checkout in &tree[0].repositories[0].checkouts {
        d.set_note(
            NoteTarget::Checkout(checkout.id),
            "- [ ] a
- [ ] b
- [x] c
".to_string(),
        )
        .unwrap();
    }
    d.set_note(NoteTarget::Project(project), "- [!] read me first".to_string())
        .unwrap();

    let tree = d.snapshot();
    assert_eq!(
        tree[0].repositories[0].note_rollup(),
        NoteCounts {
            open: 4,
            done: 2,
            pinned: 0
        },
        "the repository sums its checkouts and holds no note of its own"
    );
    assert_eq!(
        tree[0].note_rollup(),
        NoteCounts {
            open: 4,
            done: 2,
            pinned: 1
        },
        "the project adds its own note to what is beneath it"
    );
}

#[test]
fn emptying_a_note_clears_the_row() {
    let d = daemon_with_primary("/repo");
    let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
    d.set_note(target, "- [ ] something".to_string()).unwrap();
    assert!(d.snapshot()[0].repositories[0].checkouts[0].has_note);

    d.set_note(target, String::new()).unwrap();

    let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
    assert!(!checkout.has_note);
    assert!(checkout.notes.is_empty());
}

#[test]
fn toggling_a_checkbox_leaves_the_rest_of_the_note_alone() {
    let d = daemon_with_primary("/repo");
    let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
    d.set_note(target, "# Plan

- [ ] first
- [ ] second
".to_string())
        .unwrap();

    let note = d.set_todo(target, 2, TodoState::Done).unwrap();

    assert_eq!(note.body, "# Plan

- [x] first
- [ ] second
");
    assert_eq!(note.counts(), NoteCounts { open: 1, done: 1, pinned: 0 });
}

#[test]
fn toggling_a_line_that_is_not_a_checkbox_is_refused() {
    let d = daemon_with_primary("/repo");
    let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
    d.set_note(target, "# Plan
- [ ] first
".to_string())
        .unwrap();

    let err = d.set_todo(target, 0, TodoState::Done).unwrap_err().to_string();

    assert!(err.contains("not a checkbox"), "{err}");
    assert_eq!(d.note(target).unwrap().body, "# Plan
- [ ] first
");
}

#[test]
fn a_note_on_a_stale_id_is_refused_rather_than_filed_somewhere_else() {
    let d = daemon_with_primary("/repo");
    let err = d
        .set_note(NoteTarget::Checkout(CheckoutId(9999)), "x".to_string())
        .unwrap_err()
        .to_string();
    assert!(err.contains("no such checkout"), "{err}");
}

#[test]
fn a_note_too_large_to_carry_is_refused() {
    let d = daemon_with_primary("/repo");
    let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
    let err = d
        .set_note(target, "x".repeat(MAX_NOTE_BYTES + 1))
        .unwrap_err()
        .to_string();
    assert!(err.contains("exceeds"), "{err}");
}
