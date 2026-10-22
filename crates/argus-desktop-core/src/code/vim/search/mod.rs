//! The search every editor shares: the pattern last searched for, which
//! way it runs, and what `:s` last put in. `/`, `?`, `*`, `n` and the ex
//! commands that take a pattern all set or read it.

pub mod pattern;
pub mod scan;

use pattern::{parse_query, Query};

use super::registers::Registers;

#[derive(Debug, Default, Clone)]
pub struct Search {
    pub query: Option<Query>,
    /// A `?` made the search run backward, so `n` does too.
    pub reversed: bool,
    /// What `:s` last replaced with, which `:s/x` and `:&` use again.
    pub replacement: Option<String>,
    /// The matches are marked until a command that clears them.
    pub highlight: bool,
}

impl Search {
    /// Makes `raw`, as typed after `/`, the pattern searched for and files
    /// it in the `/` register. An empty pattern leaves the last one. The
    /// error is a pattern that does not compile.
    pub fn update(
        &mut self,
        regs: &mut Registers,
        raw: &str,
        ignore_case: bool,
        smart_case: bool,
        pcre: bool,
    ) -> Result<(), String> {
        if raw.is_empty() {
            return Ok(());
        }
        regs.searched(raw);
        if let Some(query) = parse_query(raw, ignore_case, smart_case, pcre)? {
            self.query = Some(query);
            self.highlight = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
