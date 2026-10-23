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

/// Puts a picture on the clipboard as an image, from its encoded bytes (a browser's JPEG or PNG), so
/// a paste anywhere, an agent's pane among them, takes the picture.
pub fn write_picture(bytes: &[u8]) -> Result<(), String> {
    let (width, height, rgba) = rgba(bytes)?;
    let image = arboard::ImageData { width, height, bytes: rgba.into() };
    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_image(image)).map_err(|e| e.to_string())
}

/// An encoded picture's pixels as raw RGBA, which the clipboard takes.
fn rgba(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    Ok((image.width() as usize, image.height() as usize, image.into_raw()))
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
    fn a_browsers_picture_is_put_on_the_clipboard_as_its_pixels() {
        let png = encode_png(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        assert_eq!(rgba(&png), Ok((2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255])));
        assert!(rgba(b"not a picture").is_err());
    }

    #[test]
    fn pixels_that_do_not_fill_the_size_are_no_picture() {
        assert!(encode_png(2, 2, vec![0; 4]).is_none());
    }
}
