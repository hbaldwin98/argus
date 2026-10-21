use super::*;

fn checkout_with(name: &str, body: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), body).unwrap();
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn change(from: u32, to: u32, insert: &str) -> TextChange {
    TextChange { from, to, insert: insert.into() }
}

#[test]
fn a_change_on_the_version_held_is_written_and_saved_as_the_next() {
    let (_dir, root) = checkout_with("a.txt", b"one\ntwo\n");
    let mut held = Held::open(CheckoutId(1), &root, "a.txt").unwrap();

    let said = held.write(0, &[change(4, 7, "TWO")]);
    assert!(matches!(said[..], [ServerMsg::EditSaved { base: 0, version: 1, .. }]), "{said:?}");
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "one\nTWO\n");
}

#[test]
fn a_write_by_anything_else_is_told_before_a_change_on_the_old_version_is_refused() {
    // The rule the client's buffer rests on: nothing it did not see is
    // ever written over.
    let (_dir, root) = checkout_with("a.txt", b"one\n");
    let mut held = Held::open(CheckoutId(1), &root, "a.txt").unwrap();
    std::fs::write(root.join("a.txt"), "one\nagent\n").unwrap();

    let said = held.write(0, &[change(0, 3, "mine")]);
    match &said[..] {
        [ServerMsg::EditChanged(c), ServerMsg::EditRefused { base: 0, reason: None, .. }] => {
            assert_eq!((c.from, c.version), (0, 1));
            assert_eq!(c.changes, [change(4, 4, "agent\n")]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "one\nagent\n", "the agent's write stands");
}

#[test]
fn a_crlf_file_is_held_with_lf_and_written_back_with_crlf() {
    let (_dir, root) = checkout_with("a.txt", b"a\r\nb\r\n");
    let mut held = Held::open(CheckoutId(1), &root, "a.txt").unwrap();
    let ServerMsg::EditOpened(opened) = held.opened() else { panic!() };
    assert_eq!(opened.text, "a\nb\n");

    held.write(0, &[change(2, 2, "x\n")]);
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a\r\nx\r\nb\r\n");
    assert!(matches!(held.reread(), Ok(None)), "its own write read back is no change");
}

#[test]
fn a_change_that_does_not_fit_the_text_is_refused_with_why_and_the_file_left() {
    let (_dir, root) = checkout_with("a.txt", b"short");
    let mut held = Held::open(CheckoutId(1), &root, "a.txt").unwrap();
    let said = held.write(0, &[change(2, 99, "")]);
    assert!(matches!(&said[..], [ServerMsg::EditRefused { reason: Some(_), .. }]), "{said:?}");
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "short");
}

#[test]
fn only_a_text_file_inside_the_checkout_is_opened() {
    let (_dir, root) = checkout_with("bin", b"\0\x01\x02");
    assert!(Held::open(CheckoutId(1), &root, "bin").unwrap_err().contains("binary"));
    assert!(Held::open(CheckoutId(1), &root, "../bin").is_err());
    assert!(Held::open(CheckoutId(1), &root, "missing.rs").is_err());
    std::fs::create_dir(root.join("dir")).unwrap();
    assert!(Held::open(CheckoutId(1), &root, "dir").is_err());
}

#[test]
fn a_deleted_file_is_written_again_by_the_next_change() {
    // Deleting is often the first half of a replace; the buffer is still
    // the person's, and their next change puts the file back.
    let (_dir, root) = checkout_with("a.txt", b"one");
    let mut held = Held::open(CheckoutId(1), &root, "a.txt").unwrap();
    std::fs::remove_file(root.join("a.txt")).unwrap();
    assert!(matches!(held.reread(), Ok(None)));
    held.write(0, &[change(3, 3, "!")]);
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "one!");
}
