//! `:registers` and `:marks`: what the registers hold and where the marks
//! are, put in the message the editor shows. With names after it, only
//! those. A mark is listed as its name, its line and its column, both
//! counted from 0.

use super::range::Parsed;
use super::super::{Shared, Vim};

impl Vim {
    pub(super) fn ex_registers(&mut self, sh: &Shared, p: &Parsed) {
        let args = p.args();
        let names = args.concat();
        self.message = Some(sh.regs.listing((!args.is_empty()).then_some(names.as_str())));
    }

    pub(super) fn ex_marks(&mut self, p: &Parsed) {
        let t = self.buf.text();
        let args = p.args();
        let filter: Vec<char> = args.concat().chars().collect();
        let mut out = String::from("-----------Marks-----------\nmark\tline\tcol\n\n");
        for (name, at) in self.buf.marks.all() {
            if args.is_empty() || filter.contains(&name) {
                let (line, col) = t.pos(at.min(t.len()));
                out.push_str(&format!("{name}\t{line}\t{col}\n"));
            }
        }
        self.message = Some(out);
    }
}

#[cfg(test)]
mod tests;
