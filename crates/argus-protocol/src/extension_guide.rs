//! How to build an extension, from nothing to one that runs: the guide
//! `argus extension guide` prints, written for an agent that has never
//! seen Argus.
//!
//! It lives beside the types it teaches so its tests can hold it to them:
//! every JSON example, and every literal the walkthrough's Python sends, is
//! read through the real types, so a renamed field fails here rather than
//! in somebody's extension. Its limits and key lists are not written in it
//! at all but filled in from the constants as it is printed, so they cannot
//! drift from what the daemon enforces.

use crate::views::agent_view::Tone;
use crate::views::check::{MAX_TABLE_ROWS, MAX_VIEW_ID_BYTES, MAX_VIEW_TITLE_BYTES};
use crate::ext_console::{MAX_CONSOLE_ARGS, MAX_CONSOLE_ARG_BYTES, MAX_CONSOLE_ENV};
use crate::extension_keys::{KEPT, STAGE_KEYS};
use crate::extensions::{
    ExtKey, EXT_ICONS, MAX_ABOUT_GUIDE_BYTES, MAX_ACTIONS, MAX_COMMANDS, MAX_EXTENSION_BODY, MAX_EXTENSION_NAME,
    MAX_HELP_BYTES, MAX_ICON_NAME, MAX_ITEMS, MAX_NOTICE_CHARS, MAX_SUMMARY_BYTES, MAX_VIEWS,
};

/// The guide as written, its `{{NAME}}` placeholders unfilled.
const SOURCE: &str = include_str!("extension_guide.md");

/// Every tone, in the order the guide lists them. The match is what makes
/// a new tone a compile error here until the guide names it.
const TONES: [Tone; 6] = [Tone::Neutral, Tone::Good, Tone::Warn, Tone::Bad, Tone::Active, Tone::Info];

fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "neutral",
        Tone::Good => "good",
        Tone::Warn => "warn",
        Tone::Bad => "bad",
        Tone::Active => "active",
        Tone::Info => "info",
    }
}

/// A key as the guide spells it, Space named rather than left blank.
fn spoken(key: &ExtKey) -> String {
    if key.key == " " {
        return ExtKey { key: "Space".into(), ..key.clone() }.spoken();
    }
    key.spoken()
}

/// `items` as inline code, each once: a list of keys holds commas and
/// backticks of its own, and `Ctrl v` and `Ctrl V` are both spoken `Ctrl V`.
fn code_list<I: IntoIterator<Item = String>>(items: I) -> String {
    let mut listed: Vec<String> = Vec::new();
    for item in items {
        let code = if item.contains('`') { format!("`` {item} ``") } else { format!("`{item}`") };
        if !listed.contains(&code) {
            listed.push(code);
        }
    }
    listed.join(", ")
}

/// What each placeholder is filled with.
fn fills() -> Vec<(&'static str, String)> {
    let kept = KEPT.iter().map(|k| ExtKey { key: k.key.to_string(), ctrl: k.ctrl, alt: k.alt, shift: k.shift });
    vec![
        ("MAX_EXTENSION_NAME", MAX_EXTENSION_NAME.to_string()),
        ("MAX_EXTENSION_BODY", MAX_EXTENSION_BODY.to_string()),
        ("MAX_VIEWS", MAX_VIEWS.to_string()),
        ("MAX_VIEW_ID_BYTES", MAX_VIEW_ID_BYTES.to_string()),
        ("MAX_VIEW_TITLE_BYTES", MAX_VIEW_TITLE_BYTES.to_string()),
        ("MAX_ICON_NAME", MAX_ICON_NAME.to_string()),
        ("MAX_TABLE_ROWS", MAX_TABLE_ROWS.to_string()),
        ("MAX_ACTIONS", MAX_ACTIONS.to_string()),
        ("MAX_COMMANDS", MAX_COMMANDS.to_string()),
        ("MAX_HELP_BYTES", MAX_HELP_BYTES.to_string()),
        ("MAX_SUMMARY_BYTES", MAX_SUMMARY_BYTES.to_string()),
        ("MAX_ABOUT_GUIDE_BYTES", MAX_ABOUT_GUIDE_BYTES.to_string()),
        ("MAX_ITEMS", MAX_ITEMS.to_string()),
        ("MAX_NOTICE_CHARS", MAX_NOTICE_CHARS.to_string()),
        ("MAX_CONSOLE_ARGS", MAX_CONSOLE_ARGS.to_string()),
        ("MAX_CONSOLE_ARG_BYTES", MAX_CONSOLE_ARG_BYTES.to_string()),
        ("MAX_CONSOLE_ENV", MAX_CONSOLE_ENV.to_string()),
        ("EXT_ICONS", code_list(EXT_ICONS.iter().map(|icon| icon.to_string()))),
        ("TONES", code_list(TONES.iter().map(|tone| tone_name(*tone).to_string()))),
        ("KEPT_KEYS", code_list(kept.map(|key| spoken(&key)))),
        ("STAGE_KEYS", code_list(STAGE_KEYS.iter().map(|key| spoken(&ExtKey::plain(key))))),
    ]
}

/// The guide as `argus extension guide` prints it.
pub fn guide() -> String {
    fills().into_iter().fold(SOURCE.to_string(), |text, (name, fill)| text.replace(&format!("{{{{{name}}}}}"), &fill))
}

#[cfg(test)]
mod tests;
