//! The desktop clipboard's text: what a dragged selection copies to, and
//! what the paste key reads.

/// The clipboard's text, or `None` when there is none to read.
pub fn read() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub fn write(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|e| e.to_string())
}
