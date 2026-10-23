//! The Code stage's change ruler, which is the file's scrollbar: each change and each search hit
//! where it is in the whole file (`kit::ruler` draws it). It measures the lines read, or while the
//! editor stands in for them the editor's lines, which never wrap; a mark pressed sends the line it
//! stands for, which the session moves the cursor to.

use std::rc::Rc;

use argus_desktop_core::code::ruler::mark_title;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture, CodeMark};
use argus_desktop_core::connection::page::Call;
use gpui_kit::{App, AppContext as _, Entity, SharedString};

use super::editor::CodeEditor;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::diff::DiffView;
use crate::kit::ruler::{Ids, Mark, Ruler, Track};

const IDS: Ids = Ids { ruler: "code-ruler", band: "code-view", mark: "code-mark", label: "Where this file changed" };

/// What the ruler is the scrollbar of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    Lines,
    Editor,
}

impl Track for Entity<CodeEditor> {
    fn measures(&self, cx: &App) -> (f32, f32, f32) {
        self.read(cx).measures(cx).map_or((0.0, 0.0, 0.0), |m| (m.scrolled, m.viewport, m.content))
    }

    fn span(&self, first: usize, last: usize, cx: &App) -> Option<(f32, f32)> {
        self.read(cx).measures(cx).map(|m| m.span(first, last))
    }

    fn scroll_to(&self, y: f32, cx: &mut App) {
        self.update(cx, |editor, cx| editor.scroll_to_px(y, cx));
    }
}

pub struct CodeRuler {
    pub view: Entity<Ruler>,
    lines: Rc<dyn Track>,
    editor: Rc<dyn Track>,
}

impl CodeRuler {
    pub fn new(heard: Entity<Heard>, caller: Caller, diff: Entity<DiffView>, editor: Entity<CodeEditor>, cx: &mut App) -> Self {
        let (lines, editor): (Rc<dyn Track>, Rc<dyn Track>) = (Rc::new(diff), Rc::new(editor));
        let pressed = Rc::new(move |found: &Mark, cx: &mut App| {
            if !heard.read(cx).held() {
                caller.call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Go { line: found.first as u32 } } });
            }
        });
        let view = cx.new(|cx| Ruler::new(IDS, lines.clone(), pressed, cx));
        Self { view, lines, editor }
    }

    /// The marks the session sends, over what stands for the file's lines.
    pub fn show(&self, marks: &[CodeMark], over: Over, cx: &mut App) {
        let marks = marks
            .iter()
            .map(|found| {
                let (title, label) = mark_title(found.kind, found.line);
                let first = found.line as usize;
                Mark { first, last: first + found.span.max(1) as usize - 1, kind: found.kind, title: SharedString::from(title), label: SharedString::from(label) }
            })
            .collect();
        let track = match over {
            Over::Lines => self.lines.clone(),
            Over::Editor => self.editor.clone(),
        };
        self.view.update(cx, |ruler, cx| ruler.show(marks, track, cx));
    }
}
