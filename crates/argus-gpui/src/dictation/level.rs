//! How loud the microphone is now, shared between the engine that writes it, on the audio thread, and
//! the panel's meter that reads it each frame: twenty times a second is too often to go through the
//! session, and nothing else needs it.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use gpui_kit::{App, Global};

#[derive(Clone, Default)]
pub struct Level(Arc<AtomicU32>);

impl Global for Level {}

impl Level {
    /// The window's level, made the first time it is asked for.
    pub fn of(cx: &mut App) -> Level {
        cx.default_global::<Level>().clone()
    }

    /// The window's level from where it cannot be made: silence when there is none.
    pub fn peek(cx: &App) -> f32 {
        cx.try_global::<Level>().map_or(0.0, Level::get)
    }

    /// The root mean square of the last chunk the microphone gave.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, rms: f32) {
        self.0.store(rms.to_bits(), Ordering::Relaxed);
    }

    /// What the engine writes into.
    pub fn cell(&self) -> Arc<AtomicU32> {
        self.0.clone()
    }
}
