//! The keys the table answers to, quietly under it, in the one key bar (`kit::hints`): `Enter
//! open`, `R review` and the rest. Which are listed is `argus-desktop-core`'s (`words::key_bar`).

use argus_desktop_core::checkouts::words::key_bar;
use gpui_kit::prelude::*;

use crate::kit::hints::{self, Hint};
use crate::theme::Tokens;

pub fn bar(files_offered: bool, tokens: &Tokens) -> impl IntoElement {
    let keys = key_bar(files_offered);
    let said: Vec<Hint> = keys.iter().map(|(key, what)| (std::slice::from_ref(key), *what)).collect();
    hints::bar("checkouts-keys", &said, &[], None, tokens)
}
