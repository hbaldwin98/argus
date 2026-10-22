//! What the `:` and `/` lines remember: each line run, and Up and Down
//! through the ones that begin with what is typed. The library's history
//! controller, one for ex commands and one for searches.

#[derive(Debug, Default, Clone)]
pub struct Recall {
    lines: Vec<String>,
    at: usize,
    /// What was typed when the walk began, which the lines must start with.
    prefix: Option<String>,
}

impl Recall {
    /// The previous line (`up`) or the next one that begins with what was
    /// typed when the walk began; at the newer end, what was typed.
    pub fn next_match(&mut self, typed: &str, up: bool) -> String {
        let prefix = self.prefix.get_or_insert_with(|| typed.to_owned()).clone();
        let mut i = self.at as isize;
        loop {
            i += if up { -1 } else { 1 };
            if i < 0 || i as usize >= self.lines.len() {
                break;
            }
            if self.lines[i as usize].starts_with(&prefix) {
                self.at = i as usize;
                return self.lines[i as usize].clone();
            }
        }
        if i >= 0 {
            self.at = self.lines.len();
            return prefix;
        }
        typed.to_owned()
    }

    /// A line was run (or given up): it goes last, and an equal one earlier
    /// is dropped.
    pub fn push(&mut self, line: &str) {
        if let Some(i) = self.lines.iter().position(|l| l == line) {
            self.lines.remove(i);
        }
        if !line.is_empty() {
            self.lines.push(line.to_owned());
        }
    }

    /// The walk ends: the next one starts from the newest line.
    pub fn reset(&mut self) {
        self.prefix = None;
        self.at = self.lines.len();
    }
}

#[cfg(test)]
mod tests;
