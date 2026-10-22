use super::*;

#[test]
fn a_block_is_named_by_its_place_under_the_view_not_by_an_index_into_the_views() {
    let ctx = ViewCtx::agent(7, "trim.v2");
    assert_eq!(ctx.path, "7/trim.v2");
    assert_eq!(ctx.at(2).at(0).path, "7/trim.v2.2.0");
    assert_eq!(ctx.at(2).pane, 7, "the view it is in goes down with it");
}

#[test]
fn only_an_agents_view_has_a_pane_to_read_references_against() {
    let agent = ViewCtx::agent(7, "ci");
    let saved = ViewCtx { kind: Kind::Saved, pane: 0, ..agent.clone() };
    assert_eq!(ViewCtx::pane_of(Some(&agent)), Some(7));
    assert_eq!(ViewCtx::pane_of(Some(&saved)), None);
    assert_eq!(ViewCtx::pane_of(None), None, "an extension's table has none");
}

#[test]
fn a_block_is_changed_when_its_path_under_the_view_is_one_the_session_named() {
    let mut ctx = ViewCtx::agent(7, "a.b");
    ctx.changed = Rc::new(["1.0".to_owned()].into());
    assert!(ctx.at(1).at(0).is_changed());
    assert!(!ctx.at(1).is_changed(), "the container is not what changed");
    assert!(!ctx.at(0).at(1).is_changed());
    assert!(!ViewCtx::agent(7, "a.b").at(1).at(0).is_changed(), "nothing changed in a view that named none");
}

#[test]
fn an_id_with_dots_in_it_does_not_confuse_the_path_under_the_view() {
    let mut ctx = ViewCtx::agent(7, "a.1");
    ctx.changed = Rc::new(["0".to_owned()].into());
    assert!(ctx.at(0).is_changed());
    assert!(!ctx.at(1).is_changed());
}
