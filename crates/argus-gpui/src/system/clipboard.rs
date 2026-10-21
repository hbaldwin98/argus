//! The system clipboard, through arboard: the GPUI window's answer to
//! `effects::Clipboard`, which holds the order the paste key reads it in.
//!
//! Read in Rust rather than by a view, as the Tauri window does, so a copy
//! that finished on another thread needs no gesture to write.

use std::io::Cursor;
use std::path::PathBuf;

use argus_desktop_core::connection::effects;
use image::{ImageFormat, RgbaImage};

/// The system clipboard.
pub struct System;

impl effects::Clipboard for System {
    fn write(&self, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text))
            .map_err(|e| e.to_string())
    }

    fn read(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    fn read_image(&self) -> Option<Vec<u8>> {
        let image = arboard::Clipboard::new().ok()?.get_image().ok()?;
        encode_png(image.width, image.height, image.bytes.into_owned())
    }

    fn read_files(&self) -> Vec<PathBuf> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get().file_list())
            .unwrap_or_default()
    }
}

/// Raw RGBA as a PNG, which is what every harness's image path takes.
fn encode_png(width: usize, height: usize, rgba: Vec<u8>) -> Option<Vec<u8>> {
    let image = RgbaImage::from_raw(width as u32, height as u32, rgba)?;
    let mut out = Vec::new();
    image.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clipboard_picture_is_encoded_as_a_png() {
        let encoded = encode_png(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        assert!(encoded.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn pixels_that_do_not_fill_the_size_are_no_picture() {
        assert!(encode_png(2, 2, vec![0; 4]).is_none());
    }
}
