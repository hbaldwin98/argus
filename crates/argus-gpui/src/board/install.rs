//! Puts the board's surfaces where they are drawn: the note and an open feature are what the stage is
//! while the Feature stage has one of them up, and the strip carries an open feature's title, its trail
//! and its lenses and menu.
//!
//! Called once by the shell. The stage is the note while one is up, else an open feature, else the
//! index; the strip carries an open feature's words and tools, else the index's, which the page also
//! shows over a note.

use argus_desktop_core::chrome::strip::{Part, Says};
use argus_desktop_core::connection::page::Stage;
use gpui_kit::{div, prelude::*, AnyElement, App, Context, Entity, Window};

use super::capture::CaptureNote;
use super::feature_stage::{self, strip, FeatureStage};
use super::feature_view::OpenFeature;
use super::lenses::head::tools::tools;
use crate::chrome::header::slots::{Offer, Slots};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::layout::bodies::Contents;

/// What the Feature stage draws: the note, an open feature or the index.
pub struct Surface {
    heard: Entity<Heard>,
    note: Entity<CaptureNote>,
    open: Entity<OpenFeature>,
    index: Entity<FeatureStage>,
}

impl Surface {
    /// Whether the session holds a note or a feature open: the index is drawn only when it does not.
    pub fn up(heard: &Heard) -> bool {
        heard.capture.is_some() || heard.features.as_ref().is_some_and(|features| features.open.is_some())
    }
}

impl Render for Surface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let heard = self.heard.read(cx);
        if heard.capture.is_some() {
            return div().size_full().child(self.note.clone()).into_any_element();
        }
        if heard.features.as_ref().is_some_and(|features| features.open.is_some()) {
            return div().size_full().child(self.open.clone()).into_any_element();
        }
        div().size_full().child(self.index.clone()).into_any_element()
    }
}

/// What the strip says of an open feature: its title in the name's place and `Features / #12` after the
/// repository (BOARD-HEAD-9). Nothing while none is open.
pub fn strip_says(heard: &Heard) -> Option<Says> {
    let view = heard.features.as_ref()?.open.as_ref()?;
    Some(Says { title: Some(view.title.clone()), trail: vec![Part::Text("Features".into()), Part::Mono(format!("#{}", view.number))], ..Says::default() })
}

/// What the strip carries for an open feature: the lenses and the menu. Nothing while none is open.
pub fn strip_tools(open: &Entity<OpenFeature>, cx: &App) -> AnyElement {
    tools(open, cx)
}

pub fn install(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let note = cx.new(|cx| CaptureNote::new(heard.clone(), caller.clone(), window, cx));
    let open = cx.new(|cx| OpenFeature::new(heard.clone(), caller.clone(), window, cx));
    let index = feature_stage::build(heard, caller, window, cx);
    let index_tools = index.read(cx).strip_tools();
    let surface = cx.new(|cx| {
        cx.observe(heard, |_, _, cx| cx.notify()).detach();
        Surface { heard: heard.clone(), note, open: open.clone(), index }
    });
    Contents::of(cx).offer_stage(Stage::Feature, surface.into());
    let (saying, offered, showing) = (heard.clone(), open.clone(), heard.clone());
    Slots::of(cx).offer(
        Stage::Feature,
        Offer::default()
            .says(move |cx| {
                let heard = saying.read(cx);
                strip_says(heard).unwrap_or_else(|| strip::says(heard.features.as_deref()))
            })
            .tools(move |_, cx| {
                if strip_says(showing.read(cx)).is_some() {
                    strip_tools(&offered, cx)
                } else {
                    index_tools.clone().into_any_element()
                }
            }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::snapshots;
    use argus_desktop_core::connection::page::Event;

    // BOARD-HEAD-9: an open feature's title is the strip's name and `Features / #12` is its trail.
    #[test]
    fn the_strip_names_an_open_feature_and_trails_its_number() {
        let mut heard = Heard::new();
        assert_eq!(strip_says(&heard), None, "nothing before the Feature stage is heard");
        let page = snapshots::features("feature-work");
        let view = page.open.clone().expect("a feature is open");
        heard.hear(Event::Features(Box::new(page)));
        let says = strip_says(&heard).expect("an open feature says itself");
        assert_eq!(says.title.as_deref(), Some(view.title.as_str()));
        assert_eq!(says.trail, [Part::Text("Features".into()), Part::Mono(format!("#{}", view.number))]);
        let mut index = snapshots::features("feature-work");
        index.open = None;
        heard.hear(Event::Features(Box::new(index)));
        assert_eq!(strip_says(&heard), None, "the index says its own words");
    }
}
