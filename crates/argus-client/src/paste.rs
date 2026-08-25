//! Turns a burst of typed characters back into a paste.
//!
//! Bracketed paste only reaches us as `Event::Paste` where the terminal
//! backend supports it; on Windows crossterm reads console records and a
//! paste arrives as ordinary key events instead. Sending those on one at a
//! time submits the agent's prompt at every newline, so a burst — text keys
//! closer together than a person can type — is held back and delivered as a
//! single paste.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Longest gap between two keys still counted as one burst. Key repeat is
/// an order of magnitude slower than this, so a held key never coalesces.
pub const BURST_GAP: Duration = Duration::from_millis(10);

/// Below this a burst is replayed as keystrokes: a couple of characters
/// gain nothing from becoming a paste, and a newline is what actually
/// makes the difference.
const PASTE_MIN: usize = 3;

/// What the caller should do with a key it just handed over.
#[derive(Debug)]
pub enum Step {
    /// Not part of a burst — dispatch it now.
    Dispatch(KeyEvent),
    /// Held back as part of the open burst.
    Buffered,
    /// Ends the burst: flush first, then dispatch this key.
    FlushThen(KeyEvent),
}

/// A burst that has come due, in the form it should be delivered.
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
        let Some(_) = text_char(&key) else {
            self.last_text = None;
            return if self.buffer.is_empty() {
                Step::Dispatch(key)
            } else {
                Step::FlushThen(key)
            };
        };
        let continues = self
            .last_text
            .is_some_and(|last| now.duration_since(last) < BURST_GAP);
        self.last_text = Some(now);
        if !continues && self.buffer.is_empty() {
            // The first key of a burst still goes straight through: it costs
            // nothing (a bare character never submits) and normal typing
            // keeps its echo latency.
            return Step::Dispatch(key);
        }
        self.buffer.push(key);
        self.deadline = Some(now + BURST_GAP);
        Step::Buffered
    }

    /// When the open burst goes idle, if one is open.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Takes whatever is buffered. `accepts_paste` is the app's answer to
    /// whether a paste has anywhere to land right now; when it does not,
    /// the keys are replayed so they are never silently dropped.
    pub fn take(&mut self, accepts_paste: bool) -> Option<Flush> {
        self.deadline = None;
        if self.buffer.is_empty() {
            return None;
        }
        let keys = std::mem::take(&mut self.buffer);
        let text: String = keys.iter().filter_map(text_char).collect();
        let worth_pasting = text.contains('\n') || text.chars().count() >= PASTE_MIN;
        if accepts_paste && worth_pasting {
            Some(Flush::Paste(text))
        } else {
            Some(Flush::Keys(keys))
        }
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

    fn feed(burst: &mut PasteBurst, keys: &[KeyEvent], gap: Duration) -> Vec<KeyEvent> {
        let mut now = Instant::now();
        let mut dispatched = Vec::new();
        for key in keys {
            match burst.push(*key, now) {
                Step::Dispatch(k) | Step::FlushThen(k) => dispatched.push(k),
                Step::Buffered => {}
            }
            now += gap;
        }
        dispatched
    }

    #[test]
    fn a_pasted_block_arrives_as_one_paste_not_a_submit_per_line() {
        let mut burst = PasteBurst::default();
        let keys = [key('a'), enter(), key('b'), enter(), key('c')];

        let dispatched = feed(&mut burst, &keys, Duration::from_millis(1));

        assert_eq!(dispatched, vec![key('a')], "only the leading key goes early");
        assert_eq!(
            burst.take(true),
            Some(Flush::Paste("\nb\nc".to_string())),
            "the newlines must not reach the pane as separate keys"
        );
    }

    #[test]
    fn typing_at_human_speed_is_never_coalesced() {
        let mut burst = PasteBurst::default();
        let keys = [key('h'), key('i'), enter()];

        let dispatched = feed(&mut burst, &keys, Duration::from_millis(40));

        assert_eq!(dispatched, keys.to_vec());
        assert_eq!(burst.take(true), None);
        assert!(burst.deadline().is_none());
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
        assert_eq!(burst.take(true), Some(Flush::Keys(vec![key('b')])));
    }

    #[test]
    fn a_burst_with_nowhere_to_paste_is_replayed_as_keys() {
        let mut burst = PasteBurst::default();
        let keys = [key('a'), key('b'), key('c'), key('d')];

        feed(&mut burst, &keys, Duration::from_millis(1));

        assert_eq!(
            burst.take(false),
            Some(Flush::Keys(vec![key('b'), key('c'), key('d')]))
        );
    }
}
