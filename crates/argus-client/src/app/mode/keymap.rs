//! What each mode's keys are, said once: the keys, what `?` and the status
//! bar call them, and what they run.
//!
//! Dispatch, the bar and `?` used to carry a copy each, and the copies
//! drifted — `u` reached dispatch and `?` and never the Panes bar. Now a
//! mode's keys are one table: dispatch looks a key up in it, `?` lists it,
//! and the bar is read off the hints it carries. Where the keys are typed
//! text or a leader chord, dispatch stays written out in `app/input` and the
//! table here is only what `?` says about them.

use super::*;
use crossterm::event::KeyCode::{
    BackTab, Char, Down, End, Enter, Esc, Home, Left, PageDown, PageUp, Right, Tab, Up,
};

/// Some keys, and the one action they all run.
type Run = (&'static [KeyCode], fn(&mut App));

/// One line of `?`, whatever the bar offers for it, and the keys behind it.
/// A line can carry several keys that run different things — `j / k` is one
/// line and two directions.
pub struct Binding {
    keys: &'static str,
    help: &'static str,
    shown: Shown,
    /// Whether it applies in the state the mode is in. A key a panel does
    /// not take falls through to nothing there, as an unbound one would.
    when: Option<fn(&App) -> bool>,
    /// Each key and what it runs. Empty for a line about the mouse, or about
    /// keys a hand-written handler takes.
    runs: &'static [Run],
}

/// Where a binding is offered.
enum Shown {
    /// On the bar, as these hints, and in `?`. Left empty it is a binding
    /// nobody has placed yet, and the keymap test says so.
    Bar(&'static [Hint]),
    /// On a bar written out by hand in `ui/status`, because what that bar
    /// says depends on more than the mode, and in `?`.
    WrittenBar,
    /// In `?` only: worth knowing, not worth the room on the bar.
    Help,
    /// In neither.
    Nowhere,
}

/// What the bar says for a binding, and how hard it holds on to the room.
pub struct Hint {
    wide: &'static str,
    narrow: &'static str,
    /// How many of the narrower tiers keep it: 0 is the widest tier only.
    keep: u8,
    when: Option<fn(&App) -> bool>,
}

/// One heading of `?` and the bindings under it. Grouped by what the key
/// acts on rather than alphabetically: every binding sorted by character is
/// a reference, and what somebody pressing `?` wants is an answer.
pub struct Group {
    title: &'static str,
    bindings: &'static [Binding],
}

/// Everything one mode's keys are.
pub struct Keymap {
    groups: &'static [&'static Group],
    /// Between hints in the widest tier. The narrower tiers close up to two
    /// spaces, since they only exist because room ran out.
    gap: &'static str,
}

const ROOMY: &str = "   ";
const TIGHT: &str = "  ";

impl Binding {
    const fn new(keys: &'static str, help: &'static str, runs: &'static [Run]) -> Binding {
        Binding {
            keys,
            help,
            shown: Shown::Bar(&[]),
            when: None,
            runs,
        }
    }

    /// A line of `?` with no key of its own behind it.
    const fn note(keys: &'static str, help: &'static str) -> Binding {
        Binding::new(keys, help, &[]).help_only()
    }

    const fn hints(self, hints: &'static [Hint]) -> Binding {
        Binding {
            shown: Shown::Bar(hints),
            ..self
        }
    }

    const fn written(self) -> Binding {
        Binding {
            shown: Shown::WrittenBar,
            ..self
        }
    }

    const fn help_only(self) -> Binding {
        Binding {
            shown: Shown::Help,
            ..self
        }
    }

    const fn hidden(self) -> Binding {
        Binding {
            shown: Shown::Nowhere,
            ..self
        }
    }

    const fn when(self, when: fn(&App) -> bool) -> Binding {
        Binding {
            when: Some(when),
            ..self
        }
    }

    /// Every key code dispatch answers to for this line.
    #[cfg(test)]
    pub fn codes(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.runs.iter().flat_map(|(codes, _)| codes.iter().copied())
    }

    #[cfg(test)]
    pub fn help(&self) -> &'static str {
        self.help
    }

    pub fn in_help(&self) -> bool {
        !matches!(self.shown, Shown::Nowhere)
    }

    #[cfg(test)]
    pub fn on_bar(&self) -> bool {
        matches!(self.shown, Shown::Bar(_) | Shown::WrittenBar)
    }

    /// Whether it only applies in some states of its mode, which is the one
    /// way two bindings may share a key.
    #[cfg(test)]
    pub fn guarded(&self) -> bool {
        self.when.is_some()
    }
}

impl Hint {
    const fn new(label: &'static str, keep: u8) -> Hint {
        Hint {
            wide: label,
            narrow: label,
            keep,
            when: None,
        }
    }

    /// What it shrinks to once the bar is short of room.
    const fn narrow(self, narrow: &'static str) -> Hint {
        Hint { narrow, ..self }
    }

    const fn when(self, when: fn(&App) -> bool) -> Hint {
        Hint {
            when: Some(when),
            ..self
        }
    }
}

impl Group {
    pub fn title(&self) -> &'static str {
        self.title
    }

    /// The lines `?` shows, as keys and what they do.
    pub fn listed(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.bindings
            .iter()
            .filter(|binding| binding.in_help())
            .map(|binding| (binding.keys, binding.help))
    }
}

impl Keymap {
    /// The groups `?` lists here, ending with what is true everywhere.
    pub fn help(&self) -> impl Iterator<Item = &'static Group> + '_ {
        self.groups.iter().copied().chain([&EVERYWHERE])
    }

    pub fn bindings(&self) -> impl Iterator<Item = &'static Binding> + '_ {
        self.groups.iter().flat_map(|group| group.bindings)
    }

    /// The bar's tiers, widest first, for the state the app is in. The
    /// widest spells every hint out; each narrower one drops the hints that
    /// do not keep that far, in the order the keymap declares them.
    pub fn tiers(&self, app: &App) -> Vec<String> {
        let hints: Vec<&Hint> = self
            .bindings()
            .flat_map(|binding| match binding.shown {
                Shown::Bar(hints) => hints,
                _ => &[],
            })
            .filter(|hint| hint.when.is_none_or(|when| when(app)))
            .collect();
        let deepest = hints.iter().map(|hint| hint.keep).max().unwrap_or(0);

        let mut tiers = vec![hints
            .iter()
            .map(|hint| hint.wide)
            .collect::<Vec<_>>()
            .join(self.gap)];
        for depth in 1..=deepest {
            let tier = hints
                .iter()
                .filter(|hint| hint.keep >= depth)
                .map(|hint| hint.narrow)
                .collect::<Vec<_>>()
                .join(TIGHT);
            tiers.push(tier);
        }
        tiers.dedup();
        tiers
    }

    /// What `code` runs here, if a binding that applies right now takes it.
    /// The first one wins, the way the first arm of a match would.
    fn action(&self, app: &App, code: KeyCode) -> Option<fn(&mut App)> {
        self.bindings()
            .filter(|binding| binding.when.is_none_or(|when| when(app)))
            .flat_map(|binding| binding.runs)
            .find(|(codes, _)| codes.contains(&code))
            .map(|&(_, run)| run)
    }
}

impl Mode {
    /// The keymap this mode declares. Exhaustive for the same reason
    /// [`App::mode`] is: a mode with no keymap does not compile.
    pub fn keymap(self) -> &'static Keymap {
        match self {
            Mode::Prompt => &PROMPT,
            Mode::DirPicker => &DIR_PICKER,
            Mode::Picker => &PICKER,
            Mode::CheckoutFilter => &CHECKOUT_FILTER,
            Mode::Overlay(OverlayMode::Review) => &REVIEW,
            Mode::Overlay(OverlayMode::History) => &HISTORY,
            Mode::Overlay(OverlayMode::Brief) => &BRIEF,
            Mode::Overlay(OverlayMode::Settings) => &SETTINGS,
            Mode::Overlay(OverlayMode::SequenceDiagram) => &DIAGRAM,
            Mode::Overlay(OverlayMode::Pane) | Mode::Pane => &PANE,
            Mode::Stage(View::Panes) => &PANES,
            Mode::Stage(View::Checkouts) => &CHECKOUTS,
            Mode::Stage(View::Feature) => &FEATURE,
            // The workspace stage is the rail and the live pane; with the
            // keys on neither it is still the rail's keys that apply.
            Mode::Stage(View::Workspace) | Mode::Rail => &RAIL,
        }
    }
}

impl App {
    /// Runs what `key` is bound to in the mode that has the keys. False when
    /// nothing here takes it, so a handler can decide what that means.
    pub(in crate::app) fn press(&mut self, key: KeyEvent) -> bool {
        let Some(run) = self.mode().keymap().action(self, key.code) else {
            return false;
        };
        run(self);
        true
    }
}

// --- the stages' shared keys ----------------------------------------------

const TO_WORKSPACE: Binding = Binding::new(
    "1",
    "workspace — the rail and the live pane",
    &[(&[Char('1')], |app| app.open_view(View::Workspace))],
)
.help_only();
const TO_FEATURE: Binding = Binding::new(
    "2",
    "feature — brief, tasks, and decisions",
    &[(&[Char('2')], |app| app.open_view(View::Feature))],
)
.help_only();
const TO_PANES: Binding = Binding::new(
    "3",
    "panes — every pane as a card",
    &[(&[Char('3')], |app| app.open_view(View::Panes))],
)
.help_only();
const TO_CHECKOUTS: Binding = Binding::new(
    "4",
    "checkouts — branches and worktrees",
    &[(&[Char('4')], |app| app.open_view(View::Checkouts))],
)
.help_only();

/// The stages only take the digits: switching workspace, host or theme is
/// the rail's, where the keys go back to whenever the workspace is shown.
const VIEWS: Group = Group {
    title: "the view",
    bindings: &[TO_WORKSPACE, TO_FEATURE, TO_PANES, TO_CHECKOUTS],
};

// --- the rail -------------------------------------------------------------

fn on_projects(app: &App) -> bool {
    app.focus == Focus::Projects
}

fn on_repositories(app: &App) -> bool {
    app.focus == Focus::Repositories
}

fn on_checkouts(app: &App) -> bool {
    app.focus == Focus::Checkouts
}

/// The pane rows, and anywhere else that leaves the keys with the rail.
fn on_panes(app: &App) -> bool {
    !matches!(
        app.focus,
        Focus::Projects | Focus::Repositories | Focus::Checkouts
    )
}

/// The bar is per row rather than one list of everything: it cannot hold
/// every key at once, and most of them only apply somewhere, so each hint
/// says which rows it is offered on and how long it lasts there.
const RAIL: Keymap = Keymap {
    groups: &[
        &Group {
            title: "moving",
            bindings: &[
                Binding::new(
                    "j / k",
                    "up and down the rail, in the order it is drawn",
                    &[
                        (&[Char('j'), Down], |app| app.step_rail(1)),
                        (&[Char('k'), Up], |app| app.step_rail(-1)),
                    ],
                )
                .hints(&[
                    Hint::new("j/k", 1).when(on_checkouts),
                    Hint::new("j/k", 0).when(|app| !on_checkouts(app)),
                ]),
                Binding::new(
                    "l  enter",
                    "into the row under the cursor",
                    &[(&[Char('l'), Enter, Right], App::enter_rail_row)],
                )
                .hints(&[Hint::new("l open", 2)]),
                Binding::new(
                    "h  esc",
                    "up to the row it hangs under",
                    &[(&[Char('h'), Left, Esc], App::ascend)],
                )
                .help_only(),
                Binding::new(
                    "N",
                    "jump to whatever needs attention",
                    &[(&[Char('N')], App::jump_to_next_attention)],
                )
                .help_only(),
            ],
        },
        &Group {
            title: "the thing under the cursor",
            bindings: &[
                Binding::new("s", "a shell here", &[(&[Char('s')], App::spawn_shell)]).hints(&[
                    Hint::new("s shell", 1).when(on_repositories),
                    Hint::new("s shell", 0).when(on_panes),
                ]),
                Binding::new("a", "an agent here", &[(&[Char('a')], App::open_picker)]).hints(&[
                    Hint::new("a agent", 2).when(on_repositories),
                    Hint::new("a agent", 1).when(on_panes),
                ]),
                Binding::new(
                    "b",
                    "switch branch",
                    &[(&[Char('b')], App::open_branch_picker)],
                )
                .hints(&[
                    Hint::new("b branch", 0).when(|app| on_repositories(app) || on_checkouts(app))
                ]),
                Binding::new(
                    "B",
                    "also list branches nothing is on",
                    &[(&[Char('B')], App::toggle_branches)],
                )
                .help_only(),
                // Only live when a click on the checkouts table has handed
                // the keys to the rail while that table is still up; the
                // table's own `/` is the one worth offering.
                Binding::new(
                    "/",
                    "filter branches by name",
                    &[(&[Char('/')], App::begin_checkout_filter)],
                )
                .when(App::checkouts_filterable)
                .hidden(),
                Binding::new("F", "fetch", &[(&[Char('F')], App::fetch)])
                    .hints(&[Hint::new("F fetch", 0).when(on_checkouts)]),
                Binding::new("P", "pull", &[(&[Char('P')], App::pull)]).help_only(),
                Binding::new("f", "open a file", &[(&[Char('f')], App::open_file_picker)])
                    .help_only(),
                Binding::new(
                    "R  tab",
                    "review the diff",
                    &[(&[Char('R'), Tab], App::open_review)],
                )
                .hints(&[Hint::new("R review", 2).when(|app| on_checkouts(app) || on_panes(app))]),
                Binding::new("H", "history", &[(&[Char('H')], App::open_history)])
                    .hints(&[Hint::new("H history", 1).when(on_checkouts)]),
                Binding::new("n", "add one", &[(&[Char('n')], App::new_prompt)]).hints(&[
                    Hint::new("n add", 2).when(on_projects),
                    Hint::new("n add", 0).when(on_repositories),
                ]),
                Binding::new(
                    "i",
                    "init a repository",
                    &[(&[Char('i')], App::new_repository_prompt)],
                )
                .help_only(),
                Binding::new("D", "remove it", &[(&[Char('D')], App::remove_prompt)])
                    .hints(&[Hint::new("D rm", 0).when(on_projects)]),
                Binding::new("x", "close the pane", &[(&[Char('x')], App::kill_selected)])
                    .hints(&[Hint::new("x close", 1).when(on_panes)]),
            ],
        },
        &Group {
            title: "the rail",
            bindings: &[
                Binding::new(
                    "o  click project",
                    "switch to another project",
                    &[(&[Char('o')], App::open_project_picker)],
                )
                .hints(&[Hint::new("o switch", 1).when(on_projects)]),
                Binding::note("click repo", "open it; the one open before folds away"),
                Binding::note("click pane", "show it, keys stay here — x closes it"),
                Binding::note("enter  click", "type into the pane shown"),
                Binding::note("wheel", "move along the rail"),
                Binding::note("↑ ↓ + !", "a checkout's ahead, behind, staged, unstaged"),
            ],
        },
        &Group {
            title: "the view",
            bindings: &[
                TO_WORKSPACE,
                TO_FEATURE,
                TO_PANES,
                TO_CHECKOUTS,
                Binding::new("t", "theme", &[(&[Char('t')], App::open_theme_picker)]).help_only(),
                Binding::new(
                    "w",
                    "workspace",
                    &[(&[Char('w')], App::open_workspace_picker)],
                )
                .hints(&[Hint::new("w wksp", 0).when(on_projects)]),
                Binding::new(
                    "W",
                    "host — this machine or one over ssh",
                    &[(&[Char('W')], App::open_host_picker)],
                )
                .hints(&[Hint::new("W host", 0).when(on_projects)]),
                Binding::new("S", "settings", &[(&[Char('S')], App::open_settings)]).help_only(),
                Binding::new(
                    "q",
                    "detach — the daemon and its panes keep running",
                    &[(&[Char('q')], |app| app.should_quit = true)],
                )
                .help_only(),
            ],
        },
    ],
    gap: TIGHT,
};

// --- the stages -------------------------------------------------------------

const PANES: Keymap = Keymap {
    groups: &[
        &Group {
            title: "panes",
            bindings: &[
                Binding::new(
                    "j / k",
                    "card by card",
                    &[
                        (&[Char('j'), Down], |app| app.move_overview_pane(1)),
                        (&[Char('k'), Up], |app| app.move_overview_pane(-1)),
                    ],
                )
                .hints(&[Hint::new("j/k move", 1).narrow("j/k")]),
                Binding::new(
                    "enter  l  click",
                    "open it and type into it",
                    &[(&[Enter, Char('l'), Right], |app| {
                        app.open_view(View::Workspace);
                        app.focus = Focus::PaneContent;
                    })],
                )
                .hints(&[Hint::new("enter open", 1)]),
                Binding::new(
                    "A",
                    "this repository, or the whole workspace",
                    &[(&[Char('A')], |app| app.show_all_panes = !app.show_all_panes)],
                )
                .hints(&[Hint::new("A all", 1)]),
                Binding::new("a", "an agent here", &[(&[Char('a')], App::open_picker)])
                    .hints(&[Hint::new("a agent", 0)]),
                Binding::new("s", "a shell here", &[(&[Char('s')], App::spawn_shell)])
                    .hints(&[Hint::new("s shell", 0)]),
                Binding::new(
                    "u",
                    "take back the newest message queued from a phone",
                    &[(&[Char('u')], App::unqueue_selected)],
                )
                .hints(&[Hint::new("u take back", 0)]),
                Binding::new(
                    "esc  q",
                    "back to the workspace",
                    &[(&[Esc, Char('q')], |app| app.open_view(View::Workspace))],
                )
                .hints(&[Hint::new("q workspace", 1).narrow("q")]),
            ],
        },
        &VIEWS,
    ],
    gap: ROOMY,
};

const CHECKOUTS: Keymap = Keymap {
    groups: &[
        &Group {
            title: "checkouts",
            bindings: &[
                Binding::new(
                    "j / k  click",
                    "row by row",
                    &[
                        (&[Char('j'), Down], |app| app.step_checkout_table(1)),
                        (&[Char('k'), Up], |app| app.step_checkout_table(-1)),
                    ],
                )
                .hints(&[Hint::new("j/k move", 3).narrow("j/k")]),
                Binding::new(
                    "/",
                    "filter branches by name",
                    &[(&[Char('/')], App::begin_checkout_filter)],
                )
                .hints(&[Hint::new("/ filter", 1)]),
                Binding::new(
                    "enter  l",
                    "open this checkout, or switch to this branch",
                    &[(&[Enter, Char('l'), Right], App::enter_checkout_row)],
                )
                .hints(&[Hint::new("enter open", 3)]),
                Binding::new(
                    "R  tab",
                    "review the diff",
                    &[(&[Char('R'), Tab], App::open_review)],
                )
                .hints(&[Hint::new("R review", 2)]),
                Binding::new("H", "history", &[(&[Char('H')], App::open_history)])
                    .hints(&[Hint::new("H history", 1)]),
                Binding::new(
                    "B",
                    "also list branches nothing is on",
                    &[(&[Char('B')], App::toggle_branches)],
                )
                .help_only(),
                Binding::new("a", "an agent here", &[(&[Char('a')], App::open_picker)])
                    .help_only(),
                Binding::new("s", "a shell here", &[(&[Char('s')], App::spawn_shell)])
                    .help_only(),
                Binding::new(
                    "m  b",
                    "checkout: put it on another branch",
                    &[(&[Char('m'), Char('b')], App::open_branch_picker)],
                )
                .hints(&[Hint::new("m checkout", 0)]),
                Binding::new("n", "a new worktree", &[(&[Char('n')], App::new_prompt)])
                    .hints(&[Hint::new("n worktree", 0)]),
                Binding::new("D", "remove it", &[(&[Char('D')], App::remove_prompt)])
                    .hints(&[Hint::new("D remove", 0)]),
                Binding::new(
                    "F  P",
                    "fetch, pull",
                    &[(&[Char('F')], App::fetch), (&[Char('P')], App::pull)],
                )
                .help_only(),
                Binding::note("↑ ↓ + !", "state: ahead, behind, staged, unstaged"),
                Binding::new(
                    "esc  q",
                    "back to the workspace",
                    &[(&[Esc, Char('q')], |app| app.open_view(View::Workspace))],
                )
                .hints(&[Hint::new("q workspace", 3).narrow("q")]),
            ],
        },
        &VIEWS,
    ],
    gap: ROOMY,
};

fn in_features(app: &App) -> bool {
    app.panel == FeaturePanel::Features
}

fn in_tasks(app: &App) -> bool {
    app.panel == FeaturePanel::Tasks
}

/// The keys are one set rather than a set per panel: `j`/`k` moves in
/// whichever panel has them and `Tab` crosses between panels, so stepping
/// from the feature list into its tasks does not change what the keys mean
/// — only what they act on. Its bar is written out per panel in
/// `ui/status`, since what a key is called there depends on the panel.
const FEATURE: Keymap = Keymap {
    groups: &[
        &Group {
            title: "a feature",
            bindings: &[
                Binding::new(
                    "h / l",
                    "the features, and the feature under the cursor",
                    &[
                        (&[Char('h'), Left], |app| app.go_to_panel(FeaturePanel::Features)),
                        (&[Char('l'), Right], App::enter_feature),
                    ],
                )
                .written(),
                Binding::new(
                    "tab",
                    "features, tasks, diagrams, decisions in turn",
                    &[
                        (&[Tab], |app| app.step_panel(1)),
                        (&[BackTab], |app| app.step_panel(-1)),
                    ],
                )
                .help_only(),
                Binding::new(
                    "j / k",
                    "row by row, in whichever panel has the keys",
                    &[
                        (&[Char('j'), Down], |app| app.move_in_feature(1)),
                        (&[Char('k'), Up], |app| app.move_in_feature(-1)),
                    ],
                )
                .written(),
                Binding::new(
                    "d / u",
                    "ten at a time",
                    &[
                        (&[Char('d'), PageDown], |app| app.move_in_feature(10)),
                        (&[Char('u'), PageUp], |app| app.move_in_feature(-10)),
                    ],
                )
                .written(),
                Binding::new(
                    "g / G",
                    "top and bottom",
                    &[
                        (&[Char('g'), Home], |app| app.move_in_feature(i32::MIN)),
                        (&[Char('G'), End], |app| app.move_in_feature(i32::MAX)),
                    ],
                )
                .written(),
                Binding::new(
                    "a",
                    "a new feature, or a new root task under one",
                    &[(&[Char('a')], App::add_in_feature)],
                )
                .written(),
                Binding::new(
                    "s",
                    "a subtask under the selected task",
                    &[(&[Char('s')], App::begin_subtask)],
                )
                .when(in_tasks)
                .written(),
                Binding::new(
                    "e",
                    "rewrite this feature's brief, or this task's title",
                    &[(&[Char('e')], App::edit_in_feature)],
                )
                .written(),
                Binding::new(
                    "enter",
                    "open this feature or task brief",
                    &[(&[Enter], App::open_in_feature)],
                )
                .written(),
                Binding::new(
                    "R",
                    "rename the feature",
                    &[(&[Char('R')], App::begin_feature_rename)],
                )
                .written(),
                Binding::new(
                    "x",
                    "remove it, keeping its decisions",
                    &[(&[Char('x')], App::drop_in_feature)],
                )
                .written(),
                // Acceptance, and the only state left for anyone to set.
                Binding::new(
                    ".",
                    "accept it, or reopen it",
                    &[(&[Char('.')], App::toggle_selected_feature_done)],
                )
                .written(),
                Binding::new(
                    "v",
                    "switch between active features and accepted history",
                    &[(&[Char('v')], App::toggle_feature_archive)],
                )
                .when(in_features)
                .written(),
                Binding::new(
                    "m",
                    "move this feature to another checkout",
                    &[(&[Char('m')], App::open_feature_checkout_picker)],
                )
                .when(in_features)
                .written(),
                Binding::new(
                    "p",
                    "hold this feature, saying why, or lift its hold",
                    &[(&[Char('p')], App::toggle_selected_feature_hold)],
                )
                .when(in_features)
                .written(),
                Binding::new(
                    "w",
                    "choose a feature this one comes after, or take one back",
                    &[(&[Char('w')], App::open_feature_wait_picker)],
                )
                .when(in_features)
                .written(),
                Binding::new(
                    "H / L",
                    "move this task along todo, doing, done",
                    &[
                        (&[Char('H')], |app| app.move_selected_task(-1)),
                        (&[Char('L')], |app| app.move_selected_task(1)),
                    ],
                )
                .written(),
                Binding::new(
                    "J / K",
                    "earlier or later among sibling tasks",
                    &[
                        (&[Char('J')], |app| app.reorder_selected_task(1)),
                        (&[Char('K')], |app| app.reorder_selected_task(-1)),
                    ],
                )
                .written(),
                Binding::new(
                    "> / <",
                    "under the task above it, or out beside its parent",
                    &[
                        (&[Char('>')], |app| app.indent_selected_task(true)),
                        (&[Char('<')], |app| app.indent_selected_task(false)),
                    ],
                )
                .written(),
                Binding::new(
                    "m",
                    "in tasks: move this task and its subtasks to another feature",
                    &[(&[Char('m')], App::open_task_feature_picker)],
                )
                .when(in_tasks)
                .written(),
                Binding::new(
                    "r",
                    "re-ask the daemon for all of it",
                    &[(&[Char('r')], App::refresh_feature)],
                )
                .written(),
                Binding::note("wheel", "scroll the section under the pointer, brief included"),
                Binding::note("click", "select a feature, task, or decision"),
                Binding::new(
                    "esc  q",
                    "back to the workspace",
                    &[(&[Esc, Char('q')], |app| app.open_view(View::Workspace))],
                )
                .written(),
            ],
        },
        &VIEWS,
    ],
    gap: TIGHT,
};

// --- the floating windows ---------------------------------------------------

/// Whether a diff is loaded. The keys that act on one are not taken
/// without it, so the handler can see the key went nowhere and leave.
fn has_review(app: &App) -> bool {
    app.review.is_some()
}

fn in_review(app: &mut App, act: impl FnOnce(&mut crate::review::ReviewView)) {
    if let Some(view) = &mut app.review {
        act(view);
    }
}

/// A commit reached from the history overlay goes back to it rather than
/// flipping a side that means nothing there.
fn from_history(app: &App) -> bool {
    app.review
        .as_ref()
        .is_some_and(|v| v.review.commit.is_some())
        && app.history.is_some()
}

/// `s` names where it would take you, not where you are, and the fourth
/// slot is `b` or `h` by where the diff came from: the two switches are
/// independent, so each hint says when it applies rather than the bar
/// spelling out every combination.
const REVIEW: Keymap = Keymap {
    groups: &[&Group {
        title: "reading a diff",
        bindings: &[
            Binding::new(
                "j / k",
                "line by line",
                &[
                    (&[Char('j'), Down], |app| in_review(app, |v| v.move_by(1))),
                    (&[Char('k'), Up], |app| in_review(app, |v| v.move_by(-1))),
                ],
            )
            .when(has_review)
            .hints(&[Hint::new("j/k", 1)]),
            Binding::new(
                "d / u",
                "ten at a time",
                &[
                    (&[Char('d'), PageDown], |app| in_review(app, |v| v.move_by(10))),
                    (&[Char('u'), PageUp], |app| in_review(app, |v| v.move_by(-10))),
                ],
            )
            .when(has_review)
            .help_only(),
            Binding::new(
                "] / [",
                "next and previous file",
                &[
                    (&[Char(']')], |app| in_review(app, |v| v.jump_file(true))),
                    (&[Char('[')], |app| in_review(app, |v| v.jump_file(false))),
                ],
            )
            .when(has_review)
            .hints(&[Hint::new("]/[ file", 2)]),
            Binding::new(
                "g / G",
                "top and bottom",
                &[
                    (&[Char('g'), Home], |app| in_review(app, |v| v.top_of_diff())),
                    (&[Char('G'), End], |app| in_review(app, |v| v.bottom_of_diff())),
                ],
            )
            .when(has_review)
            .help_only(),
            Binding::new("f", "jump to a change", &[(&[Char('f')], App::open_change_picker)])
                .hints(&[Hint::new("f jump", 0)]),
            Binding::new(
                "v",
                "mark a range",
                &[(&[Char('V'), Char('v')], |app| in_review(app, |v| v.toggle_mark()))],
            )
            .when(has_review)
            .help_only(),
            Binding::new(
                "c",
                "comment to the agent",
                &[(&[Char('c')], App::comment_on_reviewed_lines)],
            )
            .when(has_review)
            .hints(&[Hint::new("c comment", 2)]),
            Binding::new(
                "e",
                "open it in your editor",
                &[(&[Char('e')], App::open_reviewed_line_in_editor)],
            )
            .when(has_review)
            .hints(&[Hint::new("e edit", 0)]),
            Binding::new(
                "s",
                "split and unified",
                &[(&[Char('s')], App::toggle_review_split)],
            )
            .hints(&[
                Hint::new("s unified", 2).when(|app| app.review_split),
                Hint::new("s split", 2).when(|app| !app.review_split),
            ]),
            Binding::new(
                "b",
                "staged, unstaged, and the branch",
                &[(&[Char('b')], App::flip_review_base)],
            )
            .hints(&[Hint::new("b staged/unstaged", 1).when(|app| !from_history(app))]),
            Binding::new("r  R", "refresh", &[(&[Char('r'), Char('R')], App::refresh_review)])
                .help_only(),
            Binding::new("H", "history", &[(&[Char('H')], App::open_history)]).help_only(),
            Binding::new(
                "h",
                "back to the list",
                &[(&[Char('h'), Left], App::close_review)],
            )
            .hints(&[Hint::new("h history", 1).when(from_history)]),
            Binding::new("esc  q", "close", &[(&[Esc, Char('q')], App::close_overlay)])
                .hints(&[Hint::new("esc close", 2).narrow("esc")]),
        ],
    }],
    gap: TIGHT,
};

/// Whether the commit list is loaded; see [`has_review`].
fn has_history(app: &App) -> bool {
    app.history.is_some()
}

fn in_history(app: &mut App, act: impl FnOnce(&mut crate::history::HistoryView)) {
    if let Some(view) = &mut app.history {
        act(view);
    }
}

const HISTORY: Keymap = Keymap {
    groups: &[&Group {
        title: "reading history",
        bindings: &[
            Binding::new(
                "j / k",
                "commit by commit",
                &[
                    (&[Char('j'), Down], |app| in_history(app, |v| v.move_by(1))),
                    (&[Char('k'), Up], |app| in_history(app, |v| v.move_by(-1))),
                ],
            )
            .when(has_history)
            .hints(&[Hint::new("j/k", 0)]),
            Binding::new(
                "d / u",
                "ten at a time",
                &[
                    (&[Char('d'), PageDown], |app| in_history(app, |v| v.move_by(10))),
                    (&[Char('u'), PageUp], |app| in_history(app, |v| v.move_by(-10))),
                ],
            )
            .when(has_history)
            .help_only(),
            Binding::new(
                "] / [",
                "next and previous commit",
                &[
                    (&[Char(']')], |app| in_history(app, |v| v.jump_commit(true))),
                    (&[Char('[')], |app| in_history(app, |v| v.jump_commit(false))),
                ],
            )
            .when(has_history)
            .hints(&[Hint::new("]/[ commit", 2)]),
            Binding::new(
                "l  enter",
                "its files, then the diff",
                &[(&[Char('l'), Enter, Right], App::drill_into_history)],
            )
            .hints(&[Hint::new("l files/open", 1).narrow("l open")]),
            Binding::new(
                "h",
                "fold it back up",
                &[(&[Char('h'), Left], App::fold_or_close_history)],
            )
            .hints(&[Hint::new("h fold", 1)]),
            Binding::new(
                "g / G",
                "top and bottom",
                &[
                    (&[Char('g'), Home], |app| in_history(app, |v| v.top_of_list())),
                    (&[Char('G'), End], |app| in_history(app, |v| v.bottom_of_list())),
                ],
            )
            .when(has_history)
            .help_only(),
            Binding::new("r", "refresh", &[(&[Char('r'), Char('H')], App::open_history)])
                .hints(&[Hint::new("r refresh", 0)]),
            Binding::new(
                "R",
                "review the working tree instead",
                &[(&[Char('R')], App::open_review)],
            )
            .hints(&[Hint::new("R review", 2)]),
            Binding::new("esc  q", "close", &[(&[Esc, Char('q')], App::close_overlay)])
                .hints(&[Hint::new("esc close", 2).narrow("esc")]),
        ],
    }],
    gap: TIGHT,
};

fn in_brief(app: &mut App, act: impl FnOnce(&mut BriefView)) {
    if let Some(view) = &mut app.brief {
        act(view);
    }
}

/// Reading a brief. Writing one is typing, so insert mode's keys are
/// handled in `app/input` and only described here.
const BRIEF: Keymap = Keymap {
    groups: &[&Group {
        title: "a brief",
        bindings: &[
            Binding::new(
                "j / k  h / l",
                "move the cursor",
                &[
                    (&[Char('j'), Down], |app| in_brief(app, |v| v.move_by(1))),
                    (&[Char('k'), Up], |app| in_brief(app, |v| v.move_by(-1))),
                    (&[Char('h'), Left], |app| in_brief(app, |v| v.move_column(-1))),
                    (&[Char('l'), Right], |app| in_brief(app, |v| v.move_column(1))),
                ],
            )
            .hints(&[Hint::new("j/k move", 1).narrow("j/k")]),
            Binding::new(
                "d / u",
                "ten lines at a time",
                &[
                    (&[Char('d'), PageDown], |app| in_brief(app, |v| v.move_by(10))),
                    (&[Char('u'), PageUp], |app| in_brief(app, |v| v.move_by(-10))),
                ],
            )
            .help_only(),
            Binding::new(
                "g / G",
                "top and bottom",
                &[
                    (&[Char('g')], |app| in_brief(app, BriefView::top)),
                    (&[Char('G')], |app| in_brief(app, BriefView::bottom)),
                ],
            )
            .help_only(),
            Binding::new(
                "0 / $",
                "start and end of the line",
                &[
                    (&[Char('0'), Home], |app| in_brief(app, BriefView::start_of_line)),
                    (&[Char('$'), End], |app| in_brief(app, BriefView::end_of_line)),
                ],
            )
            .help_only(),
            Binding::new(
                "i  a",
                "start typing",
                &[
                    (&[Char('i')], |app| in_brief(app, BriefView::insert_mode)),
                    (&[Char('a')], |app| {
                        in_brief(app, |v| {
                            v.move_column(1);
                            v.insert_mode();
                        })
                    }),
                ],
            )
            .hints(&[Hint::new("i insert", 2)]),
            Binding::new(
                "o",
                "a new line below",
                &[(&[Char('o')], |app| in_brief(app, BriefView::open_below))],
            )
            .hints(&[Hint::new("o new line", 0)]),
            Binding::note("esc", "stop typing, and save"),
            Binding::new(
                "q",
                "close",
                &[(&[Char('q'), Esc], |app| {
                    app.save_brief();
                    app.close_overlay();
                })],
            )
            .hints(&[Hint::new("q close", 2)]),
        ],
    }],
    gap: TIGHT,
};

const SETTINGS: Keymap = Keymap {
    groups: &[&Group {
        title: "settings",
        bindings: &[
            Binding::new(
                "j / k",
                "move",
                &[
                    (&[Char('j'), Down], |app| app.move_setting(1)),
                    (&[Char('k'), Up], |app| app.move_setting(-1)),
                ],
            )
            .hints(&[Hint::new("j/k move", 0)]),
            Binding::new(
                "h / l",
                "change this one",
                &[
                    (&[Char('l'), Right, Enter], |app| app.cycle_selected_setting(1)),
                    (&[Char('h'), Left], |app| app.cycle_selected_setting(-1)),
                ],
            )
            .hints(&[Hint::new("h/l change", 1)]),
            Binding::new("esc  q", "close", &[(&[Esc, Char('q')], App::close_overlay)])
                .hints(&[Hint::new("esc close", 1).narrow("esc")]),
        ],
    }],
    gap: ROOMY,
};

const DIAGRAM: Keymap = Keymap {
    groups: &[&Group {
        title: "a sequence diagram",
        bindings: &[
            Binding::new(
                "j / k",
                "scroll",
                &[
                    (&[Char('j'), Down, Char('d'), PageDown], |app| app.scroll_diagram(1)),
                    (&[Char('k'), Up, Char('u'), PageUp], |app| app.scroll_diagram(-1)),
                ],
            )
            .hints(&[Hint::new("j/k scroll", 1).narrow("j/k")]),
            Binding::new(
                "h / l",
                "pan across a diagram wider than the window",
                &[
                    (&[Char('h'), Left], |app| app.pan_diagram(-1)),
                    (&[Char('l'), Right], |app| app.pan_diagram(1)),
                ],
            )
            .hints(&[Hint::new("h/l pan", 2).narrow("h/l")]),
            Binding::new("q  esc", "close it", &[(&[Esc, Char('q')], App::close_overlay)])
                .hints(&[Hint::new("q close", 2).narrow("q")]),
        ],
    }],
    gap: TIGHT,
};

// --- keys handled by hand -----------------------------------------------------
//
// These modes take typed text or a leader chord, which a table of single
// keys cannot express, so `app/input` dispatches them and `ui/status` writes
// their bar. What is here is only what `?` says about them.

const PANE: Keymap = Keymap {
    groups: &[&Group {
        title: "in a pane",
        bindings: &[
            Binding::note("", "every key goes to the program"),
            Binding::note("ctrl-space esc", "hand the keyboard back"),
            Binding::note("ctrl-space f", "fullscreen, and back"),
            Binding::note("ctrl-space x", "close it"),
            Binding::note("ctrl-space tab", "review"),
            Binding::note("ctrl-space H", "history"),
            Binding::note("ctrl-space N", "next needing attention"),
            Binding::note("ctrl-space 1-4", "another view"),
            Binding::note("shift-pgup", "back through the scrollback"),
        ],
    }],
    gap: TIGHT,
};

const PROMPT: Keymap = Keymap {
    groups: &[&Group {
        title: "a question",
        bindings: &[Binding::note("y  enter", "yes"), Binding::note("n  esc", "no")],
    }],
    gap: TIGHT,
};

const PICKER: Keymap = Keymap {
    groups: &[&Group {
        title: "a list to choose from",
        bindings: &[
            Binding::note("j / k  ↑ / ↓", "move"),
            Binding::note("type", "narrow it, where it takes text"),
            Binding::note("enter", "choose"),
            Binding::note("esc", "cancel"),
        ],
    }],
    gap: TIGHT,
};

const DIR_PICKER: Keymap = Keymap {
    groups: &[&Group {
        title: "finding a directory",
        bindings: &[
            Binding::note("type", "narrow this directory's entries"),
            Binding::note("↑ / ↓", "move"),
            Binding::note("→  tab", "into the directory under the cursor"),
            Binding::note("←", "up a directory"),
            Binding::note("enter", "choose it"),
            Binding::note("esc", "cancel"),
        ],
    }],
    gap: TIGHT,
};

const CHECKOUT_FILTER: Keymap = Keymap {
    groups: &[&Group {
        title: "filtering branches",
        bindings: &[
            Binding::note("type", "narrow the rows to matching names"),
            Binding::note("enter", "keep the filter"),
            Binding::note("esc", "clear it"),
        ],
    }],
    gap: TIGHT,
};

/// True everywhere, so it is worth saying once rather than per mode. All of
/// it is taken before any mode's keymap is asked.
const EVERYWHERE: Group = Group {
    title: "everywhere",
    bindings: &[
        Binding::note("?", "this list"),
        Binding::note("ctrl-v, alt-v", "paste; an image on the clipboard pastes as a file"),
        Binding::note("F12", "close the floating window, whatever it is"),
    ],
};
