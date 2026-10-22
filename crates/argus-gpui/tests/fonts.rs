//! The page's families load into the real text system and the theme names them.
//!
//! GPUI's test platform has a no-op text system, so this runs the real
//! platform's, as `examples/screenshot.rs` does; no window is opened. It is
//! the only test here, because one process can start the application once.

use std::cell::RefCell;
use std::rc::Rc;

use argus_gpui::fonts::SYMBOLS;
use argus_gpui::theme::ledger;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{font, px};

#[test]
fn the_pages_families_load_and_the_theme_names_them() {
    let outcome: Rc<RefCell<Option<Result<(), String>>>> = Rc::new(RefCell::new(None));
    let sink = outcome.clone();
    argus_gpui::window::application().run(move |cx| {
        argus_gpui::window::init(cx);
        *sink.borrow_mut() = Some(check(cx));
        cx.quit();
    });
    let outcome = outcome.borrow_mut().take();
    outcome.expect("the application quit before the check ran").unwrap();
}

fn check(cx: &gpui_kit::App) -> Result<(), String> {
    let text = cx.text_system();
    let names = text.all_font_names();
    // An unknown family resolves to a substitute, so a face counts as found
    // only if it is listed and does not resolve to that substitute.
    let substitute = text.resolve_font(&font("No Such Family 7f3a"));
    let tokens = ledger::tokens();
    let (ui, mono) = (tokens.font_ui, tokens.font_mono);
    for family in [ui, mono, SYMBOLS] {
        if !names.iter().any(|name| name == family) {
            return Err(format!("{family} is not registered"));
        }
        if text.resolve_font(&font(family)) == substitute {
            return Err(format!("{family} resolves to the substitute"));
        }
    }
    let theme = cx.theme();
    if theme.font_family != ui || theme.mono_font_family != mono {
        return Err(format!("the theme names {} and {}", theme.font_family, theme.mono_font_family));
    }
    if theme.font_size != px(13.0) || theme.radius != px(6.0) || theme.radius_lg != px(12.0) {
        return Err("the theme's size and radii are not the page's".to_owned());
    }
    Ok(())
}
