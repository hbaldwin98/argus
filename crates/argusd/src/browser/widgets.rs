//! The widgets headless Chrome draws nowhere, shown to the person to answer
//! instead: a script's dialog, a `<select>`'s list, and Chrome's own menu.
//!
//! A dialog stops the page until it is answered, so it is held from the
//! moment the shown tab opens one until anyone answers it, the person or an
//! agent's own tool. A press on a `<select>` that would open its list is
//! not sent to the page at all, since the list it opens could not be seen
//! and would eat the next click; the select is focused and its options
//! read instead, and a choice is set on it as the page's own list would,
//! with the `input` and `change` events a page listens for. A `<select>`
//! drawn as a list box (`multiple`, or a `size` above 1) is in the page and
//! left to it, as is any dropdown a page builds of its own elements.
//!
//! Chrome's menu opens on a right-click the page does not handle. Nothing
//! in DevTools says when that is, so each shown tab is given a listener
//! that notes what the click landed on and, once every handler has run,
//! whether one prevented it; after the person's right button comes up the
//! note is read, and the menu is put up only when the page left it alone.

use argus_protocol::{navigable, BrowserContext, BrowserDialog, BrowserDialogKind, BrowserInput, BrowserKey, BrowserMenuAction, BrowserSelect};
use serde_json::{json, Value};

use super::cdp::{Cdp, Event};
use super::input::commands;

/// What is open on the shown tab, waiting on the person.
#[derive(Debug, Default)]
pub(super) struct Widgets {
    pub(super) dialog: Option<BrowserDialog>,
    /// The select whose list is open: the page's handle on it, and what the
    /// person is shown of it.
    select: Option<(String, BrowserSelect)>,
    /// Chrome's menu, open where a right-click the page left alone landed.
    pub(super) context: Option<BrowserContext>,
}

impl Widgets {
    pub(super) fn select(&self) -> Option<&BrowserSelect> {
        self.select.as_ref().map(|(_, select)| select)
    }

    /// What the shown tab's event says opened or closed. A page that moves
    /// on takes an open list with it.
    pub(super) fn heard(&mut self, event: &Event) {
        match event.method.as_str() {
            "Page.javascriptDialogOpening" => {
                let text = |key: &str| event.params[key].as_str().unwrap_or_default().to_string();
                let kind = BrowserDialogKind::from_devtools(&text("type"));
                self.dialog = Some(BrowserDialog { kind, message: text("message"), default_prompt: text("defaultPrompt") });
            }
            "Page.javascriptDialogClosed" => self.dialog = None,
            "Page.frameNavigated" if event.params["frame"]["parentId"].is_null() => {
                self.select = None;
                self.context = None;
            }
            _ => {}
        }
    }

    /// Another tab is shown: what was open on the last is not on this one,
    /// which tells of its own dialog as its page is enabled.
    pub(super) fn shown_changed(&mut self) {
        *self = Widgets::default();
    }

    /// Opens the list of the `<select>` under a left press, if one is there;
    /// whether it did, in which case the press is not the page's.
    pub(super) async fn press(&mut self, cdp: &Cdp, session: &str, x: f64, y: f64) -> bool {
        let found = json!({ "expression": select_at(x, y), "returnByValue": false });
        let Ok(answer) = cdp.call("Runtime.evaluate", found, Some(session)).await else { return false };
        let Some(object) = answer["result"]["objectId"].as_str() else { return false };
        let read = json!({ "objectId": object, "functionDeclaration": DESCRIBE, "returnByValue": true });
        let described = cdp.call("Runtime.callFunctionOn", read, Some(session)).await.ok().and_then(|answer| described(&answer["result"]["value"]));
        match described {
            Some(select) => {
                self.select = Some((object.to_string(), select));
                true
            }
            None => false,
        }
    }

    /// Puts the open list away, setting the option at `index` when one was
    /// picked.
    pub(super) fn choose(&mut self, cdp: &Cdp, session: &str, index: Option<u32>) {
        let Some((object, _)) = self.select.take() else { return };
        if let Some(index) = index {
            let set = json!({ "objectId": object, "functionDeclaration": CHOOSE, "arguments": [{ "value": index }] });
            cdp.tell("Runtime.callFunctionOn", set, Some(session));
        }
        cdp.tell("Runtime.releaseObject", json!({ "objectId": object }), Some(session));
    }

    /// Starts noting right-clicks on the tab `session` shows, on this
    /// document and every one it loads after.
    pub(super) async fn watch(cdp: &Cdp, session: &str) {
        let _ = cdp.call("Page.addScriptToEvaluateOnNewDocument", json!({ "source": NOTE }), Some(session)).await;
        let _ = cdp.call("Runtime.evaluate", json!({ "expression": NOTE }), Some(session)).await;
    }

    /// The person's right button came up: Chrome's menu opens where it was,
    /// if the page left the click alone.
    pub(super) async fn right_released(&mut self, cdp: &Cdp, session: &str) {
        let read = json!({ "expression": TAKE, "awaitPromise": true, "returnByValue": true });
        let noted = tokio::time::timeout(std::time::Duration::from_secs(1), cdp.call("Runtime.evaluate", read, Some(session))).await;
        let Ok(Ok(answer)) = noted else { return };
        self.context = noted_menu(&answer["result"]["value"]);
    }

    /// Any other press puts Chrome's menu away, as it does in a window.
    pub(super) fn pressed(&mut self) {
        self.context = None;
    }

    /// Does what was chosen from Chrome's menu, and puts it away.
    pub(super) fn choose_from_menu(&mut self, cdp: &Cdp, session: &str, action: Option<&BrowserMenuAction>) {
        let Some(context) = self.context.take() else { return };
        let Some(action) = action else { return };
        let page = |input: BrowserInput| {
            for (method, params) in commands(&input) {
                cdp.tell(method, params, Some(session));
            }
        };
        match action {
            BrowserMenuAction::OpenLink => {
                if let Some(link) = context.link.filter(|link| navigable(link)) {
                    cdp.tell("Target.createTarget", json!({ "url": link }), None);
                }
            }
            BrowserMenuAction::Back => page(BrowserInput::Back),
            BrowserMenuAction::Forward => page(BrowserInput::Forward),
            BrowserMenuAction::Reload => page(BrowserInput::Reload),
            BrowserMenuAction::Cut => page(BrowserInput::Key { key: BrowserKey::Backspace }),
            BrowserMenuAction::Paste { text } => page(BrowserInput::Text { text: text.clone() }),
            BrowserMenuAction::SelectAll => {
                let key = |kind: &str| json!({ "type": kind, "key": "a", "code": "KeyA", "windowsVirtualKeyCode": 65, "modifiers": 2, "commands": ["selectAll"] });
                cdp.tell("Input.dispatchKeyEvent", key("rawKeyDown"), Some(session));
                cdp.tell("Input.dispatchKeyEvent", key("keyUp"), Some(session));
            }
        }
    }

    /// The person's answer to the dialog; it stays held until the page says
    /// it closed.
    pub(super) fn answer(&self, cdp: &Cdp, session: &str, accept: bool, text: Option<&str>) {
        if self.dialog.is_some() {
            let mut answer = json!({ "accept": accept });
            if let Some(text) = text {
                answer["promptText"] = json!(text);
            }
            cdp.tell("Page.handleJavaScriptDialog", answer, Some(session));
        }
    }
}

/// The script that finds a `<select>` whose list a press at a point opens,
/// focuses it as the press would, and gives it; else `null`.
fn select_at(x: f64, y: f64) -> String {
    format!(
        "(() => {{ const hit = document.elementFromPoint({x}, {y}); const s = hit && hit.closest('select'); \
         if (!s || s.multiple || s.size > 1 || s.disabled) return null; s.focus(); return s; }})()"
    )
}

/// The listener every shown tab is given: what a right-click landed on, and,
/// once every handler has run, whether one prevented Chrome's menu. Kept
/// under a symbol a page's own names cannot meet.
const NOTE: &str = "(() => { const key = Symbol.for('argus.contextmenu'); if (key in window) return; window[key] = null; \
    addEventListener('contextmenu', (e) => { const t = e.target instanceof Element ? e.target : e.target && e.target.parentElement; \
      setTimeout(() => { const link = t && t.closest('a[href]'); const image = t && t.closest('img'); \
        const field = t && t.matches('textarea, input') && !t.disabled && !t.readOnly && /^(text|search|url|tel|email|password|number|)$/.test(t.getAttribute('type') || '') ? t : null; \
        const selected = field ? field.value.substring(field.selectionStart || 0, field.selectionEnd || 0) : String(getSelection()); \
        window[key] = { prevented: e.defaultPrevented, x: e.clientX, y: e.clientY, link: link ? link.href : null, \
          image: image ? (image.currentSrc || image.src) : null, selection: selected || null, editable: !!(field || (t && t.isContentEditable)) }; }, 0); \
    }, true); })()";

/// Takes the last note, after the turn its listener waits for.
const TAKE: &str = "new Promise((done) => setTimeout(() => { const key = Symbol.for('argus.contextmenu'); \
    const noted = window[key] || null; if (key in window) window[key] = null; done(noted); }, 50))";

/// Chrome's menu for a note, or none when the page prevented it or nothing
/// was noted.
fn noted_menu(note: &Value) -> Option<BrowserContext> {
    if note.is_null() || note["prevented"].as_bool() != Some(false) {
        return None;
    }
    let text = |key: &str| note[key].as_str().filter(|text| !text.is_empty()).map(str::to_string);
    Some(BrowserContext {
        x: note["x"].as_f64()?,
        y: note["y"].as_f64()?,
        link: text("link"),
        image: text("image"),
        selection: text("selection"),
        editable: note["editable"].as_bool().unwrap_or(false),
    })
}

/// Read on the select: its options a person can see and where it is.
const DESCRIBE: &str = "function () { const r = this.getBoundingClientRect(); \
    const options = [...this.options].map((o, index) => { const g = o.parentElement.tagName === 'OPTGROUP' ? o.parentElement : null; \
      return { index, label: o.label || o.text, group: g ? g.label : null, disabled: o.disabled || !!(g && g.disabled), hidden: o.hidden }; }) \
      .filter(o => !o.hidden); \
    return { options, selected: this.selectedIndex < 0 ? null : this.selectedIndex, x: r.left, y: r.top, width: r.width, height: r.height }; }";

/// Run on the select: the option picked, and the events its own list sends.
const CHOOSE: &str = "function (index) { if (this.selectedIndex === index) return; this.selectedIndex = index; \
    this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); }";

/// What the page said of a select, or none for what does not read as one.
fn described(value: &Value) -> Option<BrowserSelect> {
    let number = |key: &str| value[key].as_f64();
    let options = value["options"].as_array()?.iter().map(|option| {
        Some(argus_protocol::BrowserOption {
            index: u32::try_from(option["index"].as_u64()?).ok()?,
            label: option["label"].as_str().unwrap_or_default().to_string(),
            group: option["group"].as_str().map(str::to_string),
            disabled: option["disabled"].as_bool().unwrap_or(false),
        })
    });
    Some(BrowserSelect {
        options: options.collect::<Option<Vec<_>>>()?,
        selected: value["selected"].as_u64().and_then(|index| u32::try_from(index).ok()),
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(method: &str, params: Value) -> Event {
        Event { method: method.into(), params, session: Some("s".into()) }
    }

    #[test]
    fn a_dialog_is_held_from_its_opening_until_it_closes() {
        let mut widgets = Widgets::default();
        widgets.heard(&event("Page.javascriptDialogOpening", json!({ "type": "prompt", "message": "Name?", "defaultPrompt": "Ada" })));
        let dialog = widgets.dialog.clone().unwrap();
        assert_eq!((dialog.kind, dialog.message.as_str(), dialog.default_prompt.as_str()), (BrowserDialogKind::Prompt, "Name?", "Ada"));
        widgets.heard(&event("Page.frameNavigated", json!({ "frame": { "id": "f" } })));
        assert!(widgets.dialog.is_some(), "a page that moves on does not answer its dialog");
        widgets.heard(&event("Page.javascriptDialogClosed", json!({ "result": true })));
        assert!(widgets.dialog.is_none());
    }

    #[test]
    fn a_select_is_read_with_its_groups_and_without_its_hidden_options() {
        let value = json!({
            "options": [{ "index": 0, "label": "One", "group": null, "disabled": false }, { "index": 2, "label": "Three", "group": "Odd", "disabled": true }],
            "selected": 2, "x": 10.0, "y": 20.0, "width": 120.0, "height": 24.0,
        });
        let select = described(&value).unwrap();
        assert_eq!(select.options.len(), 2);
        assert_eq!((select.options[1].index, select.options[1].group.as_deref(), select.options[1].disabled), (2, Some("Odd"), true));
        assert_eq!((select.selected, select.y), (Some(2), 20.0));
        assert_eq!(described(&json!({ "options": [] })), None, "a select with no place is not one");
    }

    #[test]
    fn chromes_menu_opens_only_for_a_click_the_page_left_alone() {
        let note = json!({ "prevented": false, "x": 10.0, "y": 20.0, "link": "https://a.test/", "image": null, "selection": "", "editable": false });
        let menu = noted_menu(&note).unwrap();
        assert_eq!((menu.link.as_deref(), menu.selection, menu.editable), (Some("https://a.test/"), None, false), "empty text is none");
        assert_eq!(noted_menu(&json!({ "prevented": true, "x": 1.0, "y": 1.0 })), None, "the page's own menu is the page's");
        assert_eq!(noted_menu(&Value::Null), None);
    }

    #[test]
    fn the_point_is_put_in_the_select_script_as_numbers() {
        assert!(select_at(3.5, 7.0).contains("elementFromPoint(3.5, 7)"));
    }
}
