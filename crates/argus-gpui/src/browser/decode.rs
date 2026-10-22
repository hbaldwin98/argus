//! How the bytes of a browser's picture become an image the window draws: the format sniffed, the pixels decoded and put in the order GPUI's atlas takes them.
//!
//! The daemon's screencast is JPEG; the scenes the page is photographed in carry PNG in the same
//! field, so the format is read from the bytes and never from the field's name. Core has undone the
//! base64 once (`Browsers::picture`), so these are the bytes themselves. Nothing else is
//! drawn: a picture of a format this does not know is a failure the caller drops, not a guess.
//! This touches no window and runs on any thread.

use std::fmt;

use gpui_kit::RenderImage;
use image::{Frame, ImageFormat, RgbaImage};

/// Why a picture was not drawn.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// Neither a JPEG nor a PNG.
    Format,
    /// The format was right and the pixels were not.
    Pixels,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Failure::Format => "the picture was neither a JPEG nor a PNG",
            Failure::Pixels => "the picture's pixels did not decode",
        })
    }
}

/// The image `bytes` hold, in the BGRA order the atlas takes, premultiplied by its alpha.
pub fn decode(bytes: &[u8]) -> Result<RenderImage, Failure> {
    let format = match image::guess_format(bytes) {
        Ok(format @ (ImageFormat::Jpeg | ImageFormat::Png)) => format,
        _ => return Err(Failure::Format),
    };
    let mut pixels: RgbaImage = image::load_from_memory_with_format(bytes, format).map_err(|_| Failure::Pixels)?.into_rgba8();
    for pixel in pixels.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        // A screencast has no alpha; a PNG may.
        let premultiply = |channel: u8| if a == 255 { channel } else { ((u16::from(channel) * u16::from(a) + 127) / 255) as u8 };
        pixel.0 = [premultiply(b), premultiply(g), premultiply(r), a];
    }
    Ok(RenderImage::new(vec![Frame::new(pixels)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, codecs::png::PngEncoder, ExtendedColorType, ImageEncoder as _};

    fn png(rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        PngEncoder::new(&mut out).write_image(&rgba.repeat(2 * 3), 2, 3, ExtendedColorType::Rgba8).unwrap();
        out
    }

    fn jpeg() -> Vec<u8> {
        let mut out = Vec::new();
        JpegEncoder::new(&mut out).write_image(&[200u8, 30, 30].repeat(8 * 4), 8, 4, ExtendedColorType::Rgb8).unwrap();
        out
    }

    #[test]
    fn a_png_is_sniffed_and_its_pixels_come_out_blue_first() {
        let image = decode(&png([255, 0, 0, 255])).unwrap();
        assert_eq!(image.as_bytes(0).unwrap()[..4], [0, 0, 255, 255]);
        let size = image.size(0);
        assert_eq!((size.width.0, size.height.0), (2, 3));
    }

    #[test]
    fn a_jpeg_is_sniffed_too() {
        let image = decode(&jpeg()).unwrap();
        let size = image.size(0);
        assert_eq!((size.width.0, size.height.0), (8, 4));
        let first = &image.as_bytes(0).unwrap()[..4];
        assert!(first[2] > first[0], "red is the loud channel, so the third byte of BGRA: {first:?}");
    }

    #[test]
    fn a_translucent_pixel_is_premultiplied() {
        let image = decode(&png([200, 100, 50, 128])).unwrap();
        assert_eq!(image.as_bytes(0).unwrap()[..4], [25, 50, 100, 128]);
    }

    #[test]
    fn what_is_not_a_picture_is_a_failure_for_the_caller_to_drop() {
        assert_eq!(decode(b"GIF89a....").err(), Some(Failure::Format));
        let mut cut = png([1, 2, 3, 255]);
        cut.truncate(20);
        assert_eq!(decode(&cut).err(), Some(Failure::Pixels));
    }
}
