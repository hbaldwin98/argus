//! A log block: a tail of lines, each with its level. A log is a tail, so one the person has not
//! scrolled up in follows the newest line, and one they have keeps its place across a redraw (core's
//! `blocks::log` says which is which; where it was left is `views/memory`'s).

use argus_desktop_core::views::blocks::log::at_end;
use argus_desktop_core::views::values::ago;
use argus_protocol::{LogLevel, LogLine};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::pen::Pen;
use crate::kit::wheel::Wheel;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::ctx::ViewCtx;
use crate::views::memory::LogPlace;

fn word(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

/// `.vlog-level`: debug is faint, warn and error take their colours, the rest are muted.
fn level_colour(level: LogLevel, tokens: &Tokens) -> gpui_kit::Hsla {
    match level {
        LogLevel::Error => tokens.danger,
        LogLevel::Warn => tokens.attention,
        LogLevel::Debug => tokens.fg_faint,
        LogLevel::Info => tokens.fg_muted,
    }
}

pub fn log(lines: &[LogLine], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let handle = pen.log_scroll(&ctx.path);
    // What the last frame laid out says where the person is; a log not laid out yet has no box.
    let shown = f32::from(handle.bounds().size.height);
    if shown > 0.0 {
        let top = -f32::from(handle.offset().y);
        let tail = at_end(top, f32::from(handle.max_offset().y) + shown, shown);
        pen.memory.borrow_mut().set_log(&ctx.path, LogPlace { top, tail });
        if tail {
            handle.scroll_to_bottom();
        }
    }
    let wheel = Wheel::new(&handle);
    v_flex()
        .id(SharedString::from(format!("{}-log", ctx.path)))
        .role(Role::Log)
        .test_support()
        .max_h(rem(tokens.log_max_h))
        .overflow_y_scroll()
        .track_scroll(&handle)
        .on_scroll_wheel(wheel.keep())
        .child(wheel.probe())
        .bg(tokens.bg_sunken)
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_sm))
        .font_family(tokens.font_mono)
        .text_size(rem(tokens.fs_sm))
        // The padding is on what scrolls, not on the box: GPUI counts a scroll box's own padding as
        // room to scroll, and a log with nowhere to go would then keep the wheel.
        .child(v_flex().px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).children(lines.iter().map(|line| {
            h_flex().items_start().gap(rem(tokens.sp_8)).children(line.at.as_ref().map(|at| div().flex_shrink_0().text_color(tokens.fg_faint).child(ago(pen.draw.clock, at)))).child(
                div().flex_shrink_0().text_color(level_colour(line.level, tokens)).child(format!("{:<5}", word(line.level).to_uppercase())),
            ).child(div().min_w_0().flex_1().when(line.level == LogLevel::Debug, |text| text.text_color(tokens.fg_faint)).child(SharedString::from(line.text.clone())))
        })))
        .into_any_element()
}
