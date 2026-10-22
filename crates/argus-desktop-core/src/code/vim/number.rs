//! `Ctrl-a` and `Ctrl-x`: the decimal number under or after the cursor on
//! its line, changed by the count (EDITOR-VIM-21). Hex, octal and binary
//! forms are not read; their digits are taken as a decimal number.

use super::Vim;

impl Vim {
    pub(super) fn increment(&mut self, up: bool, count: usize) {
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        let (start, end) = (t.line_start(line), t.line_end(line));
        let text = t.slice(start, end);
        let bytes = text.as_bytes();
        let at = self.cursor - start;
        let mut i = at.min(bytes.len());
        if !bytes.get(i).is_some_and(u8::is_ascii_digit) {
            match bytes[i..].iter().position(u8::is_ascii_digit) {
                Some(n) => i += n,
                None => return,
            }
        } else {
            while i > 0 && bytes[i - 1].is_ascii_digit() {
                i -= 1;
            }
        }
        let mut stop = i;
        while bytes.get(stop).is_some_and(u8::is_ascii_digit) {
            stop += 1;
        }
        let negative = i > 0 && bytes[i - 1] == b'-' && !(i > 1 && bytes[i - 2].is_ascii_alphanumeric());
        let from = if negative { i - 1 } else { i };
        let Ok(value) = text[i..stop].parse::<i128>() else { return };
        let value = if negative { -value } else { value };
        let by = count as i128;
        let value = if up { value + by } else { value - by };
        let written = value.to_string();
        self.buf.replace(start + from, start + stop, &written);
        self.cursor = start + from + written.len() - 1;
        self.want = self.buf.text().col(self.cursor);
    }
}
