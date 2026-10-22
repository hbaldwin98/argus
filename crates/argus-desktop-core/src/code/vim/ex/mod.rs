//! The `:` line, run: read the range and the command's name off the front,
//! find the command, and hand it the rest. The commands are the library's
//! table (EDITOR-VIM-13) with Argus's `:w`; a name that is none of them is
//! the library's own error. The line is kept in the `:` register for `@:`
//! unless the command only shows something.

pub mod delmarks;
pub mod editor;
pub mod global;
pub mod keys;
pub mod lines;
pub mod map;
pub mod range;
pub mod set;
pub mod show;
pub mod sort;
pub mod substitute;
pub mod table;

use crate::code::vim::key;
use crate::code::vim::macros::MOST_DEPTH;
use crate::code::vim::maps::ExMap;
use range::{Parsed, Reader};

use super::{Shared, Vim};

impl Vim {
    /// Runs the ex line `input`, as typed after the `:`.
    pub(super) fn ex_run(&mut self, sh: &mut Shared, input: &str) {
        // A mapped command may map to another: they end.
        if sh.macros.depth >= MOST_DEPTH {
            return;
        }
        let previous = sh.regs.get(Some(':'));
        sh.regs.ran(input);
        let last_pattern = sh.regs.get(Some('/')).map(|r| r.text);
        let t = self.buf.text();
        let reader = Reader {
            t,
            cursor_line: t.line_of(self.cursor),
            visual: self.is_visual(),
            marks: &self.buf.marks,
            last_pattern: last_pattern.as_deref(),
        };
        let parsed = match reader.parse(input) {
            Ok(p) => p,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        self.exit_visual();
        if parsed.name.is_empty() {
            match parsed.line {
                Some(line) => self.goto_line(line),
                None => self.message = Some(format!("Not an editor command \":{input}\"")),
            }
            return;
        }
        if let Some(mapped) = sh.maps.ex(&parsed.name).cloned() {
            sh.macros.depth += 1;
            match mapped {
                ExMap::Command(line) => self.ex_run(sh, &line),
                ExMap::Keys(keys) => self.feed_keys(sh, &key::parse(&keys), false),
            }
            sh.macros.depth -= 1;
            return;
        }
        let Some(entry) = table::find(&parsed.name) else {
            self.message = Some(format!("Not an editor command \":{input}\""));
            return;
        };
        if entry.quiet {
            sh.regs.ran(&previous.map(|r| r.text).unwrap_or_default());
        }
        self.dispatch(sh, entry.name, &parsed, input);
        self.want = self.buf.text().col(self.cursor.min(self.buf.doc().len()));
    }

    fn dispatch(&mut self, sh: &mut Shared, name: &str, p: &Parsed, input: &str) {
        match name {
            "write" => self.ex_write(),
            "undo" => self.undo_redo(true, 1),
            "redo" => self.undo_redo(false, 1),
            "nohlsearch" => sh.search.highlight = false,
            "startinsert" => self.ex_start_insert(sh, p),
            "colorscheme" => {}
            "version" => self.message = Some("Codemirror-vim version: <DEV>".into()),
            "delete" => self.ex_delete(sh, p),
            "yank" => self.ex_yank(sh, p),
            "put" => self.ex_put(sh, p),
            "join" => self.ex_join(p),
            "normal" => self.ex_normal(sh, p),
            "substitute" => {
                let in_global = self.in_global;
                self.ex_substitute(sh, p, in_global);
            }
            "global" | "vglobal" => self.ex_global(sh, p),
            "sort" => self.ex_sort(p),
            "registers" => self.ex_registers(sh, p),
            "marks" => self.ex_marks(p),
            "delmarks" => self.ex_delmarks(p),
            "set" | "setlocal" | "setglobal" => self.ex_set(sh, p, name, input),
            "unmap" => self.ex_unmap(sh, p, input),
            n if n.ends_with("mapclear") => self.ex_mapclear(sh, n),
            n if n.ends_with("map") || n.ends_with("noremap") => self.ex_map(sh, p, n, input),
            _ => {}
        }
    }
}
