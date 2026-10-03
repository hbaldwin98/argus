//! Turns a burst of typed characters back into a paste.
//!
//! Bracketed paste only reaches us as `Event::Paste` where the terminal
//! backend supports it; on Windows crossterm reads console records and a
//! paste, or a file dropped on the terminal, arrives as ordinary key events
//! instead. Sending those on one at a time submits the agent's prompt at
//! every newline, and types a dropped path's letters as commands wherever
//! the focus is not a pane — so text keys are held until the burst goes
//! idle, and a burst longer than a person types is delivered as one paste.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Longest gap between two keys still counted as one burst, and how long
/// a key is held waiting for the next. Key repeat is an order of magnitude
/// slower than this, so a held key never coalesces; and it is under the
/// redraw tick, so the hold never costs a frame of echo.
pub const BURST_GAP: Duration = Duration::from_millis(10);

/// Below this a burst is replayed as keystrokes: a couple of characters
/// gain nothing from becoming a paste, and a newline is what actually
/// makes the difference.
const PASTE_MIN: usize = 3;

/// What the caller should do with a key it just handed over.
#[derive(Debug)]
pub enum Step {
    /// Not input at all — a key release, or a modifier on its own.
    /// Swallowed here so it neither reaches the app nor breaks a burst.
    Drop,
    /// Not part of a burst — dispatch it now.
    Dispatch(KeyEvent),
    /// Held back as part of the open burst.
    Buffered,
    /// Ends the burst: flush first, then dispatch this key.
    FlushThen(KeyEvent),
}

/// A burst that has come due, in the form it should be delivered.
///
/// A paste is a paste wherever it lands: one with nowhere to go is for
/// the app to say so about, never to replay as keystrokes — a dropped
/// path typed into the tree as commands opened whatever its letters were
/// bound to.
#[derive(Debug, PartialEq)]
pub enum Flush {
    Paste(String),
    Keys(Vec<KeyEvent>),
}

#[derive(Default)]
pub struct PasteBurst {
    last_text: Option<Instant>,
    buffer: Vec<KeyEvent>,
    deadline: Option<Instant>,
}

impl PasteBurst {
    pub fn push(&mut self, key: KeyEvent, now: Instant) -> Step {
        match classify(&key) {
            Class::Ignore => return Step::Drop,
            Class::Text => {}
            Class::Other => {
                self.last_text = None;
                return if self.buffer.is_empty() {
                    Step::Dispatch(key)
                } else {
                    Step::FlushThen(key)
                };
            }
        }
        // A buffer the loop has not flushed yet, though its burst is over:
        // the key arrived in the same turn as the deadline. It is not part
        // of that burst, and goes straight through rather than joining it.
        let stale = self
            .last_text
            .is_some_and(|last| now.duration_since(last) >= BURST_GAP);
        if stale && !self.buffer.is_empty() {
            self.last_text = None;
            return Step::FlushThen(key);
        }
        self.last_text = Some(now);
        self.buffer.push(key);
        self.deadline = Some(now + BURST_GAP);
        Step::Buffered
    }

    /// When the open burst goes idle, if one is open.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Takes whatever is buffered: a paste when it is more than a person
    /// types in the time, the keys themselves otherwise.
    pub fn take(&mut self) -> Option<Flush> {
        self.deadline = None;
        if self.buffer.is_empty() {
            return None;
        }
        let keys = std::mem::take(&mut self.buffer);
        let text: String = keys.iter().filter_map(text_char).collect();
        // A lone Enter is a keystroke, now that every key is held: as a
        // paste it would be a newline in the prompt, not a submit.
        let chars = text.chars().count();
        let worth_pasting = chars >= PASTE_MIN || (chars >= 2 && text.contains('\n'));
        if worth_pasting {
            Some(Flush::Paste(text))
        } else {
            Some(Flush::Keys(keys))
        }
    }
}

/// What a key event is worth to a burst.
enum Class {
    /// Contributes a character.
    Text,
    /// Neither text nor a real keypress: releases, and modifiers pressed on
    /// their own. Windows reports both, and counting them was doubling
    /// every pasted character and cutting bursts at every capital letter.
    Ignore,
    /// A real key that is not text — it ends the burst.
    Other,
}

fn classify(key: &KeyEvent) -> Class {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Class::Ignore;
    }
    if matches!(key.code, KeyCode::Modifier(_)) {
        return Class::Ignore;
    }
    if text_char(key).is_some() {
        Class::Text
    } else {
        Class::Other
    }
}

/// The character a key contributes to pasted text, if it is text at all.
fn text_char(key: &KeyEvent) -> Option<char> {
    let plain = (key.modifiers - KeyModifiers::SHIFT).is_empty();
    if !plain {
        return None;
    }
    match key.code {
        KeyCode::Char(c) => Some(c),
        KeyCode::Enter => Some('\n'),
        KeyCode::Tab => Some('\t'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    fn released(key: KeyEvent) -> KeyEvent {
        KeyEvent {
            kind: KeyEventKind::Release,
            ..key
        }
    }

    /// Feeds `keys` `gap` apart, flushing at each deadline the way the
    /// loop does, and returns what was dispatched as keys along the way.
    /// A paste coming due mid-feed is a test's mistake.
    fn feed(burst: &mut PasteBurst, keys: &[KeyEvent], gap: Duration) -> Vec<KeyEvent> {
        let mut now = Instant::now();
        let mut dispatched = Vec::new();
        for key in keys {
            if burst.deadline().is_some_and(|due| now >= due) {
                match burst.take() {
                    Some(Flush::Keys(keys)) => dispatched.extend(keys),
                    Some(Flush::Paste(text)) => panic!("a paste of {text:?} came due mid-feed"),
                    None => {}
                }
            }
            match burst.push(*key, now) {
                Step::Dispatch(k) | Step::FlushThen(k) => dispatched.push(k),
                Step::Buffered | Step::Drop => {}
            }
            now += gap;
        }
        dispatched
    }

    #[test]
    fn a_key_release_is_not_a_second_character() {
        // Windows reports press and release both. Counting the release
        // doubled every pasted character on its way into the pane.
        let mut burst = PasteBurst::default();
        let keys = [key('h'), released(key('h')), key('i'), released(key('i'))];

        let dispatched = feed(&mut burst, &keys, Duration::from_millis(1));

        assert!(dispatched.is_empty());
        assert_eq!(burst.take(), Some(Flush::Keys(vec![key('h'), key('i')])));
    }

    #[test]
    fn a_modifier_on_its_own_does_not_cut_a_burst_in_half() {
        // A capital letter in pasted text arrives with its own Shift
        // events. Treating those as "not text" ended the burst at every
        // capital, and the short pieces went in as keystrokes — which is
        // one submitted message per line all over again.
        let mut burst = PasteBurst::default();
        let shift = KeyEvent::new(
            KeyCode::Modifier(crossterm::event::ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT,
        );
        let keys = [key('a'), shift, key('B'), enter(), key('c')];

        feed(&mut burst, &keys, Duration::from_millis(1));

        assert_eq!(
            burst.take(),
            Some(Flush::Paste(
                "aB
c"
                .to_string()
            ))
        );
    }

    #[test]
    fn a_pasted_block_arrives_whole_as_one_paste() {
        // Whole: the leading key used to go early, which typed the opening
        // quote of a dropped Windows path into the pane and pasted the rest
        // without it, so nothing recognized the path as a file.
        let mut burst = PasteBurst::default();
        let keys = [key('"'), key('a'), enter(), key('b'), enter(), key('c')];

        let dispatched = feed(&mut burst, &keys, Duration::from_millis(1));

        assert!(dispatched.is_empty(), "nothing goes early: {dispatched:?}");
        assert_eq!(
            burst.take(),
            Some(Flush::Paste("\"a\nb\nc".to_string())),
            "the newlines must not reach the pane as separate keys"
        );
    }

    #[test]
    fn typing_at_human_speed_is_never_coalesced() {
        let mut burst = PasteBurst::default();
        let keys = [key('h'), key('i'), enter()];

        let dispatched = feed(&mut burst, &keys, Duration::from_millis(40));

        assert_eq!(dispatched, keys[..2].to_vec(), "each key is let go before the next");
        assert_eq!(
            burst.take(),
            Some(Flush::Keys(vec![enter()])),
            "a lone Enter submits; as a paste it would only be a newline"
        );
        assert!(burst.deadline().is_none());
    }

    #[test]
    fn a_key_arriving_as_its_predecessor_comes_due_does_not_join_it() {
        // The deadline and the key in the same turn of the loop: the key
        // goes through on its own rather than being held for a burst that
        // has already ended.
        let mut burst = PasteBurst::default();
        let now = Instant::now();
        burst.push(key('a'), now);

        let step = burst.push(key('b'), now + BURST_GAP);

        assert!(matches!(step, Step::FlushThen(k) if k == key('b')));
        assert_eq!(burst.take(), Some(Flush::Keys(vec![key('a')])));
    }

    #[test]
    fn a_non_text_key_ends_the_burst_and_follows_it() {
        let mut burst = PasteBurst::default();
        let now = Instant::now();
        burst.push(key('a'), now);
        burst.push(key('b'), now + Duration::from_millis(1));
        let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        let step = burst.push(escape, now + Duration::from_millis(2));

        assert!(matches!(step, Step::FlushThen(k) if k == escape));
        assert_eq!(burst.take(), Some(Flush::Keys(vec![key('a'), key('b')])));
    }

    #[test]
    fn a_burst_is_a_paste_whether_or_not_anything_takes_one() {
        // Where the burst lands is the app's question. Replaying one with
        // nowhere to go as keystrokes typed a dropped path into the tree,
        // and its letters opened whatever they were bound to.
        let mut burst = PasteBurst::default();
        let keys = [key('a'), key('b'), key('c'), key('d')];

        feed(&mut burst, &keys, Duration::from_millis(1));

        assert_eq!(burst.take(), Some(Flush::Paste("abcd".to_string())));
    }
}
