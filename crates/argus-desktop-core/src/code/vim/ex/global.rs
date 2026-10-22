//! `:g/pattern/command` and `:v/pattern/command` (or `:g!`): the command is
//! run on each line of the range (every line, with none) that has a match
//! of the pattern, or, for `:v`, has none. The lines are held as the
//! commands edit the text, so a line a command deletes is not run, and the
//! command is run as `{line}{command}`. With no command the matching lines
//! are shown.

use super::range::Parsed;
use super::super::search::pattern::split_by;
use super::super::{Shared, Vim};

impl Vim {
    pub(super) fn ex_global(&mut self, sh: &mut Shared, p: &Parsed) {
        let Some(arg) = p.arg.clone().filter(|a| !a.is_empty()) else {
            self.message = Some("Regular Expression missing from global".into());
            return;
        };
        let mut arg = arg.as_str();
        let mut inverted = p.name.starts_with('v');
        if arg.starts_with('!') && p.name.starts_with('g') {
            inverted = true;
            arg = &arg[1..];
        }
        let t = self.buf.text();
        let last = t.last_line();
        let first = p.line.unwrap_or(0).min(last);
        let end = p.line_end.filter(|e| *e != 0).or(p.line.filter(|l| *l != 0)).unwrap_or(last).min(last);
        let (mut regex_part, mut cmd) = (arg.to_owned(), String::new());
        if let Some(tokens) = split_by(arg, '/') {
            if !tokens.is_empty() {
                regex_part = tokens[0].clone();
                cmd = tokens[1..].join("/");
            }
        }
        if !regex_part.is_empty() {
            let pcre = self.pcre(sh);
            if let Err(e) = sh.search.update(&mut sh.regs, &regex_part, true, true, pcre) {
                self.message = Some(format!("Invalid regex: {regex_part}"));
                let _ = e;
                return;
            }
        }
        let Some(query) = sh.search.query.clone() else { return };
        let matched: Vec<usize> = (first..=end)
            .filter(|&l| query.regex.is_match(&t.line_text(l)).unwrap_or(false) != inverted)
            .collect();
        if cmd.is_empty() {
            let lines: Vec<String> = matched.iter().map(|&l| t.line_text(l)).collect();
            self.message = Some(lines.join("\n"));
            return;
        }
        let starts: Vec<usize> = matched.iter().map(|&l| t.line_start(l)).collect();
        let handles: Vec<usize> = starts.into_iter().map(|at| self.buf.marks.track(at)).collect();
        let was = std::mem::replace(&mut self.in_global, true);
        for handle in handles {
            let Some(at) = self.buf.marks.line_at(handle) else { continue };
            let t = self.buf.text();
            if at > t.len() || at != t.line_start(t.line_of(at)) {
                continue;
            }
            let line = t.line_of(at);
            self.ex_run(sh, &format!("{}{cmd}", line + 1));
        }
        self.in_global = was;
        self.buf.marks.release();
    }
}

#[cfg(test)]
mod tests;
