//! The dictation panel: a small float at the foot of the pane being spoken into, with the
//! microphone's level, the state, where the text goes, the words so far and the keys that answer it
//! (the Argus Dictation design's `.pill`).
//!
//! What it says is core's (`dictation::panel`); this draws it. It never takes the keys (they stay
//! where the person was typing) and dims nothing; Escape, Enter and C reach it through
//! `dictation::keys`. A panel with nothing more to say goes after the time core gives it.

use std::time::{Duration, Instant};

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::dictation::panel::{Meter, PanelPage, Tone};
use argus_desktop_core::dictation::DictationGesture;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, Entity, HighlightStyle, Hsla, Role, SharedString, StyledText, Subscription, Task, TestSupportExt as _, Window};

use super::level::Level;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::bars::{bars, Bars};
use crate::kit::keycap::keycap;
use crate::kit::overlay::float::float;
use crate::kit::spots::Spot;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct PanelView {
    heard: Entity<Heard>,
    caller: Caller,
    /// The run and state whose lingering is timed, so a page heard again does not start it over.
    timed: Option<(u64, String)>,
    _linger: Option<Task<()>>,
    /// When the meter started moving, for its stir.
    started: Instant,
    _observing: Subscription,
}

/// The colour a state is said in.
pub fn tone(tone: Tone, tokens: &Tokens) -> Hsla {
    match tone {
        Tone::Plain => tokens.fg,
        Tone::Ok => tokens.ok,
        Tone::Warn => tokens.attention,
        Tone::Bad => tokens.danger,
    }
}

impl PanelView {
    pub fn new(heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&heard, |this, _, cx| this.follow(cx));
        Self { heard, caller, timed: None, _linger: None, started: Instant::now(), _observing: observing }
    }

    fn page(&self, cx: &gpui_kit::App) -> Option<PanelPage> {
        self.heard.read(cx).dictation.as_ref().and_then(|page| page.panel.clone())
    }

    /// Times a panel with nothing more to say, once for each run and state.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let page = self.page(cx);
        let now = page.as_ref().map(|page| (page.run, page.state.clone()));
        if now != self.timed {
            self.timed = now;
            self._linger = page.and_then(|page| {
                let ms = page.lingers_ms?;
                let (caller, run) = (self.caller.clone(), page.run);
                Some(cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(Duration::from_millis(ms)).await;
                    caller.call(Call::Dictation { gesture: DictationGesture::Dismiss { run } });
                }))
            });
        }
        cx.notify();
    }
}

/// The panel's body: the state's line, the text, and the keys.
pub fn body(page: &PanelPage, level: f32, at: f32, tokens: &Tokens) -> impl IntoElement {
    let meter = page.meter.map(|meter| match meter {
        Meter::Live => bars("dictation-meter", Bars::Live { level, at }, tokens.accent, tokens),
        Meter::Loading => bars("dictation-meter", Bars::Waiting, tokens.accent, tokens),
        Meter::Flat => bars("dictation-meter", Bars::Flat, tokens.fg_faint, tokens),
    });
    let target = page.target.as_ref().map(|(word, name)| {
        h_flex()
            .id("dictation-target")
            .test_support()
            .aria_label(SharedString::from(format!("{word} {name}")))
            .ml_auto()
            .min_w_0()
            .gap(rem(tokens.sp_4))
            .text_color(tokens.fg_muted)
            .child(SharedString::from(word.clone()))
            .child(div().min_w_0().truncate().text_color(tokens.fg).weighted(tokens.weight_medium).child(SharedString::from(name.clone())))
    });
    let top = h_flex()
        .gap(rem(tokens.sp_8))
        .items_center()
        .typed(tokens.type_small)
        .children(meter)
        .child(div().id("dictation-state").test_support().aria_label(SharedString::from(page.state.clone())).flex_none().text_color(tone(page.tone, tokens)).weighted(tokens.weight_medium).child(SharedString::from(page.state.clone())))
        .children(target);
    let words = if let Some(note) = &page.note {
        div().text_color(tokens.fg_muted).child(SharedString::from(note.clone())).into_any_element()
    } else if let Some(placeholder) = &page.placeholder {
        div().text_color(tokens.fg_faint).italic().child(SharedString::from(placeholder.clone())).into_any_element()
    } else {
        // One run of text, so the guess flows on from the settled words and wraps with them.
        let said = format!("{}{}", page.committed, page.tentative);
        let guess = HighlightStyle { color: Some(tokens.fg_fainter), ..HighlightStyle::default() };
        let text = StyledText::new(SharedString::from(said)).with_highlights(vec![(page.committed.len()..page.committed.len() + page.tentative.len(), guess)]);
        div().id("dictation-words").test_support().aria_label(SharedString::from(page.committed.clone())).text_color(tokens.fg).child(text).into_any_element()
    };
    let mono = SharedString::from(tokens.font_mono);
    let keys = (!page.keys.is_empty()).then(|| {
        h_flex().flex_wrap().gap(rem(tokens.sp_12)).typed(tokens.type_small).text_color(tokens.fg_faint).children(page.keys.iter().map(|(key, does)| {
            h_flex().gap(rem(tokens.sp_4)).items_center().child(keycap(SharedString::from(key.clone()), mono.clone(), tokens)).child(SharedString::from(does.clone()))
        }))
    });
    v_flex()
        .id("dictation-panel")
        .role(Role::Status)
        .test_support()
        .aria_label(SharedString::from(format!("Dictation: {}", page.state)))
        .w_full()
        .gap(rem(tokens.sp_6))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .rounded(rem(tokens.r_lg))
        .border_1()
        .border_color(tokens.line_strong)
        .bg(tokens.bg_raised)
        .shadow(tokens.shadow_pop())
        // `.caption`: the words wrap, and the line keeps its height before there are any.
        .child(top)
        .child(div().w_full().min_h(tokens.type_body.line_height()).typed(tokens.type_body).child(words))
        .children(keys)
}

impl Render for PanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(page) = self.page(cx) else {
            return div().into_any_element();
        };
        let tokens = *Tokens::global(cx);
        // The meter moves with the voice, so a live panel is drawn every frame.
        if page.meter == Some(Meter::Live) {
            window.request_animation_frame();
        }
        let (level, at) = (Level::peek(cx), self.started.elapsed().as_secs_f32());
        let keyed = self.heard.read(cx).keyed_cell().unwrap_or(0);
        float(vec![Spot::Cell(keyed), Spot::Stage], tokens.sp_12, tokens.dictate_lift, tokens.dictate_max_w, body(&page, level, at, &tokens)).into_any_element()
    }
}
