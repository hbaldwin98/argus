use super::*;

/// SHELL-KEYS-13: the three actions the window opens itself, and nothing else.
#[test]
fn only_the_palette_the_sheet_and_the_settings_are_opened_over_the_stage() {
    assert_eq!(overlay_of(Action::Palette), Some(Overlay::Palette));
    assert_eq!(overlay_of(Action::Shortcuts), Some(Overlay::Sheet));
    assert_eq!(overlay_of(Action::Settings), Some(Overlay::Settings));
    for other in [Action::Workspace, Action::CloseLayer, Action::ZoomIn, Action::Compose] {
        assert_eq!(overlay_of(other), None, "{other:?}");
    }
}

/// All three are exclusive of one another in the host, so a Run from a pane cannot leave two up.
#[test]
fn what_the_window_opens_is_what_the_host_keeps_exclusive() {
    for action in [Action::Palette, Action::Shortcuts, Action::Settings] {
        assert!(overlay_of(action).expect("opens").exclusive(), "{action:?}");
    }
}
