//! The page's families load into the real text system and the theme names them.
//!
//! GPUI's test platform has a no-op text system, so this runs the real
//! platform's, as `examples/screenshot.rs` does; no window is opened. It is
//! the only test here, because one process can start the application once.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::settings::flavours::FLAVOURS;
use argus_gpui::kit::fonts::SYMBOLS;
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
    // The app starts in the page's default theme.
    let tokens = *argus_gpui::theme::Tokens::global(cx);
    let (ui, mono) = (tokens.font_ui, tokens.font_mono);
    // Every theme's own faces ship with it (decision #68): none may fall to the substitute.
    let mut wanted = vec![ui, mono, SYMBOLS];
    for (chosen, _) in FLAVOURS {
        let own = argus_gpui::theme::derive::tokens(argus_gpui::theme::choice::theme(chosen, true));
        wanted.extend([own.font_ui, own.font_mono]);
    }
    for family in wanted {
        if !names.iter().any(|name| name == family) {
            return Err(format!("{family} is not registered"));
        }
        if text.resolve_font(&font(family)) == substitute {
            return Err(format!("{family} resolves to the substitute"));
        }
    }
    // The families the machine is offered leave the window's own out.
    for own in argus_gpui::kit::fonts::BUNDLED {
        if !names.iter().any(|name| name == own) {
            return Err(format!("the bundled {own} is not in the text system"));
        }
    }
    let theme = cx.theme();
    if theme.font_family != ui || theme.mono_font_family != mono {
        return Err(format!("the theme names {} and {}", theme.font_family, theme.mono_font_family));
    }
    if theme.font_size != px(tokens.fs_md) || theme.radius != px(tokens.r_sm) || theme.radius_lg != px(tokens.r_lg) {
        return Err("the theme's size and radii are not the page's".to_owned());
    }
    Ok(())
}
