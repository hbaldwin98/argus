//! Which colour a highlight kind and a side get: the tone of a token, the wash of an added or
//! removed side, and what the pick and the cursor lay over it.

use gpui_kit::Hsla;

use crate::theme::Tokens;

/// How a highlight kind is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub colour: Hsla,
    pub italic: bool,
}

/// The colour of a highlight kind, from the Ledger's roles (the page's
/// `--mauve`, `--green`, `--peach`, `--yellow`, `--blue` and `--teal`).
pub fn tone(kind: &str, tokens: &Tokens) -> Option<Tone> {
    let (colour, italic) = match kind {
        "k" => (tokens.accent, false),
        "s" => (tokens.ok, false),
        "c" => (tokens.fg_faint, true),
        "n" => (tokens.changed, false),
        "t" => (tokens.attention, false),
        "f" => (tokens.ref_, false),
        "p" => (tokens.info, false),
        "o" => (tokens.fg_muted, false),
        _ => return None,
    };
    Some(Tone { colour, italic })
}

/// The wash behind a side by what it is: added green, removed red, a
/// context line none.
pub fn wash(kind: &str, tokens: &Tokens) -> Option<Hsla> {
    match kind {
        "added" => Some(tokens.ok.opacity(tokens.share_tone)),
        "removed" => Some(tokens.danger.opacity(tokens.share_tone)),
        _ => None,
    }
}

/// What a side is filled with: its wash, with the pick laid over it rather
/// than in its place, so a picked line's side stays readable, or the cursor's
/// lift to `--fill`, which the page lays over the wash whole. Composed over the window's background by hand: `Hsla::blend`
/// keeps its receiver's alpha, so a see-through wash blended with a
/// see-through pick would paint nothing.
pub fn fill(kind: &str, picked: bool, cursor: bool, tokens: &Tokens) -> Option<Hsla> {
    let wash = wash(kind, tokens);
    let over = if picked {
        Some(tokens.accent.opacity(tokens.share_pick))
    } else if cursor {
        Some(tokens.fill)
    } else {
        None
    };
    if wash.is_none() && over.is_none() {
        return None;
    }
    let mut fill = tokens.bg_sunken;
    for layer in wash.into_iter().chain(over) {
        fill = fill.blend(layer);
    }
    Some(fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_is_laid_over_a_sides_wash_and_each_side_stays_distinct() {
        let tokens = crate::theme::ledger::tokens();
        assert_eq!(fill("context", false, false, &tokens), None);
        let added = fill("added", false, false, &tokens).unwrap();
        let removed = fill("removed", false, false, &tokens).unwrap();
        let picked_added = fill("added", true, false, &tokens).unwrap();
        let picked_removed = fill("removed", true, false, &tokens).unwrap();
        assert_ne!(picked_added, added, "picking brightens");
        assert_ne!(picked_added, picked_removed, "a picked line still says which side it is");
        assert_ne!(added, removed);
        assert_eq!(fill("context", true, false, &tokens).unwrap().a, 1.0, "opaque, so it paints");
        assert_ne!(fill("context", false, true, &tokens), None, "the cursor lifts a context line");
        assert_eq!(fill("added", true, true, &tokens), fill("added", true, false, &tokens), "a picked line is drawn picked");
    }

    #[test]
    fn each_highlight_kind_has_a_tone_and_an_unknown_one_none() {
        let tokens = crate::theme::ledger::tokens();
        for kind in ["k", "s", "c", "n", "t", "f", "p", "o"] {
            assert!(tone(kind, &tokens).is_some(), "{kind}");
        }
        assert!(tone("zz", &tokens).is_none());
        assert!(tone("c", &tokens).unwrap().italic, "a comment is italic");
        assert!(wash("added", &tokens).is_some() && wash("removed", &tokens).is_some() && wash("context", &tokens).is_none());
    }
}
