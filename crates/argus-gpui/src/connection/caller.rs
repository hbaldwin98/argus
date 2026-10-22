//! How a view hands a `Call` on without knowing who answers it.
//!
//! A view built over a `Caller` is tested by handing it one that collects
//! what it was asked, and runs in the window over the owner's own answer.

use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::Call;

#[derive(Clone)]
pub struct Caller {
    answer: Rc<dyn Fn(Call)>,
    /// When the session holds a browser size back until; none for a caller with no session behind it.
    next_viewport: Rc<dyn Fn() -> Option<Duration>>,
}

impl Caller {
    pub fn new(answer: impl Fn(Call) + 'static) -> Self {
        Self { answer: Rc::new(answer), next_viewport: Rc::new(|| None) }
    }

    /// Says where to ask when a held browser size may go (`Browsers::next_viewport`).
    pub fn asking_viewport(mut self, next: impl Fn() -> Option<Duration> + 'static) -> Self {
        self.next_viewport = Rc::new(next);
        self
    }

    pub fn call(&self, call: Call) {
        (self.answer)(call)
    }

    /// When a browser size held back may go, on the clock of `Call::BrowserViewport`'s `at`.
    pub fn next_viewport(&self) -> Option<Duration> {
        (self.next_viewport)()
    }
}
