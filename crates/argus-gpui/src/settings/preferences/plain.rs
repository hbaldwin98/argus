//! The sections after Appearance but Dictation's, and the segment every choice of a few is drawn
//! as: Editor, Notifications, Features, Views and Connection, with Dictation (`dictation`) in its
//! place among them.
//!
//! Every control here sends its choice at once and draws only what the session sends back, so a
//! refused choice leaves the control showing what is true (CROSS-27). A switch asks for the
//! opposite of what it shows; a segment asks for its own value.

use argus_desktop_core::board::features::Layout;
use argus_desktop_core::connection::page::{SettingChange, SettingsPage};
use argus_desktop_core::settings::remembered::{EditorMode, SavedLinks};
use gpui_kit::component::input::Input;
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString};

use super::rows::{meta, row, section, Group};
use super::{Asked, PreferencesView, SECTIONS_AT};
use crate::kit::segment::{pill, track};
use crate::kit::tip::tip;
use crate::kit::toggle::toggle;
use crate::theme::scale::rem;
use crate::theme::Tokens;

const EDITORS: [(EditorMode, &str); 3] = [(EditorMode::Floating, "Floating"), (EditorMode::Checkout, "In the checkout"), (EditorMode::Separately, "Separately")];
const LAYOUTS: [(Layout, &str); 2] = [(Layout::List, "List"), (Layout::Board, "Board")];
const LINKS: [(SavedLinks, &str); 2] = [(SavedLinks::Dialog, "Dialog"), (SavedLinks::Column, "Column")];

impl PreferencesView {
    /// `.seg`: `options` as pills in one track, `chosen` lit. `ask` is the change a pill asks for;
    /// one in `unavailable` (with its reason) is dimmed and answers no press.
    pub(super) fn segment<T: Copy + PartialEq + 'static, C: Asked + 'static>(
        &self,
        label: &'static str,
        options: &[(T, &'static str)],
        chosen: T,
        unavailable: &[(T, String)],
        ask: fn(T) -> C,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = *Tokens::global(cx);
        let tokens = &tokens;
        track(label, label, Role::RadioGroup, tokens)
            .children(options.iter().enumerate().map(|(at, (value, name))| {
                let value = *value;
                let why = unavailable.iter().find(|(offered, _)| *offered == value).map(|(_, why)| why.clone());
                let pill = pill((label, at), Role::RadioButton, value == chosen, tokens).aria_label(SharedString::from(*name)).child(SharedString::from(*name));
                match why {
                    Some(why) => pill.text_color(tokens.fg_disabled).cursor_default().tooltip(tip(why)).into_any_element(),
                    None => pill.on_click(cx.listener(move |this, _, _, _| this.set(ask(value)))).into_any_element(),
                }
            }))
            .into_any_element()
    }

    /// A switch asking for the opposite of what it shows.
    pub(super) fn switch<C: Asked + 'static>(&self, id: &'static str, label: &'static str, on: bool, off: bool, ask: fn(bool) -> C, cx: &mut Context<Self>) -> AnyElement {
        let tokens = *Tokens::global(cx);
        let tokens = &tokens;
        let press = toggle(id, label, on, off, tokens);
        if off {
            return press.into_any_element();
        }
        press.on_click(cx.listener(move |this, _, _, _| this.set(ask(!on)))).into_any_element()
    }

    /// Editor, Notifications, Dictation, Features, Views and Connection, in the order the nav lists them.
    pub(super) fn plain(&self, page: &SettingsPage, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let chosen = &page.chosen;
        let field = div().w(rem(tokens.setting_field_w)).max_w(gpui_kit::relative(0.5)).flex_shrink_0().font_family(SharedString::from(tokens.font_mono)).child(
            Input::new(self.command.input()).id("settings-command").aria_label("Editor command").h(rem(tokens.control_h)),
        );
        let files = vec![
            row("Files open", Some("A known GUI editor always opens on its own"), vec![], self.segment("Files open", &EDITORS, chosen.editor, &[], |mode| SettingChange::Editor { mode }, cx), tokens),
            row("Editor command", Some("Empty uses $VISUAL, then $EDITOR"), vec![], field, tokens),
        ];
        let code = vec![
            row("Vim keys", Some("In the Code stage: normal, insert and visual modes, and :w"), vec![], self.switch("settings-vim", "Vim's keys in the Code stage", chosen.vim, false, |on| SettingChange::Vim { on }, cx), tokens),
            row(
                "Install grammars without asking",
                Some("Colours for a language Argus knows download from npm when its file opens; one found by searching npm is still asked about"),
                vec![],
                self.switch("settings-grammars", "Install known grammars without asking", chosen.install_grammars, false, |on| SettingChange::InstallGrammars { on }, cx),
                tokens,
            ),
        ];
        let editor = vec![Group::named("Files", files), Group::named("Code stage", code)];
        let notifications = vec![
            row(
                "When an agent needs you",
                Some("A system notification naming the pane: waiting, needs-review or failed"),
                vec![],
                self.switch("settings-notify", "Notify when an agent needs you", chosen.notify, false, |on| SettingChange::Notify { on }, cx),
                tokens,
            ),
            row(
                "Only while this window is in the background",
                None,
                vec![],
                self.switch("settings-background", "Only while this window is in the background", chosen.notify_in_background, !chosen.notify, |only| SettingChange::NotifyInBackground { only }, cx),
                tokens,
            ),
        ];
        let features = vec![row(
            "Feature index",
            Some("Remembered per repository; this is what a new one opens as"),
            vec![],
            self.segment("Feature index layout", &LAYOUTS, chosen.feature_layout, &[], |layout| SettingChange::FeatureLayout { layout }, cx),
            tokens,
        )];
        let views = vec![row(
            "Links in a saved view open",
            Some("A dialog over the stage, or the next column beside the view"),
            vec![],
            self.segment("Links in a saved view open", &LINKS, chosen.saved_links, &[], |opens| SettingChange::SavedLinks { opens }, cx),
            tokens,
        )];
        let connection = vec![div().py(rem(tokens.sp_12)).child(meta("Nothing to set here yet.", tokens).text_color(tokens.fg_faint)).into_any_element()];
        let one = |rows| vec![Group::plain(rows)];
        let dictation = self.dictation(tokens, cx);
        [editor, one(notifications), dictation, one(features), one(views), one(connection)]
            .into_iter()
            .enumerate()
            .map(|(at, groups)| {
                let named = &super::nav::SECTIONS[SECTIONS_AT + at];
                section(("settings-section", SECTIONS_AT + at), named.name, named.about, groups, tokens)
            })
            .collect()
    }
}
