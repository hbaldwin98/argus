//! How a view hands a `Call` on without knowing who answers it.
//!
//! A view built over a `Caller` is tested by handing it one that collects
//! what it was asked, and runs in the window over the owner's own answer.

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;

#[derive(Clone)]
pub struct Caller(Rc<dyn Fn(Call)>);

impl Caller {
    pub fn new(answer: impl Fn(Call) + 'static) -> Self {
        Self(Rc::new(answer))
    }

    pub fn call(&self, call: Call) {
        (self.0)(call)
    }
}
