//! Macros: `q` records the keys typed into a register as text, and `@`
//! plays a register back as keys. The library records only the keys of
//! normal mode and replays an insert from what it typed; here every key
//! typed is recorded, so a register holds what was done and replays it.
//! Recording and playing are shared by every editor, as the registers are.

/// The most keys one typed key may run through macros, so a macro that
/// plays itself ends.
pub const MOST_KEYS: usize = 10_000;

/// The deepest `@` inside `@`.
pub const MOST_DEPTH: usize = 100;

#[derive(Debug, Default, Clone)]
pub struct Macros {
    /// The register being recorded into, and the text so far.
    pub recording: Option<(char, String)>,
    /// The register `@@` plays.
    pub latest: Option<char>,
    /// How many `@` are running inside one another.
    pub depth: usize,
    /// How many more keys macros may run for the key being typed.
    pub left: usize,
}

impl Macros {
    pub fn playing(&self) -> bool {
        self.depth > 0
    }
}

#[cfg(test)]
mod tests;
