//! The tag object, `it` and `at`: the element around a position, found by
//! reading the text as markup. The page's editor has no markup grammar
//! loaded, so there the object finds nothing; here it reads tags itself, so
//! the keys do what they say in any file. Comments, `<!...>` and `<?...?>`
//! are skipped, a self-closing or void tag has no body, and a close tag
//! closes the nearest open one of its name.

use super::text::Text;

/// An element: its whole span, and where its open tag ends and its close
/// tag begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Element {
    from: usize,
    open_to: usize,
    close_from: usize,
    to: usize,
}

const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

fn is_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-')
}

/// The end of a tag that opens at `from`: past its `>`, outside quotes.
fn tag_end(s: &[u8], from: usize) -> Option<usize> {
    let mut quote = None;
    let mut i = from;
    while i < s.len() {
        match (quote, s[i]) {
            (Some(q), b) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(s[i]),
            (None, b'>') => return Some(i + 1),
            _ => {}
        }
        i += 1;
    }
    None
}

fn find(s: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    s[from.min(s.len())..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

fn elements(text: &str) -> Vec<Element> {
    let s = text.as_bytes();
    let mut open: Vec<(String, usize, usize)> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] != b'<' {
            i += 1;
            continue;
        }
        if s[i..].starts_with(b"<!--") {
            i = find(s, i + 4, b"-->").map_or(s.len(), |e| e + 3);
        } else if s[i..].starts_with(b"<![CDATA[") {
            i = find(s, i + 9, b"]]>").map_or(s.len(), |e| e + 3);
        } else if s[i..].starts_with(b"<!") || s[i..].starts_with(b"<?") {
            i = tag_end(s, i + 1).unwrap_or(s.len());
        } else if s[i..].starts_with(b"</") {
            let name_end = (i + 2..s.len()).find(|&k| !is_name(s[k])).unwrap_or(s.len());
            let name = text[i + 2..name_end].to_ascii_lowercase();
            let end = tag_end(s, name_end).unwrap_or(s.len());
            if !name.is_empty() {
                if let Some(k) = open.iter().rposition(|(n, ..)| *n == name) {
                    let (_, from, open_to) = open[k].clone();
                    open.truncate(k);
                    out.push(Element { from, open_to, close_from: i, to: end });
                }
            }
            i = end;
        } else if s.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            let name_end = (i + 1..s.len()).find(|&k| !is_name(s[k])).unwrap_or(s.len());
            let name = text[i + 1..name_end].to_ascii_lowercase();
            let end = tag_end(s, name_end).unwrap_or(s.len());
            let self_closing = end >= 2 && s[end - 2] == b'/';
            if !self_closing && !VOID.contains(&name.as_str()) {
                open.push((name, i, end));
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// The span of the `count`th element around `at`, `inner` for the body
/// without its tags; none if no element holds `at`.
pub fn object(t: Text, at: usize, inner: bool, count: usize) -> Option<(usize, usize)> {
    let text = t.0.to_string();
    // On a `<` the position is taken as inside the tag it opens.
    let at = if t.char_at(at) == Some('<') { at + 1 } else { at };
    let mut around: Vec<Element> = elements(&text).into_iter().filter(|e| e.from <= at && at < e.to).collect();
    around.sort_by_key(|e| e.to - e.from);
    let e = around.get(count.max(1) - 1)?;
    Some(if inner { (e.open_to, e.close_from) } else { (e.from, e.to) })
}

#[cfg(test)]
mod tests;
