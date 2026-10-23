//! A review's drafts: kept per checkout, and moved whole into its comments
//! when sent.

use super::*;

#[test]
fn drafts_are_kept_per_checkout_in_the_order_written() {
    let s = store();
    let first = s.add_review_draft(Path::new("/repo"), &anchor(4), "first").unwrap();
    let second = s.add_review_draft(Path::new("/repo"), &anchor(9), "second").unwrap();
    s.add_review_draft(Path::new("/other"), &anchor(1), "elsewhere").unwrap();

    let drafts = s.review_drafts(Path::new("/repo")).unwrap();
    assert_eq!(drafts.iter().map(|d| (d.id, d.body.as_str())).collect::<Vec<_>>(), [(first, "first"), (second, "second")]);
    assert_eq!(drafts[1].anchor, anchor(9));
}

#[test]
fn a_draft_is_edited_and_deleted_only_in_its_own_checkout() {
    let s = store();
    let id = s.add_review_draft(Path::new("/repo"), &anchor(4), "first").unwrap();

    assert!(!s.edit_review_draft(Path::new("/other"), id, "stolen").unwrap());
    assert!(!s.delete_review_draft(Path::new("/other"), id).unwrap());
    assert!(s.edit_review_draft(Path::new("/repo"), id, "better").unwrap());
    assert_eq!(s.review_drafts(Path::new("/repo")).unwrap()[0].body, "better");
    assert!(s.delete_review_draft(Path::new("/repo"), id).unwrap());
    assert!(s.review_drafts(Path::new("/repo")).unwrap().is_empty());
}

#[test]
fn sending_makes_every_draft_a_comment_and_leaves_none() {
    let s = store();
    s.add_review_comment(Path::new("/repo"), anchor(1), "sent before".to_string()).unwrap();
    s.add_review_draft(Path::new("/repo"), &anchor(4), "one").unwrap();
    s.add_review_draft(Path::new("/repo"), &anchor(9), "two").unwrap();
    s.add_review_draft(Path::new("/other"), &anchor(2), "not this review").unwrap();

    let sent = s.send_review_drafts(Path::new("/repo")).unwrap();

    assert_eq!(sent.iter().map(|c| c.body.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(s.review_comments(Path::new("/repo")).unwrap().iter().map(|c| c.body.as_str()).collect::<Vec<_>>(), ["sent before", "one", "two"]);
    assert_eq!(s.review_comments(Path::new("/repo")).unwrap()[1..], sent[..]);
    assert!(s.review_drafts(Path::new("/repo")).unwrap().is_empty());
    assert_eq!(s.review_drafts(Path::new("/other")).unwrap().len(), 1);
}

#[test]
fn discarding_drops_only_this_checkouts_drafts() {
    let s = store();
    s.add_review_draft(Path::new("/repo"), &anchor(4), "one").unwrap();
    s.add_review_draft(Path::new("/other"), &anchor(2), "two").unwrap();
    s.discard_review_drafts(Path::new("/repo")).unwrap();
    assert!(s.review_drafts(Path::new("/repo")).unwrap().is_empty());
    assert!(s.review_comments(Path::new("/repo")).unwrap().is_empty(), "discarded is not sent");
    assert_eq!(s.review_drafts(Path::new("/other")).unwrap().len(), 1);
}

#[test]
fn a_review_holds_no_more_drafts_than_are_shown() {
    let s = store();
    for line in 0..MAX_REVIEW_DRAFTS as u32 {
        s.add_review_draft(Path::new("/repo"), &anchor(line), "x").unwrap();
    }
    assert!(s.add_review_draft(Path::new("/repo"), &anchor(0), "one more").is_err());
    assert!(s.add_review_draft(Path::new("/other"), &anchor(0), "another review").is_ok());
}

#[test]
fn migrating_a_v28_store_keeps_its_comments_and_starts_with_no_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    {
        let conn = legacy_connection(&path, 28);
        conn.execute(
            "INSERT INTO review_comment (checkout_path, anchor, body) VALUES ('/repo', ?1, 'kept')",
            [encode(&anchor(3)).unwrap()],
        )
        .unwrap();
    }
    let s = Store::open_at(&path).unwrap();
    assert_eq!(s.review_comments(Path::new("/repo")).unwrap()[0].body, "kept");
    assert!(s.review_drafts(Path::new("/repo")).unwrap().is_empty());
    s.add_review_draft(Path::new("/repo"), &anchor(5), "new").unwrap();
    assert_eq!(s.review_drafts(Path::new("/repo")).unwrap().len(), 1);
}
