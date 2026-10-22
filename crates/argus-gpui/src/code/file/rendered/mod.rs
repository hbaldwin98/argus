//! A markdown file of the Code stage read as a document: its headings, lists, code, tables, quotes
//! and links drawn by the kit's markdown view, at a reading width, in a column that scrolls.
//!
//! The text is the editor's while the stage edits the file, so what was just typed is what is
//! read, and otherwise the file the session holds, joined back from its lines; what to do with a link is
//! `link`, and an image is its alt text (`images`). A press on a link sends what the tree sends for
//! a file, so the session reads it and the stage shows what is true; this view never opens a file
//! itself. Which of this and the lines a file is shown as is `mode`, kept by the stage.

pub mod choice;
pub mod images;
pub mod link;
pub mod mode;
pub mod style;

use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodePage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::text::TextView;
use gpui_kit::{div, prelude::*, Context, Entity, ScrollHandle, SharedString, TestSupportExt as _, Window};

use super::editor::CodeEditor;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use link::Goes;

#[cfg(test)]
thread_local! {
    static LAST: std::cell::RefCell<Option<gpui_kit::WeakEntity<Rendered>>> = const { std::cell::RefCell::new(None) };
}

/// The window's document view, recorded as it is made.
#[cfg(test)]
pub(crate) fn last() -> Entity<Rendered> {
    LAST.with(|slot| slot.borrow().as_ref().and_then(gpui_kit::WeakEntity::upgrade)).expect("the window has a document view")
}

pub struct Rendered {
    heard: Entity<Heard>,
    caller: Caller,
    /// The file shown, which a relative link is read from, and its text.
    path: String,
    /// The text last drawn, as the file has it and as the document reads it (images as words).
    raw: String,
    source: SharedString,
    scroll: ScrollHandle,
    editor: Entity<CodeEditor>,
    _typing: gpui_kit::Subscription,
}

impl Rendered {
    pub fn new(heard: Entity<Heard>, caller: Caller, editor: Entity<CodeEditor>, cx: &mut Context<Self>) -> Self {
        let typing = cx.observe(&editor, |_, _, cx| cx.notify());
        #[cfg(test)]
        LAST.with(|slot| *slot.borrow_mut() = Some(cx.weak_entity()));
        Self { heard, caller, path: String::new(), raw: String::new(), source: SharedString::default(), scroll: ScrollHandle::new(), editor, _typing: typing }
    }

    /// Takes the file's text from the editor or the page. Another file starts at the top; the same one
    /// keeps where it was read to, since a page comes for every cursor move and nothing in the
    /// document changed.
    fn take(&mut self, page: &CodePage, cx: &gpui_kit::App) {
        let Some(path) = &page.path else { return };
        let text = page.editing.then(|| self.editor.read(cx).text(cx)).flatten().unwrap_or_else(|| page.lines.iter().map(|row| row.parts.iter().map(|(text, _)| text.as_str()).collect::<String>() + "
").collect());
        if &self.path != path {
            self.path = path.clone();
            self.scroll.set_offset(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(0.0)));
        }
        if self.raw != text {
            self.source = SharedString::from(images::as_alt_text(&text));
            self.raw = text;
        }
    }

    /// The text the document reads, for a test to see what the file said.
    #[cfg(test)]
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// A press on a link: the browser for a web address, the Code stage for a file of the checkout,
    /// nothing for the rest. A held window (the daemon gone) sends nothing.
    pub(crate) fn follow(&self, href: &str, cx: &gpui_kit::App) {
        if self.heard.read(cx).held() {
            return;
        }
        match link::goes(&self.path, href) {
            Goes::Browser(url) => self.caller.call(Call::OpenUrl { url }),
            Goes::File(path) => self.caller.call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Tree { path } } }),
            Goes::Nowhere => {}
        }
    }
}

impl Render for Rendered {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        if let Some(page) = self.heard.read(cx).code.clone() {
            self.take(&page, cx);
        }
        // The editor is not drawn while this stands over it, but its field still has the window's
        // focus, which would swallow every key of the stage: they rest on nothing instead.
        if self.editor.read(cx).has_keys(window, cx) {
            window.blur(cx);
        }
        let me = cx.weak_entity();
        let document = TextView::markdown(SharedString::from(format!("code-rendered-{}", self.path)), self.source.clone())
            .style(style::document(&tokens))
            .on_link_click(move |href, _, _, cx| {
                me.update(cx, |this, cx| this.follow(href, cx)).ok();
            });
        div()
            .id("code-rendered")
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .bg(tokens.bg_sunken)
            .child(
                div().id("code-rendered-scroll").size_full().overflow_y_scroll().track_scroll(&self.scroll).child(
                    // The column is centred and no wider than a line is comfortable to read.
                    div()
                        .mx_auto()
                        .w_full()
                        .max_w(rem(tokens.read_max_w))
                        .px(rem(tokens.sp_24))
                        .py(rem(tokens.sp_24))
                        .typed(tokens.type_body)
                        .text_color(tokens.fg)
                        .child(document),
                ),
            )
            .child(div().absolute().top_0().left_0().size_full().child(Scrollbar::new(&self.scroll).viewport_from_layout().mode(ScrollbarMode::Always)))
    }
}
