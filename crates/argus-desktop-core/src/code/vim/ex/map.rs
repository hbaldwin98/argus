//! The mapping commands: `:map` and its `n`, `i`, `v`, `o` forms make keys
//! stand for other keys in a mode (with no mode, in normal, visual and
//! operator-pending mode), the `noremap` forms do not map what they type,
//! `:unmap` removes a mapping with no mode, and `:mapclear` removes
//! them, all or one mode's. A command name given with a `:` maps to
//! another command or to keys.

use crate::code::vim::maps::Mode;

use super::range::Parsed;
use super::super::{Shared, Vim};

impl Vim {
    /// `name` is `map`, `nmap`, `inoremap` and the rest.
    pub(super) fn ex_map(&mut self, sh: &mut Shared, p: &Parsed, name: &str, input: &str) {
        let args = p.args();
        let [lhs, rhs, ..] = args[..] else {
            self.message = Some(format!("Invalid mapping: {input}"));
            return;
        };
        let context = match name.chars().next() {
            Some('i') => Some(Mode::Insert),
            Some('n') if name != "noremap" => Some(Mode::Normal),
            Some('v') => Some(Mode::Visual),
            Some('o') if name.starts_with("om") || name == "onoremap" => Some(Mode::OperatorPending),
            _ => None,
        };
        let noremap = name.ends_with("noremap");
        if let Err(e) = sh.maps.add(lhs, rhs, context, noremap) {
            self.message = Some(e);
        }
    }

    pub(super) fn ex_unmap(&mut self, sh: &mut Shared, p: &Parsed, input: &str) {
        let args = p.args();
        let removed = match args.first() {
            Some(lhs) => sh.maps.remove(lhs, None),
            None => Ok(false),
        };
        match removed {
            Ok(true) => {}
            Ok(false) => self.message = Some(format!("No such mapping: {input}")),
            Err(e) => self.message = Some(e),
        }
    }

    /// `:mapclear` and `:nmapclear`, `:imapclear`, `:vmapclear`, `:omapclear`.
    pub(super) fn ex_mapclear(&mut self, sh: &mut Shared, name: &str) {
        let mode = match name.chars().next() {
            Some('n') => Some(Mode::Normal),
            Some('i') => Some(Mode::Insert),
            Some('v') => Some(Mode::Visual),
            Some('o') => Some(Mode::OperatorPending),
            _ => None,
        };
        sh.maps.clear(mode);
    }
}

#[cfg(test)]
mod tests;
