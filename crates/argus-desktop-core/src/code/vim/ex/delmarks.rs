//! `:delmarks`: marks named by letters, singly (`a`) or as a range (`a-d`)
//! of letters of one case, are removed. Anything else is an invalid
//! argument.

use super::range::Parsed;
use super::super::Vim;

impl Vim {
    pub(super) fn ex_delmarks(&mut self, p: &Parsed) {
        let arg = p.arg.as_deref().map(str::trim).unwrap_or_default();
        if arg.is_empty() {
            self.message = Some("Argument required".into());
            return;
        }
        let chars: Vec<char> = arg.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                i += 1;
            }
            let start = i;
            let rest = || chars[start..].iter().collect::<String>();
            let Some(&sym) = chars.get(i).filter(|c| c.is_ascii_alphabetic()) else {
                self.message = Some(format!("Invalid argument: {}", rest()));
                return;
            };
            i += 1;
            if chars.get(i) == Some(&'-') {
                i += 1;
                let Some(&end) = chars.get(i).filter(|c| c.is_ascii_alphabetic()) else {
                    self.message = Some(format!("Invalid argument: {}", rest()));
                    return;
                };
                i += 1;
                if sym.is_ascii_lowercase() != end.is_ascii_lowercase() {
                    self.message = Some(format!("Invalid argument: {sym}-"));
                    return;
                }
                if sym >= end {
                    self.message = Some(format!("Invalid argument: {}", rest()));
                    return;
                }
                for c in sym..=end {
                    self.buf.marks.delete(c);
                }
            } else {
                self.buf.marks.delete(sym);
            }
        }
    }
}

#[cfg(test)]
mod tests;
