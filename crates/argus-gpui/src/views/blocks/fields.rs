//! A label column beside a value column: the shape of a ticket's details.

use argus_protocol::Field;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::ctx::Draw;
use crate::views::values::value;

/// `.vfields`: each label in the small muted type in a column `--fields-label-w` wide, its value
/// beside it, a row centred on the taller of the two. `key` names the block for the ids of what is
/// pressable in its values; `draw.view` is the view it is in.
pub fn fields(list: &[Field], key: &str, draw: &Draw) -> AnyElement {
    let tokens = draw.tokens;
    v_flex()
        .gap(rem(tokens.sp_8))
        .children(list.iter().enumerate().map(|(k, field)| {
            h_flex()
                .gap(rem(tokens.sp_12))
                .child(div().w(rem(tokens.fields_label_w)).flex_shrink_0().text_color(tokens.fg_muted).typed(tokens.type_small).child(SharedString::from(field.label.clone())))
                .child(div().min_w_0().flex_1().flex().items_center().child(value(&field.value, &format!("{key}.{k}"), draw)))
        }))
        .into_any_element()
}
