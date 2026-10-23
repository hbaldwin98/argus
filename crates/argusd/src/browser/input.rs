//! What the person's hand on a browser becomes: the DevTools commands, in
//! order, that do it to the page shown.

use argus_protocol::{navigable, BrowserButton, BrowserInput};
use serde_json::{json, Value};

/// The commands `input` is sent as, each for the shown page's session.
/// An address the person may not open is none at all.
pub(crate) fn commands(input: &BrowserInput) -> Vec<(&'static str, Value)> {
    match input {
        BrowserInput::Click { x, y } => {
            let at = |kind: &str| {
                json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": 1, "clickCount": 1 })
            };
            vec![
                ("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })),
                ("Input.dispatchMouseEvent", at("mousePressed")),
                ("Input.dispatchMouseEvent", at("mouseReleased")),
            ]
        }
        BrowserInput::Move { x, y, held } => {
            let (button, buttons) = held.map_or(("none", 0), named);
            vec![("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y, "button": button, "buttons": buttons }))]
        }
        BrowserInput::Press { x, y, button, count } => {
            let (button, buttons) = named(*button);
            vec![(
                "Input.dispatchMouseEvent",
                json!({ "type": "mousePressed", "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": count }),
            )]
        }
        BrowserInput::Release { x, y, button, count } => {
            let (button, _) = named(*button);
            vec![(
                "Input.dispatchMouseEvent",
                json!({ "type": "mouseReleased", "x": x, "y": y, "button": button, "buttons": 0, "clickCount": count }),
            )]
        }
        // A point off the page, which ends every hover as a pointer leaving
        // the window does.
        BrowserInput::Leave => vec![("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": -1, "y": -1 }))],
        BrowserInput::Wheel { x, y, dx, dy } => vec![(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy }),
        )],
        BrowserInput::Text { text } => vec![("Input.insertText", json!({ "text": text }))],
        BrowserInput::Key { key } => {
            let (name, code) = key.dom();
            let key = |kind: &str| json!({ "type": kind, "key": name, "code": name, "windowsVirtualKeyCode": code });
            let mut down = key("rawKeyDown");
            // Enter is the one of these that also types: without its text a
            // form is not submitted.
            if name == "Enter" {
                down = json!({ "type": "keyDown", "key": name, "code": name, "windowsVirtualKeyCode": code, "text": "\r" });
            }
            vec![("Input.dispatchKeyEvent", down), ("Input.dispatchKeyEvent", key("keyUp"))]
        }
        BrowserInput::Navigate { url } if navigable(url) => vec![("Page.navigate", json!({ "url": url }))],
        BrowserInput::Navigate { .. } => Vec::new(),
        BrowserInput::Back => vec![("Runtime.evaluate", json!({ "expression": "history.back()" }))],
        BrowserInput::Forward => vec![("Runtime.evaluate", json!({ "expression": "history.forward()" }))],
        BrowserInput::Reload => vec![("Page.reload", json!({}))],
        // The browser's own, or `screen`'s: none is a command on the page.
        BrowserInput::ShowTab { .. }
        | BrowserInput::FollowAgent
        | BrowserInput::Hold { .. }
        | BrowserInput::NewTab
        | BrowserInput::CloseTab { .. }
        | BrowserInput::Viewport { .. }
        | BrowserInput::Answer { .. }
        | BrowserInput::Choose { .. }
        | BrowserInput::FitView => {
            Vec::new()
        }
    }
}

/// A button as DevTools names it, and its bit in the buttons held.
fn named(button: BrowserButton) -> (&'static str, u32) {
    match button {
        BrowserButton::Left => ("left", 1),
        BrowserButton::Right => ("right", 2),
        BrowserButton::Middle => ("middle", 4),
    }
}

/// The command `input` is sent as to the browser itself, for what is not
/// done to a page but to its tabs.
pub(crate) fn browser_command(input: &BrowserInput) -> Option<(&'static str, Value)> {
    match input {
        BrowserInput::NewTab => Some(("Target.createTarget", json!({ "url": "about:blank" }))),
        BrowserInput::CloseTab { id } => Some(("Target.closeTarget", json!({ "targetId": id }))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::BrowserKey;

    #[test]
    fn a_click_presses_and_releases_where_it_lands() {
        let sent = commands(&BrowserInput::Click { x: 10.0, y: 20.0 });
        let kinds: Vec<&str> = sent.iter().map(|(_, params)| params["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["mouseMoved", "mousePressed", "mouseReleased"]);
        assert!(sent.iter().all(|(_, params)| params["x"] == 10.0 && params["y"] == 20.0));
    }

    #[test]
    fn a_drag_holds_its_button_down_until_it_is_let_go() {
        let left = BrowserButton::Left;
        let press = commands(&BrowserInput::Press { x: 1.0, y: 2.0, button: left, count: 2 });
        assert_eq!((press[0].1["type"].as_str(), press[0].1["button"].as_str()), (Some("mousePressed"), Some("left")));
        assert_eq!((press[0].1["buttons"].as_u64(), press[0].1["clickCount"].as_u64()), (Some(1), Some(2)));
        let dragged = commands(&BrowserInput::Move { x: 5.0, y: 2.0, held: Some(left) });
        assert_eq!((dragged[0].1["type"].as_str(), dragged[0].1["buttons"].as_u64()), (Some("mouseMoved"), Some(1)));
        let released = commands(&BrowserInput::Release { x: 5.0, y: 2.0, button: left, count: 2 });
        assert_eq!((released[0].1["type"].as_str(), released[0].1["buttons"].as_u64()), (Some("mouseReleased"), Some(0)));
    }

    #[test]
    fn a_hover_holds_no_button_and_leaving_moves_off_the_page() {
        let hover = commands(&BrowserInput::Move { x: 5.0, y: 2.0, held: None });
        assert_eq!((hover[0].1["button"].as_str(), hover[0].1["buttons"].as_u64()), (Some("none"), Some(0)));
        let left = commands(&BrowserInput::Leave);
        assert!(left[0].1["x"].as_f64().unwrap() < 0.0 && left[0].1["y"].as_f64().unwrap() < 0.0);
        let right = commands(&BrowserInput::Press { x: 0.0, y: 0.0, button: BrowserButton::Right, count: 1 });
        assert_eq!((right[0].1["button"].as_str(), right[0].1["buttons"].as_u64()), (Some("right"), Some(2)));
    }

    #[test]
    fn enter_types_a_return_so_a_form_submits() {
        let sent = commands(&BrowserInput::Key { key: BrowserKey::Enter });
        assert_eq!(sent[0].1["text"], "\r");
        let tab = commands(&BrowserInput::Key { key: BrowserKey::Tab });
        assert_eq!(tab[0].1["type"], "rawKeyDown");
        assert_eq!(tab[0].1["windowsVirtualKeyCode"], 9);
    }

    #[test]
    fn a_tab_is_opened_and_closed_by_the_browser_not_the_page() {
        assert_eq!(browser_command(&BrowserInput::NewTab).unwrap().0, "Target.createTarget");
        let close = browser_command(&BrowserInput::CloseTab { id: "t".into() }).unwrap();
        assert_eq!((close.0, close.1["targetId"].as_str()), ("Target.closeTarget", Some("t")));
        assert!(commands(&BrowserInput::NewTab).is_empty());
        assert!(browser_command(&BrowserInput::Reload).is_none());
    }

    #[test]
    fn the_person_cannot_send_it_off_the_web() {
        assert!(commands(&BrowserInput::Navigate { url: "file:///etc/passwd".into() }).is_empty());
        assert!(commands(&BrowserInput::Navigate { url: "javascript:alert(1)".into() }).is_empty());
        assert_eq!(commands(&BrowserInput::Navigate { url: "https://example.com".into() }).len(), 1);
    }
}
