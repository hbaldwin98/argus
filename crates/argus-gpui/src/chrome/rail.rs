//! The sidebar: the nav, the projects, the picked project's repositories with
//! their checkouts and panes, and the foot.
//!
//! Our own list rather than `gpui-kit`'s `Sidebar`, whose selection a test
//! cannot observe (`GPUI.md`, what the spike found). Everything it draws is
//! the latest `Rail` event, and a press only sends the `Call` the page sends:
//! the rail changes when the daemon's tree says it did. The one thing held
//! here is the picked row (SHELL-RAIL-19), which only the window knows, and
//! it follows the tree: a fold or a close moves it to the checkout it was
//! under (SHELL-RAIL-8, 19), so the keys never act on a row that is gone.
//!
//! The rail takes the keys when a row is clicked, and holds them while a row
//! is picked (SHELL-RAIL-13, 19, SHELL-SEAM-9 f, g): `x` closes a picked pane,
//! Enter or Escape hands them back to the stage, and so does a pane newly
//! shown, or any stage change with a pane shown, while nothing is picked
//! (not into the composer or the Feature stage, which SEAM-9 d and e keep
//! for their own). A press anywhere outside the rail drops the pick and
//! hands the stage the keys too (SHELL-SEAM-14): a press on a header or a
//! button focuses nothing, so a rail that kept the keys would hold every
//! one unheard. A key it has no action for is held, not typed into the
//! shell (SHELL-KEYS-16).
//!
//! While the daemon is gone the list is dimmed and answers no press
//! (`Heard::held`): no call, and no pick either.

use argus_desktop_core::connection::page::{Call, Checkout, Pane, ProjectChoice, Rail, Repository, Stage};
use argus_desktop_core::keys::keymap::Place;
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, Context, Entity, FocusHandle, Hsla, KeyDownEvent, MouseButton,
    MouseUpEvent, Role, SharedString, Subscription, Window, WindowControlArea,
};

use crate::chrome::foot::{foot, Console};
use crate::chrome::keycap::keycap;
use crate::chrome::nav::Nav;
use crate::chrome::status_dot::status_dot;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::icons::Glyph;
use crate::keys::caps;
use crate::review::entry::checkout_here;
use crate::style::Typed as _;
use crate::theme::Ledger;
use crate::tokens::{fs, radius, size, sp, weight, Type, OPACITY_DIM};

/// `--rail-w`.
pub const WIDTH: f32 = size::RAIL_W;

/// The row that holds the keys: a checkout, or a pane and the checkout it is
/// under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picked {
    pub checkout: u64,
    pub pane: Option<u64>,
}

pub struct RailView {
    heard: Entity<Heard>,
    caller: Caller,
    picked: Option<Picked>,
    focus: FocusHandle,
    /// The stage's keys, which the rail hands back.
    stage: FocusHandle,
    /// The pane on screen when the tree last changed.
    shown: Option<u64>,
    /// The stage when the tree last changed.
    last_stage: Stage,
    /// Whether the Git stage was up when the tree last changed.
    last_git: bool,
    _subscriptions: Vec<Subscription>,
}

impl RailView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.observe_in(&heard, window, |this, heard, window, cx| {
                let (picked, shown, stage, git) = {
                    let heard = heard.read(cx);
                    let rail = &heard.rail;
                    (settled(this.picked, rail), rail.shown.as_ref().map(|shown| shown.pane), rail.stage, heard.git_up())
                };
                this.picked = picked;
                // A pane newly shown, or any shown pane after a stage change,
                // takes the keys unless a row holds them (SEAM-9 f, g). The
                // composer and the Feature stage keep them for their own
                // text (SEAM-9 d, e). Git takes them itself as it appears (its observer runs
                // after this one) and they come back to the pane through this once
                // it goes (CROSS-11).
                let moved = shown != this.shown || stage != this.last_stage || git != this.last_git;
                if shown.is_some() && moved && this.picked.is_none() && !matches!(stage, Stage::Composer | Stage::Feature) {
                    window.focus(&this.stage, cx);
                }
                this.shown = shown;
                this.last_stage = stage;
                this.last_git = git;
                cx.notify();
            }),
            // A press outside the rail lets the pick go (SHELL-RAIL-19).
            cx.on_focus_out(&focus, window, |this, _, _, cx| {
                this.picked = None;
                cx.notify();
            }),
        ];
        let (shown, last_stage, last_git) = {
            let heard = heard.read(cx);
            (heard.rail.shown.as_ref().map(|shown| shown.pane), heard.rail.stage, heard.git_up())
        };
        Self { heard, caller, picked: None, focus, stage, shown, last_stage, last_git, _subscriptions: subscriptions }
    }

    /// A press outside the rail: the pick goes, and the keys go to the pane
    /// on screen (SHELL-SEAM-14).
    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        if self.shown.is_some() {
            window.focus(&self.stage, cx);
        }
        cx.notify();
    }

    /// A key while the rail has the keys: only a picked row answers one, and
    /// every key is held (SHELL-KEYS-16) so none is typed into the shell.
    fn press(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let held = self.heard.read(cx).held();
        let Some(picked) = self.picked else { return };
        match event.keystroke.key.as_str() {
            "escape" => self.give_back(window, cx),
            "enter" if picked.pane.is_some() => self.give_back(window, cx),
            "x" if picked.pane.is_some() && !held => {
                let pane = picked.pane.expect("checked");
                self.caller.call(Call::Close { pane });
                self.picked = Some(Picked { checkout: picked.checkout, pane: None });
                cx.notify();
            }
            _ => {}
        }
    }

    /// Unpicks and gives the stage the keys (SHELL-RAIL-24, 27).
    fn give_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked = None;
        window.focus(&self.stage, cx);
        cx.notify();
    }

    pub fn picked(&self) -> Option<Picked> {
        self.picked
    }

    /// Picks `row` unless the sidebar is held.
    fn pick(&mut self, held: bool, row: Picked) {
        if !held {
            self.picked = Some(row);
        }
    }

    /// Sends `call` unless the sidebar is held.
    fn ask(&self, held: bool, call: Call) {
        if !held {
            self.caller.call(call);
        }
    }

    /// A press of a nav item. Git asks for the review of the checkout the
    /// window is on; every item that goes to another stage first closes what
    /// the session has open of Git, in the same breath (`review::entry`), so
    /// the stage is the session's to say and is never left standing under
    /// another.
    fn go(&mut self, nav: Nav, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let picked = self.picked.map(|picked| picked.checkout);
        let calls = match nav {
            Nav::Git => self.heard.update(cx, |heard, cx| {
                let (open, here) = (heard.open(), checkout_here(&heard.rail, picked));
                let calls = heard.git.enter(open, here);
                // A Git pressed with nothing to open puts the stage up on the window's word alone.
                cx.notify();
                calls
            }),
            _ if nav.leaves_git() => self.leave_git(cx),
            _ => Vec::new(),
        };
        for call in calls.into_iter().chain(nav.call()) {
            self.caller.call(call);
        }
    }

    /// The close of whatever the session has open of Git, for a press that
    /// goes somewhere else.
    fn leave_git(&mut self, cx: &mut Context<Self>) -> Vec<Call> {
        self.heard.update(cx, |heard, cx| {
            let open = heard.open();
            let calls = heard.git.leave(open);
            cx.notify();
            calls
        })
    }

    /// The brand row: the logo and the project the rail lists, as the page's
    /// `.brand` with its switcher button. The projects themselves are rows
    /// under it (`projects`), so no press is lost to a menu that is not here.
    fn brand(&self, rail: &Rail, ledger: &Ledger) -> impl IntoElement {
        let name = rail.project.as_ref().map_or_else(|| "Argus".to_owned(), |project| project.name.clone());
        let switchable = rail.projects.len() > 1;
        h_flex()
            .id("rail-brand")
            .flex_shrink_0()
            .h(px(size::HEADER_H))
            .px(px(sp::S16))
            .gap(px(sp::S8))
            .items_center()
            // The sidebar runs to the window's top edge: its brand row is the
            // title bar's left end (`.rail .brand`), so it moves the window.
            .window_control_area(WindowControlArea::Drag)
            .when(!cfg!(windows), |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            })
            .child(Glyph::Logo.at(16.0).text_color(ledger.accent))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .typed(Type { weight: weight::SEMIBOLD, size: fs::MD, leading: 1.3 })
                    .text_color(ledger.foreground)
                    .child(name),
            )
            .when(switchable, |row| row.child(Glyph::Down.at(12.0).text_color(ledger.faint)))
    }

    #[allow(clippy::too_many_arguments)]
    fn nav(
        &self,
        rail: &Rail,
        git_up: bool,
        held: bool,
        keys: &[Vec<String>],
        mono: &SharedString,
        ledger: &Ledger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = ledger.fill();
        // `.navs`: the keycaps show while the pointer is anywhere over the group.
        v_flex()
            .group("navs")
            .flex_shrink_0()
            .px(px(sp::S8))
            .pb(px(sp::S8))
            .gap(px(sp::S2))
            .border_b_1()
            .border_color(ledger.line)
            .children(Nav::ALL.into_iter().enumerate().map(|(at, nav)| {
                let lit = nav.lit(rail.stage, git_up);
                let search = nav == Nav::Search;
                let caps = h_flex()
                    .gap(px(sp::S4))
                    .children(keys.get(at).cloned().unwrap_or_default().into_iter().map(|key| keycap(key, mono.clone(), ledger)));
                let caps = if search { caps } else { caps.opacity(0.0).group_hover("navs", |style| style.opacity(1.0)) };
                let base = line(nav.id(), if search { 34.0 } else { 32.0 }).text_size(px(fs::MD)).px(px(sp::S8));
                let base = if search {
                    // `.search`: a field's outline, not a row's fill.
                    base.mb(px(sp::S6))
                        .pl(px(sp::S12))
                        .gap(px(sp::S8))
                        .border_1()
                        .border_color(ledger.line_strong)
                        .rounded(px(radius::MD))
                        .text_color(ledger.faint)
                        .hover(|style| style.text_color(ledger.muted))
                } else {
                    base.gap(px(sp::S12))
                        .text_color(ledger.foreground)
                        .when(lit, |row| row.bg(ledger.selected))
                        .when(!lit, |row| row.hover(move |style| style.bg(fill)))
                };
                let glyph = nav.glyph().at(16.0);
                let icon = if search {
                    glyph.into_any_element()
                } else {
                    let mark = div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(size::NAV_ICON_CELL))
                        .flex_shrink_0()
                        .text_color(if lit { ledger.accent } else { ledger.muted });
                    // `.nav .ic.new`: the one item whose mark is a filled disc.
                    let mark = if nav == Nav::New { mark.bg(fill).text_color(ledger.foreground).rounded_full() } else { mark };
                    mark.child(glyph).into_any_element()
                };
                base.aria_selected(lit)
                    .aria_label(nav.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.go(nav, held, cx)))
                    .child(icon)
                    .child(div().flex_1().min_w_0().truncate().child(nav.label()))
                    .child(caps)
            }))
    }

    /// The projects to choose among, once there is more than one: the
    /// page's project menu, laid out as rows under the brand.
    fn projects(&self, rail: &Rail, held: bool, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let current = rail.project.as_ref().map(|p| p.id);
        let fill = ledger.fill();
        v_flex()
            .flex_shrink_0()
            .px(px(sp::S8))
            .pb(px(sp::S8))
            .gap(px(sp::S2))
            .border_b_1()
            .border_color(ledger.line)
            .children(rail.projects.iter().map(|choice: &ProjectChoice| {
                let id = choice.id;
                let chosen = Some(id) == current;
                line(format!("project-{id}"), size::ROW_H)
                    .px(px(sp::S8))
                    .gap(px(sp::S8))
                    .text_size(px(fs::MD))
                    .text_color(if chosen { ledger.foreground } else { ledger.muted })
                    .aria_selected(chosen)
                    .aria_label(choice.name.clone())
                    .when(chosen, |row| row.bg(ledger.selected))
                    .when(!chosen, |row| row.hover(move |style| style.bg(fill).text_color(ledger.foreground)))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::SelectProject { project: id })))
                    .child(div().flex_1().min_w_0().truncate().child(choice.name.clone()))
                    .when(choice.needs_you > 0, |row| {
                        row.child(
                            div()
                                .text_size(px(fs::XS))
                                .text_color(ledger.warning)
                                .child(format!("▲ {}", choice.needs_you)),
                        )
                    })
            }))
    }

    #[allow(clippy::too_many_arguments)]
    fn repository(
        &self,
        rail: &Rail,
        repo: &Repository,
        first: bool,
        offstage: bool,
        held: bool,
        ledger: &Ledger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = repo.id;
        let chosen = rail.repository == Some(id);
        let count = repo.used.clone().or_else(|| (repo.panes > 0).then(|| repo.panes.to_string()));
        let dot = repo.status.and_then(|word| status_colour(word, ledger));
        let fill = ledger.fill();
        // `.repobtn`: muted and semibold, the chosen one in the text colour with an
        // accent chevron, a quiet one faint and regular.
        let colour = if chosen { ledger.foreground } else if repo.quiet { ledger.faint } else { ledger.muted };
        v_flex()
            // `repo.divider` is not drawn: the page never draws it (SHELL-RAIL-2).
            // `.repo` is 2px under the nav, and 8 after another repository's rows.
            .child(
                div().px(px(sp::S8)).mt(px(if first { sp::S2 } else { sp::S8 })).child(
                    line(format!("repo-{id}"), size::ROW_H)
                        .w_full()
                        .px(px(sp::S4))
                        .gap(px(sp::S6))
                        .text_size(px(fs::SM))
                        .weighted(if repo.quiet { weight::REGULAR } else { weight::SEMIBOLD })
                        .text_color(colour)
                        .aria_selected(chosen)
                        .aria_expanded(repo.open)
                        .aria_label(repo.name.clone())
                        .hover(move |style| style.bg(fill).text_color(ledger.foreground))
                        .on_click(cx.listener(move |this, _, _, _| {
                            this.ask(held, Call::ChooseRepository { repository: id })
                        }))
                        // Only a mark: a press on it chooses the repository with the
                        // rest of the row, as the page's does (SHELL-RAIL-5).
                        .child(chevron(
                            format!("repo-{id}-chevron"),
                            repo.open,
                            size::ICON_SM,
                            if chosen { ledger.accent } else { ledger.faint },
                        ))
                        .child(div().flex_1().min_w_0().truncate().child(repo.name.clone()))
                        .when_some(dot, |row, colour| row.child(dot_of(colour)))
                        .when_some(count, |row, count| {
                            row.child(
                                div()
                                    .id(format!("repo-{id}-count"))
                                    .test_support()
                                    .aria_label(count.clone())
                                    .text_size(px(fs::XS))
                                    .weighted(weight::REGULAR)
                                    .text_color(ledger.faint)
                                    .child(count),
                            )
                        }),
                ),
            )
            // A shut repository draws none of what is under it.
            .when(repo.open, |col| {
                col.children(repo.checkouts.iter().map(|checkout| self.checkout(checkout, offstage, held, ledger, cx)))
            })
    }

    fn checkout(
        &self,
        checkout: &Checkout,
        offstage: bool,
        held: bool,
        ledger: &Ledger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = checkout.id;
        let picked = self.picked == Some(Picked { checkout: id, pane: None });
        let where_ = if checkout.kind == "main" { "main checkout" } else { "worktree" };
        let tooltip = if checkout.branch.is_empty() {
            where_.to_owned()
        } else {
            format!("{where_} on {}", checkout.branch)
        };
        let hidden_dot = if checkout.open { None } else { checkout.status.and_then(|word| status_colour(word, ledger)) };
        let git = checkout.git.clone().filter(|_| !checkout.clean);
        let fill = ledger.fill();
        let mono = cx.theme().mono_font_family.clone();
        v_flex()
            .child(
                // `.co`: a margin of 8 each side, the fold chevron in its 12px of left padding.
                div().mx(px(sp::S8)).child(
                    line(format!("checkout-{id}"), size::CONTROL_H_SM)
                        .w_full()
                        .pl(px(sp::S12 - 1.0))
                        .pr(px(sp::S4 - 1.0))
                        .gap(px(sp::S6))
                        .text_size(px(fs::XS))
                        .text_color(ledger.faint)
                        .border_1()
                        // `.co.picked`: the ring that says which row holds the keys.
                        .border_color(if picked { ledger.accent } else { Hsla::transparent_black() })
                        .aria_selected(picked)
                        .aria_label(tooltip)
                        .hover(move |style| style.bg(fill).text_color(ledger.muted))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick(held, Picked { checkout: id, pane: None });
                            cx.notify();
                        }))
                        // The fold's box is 24 wide and sits in the row's padding, so a
                        // checkout's branch does not shift when it folds.
                        .child(if checkout.count > 0 {
                            chevron(format!("checkout-{id}-fold"), checkout.open, sp::S24, ledger.faint)
                                .role(Role::Button)
                                .rounded(px(radius::SM))
                                .aria_label(if checkout.open { "Fold its panes away" } else { "Show its panes" })
                                .hover(move |style| style.bg(fill).text_color(ledger.foreground))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    // The pick leaves a pane that is about to be folded away first.
                                    if !held && this.picked.is_some_and(|p| p.checkout == id && p.pane.is_some()) {
                                        this.picked = Some(Picked { checkout: id, pane: None });
                                    }
                                    this.ask(held, Call::ToggleCheckout { checkout: id });
                                    cx.notify();
                                }))
                                .into_any_element()
                        } else {
                            div().w(px(sp::S24)).h_full().flex_shrink_0().into_any_element()
                        })
                        .child(div().ml(px(-sp::S6)).child(Glyph::Branch.at(12.0)))
                        .child(div().flex_1().min_w_0().truncate().font_family(mono.clone()).child(checkout.name.clone()))
                        .when_some(checkout.shared, |row, n| {
                            row.child(
                                div()
                                    .id(format!("checkout-{id}-shared"))
                                    .test_support()
                                    .aria_label(format!("shared by {n} agents"))
                                    .child(format!("×{n}")),
                            )
                        })
                        .when_some(git, |row, git| {
                            row.child(div().font_family(mono.clone()).text_color(ledger.changed).child(git))
                        })
                        .when(!checkout.open, |row| {
                            row.when_some(hidden_dot, |row, colour| row.child(dot_of(colour))).child(
                                div()
                                    .id(format!("checkout-{id}-folded"))
                                    .test_support()
                                    .aria_label(format!(
                                        "{} {} folded away",
                                        checkout.count,
                                        if checkout.count == 1 { "pane" } else { "panes" }
                                    ))
                                    .font_family(mono)
                                    .child(checkout.count.to_string()),
                            )
                        }),
                ),
            )
            // A folded checkout draws no pane, whatever it was given. `.nest`: its
            // panes hang from a hairline under the checkout's own branch.
            .when(checkout.open, |col| {
                col.child(
                    v_flex()
                        .ml(px(size::PANE_INDENT))
                        .mr(px(sp::S8))
                        .mb(px(sp::S4))
                        .pl(px(sp::S4))
                        .border_l_1()
                        .border_color(ledger.line)
                        .children(checkout.panes.iter().map(|pane| self.pane(id, pane, offstage, held, ledger, cx))),
                )
            })
    }

    fn pane(
        &self,
        checkout: u64,
        pane: &Pane,
        offstage: bool,
        held: bool,
        ledger: &Ledger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = pane.id;
        let picked = self.picked == Some(Picked { checkout, pane: Some(id) });
        let word = pane_word(pane.status);
        let tooltip = format!(
            "{} · {}",
            pane.title,
            pane.note.clone().unwrap_or_else(|| format!("{}, {}", if pane.agent { "agent" } else { "shell" }, pane.status))
        );
        let fill = ledger.fill();
        // Off the Workspace the pane on screen is not the selection, the stage's own
        // nav item is: it keeps an edge and its text, not the box (`.offstage`).
        let boxed = pane.on && !offstage;
        let group = SharedString::from(format!("pane-row-{id}"));
        let ring = if picked {
            ledger.accent
        } else if boxed {
            // `--mark-sel` under Ledger: a hairline on the selection fill.
            ledger.line_strong
        } else {
            Hsla::transparent_black()
        };
        line(format!("pane-{id}"), 32.0)
            .group(group.clone())
            .relative()
            .w_full()
            // One pixel less each side: the ring is a border here, an outline in the page.
            .pl(px(sp::S12 - 1.0))
            .pr(px(sp::S8 - 1.0))
            .gap(px(sp::S12))
            .text_size(px(fs::MD))
            .text_color(if pane.on || pane.agent { ledger.foreground } else { ledger.muted })
            .border_1()
            .border_color(ring)
            .aria_selected(pane.on)
            .aria_label(tooltip)
            .when(boxed, |row| row.bg(ledger.selected))
            .when(!boxed, |row| row.hover(move |style| style.bg(fill)))
            .when(pane.on && offstage, |row| {
                row.child(div().absolute().left_0().top_0().bottom_0().w(px(size::PANE_MARK_W)).bg(ledger.line_strong))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                // The pane comes on screen and the keys stay on the rail until
                // Enter or a click on the stage (SHELL-RAIL-13). The rail has
                // them because its root tracks focus and GPUI focuses that
                // root on a press inside it, held or not: a held rail picks
                // nothing, so `press` answers no key.
                this.pick(held, Picked { checkout, pane: Some(id) });
                // A pane comes on screen where Git was, so Git goes first.
                if !held {
                    for call in this.leave_git(cx) {
                        this.caller.call(call);
                    }
                }
                this.ask(held, Call::Select { pane: id });
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseUpEvent, _, _| this.ask(held, Call::Close { pane: id })),
            )
            // One status encoding per row: the pill when it has a word, else the dot.
            .when(word.is_none(), |row| row.child(status_dot(format!("pane-{id}"), pane.status, ledger)))
            .child(div().flex_1().min_w_0().truncate().child(pane.title.clone()))
            .when_some(word, |row, word| {
                // `.pane-end`: gives way to the close button while the row is pointed at.
                row.child(div().flex_shrink_0().group_hover(group.clone(), |style| style.invisible()).child(spill(
                    id,
                    word,
                    pane.status,
                    ledger,
                )))
            })
            // `.pane-row .close`: over the status word, only under the pointer.
            .child(
                div()
                    .id(format!("pane-{id}-close"))
                    .test_support()
                    .role(Role::Button)
                    .aria_label(format!("Close {}", pane.title))
                    .absolute()
                    .right(px(sp::S8))
                    .top(px((32.0 - size::ICON_BOX) / 2.0 - 1.0))
                    .size(px(size::ICON_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SM))
                    .text_color(ledger.muted)
                    .cursor_pointer()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .hover(move |style| style.bg(fill).text_color(ledger.foreground))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ask(held, Call::Close { pane: id });
                    }))
                    .child(Glyph::Close.at(12.0)),
            )
    }

    fn listing(&self, rail: &Rail, held: bool, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        // `.rail-list.held`: the tree is dimmed while the daemon is gone.
        let mut list = v_flex()
            .id("rail-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb(px(sp::S12))
            .when(held, |list| list.opacity(OPACITY_DIM));
        let offstage = rail.stage != Stage::Workspace || self.heard.read(cx).git_up();
        match &rail.project {
            // The stage is the one loading surface; the rail only waits.
            None if !rail.loaded => list = list.child(div().id("rail-wait").test_support()),
            None if rail.projects.is_empty() => {
                list = list.child(
                    div()
                        .id("rail-empty")
                        .test_support()
                        .p(px(sp::S16))
                        .text_color(ledger.faint)
                        .child("No projects yet."),
                )
            }
            None => {}
            // The project's name is the brand row's, so the list opens on its first repository.
            Some(project) => {
                list = list.children(project.repositories.iter().enumerate().map(|(at, repo)| {
                    self.repository(rail, repo, at == 0, offstage, held, ledger, cx)
                }));
            }
        }
        list
    }
}

impl Render for RailView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let mono = cx.theme().mono_font_family.clone();
        let (rail, git_up, held, link, alarm, keys, chime, console) = {
            let heard = self.heard.read(cx);
            let keys: Vec<Vec<String>> = Nav::ALL
                .iter()
                .map(|nav| {
                    nav.action()
                        .and_then(|action| caps::presses(&heard.keymap, action, Place::Anywhere).into_iter().next())
                        .unwrap_or_default()
                })
                .collect();
            (
                heard.rail.clone(),
                heard.git_up(),
                heard.held(),
                heard.link(),
                heard.status.as_ref().and_then(|s| s.alarm.clone()),
                keys,
                heard.status.as_ref().is_none_or(|s| s.chime),
                Console { shown: heard.console.place != ConsolePlace::Hidden, count: heard.console.tabs.len() },
            )
        };
        let body = v_flex()
            .flex_1()
            .min_h_0()
            .child(self.nav(&rail, git_up, held, &keys, &mono, &ledger, cx))
            .when(rail.projects.len() > 1, |body| body.child(self.projects(&rail, held, &ledger, cx)))
            .child(self.listing(&rail, held, &ledger, cx));
        v_flex()
            .id("rail")
            .key_context("Rail")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.press(event, window, cx)))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.release(window, cx)))
            .w(px(WIDTH))
            .flex_shrink_0()
            .h_full()
            .bg(ledger.chrome)
            .border_r_1()
            .border_color(ledger.line)
            .text_color(ledger.muted)
            .text_size(px(fs::MD))
            .child(self.brand(&rail, &ledger))
            .child(body)
            .child(foot(&link, &rail, alarm.as_deref(), chime, console, &self.caller, held, &ledger))
    }
}

/// A selectable line of the rail: its box, with the colours and padding left
/// to the kind of row, since the page gives each its own.
type Line = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

fn line(id: impl Into<gpui_kit::ElementId>, height: f32) -> Line {
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .h(px(height))
        .flex_shrink_0()
        .items_center()
        .rounded(px(radius::SM))
        .cursor_pointer()
}

/// The turn of a chevron is `--t-fast`'s neighbour, 0.15s, in the page; here
/// the mark swaps, since the glyph set has one chevron per direction.
fn chevron(id: String, open: bool, width: f32, colour: Hsla) -> Line {
    h_flex()
        .id(id)
        .test_support()
        .aria_expanded(open)
        .items_center()
        .justify_center()
        .w(px(width))
        .flex_shrink_0()
        .text_color(colour)
        .child(if open { Glyph::Down } else { Glyph::Right }.at(12.0))
}

fn dot_of(colour: Hsla) -> impl IntoElement {
    div().size(px(size::DOT_MD)).flex_shrink_0().rounded_full().bg(colour)
}

/// `.spill`: the one status pill, a dot and a word on a tint of the status's
/// colour. What blocks on a person is louder than what does not.
fn spill(id: u64, word: &'static str, status: &str, ledger: &Ledger) -> impl IntoElement {
    let (tone, loud, hollow) = match status {
        "waiting" | "needs-review" => (ledger.warning, true, false),
        "failed" => (ledger.danger, true, false),
        "done" => (ledger.success, false, false),
        "working" => (ledger.accent, false, false),
        _ => (ledger.faint, false, true),
    };
    let (fill, edge) = if loud { (0.22, 0.60) } else { (0.12, 0.40) };
    h_flex()
        .id(format!("pane-{id}-word"))
        .test_support()
        .aria_label(word)
        .h(px(size::CHIP_H))
        .px(px(sp::S8))
        .gap(px(sp::S6))
        .items_center()
        .border_1()
        .border_color(tone.opacity(edge))
        .rounded(px(radius::PILL))
        .bg(tone.opacity(fill))
        // Faint on faint fails 4.5:1 at 11px, so an ended or idle word is the muted one.
        .text_color(if hollow { ledger.muted } else { tone })
        .text_size(px(fs::XS))
        .weighted(weight::SEMIBOLD)
        .child(if hollow {
            Glyph::Ring6.at(size::DOT_SM).text_color(tone).into_any_element()
        } else {
            div().size(px(size::DOT_SM)).flex_shrink_0().rounded_full().bg(tone).into_any_element()
        })
        .child(word)
}

/// Where the pick goes once the tree has changed under it: only a pane no
/// longer listed (closed, or its checkout folded) moves it, to its
/// checkout. Nothing else does (SHELL-RAIL-19), so a checkout that is gone
/// keeps the pick, which then marks no row.
fn settled(picked: Option<Picked>, rail: &Rail) -> Option<Picked> {
    let picked = picked?;
    let Some(pane) = picked.pane else { return Some(picked) };
    let listed = rail
        .project
        .iter()
        .flat_map(|p| &p.repositories)
        .filter(|r| r.open)
        .flat_map(|r| &r.checkouts)
        .find(|c| c.id == picked.checkout)
        .is_some_and(|c| c.open && c.panes.iter().any(|p| p.id == pane));
    Some(if listed { picked } else { Picked { checkout: picked.checkout, pane: None } })
}

/// The dot a status earns: the attention colour for what asks for a look,
/// the accent for work under way, none otherwise.
fn status_colour(word: &str, ledger: &Ledger) -> Option<Hsla> {
    match word {
        "waiting" | "needs-review" | "failed" => Some(ledger.warning),
        "working" => Some(ledger.accent),
        _ => None,
    }
}

/// The few statuses a pane row says in words: only what asks for a look, or
/// that it has ended. Working shows in the dot alone.
fn pane_word(status: &str) -> Option<&'static str> {
    match status {
        "waiting" => Some("needs you"),
        "needs-review" => Some("review"),
        "failed" => Some("failed"),
        "exited" => Some("exited"),
        "done" => Some("done"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use argus_desktop_core::connection::page::{Connection, Event, Project, Shown, Stage, Status};
    use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
    use gpui_kit::{point, size, AnyWindowHandle, Bounds, MouseButton, TestAppContext, WindowBounds, WindowOptions};

    fn pane(id: u64, agent: bool, title: &str, status: &'static str, on: bool) -> Pane {
        Pane { id, agent, title: title.into(), status, note: None, on, views: 0 }
    }

    fn checkout_row(id: u64, name: &str, open: bool, panes: Vec<Pane>) -> Checkout {
        Checkout {
            id,
            name: name.into(),
            branch: "main".into(),
            kind: "main",
            git: None,
            clean: true,
            shared: None,
            open,
            count: panes.len(),
            status: None,
            panes,
        }
    }

    /// One project with two repositories: `argus` active, `dotfiles` quiet.
    fn busy() -> Rail {
        let argus = Repository {
            id: 11,
            name: "argus".into(),
            quiet: false,
            divider: false,
            open: true,
            panes: 3,
            status: Some("needs-review"),
            used: None,
            checkouts: vec![
                checkout_row(
                    110,
                    "main",
                    true,
                    vec![
                        pane(1, true, "Trim context", "working", true),
                        pane(2, false, "zsh", "idle", false),
                        pane(4, true, "Tracker", "needs-review", false),
                    ],
                ),
                Checkout {
                    git: Some("+1 !2".into()),
                    count: 1,
                    status: Some("waiting"),
                    clean: false,
                    shared: Some(2),
                    ..checkout_row(112, "tracker-links", false, vec![pane(5, true, "Linker", "waiting", false)])
                },
            ],
        };
        let dotfiles = Repository {
            id: 10,
            name: "dotfiles".into(),
            quiet: true,
            divider: true,
            open: false,
            panes: 0,
            status: None,
            used: Some("16m".into()),
            // Given though the repository is shut: the view must not draw it.
            checkouts: vec![checkout_row(100, "main", true, vec![pane(6, false, "zsh", "idle", false)])],
        };
        Rail {
            projects: vec![
                ProjectChoice { id: 1, name: "src".into(), needs_you: 1, working: 1 },
                ProjectChoice { id: 2, name: "other".into(), needs_you: 0, working: 0 },
            ],
            project: Some(Project {
                id: 1,
                name: "src".into(),
                needs_you: 1,
                repositories: vec![argus, dotfiles],
                running: Vec::new(),
            }),
            needs_you: 1,
            working: 1,
            loaded: true,
            repository: Some(11),
            stage: Stage::Workspace,
            ..Rail::default()
        }
    }

    fn status(connection: Connection) -> Event {
        Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None })
    }

    type Calls = Rc<RefCell<Vec<Call>>>;

    struct Opened {
        handle: AnyWindowHandle,
        heard: Entity<Heard>,
        rail: Entity<RailView>,
        calls: Calls,
    }

    fn open(cx: &mut TestAppContext, events: Vec<Event>) -> Opened {
        cx.update(crate::window::init);
        let calls: Calls = Rc::default();
        let sink = calls.clone();
        let caller = Caller::new(move |call| sink.borrow_mut().push(call));
        let mut built = None;
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(WIDTH), px(700.0)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let heard = cx.new(|_| Heard::new());
                    let stage = cx.focus_handle();
                    let rail = cx.new(|cx| RailView::new(heard.clone(), caller, stage, window, cx));
                    built = Some((heard, rail.clone()));
                    rail
                },
            )
            .expect("open test window")
        });
        let (heard, rail) = built.unwrap();
        let opened = Opened { handle, heard, rail, calls };
        hear(cx, &opened, events);
        opened
    }

    fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
        cx.update(|cx| {
            opened.heard.update(cx, |heard, cx| {
                for event in events {
                    heard.hear(event);
                }
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
        let at = cx
            .update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().center())
            .unwrap();
        press(cx, opened, at, MouseButton::Left);
    }

    fn press(cx: &mut TestAppContext, opened: &Opened, at: gpui_kit::Point<gpui_kit::Pixels>, button: MouseButton) {
        cx.update_window(opened.handle, |_, window, cx| {
            window.pointer_down(at, button, cx);
            window.pointer_up(at, button, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    fn label(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
        cx.update_window(opened.handle, |_, window, _| {
            window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned))
        })
        .unwrap()
    }

    fn present(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
        cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
    }

    fn selected(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<bool> {
        cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap()
    }

    fn taken(opened: &Opened) -> Vec<String> {
        opened.calls.borrow().iter().map(|call| format!("{call:?}")).collect()
    }

    fn connected_and_busy() -> Vec<Event> {
        vec![status(Connection::Connected), Event::Rail(Box::new(busy()))]
    }

    #[gpui_kit::test]
    fn the_rail_lists_the_rows_of_the_rail_it_was_given(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        for nav in Nav::ALL {
            assert_eq!(label(cx, &opened, nav.id()).as_deref(), Some(nav.label()));
        }
        assert_eq!(label(cx, &opened, "project-1").as_deref(), Some("src"));
        assert_eq!(label(cx, &opened, "project-2").as_deref(), Some("other"));
        assert_eq!(label(cx, &opened, "repo-11").as_deref(), Some("argus"));
        assert_eq!(label(cx, &opened, "repo-11-count").as_deref(), Some("3"));
        // A quiet repository says how long since it was used, and draws nothing under it while shut.
        assert_eq!(label(cx, &opened, "repo-10-count").as_deref(), Some("16m"));
        assert!(!present(cx, &opened, "checkout-100"));
        assert!(!present(cx, &opened, "pane-6"));
        assert_eq!(label(cx, &opened, "checkout-110").as_deref(), Some("main checkout on main"));
        assert_eq!(label(cx, &opened, "pane-1").as_deref(), Some("Trim context · agent, working"));
        assert_eq!(label(cx, &opened, "pane-2").as_deref(), Some("zsh · shell, idle"));
        // Only what asks for a look is said in words.
        assert_eq!(label(cx, &opened, "pane-4-word").as_deref(), Some("review"));
        assert!(!present(cx, &opened, "pane-1-word"));
        assert!(!present(cx, &opened, "pane-2-word"));
        // A folded checkout draws no pane and speaks for them: how many, and who shares it.
        assert!(!present(cx, &opened, "pane-5"));
        assert_eq!(label(cx, &opened, "checkout-112-folded").as_deref(), Some("1 pane folded away"));
        assert_eq!(label(cx, &opened, "checkout-112-shared").as_deref(), Some("shared by 2 agents"));
    }

    #[gpui_kit::test]
    fn the_pane_on_screen_and_the_chosen_repository_are_marked(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
        assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
        assert_eq!(selected(cx, &opened, "repo-11"), Some(true));
        assert_eq!(selected(cx, &opened, "project-1"), Some(true));
        assert_eq!(selected(cx, &opened, "nav-workspace"), Some(true));
        assert_eq!(selected(cx, &opened, "nav-code"), Some(false));
    }

    #[gpui_kit::test]
    fn a_rail_not_yet_loaded_lists_nothing_and_an_empty_one_says_so(cx: &mut TestAppContext) {
        let opened = open(cx, vec![status(Connection::Connecting)]);
        assert!(present(cx, &opened, "nav-workspace"));
        assert!(!present(cx, &opened, "rail-empty"));
        assert!(present(cx, &opened, "rail-wait"));
        let empty = Rail { loaded: true, ..Rail::default() };
        hear(cx, &opened, vec![Event::Rail(Box::new(empty))]);
        assert!(present(cx, &opened, "rail-empty"));
        assert!(!present(cx, &opened, "rail-wait"));
    }

    #[gpui_kit::test]
    fn a_click_on_a_pane_row_selects_that_pane_and_picks_the_row(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "pane-2");
        assert_eq!(taken(&opened), ["Select { pane: 2 }"]);
        cx.update(|cx| {
            assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: Some(2) }));
        });
        // The mark on screen is the daemon's to move, not the click's.
        assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
        assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
    }

    /// The whole window, rail and stage, where the keys can be told apart.
    fn open_shell(cx: &mut TestAppContext) -> Opened {
        cx.update(crate::window::init);
        let calls: Calls = Rc::default();
        let sink = calls.clone();
        let caller = Caller::new(move |call| sink.borrow_mut().push(call));
        let mut built = None;
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1000.0), px(700.0)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let heard = cx.new(|_| Heard::new());
                    let shell = cx.new(|cx| crate::chrome::shell::Shell::new(heard.clone(), caller, window, cx));
                    built = Some((heard, shell.clone()));
                    shell
                },
            )
            .expect("open test window")
        });
        let (heard, shell) = built.unwrap();
        let rail = cx.update(|cx| shell.read(cx).rail().clone());
        let opened = Opened { handle, heard, rail, calls };
        // A test window starts inactive, and an inactive one is told of no
        // focus change, so no focus-out would reach the rail.
        cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
        cx.run_until_parked();
        hear(cx, &opened, connected_and_busy());
        opened
    }

    fn type_key(cx: &mut TestAppContext, opened: &Opened, key: &str) {
        let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
        cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
        cx.run_until_parked();
    }

    fn keys_sent(opened: &Opened) -> usize {
        opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. })).count()
    }

    #[gpui_kit::test]
    fn a_key_after_a_pane_row_click_is_the_rails_and_never_typed_into_the_shell(cx: &mut TestAppContext) {
        // SHELL-RAIL-13, SHELL-KEYS-16.
        let opened = open_shell(cx);
        type_key(cx, &opened, "a");
        assert_eq!(keys_sent(&opened), 1, "the stage has the keys at open");
        click(cx, &opened, "pane-2");
        type_key(cx, &opened, "a");
        assert_eq!(keys_sent(&opened), 1, "a key the rail has no action for is held");
        type_key(cx, &opened, "x");
        assert_eq!(keys_sent(&opened), 1);
        assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()), "x closes the picked pane");
    }

    #[gpui_kit::test]
    fn enter_on_a_picked_pane_gives_the_stage_the_keys_again(cx: &mut TestAppContext) {
        // SHELL-RAIL-24.
        let opened = open_shell(cx);
        click(cx, &opened, "pane-2");
        type_key(cx, &opened, "enter");
        type_key(cx, &opened, "a");
        assert_eq!(keys_sent(&opened), 1);
    }

    #[gpui_kit::test]
    fn escape_unpicks_and_gives_the_stage_the_keys(cx: &mut TestAppContext) {
        // SHELL-RAIL-27.
        let opened = open_shell(cx);
        click(cx, &opened, "pane-2");
        type_key(cx, &opened, "escape");
        type_key(cx, &opened, "a");
        assert_eq!(keys_sent(&opened), 1);
    }

    fn showing(pane: u64, stage: Stage) -> Event {
        let mut rail = busy();
        rail.stage = stage;
        rail.shown = Some(Shown {
            pane,
            repository: "argus".into(),
            checkout: "main".into(),
            branch: "main".into(),
            agent: false,
            title: "zsh".into(),
            status: "idle",
            resumed: None,
        });
        Event::Rail(Box::new(rail))
    }

    /// Key and text calls: what reaches a shell.
    fn typed(opened: &Opened) -> usize {
        opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. } | Call::Text { .. })).count()
    }

    /// Takes the keys from everything, as a press on nothing would.
    fn blur(cx: &mut TestAppContext, opened: &Opened) {
        cx.update_window(opened.handle, |_, window, cx| window.blur(cx)).unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    fn picked(cx: &mut TestAppContext, opened: &Opened) -> Option<Picked> {
        cx.update(|cx| opened.rail.read(cx).picked())
    }

    /// An open shell showing pane 1, whose Terminal has the keys.
    fn open_showing(cx: &mut TestAppContext) -> Opened {
        let opened = open_shell(cx);
        hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1, "the Terminal has the keys while nothing is picked");
        opened
    }

    #[gpui_kit::test]
    fn a_checkout_row_click_takes_the_keys_off_the_terminal(cx: &mut TestAppContext) {
        // SHELL-RAIL-19.
        let opened = open_showing(cx);
        click(cx, &opened, "checkout-110");
        assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: None }));
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1, "the Terminal heard a key while a checkout was picked");
    }

    #[gpui_kit::test]
    fn no_key_reaches_the_shell_while_a_row_is_picked(cx: &mut TestAppContext) {
        // SHELL-KEYS-16: every key the rail has no action for is held.
        let opened = open_showing(cx);
        click(cx, &opened, "checkout-110");
        for key in ["a", "j", "k", "s", "z", "space", "tab", "ctrl-c", "enter", "backspace"] {
            type_key(cx, &opened, key);
        }
        assert_eq!(typed(&opened), 1, "a key was typed into the shell: {:?}", taken(&opened));
    }

    #[gpui_kit::test]
    fn x_on_a_picked_pane_closes_it_and_the_pick_falls_back_to_its_checkout(cx: &mut TestAppContext) {
        // SHELL-RAIL-23.
        let opened = open_showing(cx);
        click(cx, &opened, "pane-2");
        assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: Some(2) }));
        type_key(cx, &opened, "x");
        assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()), "{:?}", taken(&opened));
        assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: None }));
        // The row stays until the daemon's tree drops it, and the checkout keeps the keys.
        assert!(present(cx, &opened, "pane-2"));
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1);
    }

    #[gpui_kit::test]
    fn losing_the_focus_unpicks_the_row(cx: &mut TestAppContext) {
        // SHELL-RAIL-19: the pick holds the keys, so it goes when they do.
        let opened = open_showing(cx);
        click(cx, &opened, "pane-2");
        assert!(picked(cx, &opened).is_some());
        blur(cx, &opened);
        assert_eq!(picked(cx, &opened), None);
    }

    #[gpui_kit::test]
    fn a_press_on_the_stage_header_unpicks_and_gives_the_terminal_the_keys(cx: &mut TestAppContext) {
        // SHELL-SEAM-14, SHELL-RAIL-19.
        let opened = open_showing(cx);
        click(cx, &opened, "pane-2");
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1);
        press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
        assert_eq!(picked(cx, &opened), None);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 2, "a press on the header left the keys on the rail");
    }

    #[gpui_kit::test]
    fn a_press_outside_the_rail_with_no_pane_shown_still_unpicks(cx: &mut TestAppContext) {
        let opened = open_shell(cx);
        click(cx, &opened, "checkout-110");
        assert!(picked(cx, &opened).is_some());
        press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
        assert_eq!(picked(cx, &opened), None);
    }

    #[gpui_kit::test]
    fn a_pane_newly_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
        // SHELL-SEAM-9 g, f.
        let opened = open_showing(cx);
        click(cx, &opened, "checkout-110");
        blur(cx, &opened);
        assert_eq!(picked(cx, &opened), None);
        hear(cx, &opened, vec![showing(2, Stage::Workspace)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 2, "nothing picked: a new pane takes the keys");

        click(cx, &opened, "checkout-110");
        hear(cx, &opened, vec![showing(3, Stage::Workspace)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
    }

    #[gpui_kit::test]
    fn a_stage_change_with_a_pane_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
        // SHELL-SEAM-9 g, f: the same pane, a different stage.
        let opened = open_showing(cx);
        click(cx, &opened, "checkout-110");
        blur(cx, &opened);
        hear(cx, &opened, vec![showing(1, Stage::Checkouts)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 2, "nothing picked: a stage change hands the Terminal the keys");

        click(cx, &opened, "checkout-110");
        hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
    }

    #[gpui_kit::test]
    fn the_composer_and_the_feature_stage_keep_the_keys_from_the_terminal(cx: &mut TestAppContext) {
        // SHELL-SEAM-9 d, e.
        for stage in [Stage::Composer, Stage::Feature] {
            let opened = open_showing(cx);
            click(cx, &opened, "checkout-110");
            blur(cx, &opened);
            hear(cx, &opened, vec![showing(1, stage)]);
            type_key(cx, &opened, "a");
            assert_eq!(typed(&opened), 1, "{stage:?} handed the Terminal the keys");
        }
    }

    /// What a view draws of the tree: marks, rows and the pick.
    fn drawn(cx: &mut TestAppContext, opened: &Opened) -> Vec<String> {
        let mut ids: Vec<String> = Nav::ALL.iter().map(|nav| nav.id().to_owned()).collect();
        ids.extend(
            [
                "project-1",
                "project-2",
                "repo-10",
                "repo-11",
                "checkout-110",
                "checkout-112",
                "pane-1",
                "pane-2",
                "pane-4",
                "pane-5",
            ]
            .map(String::from),
        );
        let mut seen: Vec<String> = ids
            .into_iter()
            .map(|id| {
                let found = cx
                    .update_window(opened.handle, |_, window, _| window.try_find(id.clone()).map(|f| f.selected()))
                    .unwrap();
                format!("{id} {found:?}")
            })
            .collect();
        seen.push(format!("picked {:?}", picked(cx, opened)));
        seen
    }

    #[gpui_kit::test]
    fn a_call_the_daemon_refuses_leaves_the_rail_as_it_was(cx: &mut TestAppContext) {
        // CROSS-27: a request's result is never drawn before Rust says it; a
        // refusal reaches the views as a notice and no rail.
        let opened = open(cx, connected_and_busy());
        let before = drawn(cx, &opened);
        let refused = |cx: &mut TestAppContext, what: &str| {
            hear(cx, &opened, vec![Event::Notice { text: format!("{what} refused") }]);
            assert_eq!(drawn(cx, &opened), before, "{what} changed the rail before a Rail was heard");
        };
        click(cx, &opened, "checkout-110-fold");
        refused(cx, "ToggleCheckout");
        click(cx, &opened, "repo-10");
        refused(cx, "ChooseRepository");
        click(cx, &opened, "project-2");
        refused(cx, "SelectProject");
        // Git is the window's as much as the session's: pressed with no checkout it puts its
        // stage up on the window's word alone, and `review/stage` has its own refusal test.
        for nav in Nav::ALL.into_iter().filter(|nav| *nav != Nav::Git) {
            click(cx, &opened, nav.id());
            refused(cx, nav.id());
        }
        let at = cx.update_window(opened.handle, |_, window, _| window.find("pane-2").bounds().center()).unwrap();
        press(cx, &opened, at, MouseButton::Middle);
        refused(cx, "Close");
        let asked = taken(&opened);
        for call in [
            "ToggleCheckout { checkout: 110 }",
            "ChooseRepository { repository: 10 }",
            "SelectProject { project: 2 }",
            "Close { pane: 2 }",
        ] {
            assert!(asked.contains(&call.to_owned()), "{call} was never asked: {asked:?}");
        }
    }

    #[gpui_kit::test]
    fn a_closed_pane_stays_on_the_rail_until_the_tree_drops_it(cx: &mut TestAppContext) {
        // CROSS-27 for the `x` key: the pick falls back at once (it is the
        // window's), the row only when the daemon's tree says.
        let opened = open_shell(cx);
        click(cx, &opened, "pane-2");
        type_key(cx, &opened, "x");
        assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()));
        hear(cx, &opened, vec![Event::Notice { text: "refused".into() }]);
        assert!(present(cx, &opened, "pane-2"), "a refused close drew the pane gone");
        let mut without = busy();
        without.project.as_mut().unwrap().repositories[0].checkouts[0].panes.retain(|p| p.id != 2);
        hear(cx, &opened, vec![Event::Rail(Box::new(without))]);
        assert!(!present(cx, &opened, "pane-2"));
    }

    #[gpui_kit::test]
    fn what_a_call_asks_is_drawn_once_a_rail_says_it(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "repo-10");
        assert_eq!(selected(cx, &opened, "repo-10"), Some(false));
        let mut chosen = busy();
        chosen.repository = Some(10);
        hear(cx, &opened, vec![Event::Rail(Box::new(chosen))]);
        assert_eq!(selected(cx, &opened, "repo-10"), Some(true));
        assert_eq!(selected(cx, &opened, "repo-11"), Some(false));
        click(cx, &opened, "checkout-110-fold");
        assert!(present(cx, &opened, "pane-2"));
        let mut folded = busy();
        folded.project.as_mut().unwrap().repositories[0].checkouts[0].open = false;
        hear(cx, &opened, vec![Event::Rail(Box::new(folded))]);
        assert!(!present(cx, &opened, "pane-2"));
    }

    #[gpui_kit::test]
    fn a_middle_click_on_a_pane_row_closes_that_pane(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        let at = cx.update_window(opened.handle, |_, window, _| window.find("pane-2").bounds().center()).unwrap();
        press(cx, &opened, at, MouseButton::Middle);
        assert_eq!(taken(&opened), ["Close { pane: 2 }"]);
    }

    #[gpui_kit::test]
    fn the_rows_above_the_panes_send_the_pages_calls(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "repo-11");
        click(cx, &opened, "project-2");
        click(cx, &opened, "nav-checkouts");
        click(cx, &opened, "nav-workspace");
        assert_eq!(
            taken(&opened),
            [
                "ChooseRepository { repository: 11 }",
                "SelectProject { project: 2 }",
                "ShowCheckouts { open: true }",
                "ShowFeatures { open: false }",
            ]
        );
    }

    #[gpui_kit::test]
    fn a_chevron_folds_without_picking_and_a_checkout_row_picks_without_a_call(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        assert_eq!(label(cx, &opened, "checkout-110-fold").as_deref(), Some("Fold its panes away"));
        click(cx, &opened, "checkout-110-fold");
        assert_eq!(taken(&opened), ["ToggleCheckout { checkout: 110 }"]);
        cx.update(|cx| assert_eq!(opened.rail.read(cx).picked(), None));
        let at = cx
            .update_window(opened.handle, |_, window, _| {
                let found = window.find("checkout-110");
                found.bounds().origin + point(px(120.0), found.bounds().size.height / 2.0)
            })
            .unwrap();
        press(cx, &opened, at, MouseButton::Left);
        assert_eq!(taken(&opened).len(), 1);
        cx.update(|cx| {
            assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None }));
        });
    }

    #[gpui_kit::test]
    fn a_press_on_a_repositorys_chevron_chooses_it_with_the_row(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "repo-11-chevron");
        assert_eq!(taken(&opened), ["ChooseRepository { repository: 11 }"]);
    }

    #[gpui_kit::test]
    fn folding_the_checkout_of_the_picked_pane_moves_the_pick_to_the_checkout(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "pane-2");
        click(cx, &opened, "checkout-110-fold");
        assert_eq!(taken(&opened), ["Select { pane: 2 }", "ToggleCheckout { checkout: 110 }"]);
        cx.update(|cx| {
            assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None }));
        });
    }

    #[gpui_kit::test]
    fn closing_the_picked_pane_moves_the_pick_to_its_checkout(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        click(cx, &opened, "pane-2");
        let mut without = busy();
        let repo = &mut without.project.as_mut().unwrap().repositories[0];
        repo.checkouts[0].panes.retain(|p| p.id != 2);
        hear(cx, &opened, vec![Event::Rail(Box::new(without))]);
        assert!(!present(cx, &opened, "pane-2"));
        cx.update(|cx| {
            assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None }));
        });
    }

    #[test]
    fn a_pick_follows_the_tree_it_was_made_in() {
        let rail = busy();
        let on = |checkout, pane| Some(Picked { checkout, pane });
        assert_eq!(settled(None, &rail), None);
        assert_eq!(settled(on(110, Some(2)), &rail), on(110, Some(2)));
        assert_eq!(settled(on(110, Some(9)), &rail), on(110, None), "a pane that is gone");
        assert_eq!(settled(on(112, Some(5)), &rail), on(112, None), "a pane of a folded checkout");
        assert_eq!(settled(on(77, None), &rail), on(77, None), "a checkout that is gone is not the pick's to drop");
        assert_eq!(settled(on(100, None), &rail), on(100, None), "a checkout of a shut repository likewise");
    }

    #[gpui_kit::test]
    fn the_foot_counts_what_needs_you_while_connected(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        assert_eq!(label(cx, &opened, "foot-need-you").as_deref(), Some("▲ 1 need you"));
        assert_eq!(label(cx, &opened, "foot-working").as_deref(), Some("⋮ 1 working"));
        assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · local"));
    }

    #[gpui_kit::test]
    fn the_foot_says_reconnecting_and_the_rail_goes_on_showing_the_last_tree_held(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        hear(cx, &opened, vec![status(Connection::Connecting)]);
        assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · reconnecting"));
        assert!(!present(cx, &opened, "foot-need-you"), "tallies are for a connected daemon");
        // The last tree stays; it answers no press, since nothing would reach a daemon.
        assert!(present(cx, &opened, "pane-2"));
        click(cx, &opened, "pane-2");
        click(cx, &opened, "nav-checkouts");
        assert!(taken(&opened).is_empty(), "a held rail sent {:?}", taken(&opened));
        cx.update(|cx| assert_eq!(opened.rail.read(cx).picked(), None, "a held rail picked a row"));
        // Back, it answers again.
        hear(cx, &opened, vec![status(Connection::Connected)]);
        click(cx, &opened, "pane-2");
        assert_eq!(taken(&opened), ["Select { pane: 2 }"]);
    }

    #[gpui_kit::test]
    fn the_foot_says_stopped_and_holds_the_rail(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        hear(cx, &opened, vec![status(Connection::Stopped)]);
        assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · stopped"));
        click(cx, &opened, "pane-2");
        assert!(taken(&opened).is_empty());
    }

    #[gpui_kit::test]
    fn a_connection_before_any_tree_says_connecting(cx: &mut TestAppContext) {
        let opened = open(cx, vec![status(Connection::Connecting)]);
        assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · connecting"));
    }

    #[gpui_kit::test]
    fn the_foot_names_a_remote_host_and_shows_the_daemons_alarm(cx: &mut TestAppContext) {
        let status = Event::Status(Status {
            connection: Connection::Connected,
            detail: None,
            alarm: Some("argusd 0.1, window 0.2: run argus server restart".into()),
            chime: true,
            host: Some("dev-box".into()),
        });
        let opened = open(cx, vec![status, Event::Rail(Box::new(busy()))]);
        assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · dev-box"));
        assert_eq!(
            label(cx, &opened, "foot-alarm").as_deref(),
            Some("argusd 0.1, window 0.2: run argus server restart")
        );
    }

    fn console_at(place: ConsolePlace) -> Event {
        Event::Console(Box::new(argus_desktop_core::panes::console::ConsolePage { place, ..Default::default() }))
    }

    fn with_chime(chime: bool) -> Event {
        Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime, host: None })
    }

    // SHELL-FOOT: the page's status bar toggles the console and the chime by call,
    // and draws from what the daemon said, never from the press.
    #[gpui_kit::test]
    fn the_foot_console_chip_asks_for_the_toggle_and_is_named_by_what_was_heard(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        assert_eq!(label(cx, &opened, "foot-console").as_deref(), Some("Show the console"));
        click(cx, &opened, "foot-console");
        assert_eq!(taken(&opened), ["Console { gesture: Toggle }"]);
        assert_eq!(
            label(cx, &opened, "foot-console").as_deref(),
            Some("Show the console"),
            "the press alone does not show the console"
        );
        hear(cx, &opened, vec![console_at(ConsolePlace::Docked)]);
        assert_eq!(label(cx, &opened, "foot-console").as_deref(), Some("Hide the console"));
        click(cx, &opened, "foot-console");
        assert_eq!(taken(&opened), ["Console { gesture: Toggle }", "Console { gesture: Toggle }"]);
    }

    #[gpui_kit::test]
    fn the_foot_chime_switch_asks_for_the_opposite_of_what_was_heard(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        assert_eq!(label(cx, &opened, "foot-chime").as_deref(), Some("Mute the chime when an agent needs you"));
        click(cx, &opened, "foot-chime");
        assert_eq!(taken(&opened), ["SetChime { on: false }"]);
        assert_eq!(
            label(cx, &opened, "foot-chime").as_deref(),
            Some("Mute the chime when an agent needs you"),
            "the bell follows the daemon's word, not the press"
        );
        hear(cx, &opened, vec![with_chime(false)]);
        assert_eq!(label(cx, &opened, "foot-chime").as_deref(), Some("Chime when an agent needs you"));
        click(cx, &opened, "foot-chime");
        assert_eq!(taken(&opened), ["SetChime { on: false }", "SetChime { on: true }"]);
    }

    #[gpui_kit::test]
    fn the_foot_buttons_send_nothing_while_the_daemon_is_not_there(cx: &mut TestAppContext) {
        let opened = open(cx, connected_and_busy());
        hear(cx, &opened, vec![status(Connection::Stopped)]);
        click(cx, &opened, "foot-console");
        click(cx, &opened, "foot-chime");
        assert!(taken(&opened).is_empty(), "a held foot sent {:?}", taken(&opened));
    }
}
