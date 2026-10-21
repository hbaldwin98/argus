//! The desktop clipboard: what a dragged selection copies to, and the
//! picture, files or text the paste key reads, and the Tauri window's
//! answer to `effects::Clipboard`.

use argus_desktop_core::connection::effects;

/// The system clipboard, for the order in `effects` to call.
pub struct System;

impl effects::Clipboard for System {
    fn write(&self, text: &str) -> Result<(), String> {
        write(text)
    }
    fn read(&self) -> Option<String> {
        read()
    }
    fn read_image(&self) -> Option<Vec<u8>> {
        read_image()
    }
    fn read_files(&self) -> Vec<std::path::PathBuf> {
        read_files()
    }
}

/// The clipboard's text, or `None` when there is none to read.
pub fn read() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// The clipboard's picture as a PNG, or `None` when it holds none. PNG
/// because every harness's image path takes it, and the clipboard hands
/// over raw RGBA, so the encoding is ours to do.
pub fn read_image() -> Option<Vec<u8>> {
    let image = arboard::Clipboard::new().ok()?.get_image().ok()?;
    encode_png(image.width, image.height, &image.bytes).ok()
}

/// The files a file manager copied, which is no picture and no text: an
/// image copied in Explorer is its path. Empty when there are none.
pub fn read_files() -> Vec<std::path::PathBuf> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get().file_list())
        .unwrap_or_default()
}

pub fn write(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|e| e.to_string())
}

fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clipboard_picture_is_encoded_as_a_png() {
        let encoded = encode_png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
