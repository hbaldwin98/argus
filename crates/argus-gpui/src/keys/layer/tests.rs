use super::*;
use std::cell::RefCell;

fn key(name: &str) -> PageKey {
    PageKey { key: name.into(), ..PageKey::default() }
}

/// SHELL-KEYS-2: the first place that binds the key wins, so an open feature's digit is its lens.
#[test]
fn a_place_listed_first_takes_a_key_from_one_after_it() {
    let digit = key("1");
    assert_eq!(action_for(&[Place::Anywhere], &digit), Some(Action::Workspace));
    assert_eq!(action_for(&[Place::Outline, Place::Anywhere], &digit), Some(Action::WorkLens));
    assert_eq!(action_for(&[Place::Anywhere, Place::Outline], &digit), Some(Action::Workspace));
}

/// SHELL-KEYS-1: a leader's chord is not a key pressed on its own.
#[test]
fn a_key_after_the_leader_is_not_a_key_of_a_place() {
    assert_eq!(action_for(&[Place::Pane], &key("f")), None);
    assert_eq!(action_for(&[Place::Anywhere], &key("q")), None);
}

struct Nothing;

impl Handler for Nothing {
    fn open(&self, _: &Window, _: &App) -> bool {
        false
    }
}

#[test]
fn a_layer_is_hooked_once_a_surface_registers_it_and_the_last_to_register_has_it() {
    let layers = Layers::default();
    assert!(!layers.hooked(Layer::Rail));
    layers.register(Layer::Rail, Rc::new(Nothing));
    layers.register(Layer::Rail, Rc::new(Nothing));
    assert!(layers.hooked(Layer::Rail));
    assert!(!layers.hooked(Layer::Menu));
    assert_eq!(layers.0.borrow().handlers.len(), 1, "one handler a layer");
}

#[test]
fn a_command_goes_where_the_window_serves_them_and_nowhere_before() {
    let layers = Layers::default();
    assert!(!layers.command(Action::Checkouts));
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    layers.serve(Caller::new(move |call| sink.borrow_mut().push(format!("{call:?}"))));
    assert!(layers.command(Action::Checkouts));
    assert!(!layers.command(Action::Fill), "an action no key of the window asks the session for");
    assert_eq!(*heard.borrow(), ["ShowCheckouts { open: true }"]);
}
