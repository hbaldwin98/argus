//! Where a picture is decoded: off the UI thread, and landed in `Heard`'s pictures only if it is
//! still the newest of its pane.
//!
//! A screencast sends many pictures a second and a decode takes longer than a frame, so each is
//! given a ticket when it starts (`Pictures::start`) and `Pictures::land` keeps only the one that
//! holds the pane's latest. The bytes come from the model's `Source` (core's, base64 undone once),
//! fetched on the same thread as the decode. A picture that does not decode is dropped and logged:
//! the browser keeps the last it had, as the page's does.

use gpui_kit::Context;

use super::decode::decode;
use super::pictures::Picture;
use crate::connection::heard::{Heard, Pictured};

/// Sets `pane`'s latest picture decoding; the places that draw it are told with a [`Pictured`] when
/// it lands.
pub fn begin(heard: &mut Heard, pane: u64, cx: &mut Context<Heard>) {
    let ticket = heard.pictures.start(pane);
    let source = heard.source();
    cx.spawn(async move |this, cx| {
        let decoded = cx
            .background_executor()
            .spawn(async move {
                let picture = source(pane).ok_or("there is no picture to decode".to_owned())?;
                let image = decode(&picture.bytes).map_err(|why| why.to_string())?;
                Ok::<_, String>(Picture { image: std::sync::Arc::new(image), width: picture.width, height: picture.height })
            })
            .await;
        match decoded {
            Ok(picture) => {
                let _ = this.update(cx, |heard, cx| {
                    if heard.pictures.land(pane, ticket, picture) {
                        cx.emit(Pictured(pane));
                    }
                });
            }
            Err(why) => tracing::debug!(pane, "a browser picture was dropped: {why}"),
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::Event;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use argus_desktop_core::browser::BrowserShot;
    use gpui_kit::{AppContext as _, TestAppContext};
    use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder as _};

    fn shot(pane: u64, wide: u32) -> BrowserShot {
        let mut out = Vec::new();
        PngEncoder::new(&mut out).write_image(&[9u8, 9, 9, 255].repeat(wide as usize), wide, 1, ExtendedColorType::Rgba8).unwrap();
        BrowserShot { pane, jpeg: STANDARD.encode(out), width: 1280, height: 800 }
    }

    #[gpui_kit::test]
    fn a_picture_heard_is_decoded_and_its_pane_told(cx: &mut TestAppContext) {
        let heard = cx.new(|_| Heard::new());
        let told = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = told.clone();
        cx.update(|cx| {
            cx.subscribe(&heard, move |_, Pictured(pane): &Pictured, _| sink.borrow_mut().push(*pane)).detach();
        });
        heard.update(cx, |heard, cx| {
            heard.hear(Event::BrowserShot(shot(7, 3)));
            heard.announce(cx);
        });
        cx.run_until_parked();
        let size = heard.read_with(cx, |heard, _| heard.pictures.of(7).map(|picture| (picture.image.size(0).width.0, picture.width)));
        assert_eq!(size, Some((3, 1280)));
        assert_eq!(*told.borrow(), [7]);
    }

    #[gpui_kit::test]
    fn of_two_pictures_in_one_batch_only_the_newest_is_decoded(cx: &mut TestAppContext) {
        let heard = cx.new(|_| Heard::new());
        heard.update(cx, |heard, cx| {
            heard.hear(Event::BrowserShot(shot(7, 3)));
            heard.hear(Event::BrowserShot(shot(7, 5)));
            heard.announce(cx);
        });
        cx.run_until_parked();
        let (wide, started) = heard.read_with(cx, |heard, _| (heard.pictures.of(7).map(|picture| picture.image.size(0).width.0), heard.pictures.started()));
        assert_eq!(wide, Some(5));
        assert_eq!(started, 1, "the older picture of the batch was never set decoding");
    }

    #[gpui_kit::test]
    fn a_picture_that_does_not_decode_leaves_the_last_one(cx: &mut TestAppContext) {
        let heard = cx.new(|_| Heard::new());
        heard.update(cx, |heard, cx| {
            heard.hear(Event::BrowserShot(shot(7, 3)));
            heard.announce(cx);
        });
        cx.run_until_parked();
        heard.update(cx, |heard, cx| {
            heard.hear(Event::BrowserShot(BrowserShot { pane: 7, jpeg: "garbage!".into(), width: 1, height: 1 }));
            heard.announce(cx);
        });
        cx.run_until_parked();
        let wide = heard.read_with(cx, |heard, _| heard.pictures.of(7).map(|picture| picture.image.size(0).width.0));
        assert_eq!(wide, Some(3));
    }
}
