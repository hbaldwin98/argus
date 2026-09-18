//! The sequence-diagram overlay: clean branch frames and a usable narrow view.

use super::*;

fn alt_diagram() -> argus_protocol::SequenceDiagram {
    argus_protocol::SequenceDiagram {
        id: 2,
        feature: "x".into(),
        title: "conditional".into(),
        body: "sequenceDiagram
    participant Client
    participant Daemon
    participant Hook
    Client->>Daemon: request
    alt accepted
        Daemon-->>Client: accepted
        Daemon->>Hook: notify
    else rejected
        Daemon-->>Client: rejected
    end"
            .into(),
        at: 0,
        session: None,
    }
}

#[test]
fn a_narrow_sequence_overlay_has_no_branch_fill_noise() {
    let mut app = app_with_tree();
    app.diagram = Some(crate::diagram::DiagramView::open(&alt_diagram(), 40));
    app.overlay = Some(Overlay::SequenceDiagram);

    let rendered = lines(&draw_at(&mut app, 50, 24)).join("\n");

    assert!(!rendered.contains('░'), "branch fill leaked into the UI: {rendered}");
    assert!(
        rendered.contains("h/l"),
        "the overlay must expose horizontal panning: {rendered}"
    );
}

#[test]
fn l_pans_a_sequence_overlay_without_resizing_the_terminal() {
    let mut app = app_with_tree();
    app.diagram = Some(crate::diagram::DiagramView::open(&alt_diagram(), 40));
    app.overlay = Some(Overlay::SequenceDiagram);

    let before = lines(&draw_at(&mut app, 50, 24)).join("\n");
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    let after = lines(&draw_at(&mut app, 50, 24)).join("\n");

    assert_ne!(before, after, "l should move a diagram wider than the overlay");
}
