//! `Ctrl-a` and `Ctrl-x`: the number under or after the cursor on its line,
//! changed by the count (EDITOR-VIM-21). A number is read as the library
//! reads it: decimal, `0x` hex, `0b` binary, or octal if it begins with a
//! `0` and has digits after it; a `-` before it makes it negative, and the
//! prefix and the zeros that fill it out are kept.

use std::sync::OnceLock;

use fancy_regex::Regex;

use super::Vim;

fn number() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(-?)(?:(0x)([\da-f]+)|(0b|0|)(\d+))").expect("the pattern is valid"))
}

/// `value` written in `base`, in lower case, with a sign if negative.
fn written(value: i128, base: u32) -> String {
    let mut n = value.unsigned_abs();
    let mut digits = Vec::new();
    loop {
        digits.push(std::char::from_digit((n % base as u128) as u32, base).expect("a digit of the base"));
        n /= base as u128;
        if n == 0 {
            break;
        }
    }
    if value < 0 {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

impl Vim {
    pub(super) fn increment(&mut self, up: bool, count: usize) {
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        let (start, end) = (t.line_start(line), t.line_end(line));
        let text = t.slice(start, end);
        let at = self.cursor - start;
        let mut found = None;
        let mut from = 0;
        while from <= text.len() {
            let Ok(Some(caps)) = number().captures_from_pos(&text, from) else { break };
            let whole = caps.get(0).expect("group 0");
            found = Some((whole.start(), whole.end(), caps));
            if at < whole.end() {
                break;
            }
            from = whole.end().max(whole.start() + 1);
        }
        let Some((m_start, m_end, caps)) = found else { return };
        if m_end <= at {
            return;
        }
        let sign = caps.get(1).map_or("", |m| m.as_str());
        let prefix = caps.get(2).or(caps.get(4)).map_or("", |m| m.as_str());
        let digits = caps.get(3).or(caps.get(5)).map_or("", |m| m.as_str());
        let base = match prefix.to_ascii_lowercase().as_str() {
            "0b" => 2,
            "0" => 8,
            "0x" => 16,
            _ => 10,
        };
        // As JavaScript's `parseInt` reads it: the digits of the base from the front, the rest ignored.
        let valid: String = digits.chars().take_while(|c| c.is_digit(base)).collect();
        let Ok(value) = i128::from_str_radix(&format!("{sign}{valid}"), base) else { return };
        let by = count as i128;
        let value = if up { value.saturating_add(by) } else { value.saturating_sub(by) };
        let body = written(value, base);
        let padding = if prefix.is_empty() {
            0
        } else {
            (digits.len() + sign.len()).saturating_sub(body.len())
        };
        let new = match body.strip_prefix('-') {
            Some(rest) => format!("-{prefix}{}{rest}", "0".repeat(padding)),
            None => format!("{prefix}{}{body}", "0".repeat(padding)),
        };
        self.buf.replace(start + m_start, start + m_end, &new);
        self.cursor = start + m_start + new.len() - 1;
        self.want = self.buf.text().col(self.cursor);
    }
}

#[cfg(test)]
mod tests;
