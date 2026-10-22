//! `]` and `[` followed by a symbol: the next unmatched bracket (`]}` `[(`),
//! the next or last section start (`]]` `[[` `][` `[]`), a comment's end
//! (`]*` `[/`), a method's brace (`]m` `[M`) or a preprocessor line
//! (`[#`). A port of the library's walk, character by character, line by
//! line; a key that names none of these goes nowhere.

use super::sentence::At;
use super::text::Text;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Bracket,
    Section,
    Comment,
    Method,
    Preprocess,
}

fn mode_of(symbol: char) -> Option<Mode> {
    Some(match symbol {
        '(' | ')' | '{' | '}' => Mode::Bracket,
        '[' | ']' => Mode::Section,
        '*' | '/' => Mode::Comment,
        'm' | 'M' => Mode::Method,
        '#' => Mode::Preprocess,
        _ => return None,
    })
}

struct State {
    line: Vec<char>,
    next: Option<char>,
    last: Option<char>,
    index: isize,
    symbol: char,
    reverse: Option<char>,
    forward: bool,
    depth: i32,
    move_through: bool,
}

impl State {
    fn char_at(&self, index: isize) -> Option<char> {
        usize::try_from(index).ok().and_then(|i| self.line.get(i)).copied()
    }

    fn init(&mut self, mode: Mode) {
        match mode {
            Mode::Section => {
                self.move_through = true;
                self.symbol = if (if self.forward { ']' } else { '[' }) == self.symbol { '{' } else { '}' };
            }
            Mode::Method => {
                self.symbol = if self.symbol == 'm' { '{' } else { '}' };
                self.reverse = Some(if self.symbol == '{' { '}' } else { '{' });
            }
            Mode::Preprocess => self.index = 0,
            _ => {}
        }
    }

    fn complete(&mut self, mode: Mode) -> bool {
        match mode {
            Mode::Bracket => {
                if self.next == Some(self.symbol) {
                    self.depth += 1;
                    return self.depth >= 1;
                }
                if self.next.is_some() && self.next == self.reverse {
                    self.depth -= 1;
                }
                false
            }
            Mode::Section => self.index == 0 && self.next == Some(self.symbol),
            Mode::Comment => {
                let found = self.last == Some('*') && self.next == Some('/');
                self.last = self.next;
                found
            }
            Mode::Method => self.next == Some(self.symbol),
            Mode::Preprocess => {
                if self.next != Some('#') {
                    return false;
                }
                let starts = self.line.first() == Some(&'#');
                let rest: String = if starts {
                    self.line.iter().skip(1).take_while(|c| c.is_alphanumeric() || **c == '_').collect()
                } else {
                    String::new()
                };
                match rest.as_str() {
                    "endif" => {
                        if self.forward && self.depth == 0 {
                            return true;
                        }
                        self.depth += 1;
                    }
                    "if" => {
                        if !self.forward && self.depth == 0 {
                            return true;
                        }
                        self.depth -= 1;
                    }
                    _ => {}
                }
                rest == "else" && self.depth == 0
            }
        }
    }
}

/// Where `]symbol` (`forward`) or `[symbol` goes from `cursor`, `count`
/// times; `cursor` itself when the symbol is none of the known ones or
/// nothing is found.
pub fn find(t: Text, cursor: At, count: usize, forward: bool, symbol: char) -> At {
    let Some(mode) = mode_of(symbol) else { return cursor };
    let inc: isize = if forward { 1 } else { -1 };
    let end_line: isize = if forward { t.line_count() as isize } else { -1 };
    let mut line = cursor.0 as isize;
    let text: Vec<char> = t.line_text(cursor.0).chars().collect();
    let reverse = if forward {
        match symbol {
            ')' => Some('('),
            '}' => Some('{'),
            _ => None,
        }
    } else {
        match symbol {
            '(' => Some(')'),
            '{' => Some('}'),
            _ => None,
        }
    };
    let mut st = State {
        next: text.get(cursor.1).copied(),
        line: text,
        last: None,
        index: cursor.1 as isize,
        symbol,
        reverse,
        forward,
        depth: 0,
        move_through: false,
    };
    st.init(mode);
    let mut repeat = count.max(1);
    let mut at = cursor;
    while line != end_line && repeat > 0 {
        st.index += inc;
        st.next = st.char_at(st.index);
        if st.next.is_none() {
            line += inc;
            st.line = if line >= 0 && (line as usize) <= t.last_line() {
                t.line_text(line as usize).chars().collect()
            } else {
                Vec::new()
            };
            st.index = if inc > 0 { 0 } else { (st.line.len() as isize - 1).max(0) };
            st.next = st.char_at(st.index);
        }
        if st.complete(mode) {
            at = (line.max(0) as usize, st.index.max(0) as usize);
            repeat -= 1;
        }
    }
    if st.next.is_some() || st.move_through {
        return (line.max(0) as usize, st.index.max(0) as usize);
    }
    at
}

#[cfg(test)]
mod tests;
