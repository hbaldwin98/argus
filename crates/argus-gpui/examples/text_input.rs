//! Can gpui-kit's `Input` and `Textarea` carry the desktop's text surfaces?
//!
//! A filter field, and a compose box shaped like the agent pane's: it grows
//! from 2 to 8 rows, Ctrl+Enter submits while Enter stays a newline, the draft
//! outlives re-renders, and an image on the clipboard is caught on paste and
//! shown as an attachment instead of being dropped. Each submit prints to
//! stdout. `--probe-clipboard` skips the window and reports what GPUI and
//! arboard each read back from the real system clipboard.

use argus_gpui::theme::Ledger;
use argus_gpui::window;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    div, prelude::*, px, App, ClipboardEntry, ClipboardItem, Context, ElementId, Entity, Image,
    ImageFormat, SharedString, Subscription, TestSupportExt as _, Window,
};

const MIN_ROWS: usize = 2;
const MAX_ROWS: usize = 8;

/// What the filter narrows; stands in for the project rail's rows.
const SAMPLE: &[&str] = &[
    "argus-protocol", "argus-client", "argusd", "argus-desktop", "argus-gpui", "session.rs",
    "keymap.ts", "terminal screen", "split grid", "command palette",
];

/// One send, as the daemon call would receive it.
#[derive(Clone, Debug, PartialEq)]
pub struct Submitted {
    pub text: String,
    /// Pixel size of the image that was attached when it was sent.
    pub image: Option<(u32, u32)>,
}

pub struct Compose {
    filter: Entity<InputState>,
    draft: Entity<TextareaState>,
    /// The title the rail would be showing; changing it re-renders the view
    /// without touching either text state, as new daemon data does.
    items: Vec<SharedString>,
    attached: Option<(u32, u32)>,
    submitted: Vec<Submitted>,
    _subscriptions: [Subscription; 2],
}

impl Compose {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));
        let draft = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(MIN_ROWS, MAX_ROWS)
                .placeholder("Message the agent")
        });
        let on_filter = cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let on_draft = cx.subscribe_in(&draft, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary: true, .. } = event {
                this.submit(window, cx);
            }
        });
        Self {
            filter,
            draft,
            items: SAMPLE.iter().map(|name| SharedString::from(*name)).collect(),
            attached: None,
            submitted: Vec::new(),
            _subscriptions: [on_filter, on_draft],
        }
    }

    /// The textarea's element id, which gpui-kit derives from the state entity.
    pub fn draft_id(&self) -> ElementId {
        ("input", self.draft.entity_id()).into()
    }

    pub fn submitted(&self) -> &[Submitted] {
        &self.submitted
    }

    pub fn attached(&self) -> Option<(u32, u32)> {
        self.attached
    }

    pub fn set_items(&mut self, items: Vec<SharedString>, cx: &mut Context<Self>) {
        self.items = items;
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.draft.read(cx).value();
        // The engine binds Ctrl+Enter to the same action as Enter and inserts
        // the newline before it emits `PressEnter`, so by now the draft ends
        // in a newline the user never meant. Only that one is dropped.
        let text = raw.strip_suffix('\n').unwrap_or(&raw).to_string();
        if text.trim().is_empty() && self.attached.is_none() {
            self.draft.update(cx, |draft, cx| draft.set_value(text, window, cx));
            return;
        }
        let sent = Submitted { text, image: self.attached.take() };
        println!("submit {:?} image={:?}", sent.text, sent.image);
        self.submitted.push(sent);
        self.draft.update(cx, |draft, cx| draft.set_value("", window, cx));
        cx.notify();
    }

    /// The paste hook: an image with no text is attached and swallowed;
    /// anything else falls through to the engine's own text insertion.
    fn paste(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        if item.text().is_some() {
            return false;
        }
        let Some(size) = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image_size(image)),
            _ => None,
        }) else {
            return false;
        };
        // A format whose header we do not read: ask arboard, which decodes
        // whatever is on the clipboard itself.
        self.attached = size.or_else(arboard_image_size);
        cx.notify();
        true
    }
}

impl Render for Compose {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let needle = self.filter.read(cx).value().to_lowercase();
        let shown: Vec<_> = self
            .items
            .iter()
            .filter(|item| item.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        let view = cx.entity().downgrade();
        let attachment = self.attached.map(|(w, h)| SharedString::from(format!("image {w}x{h} attached")));

        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(Input::new(&self.filter).id("filter").cleanable(true).w(px(280.0)))
            .child(
                v_flex().gap_1().flex_1().min_h_0().text_sm().text_color(ledger.muted).children(
                    shown.into_iter().map(|item| {
                        div()
                            .id(SharedString::from(format!("item-{item}")))
                            .test_support()
                            .px_2()
                            .child(item)
                    }),
                ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        Textarea::new(&self.draft)
                            .w_full()
                            .on_paste(move |item, _, app: &mut App| {
                                view.update(app, |this, cx| this.paste(item, cx)).unwrap_or(false)
                            }),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .text_xs()
                            .text_color(ledger.faint)
                            .child(match attachment {
                                Some(label) => div()
                                    .id("attached")
                                    .role(gpui_kit::Role::Status)
                                    .test_support()
                                    .aria_label(label.clone())
                                    .text_color(ledger.info)
                                    .child(label)
                                    .into_any_element(),
                                None => div().into_any_element(),
                            })
                            .child("Ctrl+Enter sends, Enter adds a line"),
                    ),
            )
    }
}

/// Pixel size of a clipboard image, from its header alone.
///
/// GPUI's Windows clipboard hands a screenshot over as a BMP (from `CF_DIB`)
/// and a copied PNG as a PNG; the `image` crate here only has PNG enabled, so
/// the sizes are read straight out of the headers rather than decoding.
pub fn image_size(image: &Image) -> Option<(u32, u32)> {
    let b = &image.bytes;
    let be = |at: usize| b.get(at..at + 4).map(|v| u32::from_be_bytes(v.try_into().unwrap()));
    let le32 = |at: usize| b.get(at..at + 4).map(|v| i32::from_le_bytes(v.try_into().unwrap()));
    let le16 = |at: usize| b.get(at..at + 2).map(|v| u16::from_le_bytes(v.try_into().unwrap()) as u32);
    match image.format {
        // 8-byte signature, then the IHDR chunk: length, "IHDR", width, height.
        ImageFormat::Png if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.get(12..16) == Some(b"IHDR") => {
            Some((be(16)?, be(20)?))
        }
        // 14-byte file header, then BITMAPINFOHEADER; a bottom-up bitmap has a
        // positive height and a top-down one a negative, so take the magnitude.
        ImageFormat::Bmp if b.starts_with(b"BM") && le32(14)? >= 40 => {
            Some((le32(18)?.unsigned_abs(), le32(22)?.unsigned_abs()))
        }
        ImageFormat::Gif if b.starts_with(b"GIF") => Some((le16(6)?, le16(8)?)),
        ImageFormat::Jpeg => jpeg_size(b),
        _ => None,
    }
}

/// Walks a JPEG's marker segments to the first start-of-frame, which holds the size.
fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 9 <= b.len() {
        if b[at] != 0xff {
            return None;
        }
        let marker = b[at + 1];
        // SOF0..SOF15 except DHT (c4), JPG (c8) and DAC (cc).
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let height = u16::from_be_bytes([b[at + 5], b[at + 6]]) as u32;
            let width = u16::from_be_bytes([b[at + 7], b[at + 8]]) as u32;
            return Some((width, height));
        }
        at += 2 + u16::from_be_bytes([b[at + 2], b[at + 3]]) as usize;
    }
    None
}

fn arboard_image_size() -> Option<(u32, u32)> {
    let image = arboard::Clipboard::new().ok()?.get_image().ok()?;
    Some((image.width as u32, image.height as u32))
}

/// Writes a known image with arboard, then reads it back both ways, so the
/// report says which path gets a size out of a real clipboard.
fn probe_clipboard(cx: &mut App) {
    let (w, h) = (32usize, 16usize);
    let pixels = vec![0x80u8; w * h * 4];
    let wrote = arboard::Clipboard::new().and_then(|mut board| {
        board.set_image(arboard::ImageData { width: w, height: h, bytes: pixels.into() })
    });
    println!("probe: arboard set_image {w}x{h}: {wrote:?}");
    match cx.read_from_clipboard() {
        None => println!("probe: gpui read_from_clipboard: None"),
        Some(item) => {
            println!("probe: gpui read_from_clipboard: {} entries", item.entries().len());
            for entry in item.entries() {
                match entry {
                    ClipboardEntry::Image(image) => println!(
                        "probe: gpui image format={:?} bytes={} size={:?}",
                        image.format,
                        image.bytes.len(),
                        image_size(image)
                    ),
                    other => println!("probe: gpui other entry {other:?}"),
                }
            }
        }
    }
    println!("probe: arboard get_image size: {:?}", arboard_image_size());
    cx.quit();
}

fn main() {
    let probe = std::env::args().any(|arg| arg == "--probe-clipboard");
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        if probe {
            return probe_clipboard(cx);
        }
        let (_, compose) =
            window::open(cx, |window, cx| cx.new(|cx| Compose::new(window, cx))).expect("the window opens");
        // Focus the compose box so a person can type straight away.
        let draft = compose.read(cx).draft.clone();
        cx.defer(move |cx| {
            cx.active_window().map(|handle| {
                handle.update(cx, |_, window, cx| draft.update(cx, |draft, cx| draft.focus(window, cx)))
            });
        });
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AnyWindowHandle, TestAppContext};

    fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Compose>) {
        cx.update(window::init);
        cx.update(|cx| {
            let mut compose = None;
            let (handle, _) = window::open(cx, |window, cx| {
                let entity = cx.new(|cx| Compose::new(window, cx));
                compose = Some(entity.clone());
                entity
            })
            .expect("test window opens");
            (handle, compose.unwrap())
        })
    }

    fn draft_id(compose: &Entity<Compose>, cx: &mut TestAppContext) -> ElementId {
        cx.update(|cx| compose.read(cx).draft_id())
    }

    fn draft_text(compose: &Entity<Compose>, cx: &mut TestAppContext) -> String {
        cx.update(|cx| compose.read(cx).draft.read(cx).value().to_string())
    }

    fn png(width: u32, height: u32) -> Image {
        let mut bytes = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        Image::from_bytes(ImageFormat::Png, bytes)
    }

    #[gpui_kit::test]
    fn ctrl_enter_submits_and_clears(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.input("hello", cx);
            window.press("secondary-enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(draft_text(&compose, cx), "");
        cx.update(|cx| {
            // The newline the shared Enter action inserted must not be sent.
            assert_eq!(
                compose.read(cx).submitted(),
                [Submitted { text: "hello".into(), image: None }]
            );
        });
    }

    #[gpui_kit::test]
    fn enter_inserts_a_newline_and_does_not_submit(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.input("a", cx);
            window.press("enter", cx);
            window.input("b", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(draft_text(&compose, cx), "a\nb");
        cx.update(|cx| assert!(compose.read(cx).submitted().is_empty()));
    }

    #[gpui_kit::test]
    fn ctrl_enter_on_an_empty_draft_sends_nothing(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.press("secondary-enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(draft_text(&compose, cx), "");
        cx.update(|cx| assert!(compose.read(cx).submitted().is_empty()));
    }

    #[gpui_kit::test]
    fn draft_survives_a_rerender_with_new_data(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.input("half a thought", cx);
            window.press("enter", cx);
            window.input("second line", cx);
            window.click("filter", cx);
            window.input("arg", cx);
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            compose.update(cx, |view, cx| {
                view.set_items(vec!["replaced-1".into(), "argument".into()], cx)
            });
            window.render_frame(cx);
            assert!(window.try_find("item-replaced-1").is_none(), "the filter still applies");
            assert!(window.try_find("item-argument").is_some());
            assert_eq!(window.find("filter").value(), Some("arg"));
            assert_eq!(window.find(id.clone()).value(), Some("half a thought\nsecond line"));
        })
        .unwrap();
        assert_eq!(draft_text(&compose, cx), "half a thought\nsecond line");
    }

    #[gpui_kit::test]
    fn filter_narrows_the_rows(cx: &mut TestAppContext) {
        let (handle, _compose) = open(cx);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("item-argusd").is_some());
            assert!(window.try_find("item-keymap.ts").is_some());
            window.click("filter", cx);
            window.input("argusd", cx);
            window.render_frame(cx);
            assert!(window.try_find("item-argusd").is_some());
            assert!(window.try_find("item-keymap.ts").is_none());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn grows_from_two_rows_to_eight_and_stops(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        let mut heights = Vec::new();
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.render_frame(cx);
            heights.push(window.find(id.clone()).bounds().size.height);
            for line in 1..12 {
                window.input("x", cx);
                window.press("enter", cx);
                window.render_frame(cx);
                if matches!(line, 1 | 4 | 7 | 11) {
                    heights.push(window.find(id.clone()).bounds().size.height);
                }
            }
        })
        .unwrap();
        // Typed lines: 0, 2 (one typed plus the trailing empty one), 5, 8, 12.
        let [empty, two, five, eight, twelve] = heights[..] else { panic!("{heights:?}") };
        assert_eq!(empty, two, "two lines fill the two-row minimum: {heights:?}");
        assert!(five > two, "grows past the minimum: {heights:?}");
        let row = (five - two) / 3.0;
        assert_eq!(eight, two + row * 6.0, "eight rows is the cap: {heights:?}");
        assert_eq!(twelve, eight, "past eight rows it scrolls instead of growing: {heights:?}");
    }

    #[gpui_kit::test]
    fn a_pasted_image_is_attached_and_not_inserted(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.write_to_clipboard(ClipboardItem::new_image(&png(7, 3)));
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.input("see ", cx);
            window.press("secondary-v", cx);
            window.render_frame(cx);
            assert_eq!(window.find("attached").label(), Some("image 7x3 attached"));
            window.press("secondary-enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(
                compose.read(cx).submitted(),
                [Submitted { text: "see ".into(), image: Some((7, 3)) }]
            );
            assert_eq!(compose.read(cx).attached(), None, "a send clears the attachment");
        });
        assert_eq!(draft_text(&compose, cx), "");
    }

    #[gpui_kit::test]
    fn pasted_text_still_lands_in_the_draft(cx: &mut TestAppContext) {
        let (handle, compose) = open(cx);
        let id = draft_id(&compose, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("one\ntwo".into()));
        cx.update_window(handle, |_, window, cx| {
            window.click(id.clone(), cx);
            window.press("secondary-v", cx);
        })
        .unwrap();
        assert_eq!(draft_text(&compose, cx), "one\ntwo");
        cx.update(|cx| assert_eq!(compose.read(cx).attached(), None));
    }

    #[test]
    fn reads_sizes_from_png_bmp_gif_and_jpeg_headers() {
        assert_eq!(image_size(&png(5, 9)), Some((5, 9)));

        let mut bmp = b"BM".to_vec();
        bmp.resize(14, 0);
        bmp.extend(40u32.to_le_bytes());
        bmp.extend(30i32.to_le_bytes());
        bmp.extend((-20i32).to_le_bytes());
        assert_eq!(image_size(&Image::from_bytes(ImageFormat::Bmp, bmp)), Some((30, 20)));

        let mut gif = b"GIF89a".to_vec();
        gif.extend(11u16.to_le_bytes());
        gif.extend(4u16.to_le_bytes());
        assert_eq!(image_size(&Image::from_bytes(ImageFormat::Gif, gif)), Some((11, 4)));

        // SOI, an APP0 segment of length 4, then SOF0: precision, height, width.
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 6, 0, 10, 3];
        assert_eq!(image_size(&Image::from_bytes(ImageFormat::Jpeg, jpeg.to_vec())), Some((10, 6)));

        assert_eq!(image_size(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])), None);
    }
}
