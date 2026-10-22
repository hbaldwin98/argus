//! A browser's latest picture as bytes a window can draw, with no base64
//! in the way: the page cannot set a `data:` image, a native window need
//! not either.

use std::sync::Arc;

use base64::Engine;

use super::BrowserShot;

/// One picture of a pane's browser. The bytes are the JPEG the daemon sent
/// (or, in a scene, a PNG: sniff the format), decoded from base64 once and
/// shared by every place that draws the browser, a tile's and a cell's.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub pane: u64,
    /// Grows with every picture `Browsers` is given, so a window knows a
    /// picture of a pane is newer than the one it drew by this alone.
    pub serial: u64,
    pub bytes: Arc<[u8]>,
    /// The page's CSS pixels, which a click's point is sent in; the
    /// picture's own pixels are these times the viewport's scale.
    pub width: u32,
    pub height: u32,
}

/// The shot's bytes, or `None` when its base64 is not base64: that browser
/// then shows no picture rather than a broken one.
pub(super) fn decode(shot: &BrowserShot, serial: u64) -> Option<Picture> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(shot.jpeg.as_bytes()).ok()?;
    Some(Picture { pane: shot.pane, serial, bytes: bytes.into(), width: shot.width, height: shot.height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(jpeg: &str) -> BrowserShot {
        BrowserShot { pane: 4, jpeg: jpeg.into(), width: 1280, height: 800 }
    }

    #[test]
    fn a_shot_is_decoded_from_base64() {
        let picture = decode(&shot("aGk="), 3).unwrap();
        assert_eq!((&*picture.bytes, picture.pane, picture.serial, picture.width, picture.height), (&b"hi"[..], 4, 3, 1280, 800));
    }

    #[test]
    fn text_that_is_not_base64_is_no_picture() {
        assert_eq!(decode(&shot("not base64!"), 1), None);
    }
}
