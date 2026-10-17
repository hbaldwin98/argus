use super::*;

fn view(id: &str) -> AgentView {
    AgentView { id: id.into(), title: "Deps".into(), ..Default::default() }
}

fn write(id: &str) -> SavedViewWrite {
    SavedViewWrite { view: view(id), ..Default::default() }
}

fn saved() -> SavedView {
    SavedView {
        id: "deps".into(),
        scope: ViewScope::Repository { repository: RepositoryId(3) },
        features: vec![FeatureRef { repository: RepositoryId(3), number: 7 }],
        recipe: Some("run cargo tree\nand summarise".into()),
        sources: vec![
            ViewSource::File { path: "Cargo.lock".into() },
            ViewSource::Url { url: "https://crates.io".into() },
        ],
        built_at: 1_760_000_000,
        built_commit: Some("abc1234".into()),
        stale: false,
        view: view("deps"),
    }
}

#[test]
fn a_saved_view_survives_the_wire() {
    let bytes = rmp_serde::to_vec_named(&saved()).unwrap();
    assert_eq!(rmp_serde::from_slice::<SavedView>(&bytes).unwrap(), saved());
}

#[test]
fn a_source_from_a_newer_argus_decodes_as_unknown() {
    let source: ViewSource = serde_json::from_str(r#"{"kind":"database","dsn":"x"}"#).unwrap();
    assert_eq!(source, ViewSource::Unknown);
}

#[test]
fn a_write_reads_from_the_json_an_agent_sends() {
    let json = r#"{"scope":{"kind":"feature","number":4},"features":[4,2,4],
        "recipe":"read it","sources":[{"kind":"file","path":"src/lib.rs"}],
        "view":{"id":"deps","title":"Deps","blocks":[]}}"#;
    let write: SavedViewWrite = serde_json::from_str(json).unwrap();
    let write = write.checked().unwrap();
    assert_eq!(write.scope, WriteScope::Feature { number: 4 });
    assert_eq!(write.features, [2, 4], "sorted and each once");
}

#[test]
fn a_write_with_no_scope_is_the_repositorys() {
    let write: SavedViewWrite = serde_json::from_str(r#"{"view":{"id":"a","title":"t","blocks":[]}}"#).unwrap();
    assert_eq!(write.scope, WriteScope::Repository);
}

#[test]
fn a_record_can_be_saved_again_as_it_was_read() {
    let record = SavedViewRecord {
        id: "deps".into(),
        scope: WriteScope::Repository,
        features: vec![1],
        recipe: None,
        sources: Vec::new(),
        built_at: 5,
        built_commit: Some("abc1234".into()),
        stale: true,
        view: view("deps"),
    };
    let json = serde_json::to_string(&record).unwrap();
    let write: SavedViewWrite = serde_json::from_str(&json).unwrap();
    assert_eq!(write.checked().unwrap().view.id, "deps");
}

#[test]
fn an_id_given_beside_the_view_is_the_views_id() {
    let mut w = write("old");
    w.id = Some("new".into());
    assert_eq!(w.checked().unwrap().view.id, "new");
}

#[test]
fn the_limits_are_kept_and_named() {
    let mut w = write("a");
    w.recipe = Some("x".repeat(MAX_RECIPE_BYTES + 1));
    assert!(w.checked().unwrap_err().starts_with("recipe"));

    let mut w = write("a");
    w.sources = vec![ViewSource::Note { text: "n".into() }; MAX_VIEW_SOURCES + 1];
    assert!(w.checked().unwrap_err().starts_with("sources"));

    let mut w = write("a");
    w.features = (1..=MAX_ATTACHED_FEATURES as i64 + 1).collect();
    assert!(w.checked().unwrap_err().starts_with("features"));

    let mut w = write("a");
    w.features = vec![0];
    assert!(w.checked().is_err());
    let mut w = write("a");
    w.scope = WriteScope::Feature { number: 0 };
    assert!(w.checked().is_err());
}

#[test]
fn a_source_has_to_be_what_it_says() {
    for (source, ok) in [
        (ViewSource::File { path: "src/a.rs".into() }, true),
        (ViewSource::File { path: "../a.rs".into() }, false),
        (ViewSource::File { path: "/etc/passwd".into() }, false),
        (ViewSource::Commit { sha: "abc1234".into() }, true),
        (ViewSource::Commit { sha: "xyz".into() }, false),
        (ViewSource::Url { url: "https://example.com/a".into() }, true),
        (ViewSource::Url { url: "file:///etc/passwd".into() }, false),
        (ViewSource::Note { text: "hello".into() }, true),
        (ViewSource::Unknown, false),
    ] {
        let mut w = write("a");
        w.sources = vec![source.clone()];
        assert_eq!(w.checked().is_ok(), ok, "{source:?}");
    }
}

#[test]
fn a_view_that_embeds_itself_is_refused_and_another_is_not() {
    let mut w = write("a");
    w.view.blocks = vec![crate::Block::Embed { view: "a".into() }];
    assert!(w.checked().unwrap_err().contains("embed itself"));
    let mut w = write("a");
    w.view.blocks = vec![crate::Block::Embed { view: "b".into() }];
    assert!(w.checked().is_ok(), "b may be saved later");
}

#[test]
fn a_link_names_a_view_the_way_a_view_is_named() {
    let link = |id: &str| {
        let mut w = write("a");
        w.view.blocks = vec![crate::Block::Fields {
            fields: vec![crate::Field { label: "l".into(), value: crate::Value::View { id: id.into(), label: None } }],
        }];
        w.checked()
    };
    assert!(link("other").is_ok());
    assert!(link("").is_err());
    assert!(link(" padded ").is_err());
    assert!(link("a\u{1b}b").is_err());
    assert!(link(&"x".repeat(crate::MAX_VIEW_ID_BYTES + 1)).is_err());
}

#[test]
fn any_element_that_can_open_a_view_names_one_that_is_checked() {
    let json = r#"{"id":"a","title":"t","blocks":[
        {"kind":"section","heading":"s","opens":"x","blocks":[]},
        {"kind":"list","items":[{"title":"i","opens":"x"}]},
        {"kind":"tree","nodes":[{"label":"n","opens":"x","children":[{"label":"c","opens":" y"}]}]}
    ]}"#;
    let view: AgentView = serde_json::from_str(json).unwrap();
    let why = view.checked().unwrap_err();
    assert!(why.contains("blocks[2].nodes[0].children[0]"), "{why}");
    assert!(why.contains(".opens"), "{why}");
}

#[test]
fn a_saved_views_body_is_held_to_the_views_bound() {
    let mut w = write("a");
    w.view.blocks = vec![crate::Block::Divider; 15_000];
    let max = crate::MAX_VIEW_BYTES;
    assert_eq!(w.checked().unwrap_err(), format!("a view is at most {max} bytes"));
}

/// Every string at its bound and all of it a character JSON escapes, so the
/// body is the largest a save that passes its checks can be.
#[test]
fn a_save_at_every_limit_fits_the_body_it_may_arrive_in() {
    let quotes = |n: usize| "\"".repeat(n);
    let mut view = view(&"v".repeat(MAX_VIEW_ID_BYTES));
    view.blocks = vec![crate::Block::Markdown { text: String::new() }];
    let room = crate::MAX_VIEW_BYTES - serde_json::to_vec(&view).unwrap().len();
    view.blocks = vec![crate::Block::Markdown { text: "x".repeat(room) }];
    assert_eq!(serde_json::to_vec(&view).unwrap().len(), crate::MAX_VIEW_BYTES);
    let write = SavedViewWrite {
        id: Some("w".repeat(MAX_VIEW_ID_BYTES)),
        scope: WriteScope::Feature { number: i64::MAX },
        features: (0..MAX_ATTACHED_FEATURES as i64).map(|n| i64::MAX - n).collect(),
        recipe: Some(quotes(MAX_RECIPE_BYTES)),
        sources: (0..MAX_VIEW_SOURCES).map(|_| ViewSource::Note { text: quotes(MAX_SOURCE_BYTES) }).collect(),
        view,
    };
    assert!(write.clone().checked().is_ok());
    let body = serde_json::to_vec(&SavedViewChange::Save { view: Box::new(write) }).unwrap();
    assert!(body.len() <= MAX_SAVED_VIEW_WRITE_BYTES, "{} > {MAX_SAVED_VIEW_WRITE_BYTES}", body.len());
}
