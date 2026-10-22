//! The stage when no pane is on screen: the card for waiting on the daemon, for no project, and
//! for a project with nothing shown (SHELL-STAGE-11..13).
//!
//! Which of them it is, the way out of a connection that does not answer and the checkout a shell
//! is offered on are core's (`chrome::empty`); this draws the card and sends the page's calls. A
//! press changes nothing here: the stage is empty until the daemon's tree puts a pane on it.
//! `Add a project...` asks the session for the dialog (`Call::OpenAddProject`), which `chrome/projects/add`
//! draws once the session says it is open.

use argus_desktop_core::chrome::empty::{Empty, NoPane, ShellHere, WayOut};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    div, percentage, prelude::*, px, Animation, AnimationExt as _, AnyElement, Role, SharedString, TestSupportExt as _, Transformation,
};

use super::running::running;
use crate::connection::caller::Caller;
use crate::keys::over::run_over;
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycaps_on_accent;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the card needs from the session besides the empty stage itself.
pub struct Ways<'a> {
    pub caller: &'a Caller,
    /// The daemon is gone, so a press that would reach it is dropped.
    pub held: bool,
    /// The keys that open the composer, the first chord of them, for the cap on `Start an agent`.
    pub compose: Option<Vec<String>>,
}

/// The icon tile of `.empty-icon.tile`.
fn tile(glyph: Glyph, tokens: &Tokens) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(rem(tokens.tile))
        .flex_shrink_0()
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_md))
        .bg(tokens.bg_raised)
        .text_color(tokens.accent)
        .child(glyph.at(tokens.icon_lg))
}

fn title(words: &'static str, tokens: &Tokens) -> impl IntoElement {
    div().typed(tokens.type_hero).child(words)
}

fn connecting(host: Option<String>, detail: Option<String>, way_out: WayOut, ways: &Ways, tokens: &Tokens) -> Vec<AnyElement> {
    let spinner = Glyph::Spinner.at(tokens.empty_spinner).text_color(tokens.accent).with_animation(
        "empty-spinner",
        Animation::new(tokens.motion_spin_slow).repeat(),
        |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
    );
    let on = host.map_or_else(|| "on this machine".to_owned(), |host| format!("on {host}"));
    let (caller, held) = (ways.caller.clone(), ways.held);
    let out = match way_out {
        WayOut::ThisMachine => button("connecting-local", "Use this machine", Look::Plain, false, tokens)
            .child("Use this machine")
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::SetHost { host: None });
                }
            })
            .into_any_element(),
        WayOut::Settings => button("connecting-settings", "Settings", Look::Plain, false, tokens)
            .gap(rem(tokens.sp_6))
            .child(Glyph::Gear.at(tokens.icon_sm))
            .child("Settings")
            .on_click(|_, window, cx| {
                run_over(Action::Settings, window, cx);
            })
            .into_any_element(),
    };
    let mut parts = vec![
        spinner.into_any_element(),
        title("Connecting to argusd", tokens).into_any_element(),
        div().text_color(tokens.fg_muted).child(SharedString::from(format!("{on} \u{b7} the sidebar fills in when it answers"))).into_any_element(),
    ];
    parts.extend(detail.map(|detail| div().id("connecting-detail").test_support().text_color(tokens.danger).child(SharedString::from(detail)).into_any_element()));
    parts.push(out);
    parts
}

fn no_projects(ways: &Ways, tokens: &Tokens) -> Vec<AnyElement> {
    let (caller, held) = (ways.caller.clone(), ways.held);
    vec![
        tile(Glyph::Folder, tokens).into_any_element(),
        title("No projects", tokens).into_any_element(),
        div().text_color(tokens.fg_muted).child("Add a folder of repositories, or run argus init in one.").into_any_element(),
        button("add-project", "Add a project\u{2026}", Look::Primary, false, tokens)
            .gap(rem(tokens.sp_6))
            .child(Glyph::Plus.at(tokens.icon_sm))
            .child("Add a project\u{2026}")
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::OpenAddProject);
                }
            })
            .into_any_element(),
    ]
}

fn shell_button(shell: &ShellHere, ways: &Ways, tokens: &Tokens) -> impl IntoElement {
    let (caller, held, checkout) = (ways.caller.clone(), ways.held, shell.checkout);
    button("open-shell", shell.words.clone(), Look::Plain, false, tokens)
        .gap(rem(tokens.sp_6))
        .child(Glyph::Console.at(tokens.icon_sm))
        .child(SharedString::from(shell.words.clone()))
        .on_click(move |_, _, _| {
            if !held {
                caller.call(Call::SpawnShell { checkout });
            }
        })
}

/// The cap on `Start an agent`: on the accent fill, so in the button's own ink.
pub(super) fn start_caps(ways: &Ways, tokens: &Tokens) -> Option<gpui_kit::Div> {
    ways.compose.clone().map(|chord| keycaps_on_accent(&[chord], tokens))
}

fn no_pane(page: NoPane, ways: &Ways, tokens: &Tokens) -> Vec<AnyElement> {
    let (caller, held) = (ways.caller.clone(), ways.held);
    let start = button("start-agent", "Start an agent", Look::Primary, false, tokens)
        .gap(rem(tokens.sp_6))
        .child(Glyph::Plus.at(tokens.icon_sm))
        .child("Start an agent")
        .children(start_caps(ways, tokens))
        .on_click(move |_, _, _| {
            if !held {
                caller.call(Call::ShowComposer { open: true });
            }
        });
    let mono = SharedString::from(tokens.font_mono);
    let location = page.shell.as_ref().map(|shell| {
        h_flex()
            .id("empty-where")
            .test_support()
            .text_color(tokens.fg_muted)
            .typed(tokens.type_small)
            .child("on ")
            .child(div().font_family(mono).child(SharedString::from(shell.branch_or_name.clone())))
            .child(SharedString::from(format!(" in {}", shell.repository)))
    });
    let mut parts = vec![
        tile(Glyph::Terminal, tokens).into_any_element(),
        title("No pane on screen", tokens).into_any_element(),
        div().text_color(tokens.fg_muted).child("Start an agent on a checkout, open a shell, or pick up a pane that is still running.").into_any_element(),
        h_flex()
            .flex_wrap()
            .justify_center()
            .gap(rem(tokens.sp_12))
            .child(start)
            .children(page.shell.as_ref().map(|shell| shell_button(shell, ways, tokens)))
            .into_any_element(),
    ];
    parts.extend(location.map(IntoElement::into_any_element));
    parts.extend(running(&page.project, &page.running, ways.held, ways.caller, tokens).map(IntoElement::into_any_element));
    parts
}

/// The empty stage, filling the stage it is laid over.
pub fn stage(empty: Empty, ways: &Ways, tokens: &Tokens) -> impl IntoElement {
    let (width, parts, top) = match empty {
        Empty::Connecting { host, detail, way_out } => (tokens.empty_w, connecting(host, detail, way_out, ways, tokens), false),
        Empty::NoProjects => (tokens.empty_w_projects, no_projects(ways, tokens), false),
        Empty::NoPane(page) => (tokens.empty_w_pane, no_pane(page, ways, tokens), true),
    };
    // `.empty`: centred; with panes to go back to the block sits near the top, and a long list scrolls.
    v_flex()
        .id("empty")
        .role(Role::Group)
        .test_support()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .items_center()
        .when(top, |stage| stage.pt(rem(tokens.sp_44)).overflow_y_scroll())
        .when(!top, |stage| stage.justify_center())
        .bg(tokens.see_surface)
        .child(v_flex().id("empty-body").items_center().gap(rem(tokens.sp_16)).max_w(px(width)).p(rem(tokens.sp_24)).text_center().children(parts))
}
