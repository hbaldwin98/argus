//! What the page was last sent of something it draws only while open — a
//! dialog, the palette — so it hears each change once, and a closing once.
//!
//! The thing is built only while it is open; closed, the page is sent its
//! `Default`, which says so (`open` false), and then nothing more until it
//! opens again.

pub struct Sent<T> {
    last: Option<T>,
}

impl<T> Default for Sent<T> {
    fn default() -> Self {
        Sent { last: None }
    }
}

impl<T: Clone + Default + PartialEq> Sent<T> {
    /// What to send the page, if it has not heard it: `now` is the thing as
    /// it stands while open, `None` once closed.
    pub fn take(&mut self, now: Option<T>) -> Option<T> {
        if now.is_none() && self.last.as_ref().is_none_or(|last| *last == T::default()) {
            return None;
        }
        let page = now.unwrap_or_default();
        (self.last.as_ref() != Some(&page)).then(|| {
            self.last = Some(page.clone());
            page
        })
    }

    /// The page reloaded and holds nothing: whatever is open is sent again.
    pub fn forget(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, PartialEq)]
    struct Page {
        open: bool,
        rows: usize,
    }

    fn open(rows: usize) -> Option<Page> {
        Some(Page { open: true, rows })
    }

    #[test]
    fn a_thing_never_opened_is_never_sent() {
        let mut sent = Sent::<Page>::default();
        assert_eq!(sent.take(None), None);
    }

    #[test]
    fn an_unchanged_thing_is_not_sent_again() {
        let mut sent = Sent::default();
        assert_eq!(sent.take(open(1)), open(1));
        assert_eq!(sent.take(open(1)), None);
        assert_eq!(sent.take(open(2)), open(2));
    }

    #[test]
    fn a_closing_is_sent_once() {
        let mut sent = Sent::default();
        sent.take(open(1));
        assert_eq!(sent.take(None), Some(Page::default()));
        assert_eq!(sent.take(None), None);
    }

    #[test]
    fn a_reloaded_page_hears_what_is_open_again() {
        let mut sent = Sent::default();
        sent.take(open(1));
        sent.forget();
        assert_eq!(sent.take(open(1)), open(1));
        sent.forget();
        assert_eq!(sent.take(None), None, "and nothing of what is closed");
    }
}
