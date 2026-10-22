//! One of the composer's three pickers: the kit's select kept between draws, written to only when
//! what it lists or shows is not what Rust says.
//!
//! `set_items` is called only when the list changed, never on a rail update for another pane: the kit's
//! `SelectState::set_items` leaves an open popup open (it replaces the delegate and the selection and
//! never touches `open`), but calling it for nothing would still rebuild the list under a pointer
//! (COMPOSE-11). A pick the person makes is told to `picked`; what Rust sets is not a pick.

use gpui_kit::component::select::{SelectEvent, SelectItem, SelectState};
use gpui_kit::{prelude::*, App, Context, Entity, SharedString, Window};

/// An option: what it stands for and the words it shows.
#[derive(Clone)]
pub struct Item<V> {
    pub value: V,
    pub label: SharedString,
}

impl<V: Clone + PartialEq> SelectItem for Item<V> {
    type Value = V;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &V {
        &self.value
    }
}

pub struct Picker<V: Clone + PartialEq + 'static> {
    pub state: Entity<SelectState<Vec<Item<V>>>>,
    /// What the select lists now, to leave it alone when nothing changed.
    items: Vec<(V, String)>,
}

impl<V: Clone + PartialEq + 'static> Picker<V> {
    pub fn new<T: 'static>(picked: impl Fn(&V, &mut Window, &mut App) + 'static, window: &mut Window, cx: &mut Context<T>) -> Self {
        let state = cx.new(|cx| SelectState::new(Vec::<Item<V>>::new(), None, window, cx));
        window
            .subscribe(&state, cx, move |_, event: &SelectEvent<Vec<Item<V>>>, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    picked(value, window, cx);
                }
            })
            .detach();
        Self { state, items: Vec::new() }
    }

    /// Lists `items` and shows `chosen`, changing only what differs.
    pub fn sync(&mut self, items: Vec<(V, String)>, chosen: Option<&V>, window: &mut Window, cx: &mut App) {
        if self.items != items {
            let list = items.iter().map(|(value, label)| Item { value: value.clone(), label: SharedString::from(label.clone()) }).collect();
            self.state.update(cx, |state, cx| state.set_items(list, window, cx));
            self.items = items;
        }
        if let Some(value) = chosen {
            if self.state.read(cx).selected_value() != Some(value) {
                self.state.update(cx, |state, cx| state.set_selected_value(value, window, cx));
            }
        }
    }

    /// How many options it lists: a picker with none is disabled.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
