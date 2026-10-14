//! The window's keys, said once: each binding's keys, where it works, what
//! it does and what it is called.
//!
//! Dispatch reads this table on both sides — `session` for a key pressed in
//! a pane, the page for one pressed anywhere else — and it crosses to the
//! page whole, grouped as the shortcuts sheet will list it, so the sheet and
//! the palette are drawn from the bindings dispatch runs rather than from a
//! copy that could drift, as the terminal client's `app/mode/keymap` is.
//!
//! Inside a pane every key is the program's except the leader, Ctrl-Space as
//! in the terminal client: the key after it is the window's, and a second
//! leader sends one through.

use serde::{Deserialize, Serialize};

use crate::keys::PageKey;

/// One key, as the page reports it. `key` is `KeyboardEvent.key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    pub key: &'static str,
    pub ctrl: bool,
    pub alt: bool,
    /// Only asked of a named key: a character carries its own shift (`A` is
    /// shift-a), and which characters need shift depends on the layout.
    pub shift: bool,
    pub meta: bool,
    /// Pressed after the leader.
    pub leader: bool,
}

/// Where a binding works: the shortcuts sheet's groups, in its order. Rail
/// and Review join them when those surfaces take keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// Wherever the keyboard is not in a pane or a text field.
    Anywhere,
    /// In a pane, where `session` dispatches; everywhere else the page does.
    Pane,
    /// The Feature stage's index.
    Feature,
    /// The feature under the cursor, in the list or on the board.
    Card,
    /// The Checkouts stage's table.
    Checkouts,
    /// A checkout's + menu while it is open, which holds the keys.
    Start,
}

/// What a binding does. The page runs those of the places it dispatches,
/// `session` those of a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Workspace,
    Feature,
    CloseLayer,
    /// The leader itself, sent to the program.
    Leader,
    Fill,
    Close,
    Paste,
    PageUp,
    PageDown,
    Next,
    Previous,
    First,
    Last,
    Layout,
    Folded,
    Filter,
    Move,
    Hold,
    Archive,
    GoToPane,
    StartShell,
    /// The daemon's default agent: its first template.
    StartAgent,
    /// What the highlighted item of a menu does, or Enter on a row.
    Choose,
    Checkouts,
    /// Show or hide the branches no checkout is on.
    Branches,
    Fetch,
    Pull,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub chords: &'static [Chord],
    #[serde(skip)]
    pub place: Place,
    pub action: Action,
    /// What the sheet and the palette call it.
    pub label: &'static str,
}

/// One heading of the sheet and the bindings under it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub place: Place,
    pub title: &'static str,
    pub bindings: Vec<&'static Binding>,
}

impl Chord {
    const fn key(key: &'static str) -> Chord {
        Chord { key, ctrl: false, alt: false, shift: false, meta: false, leader: false }
    }

    const fn ctrl(self) -> Chord {
        Chord { ctrl: true, ..self }
    }

    const fn alt(self) -> Chord {
        Chord { alt: true, ..self }
    }

    const fn shift(self) -> Chord {
        Chord { shift: true, ..self }
    }

    const fn meta(self) -> Chord {
        Chord { meta: true, ..self }
    }

    const fn after_leader(self) -> Chord {
        Chord { leader: true, ..self }
    }

    /// Whether `page` is this key, the leader aside. The page's `matches`
    /// in `page/src/keymap.ts` is this rule again, since the page must
    /// decide before Rust hears the key whether the webview may have it.
    pub fn matches(&self, page: &PageKey) -> bool {
        let character = self.key.chars().count() == 1;
        page.key == self.key
            && (character || page.shift == self.shift)
            && page.ctrl == self.ctrl
            && page.alt == self.alt
            && page.meta == self.meta
    }
}

impl Place {
    fn title(self) -> &'static str {
        match self {
            Place::Anywhere => "Anywhere",
            Place::Pane => "In a pane",
            Place::Feature => "Feature",
            Place::Card => "Board card",
            Place::Checkouts => "Checkouts",
            Place::Start => "A checkout's + menu",
        }
    }
}

pub const LEADER: Chord = Chord::key(" ").ctrl();

/// The command key is the paste chord on a Mac. Elsewhere Ctrl-V is, as in
/// the terminal client, with Ctrl-Shift-V and Alt-V, the keys a terminal on
/// Windows keeps for itself.
const PASTE: &[Chord] = if cfg!(target_os = "macos") {
    &[Chord::key("v").meta()]
} else {
    &[Chord::key("v").ctrl(), Chord::key("V").ctrl(), Chord::key("v").alt()]
};

const fn bind(place: Place, chords: &'static [Chord], action: Action, label: &'static str) -> Binding {
    Binding { chords, place, action, label }
}

use Action::*;
use Place::{Anywhere, Card, Pane, Start};
use Place::Checkouts as CheckoutsTable;

pub static BINDINGS: &[Binding] = &[
    bind(Anywhere, &[Chord::key("1")], Workspace, "Workspace"),
    bind(Anywhere, &[Chord::key("2")], Feature, "Feature"),
    bind(Anywhere, &[Chord::key("3")], Action::Checkouts, "Checkouts"),
    bind(Anywhere, &[Chord::key("Escape")], CloseLayer, "close the menu or dialog"),
    bind(Pane, &[LEADER.after_leader()], Leader, "send ctrl-space to the program"),
    bind(Pane, &[Chord::key("f").after_leader()], Fill, "fill the window with the pane, and back"),
    bind(Pane, &[Chord::key("1").after_leader()], Workspace, "Workspace"),
    bind(Pane, &[Chord::key("2").after_leader()], Feature, "Feature"),
    bind(Pane, &[Chord::key("x").after_leader()], Close, "close the pane, ending its program"),
    bind(Pane, &[Chord::key("3").after_leader()], Action::Checkouts, "Checkouts"),
    bind(Pane, PASTE, Paste, "paste; a picture or a copied file pastes as a file"),
    bind(Pane, &[Chord::key("PageUp").shift()], PageUp, "back through the history"),
    bind(Pane, &[Chord::key("PageDown").shift()], PageDown, "forward to the live screen"),
    bind(Place::Feature, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next feature"),
    bind(Place::Feature, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous feature"),
    bind(Place::Feature, &[Chord::key("Home")], First, "first feature"),
    bind(Place::Feature, &[Chord::key("End")], Last, "last feature"),
    bind(Place::Feature, &[Chord::key("v"), Chord::key("V")], Layout, "list or board"),
    bind(Place::Feature, &[Chord::key("z")], Folded, "show held and archived, or fold them"),
    bind(Place::Feature, &[Chord::key("/")], Filter, "filter by title or number"),
    bind(Card, &[Chord::key("m"), Chord::key("M")], Move, "move to…"),
    bind(Card, &[Chord::key("h"), Chord::key("H")], Hold, "hold, or lift its hold"),
    bind(Card, &[Chord::key("A")], Archive, "archive, or reopen"),
    bind(Card, &[Chord::key("g"), Chord::key("G")], GoToPane, "go to its agent's pane"),
    bind(CheckoutsTable, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next row"),
    bind(CheckoutsTable, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous row"),
    bind(CheckoutsTable, &[Chord::key("Home")], First, "first row"),
    bind(CheckoutsTable, &[Chord::key("End")], Last, "last row"),
    bind(CheckoutsTable, &[Chord::key("Enter")], Choose, "open the checkout, or switch the primary checkout to the branch"),
    bind(CheckoutsTable, &[Chord::key("b"), Chord::key("B")], Branches, "show or hide the branches without a checkout"),
    bind(CheckoutsTable, &[Chord::key("/")], Filter, "filter by checkout or branch"),
    bind(CheckoutsTable, &[Chord::key("f"), Chord::key("F")], Fetch, "fetch every remote and prune"),
    bind(CheckoutsTable, &[Chord::key("p"), Chord::key("P")], Pull, "pull the checkout, fast-forward only"),
    bind(CheckoutsTable, &[Chord::key("s"), Chord::key("S")], StartShell, "start a shell in the checkout"),
    bind(CheckoutsTable, &[Chord::key("a"), Chord::key("A")], StartAgent, "start the default agent in the checkout"),
    bind(Start, &[Chord::key("s"), Chord::key("S")], StartShell, "start a shell"),
    bind(Start, &[Chord::key("a"), Chord::key("A")], StartAgent, "start the default agent"),
    bind(Start, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next item"),
    bind(Start, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous item"),
    bind(Start, &[Chord::key("Enter")], Choose, "start the highlighted one"),
];

const ORDER: [Place; 6] = [Anywhere, Pane, Place::Feature, Card, CheckoutsTable, Start];

/// The table as the sheet lists it: a group per place, in the sheet's order.
pub fn groups() -> Vec<Group> {
    ORDER
        .iter()
        .map(|&place| Group {
            place,
            title: place.title(),
            bindings: BINDINGS.iter().filter(|b| b.place == place).collect(),
        })
        .collect()
}

pub fn is_leader(page: &PageKey) -> bool {
    LEADER.matches(page)
}

/// What a key pressed in a pane does, if a binding takes it: one after the
/// leader when `after_leader`, else one the pane keeps from its program.
pub fn in_pane(page: &PageKey, after_leader: bool) -> Option<Action> {
    BINDINGS
        .iter()
        .filter(|b| b.place == Pane)
        .find(|b| b.chords.iter().any(|c| c.leader == after_leader && c.matches(page)))
        .map(|b| b.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(key: &str) -> PageKey {
        PageKey { key: key.into(), ..PageKey::default() }
    }

    #[test]
    fn every_binding_has_a_label_and_a_key() {
        for binding in BINDINGS {
            assert!(!binding.label.is_empty(), "{:?}", binding.action);
            assert!(!binding.chords.is_empty(), "{}", binding.label);
        }
    }

    #[test]
    fn every_place_has_a_group_and_every_binding_is_in_one() {
        let groups = groups();
        let listed: usize = groups.iter().map(|g| g.bindings.len()).sum();
        assert_eq!(listed, BINDINGS.len());
        assert!(groups.iter().all(|g| !g.bindings.is_empty()), "an empty heading");
        let titles: Vec<_> = groups.iter().map(|g| g.title).collect();
        assert_eq!(titles, ["Anywhere", "In a pane", "Feature", "Board card", "Checkouts", "A checkout's + menu"]);
    }

    /// The page hears Anywhere, Feature and Card together, so a key may
    /// be bound once among them; a pane's keys are a table of their own,
    /// and an open + menu hears only its own and Anywhere's.
    #[test]
    fn no_key_is_bound_twice_where_it_is_heard() {
        let heard_together: [&[Place]; 4] =
            [&[Anywhere, Place::Feature, Card], &[Pane], &[Anywhere, CheckoutsTable], &[Anywhere, Start]];
        for places in heard_together {
            let chords: Vec<&Chord> = BINDINGS
                .iter()
                .filter(|b| places.contains(&b.place))
                .flat_map(|b| b.chords)
                .collect();
            for (i, chord) in chords.iter().enumerate() {
                assert!(!chords[i + 1..].contains(chord), "{chord:?} is bound twice");
            }
        }
    }

    #[test]
    fn only_a_pane_has_a_leader() {
        for binding in BINDINGS.iter().filter(|b| b.place != Pane) {
            assert!(binding.chords.iter().all(|c| !c.leader), "{}", binding.label);
        }
    }

    #[test]
    fn the_leader_is_ctrl_space_with_or_without_shift() {
        assert!(is_leader(&PageKey { ctrl: true, ..page(" ") }));
        assert!(is_leader(&PageKey { ctrl: true, shift: true, ..page(" ") }));
        assert!(!is_leader(&page(" ")), "a space is typed");
    }

    #[test]
    fn a_pane_keeps_its_keys_until_the_leader() {
        assert_eq!(in_pane(&page("f"), false), None);
        assert_eq!(in_pane(&page("f"), true), Some(Fill));
        assert_eq!(in_pane(&page("2"), true), Some(Action::Feature));
        assert_eq!(in_pane(&page("x"), true), Some(Close));
        assert_eq!(in_pane(&page("x"), false), None, "an x is typed");
        assert_eq!(in_pane(&page("j"), true), None, "the Feature stage's key is not a pane's");
    }

    #[test]
    fn a_named_key_asks_for_its_shift_and_a_character_does_not() {
        let shift_page_up = PageKey { shift: true, ..page("PageUp") };
        assert_eq!(in_pane(&shift_page_up, false), Some(PageUp));
        assert_eq!(in_pane(&page("PageUp"), false), None, "the program's own paging key");
        assert!(Chord::key("A").matches(&PageKey { shift: true, ..page("A") }));
        assert!(!Chord::key("A").matches(&PageKey { ctrl: true, ..page("A") }));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_paste_chords_are_a_terminals() {
        let ctrl_v = PageKey { ctrl: true, ..page("v") };
        let ctrl_shift_v = PageKey { ctrl: true, shift: true, ..page("V") };
        let alt_v = PageKey { alt: true, ..page("v") };
        for key in [ctrl_v, ctrl_shift_v, alt_v] {
            assert_eq!(in_pane(&key, false), Some(Paste), "{key:?}");
        }
        assert_eq!(in_pane(&PageKey { alt: true, shift: true, ..page("V") }, false), None);
    }

    #[test]
    fn a_group_crosses_to_the_page_with_its_chords_and_labels() {
        let value = serde_json::to_value(&groups()[1]).unwrap();
        assert_eq!(value["place"], "pane");
        assert_eq!(value["title"], "In a pane");
        let fill = &value["bindings"][1];
        assert_eq!(fill["action"], "fill");
        assert_eq!(fill["chords"][0]["key"], "f");
        assert_eq!(fill["chords"][0]["leader"], true);
        assert!(fill.get("place").is_none(), "the group says where");
    }
}
