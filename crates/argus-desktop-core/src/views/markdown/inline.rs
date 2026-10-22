//! The marks inside a run of text: code spans, emphasis, strikethrough, links and bare URLs.
//!
//! Every string stays text and nothing else: raw HTML shows as the text it is, an image is a link,
//! and only an http(s) URL becomes a link at all. Every scan for a closer is either capped or
//! remembered, so a long run of unclosed markers stays linear, and nesting stops at `MAX_DEPTH`
//! (past it a delimiter is literal text).

use std::collections::HashSet;

use super::link::{bare_url_end, link_at, safe_url};
use super::model::{Inline, MAX_DEPTH};

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn find(src: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || src.len() < needle.len() {
        return None;
    }
    (from..=src.len() - needle.len()).find(|&k| src[k..k + needle.len()] == *needle)
}

fn is_ci_web(src: &[char], i: usize) -> bool {
    let head: String = src[i..].iter().take(8).collect::<String>().to_ascii_lowercase();
    head.starts_with("http://") || head.starts_with("https://")
}

/// `src` read for its marks.
pub fn inline(src: &[char], depth: usize, in_link: bool) -> Vec<Inline> {
    let n = src.len();
    let mut out: Vec<Inline> = Vec::new();
    // Delimiters known to have no closer anywhere ahead, so a long run of unclosed ones is not
    // rescanned from each.
    let mut dead: HashSet<(char, usize)> = HashSet::new();
    let mut buf = String::new();
    let mut i = 0;

    fn flush(buf: &mut String, out: &mut Vec<Inline>) {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    }

    'scan: while i < n {
        let c = src[i];

        if c == '\\' && i + 1 < n && src[i + 1].is_ascii_punctuation() {
            buf.push(src[i + 1]);
            i += 2;
            continue;
        }
        if c == '\n' {
            let kept = buf.trim_end_matches(' ').len();
            buf.truncate(kept);
            flush(&mut buf, &mut out);
            out.push(Inline::Break);
            i += 1;
            continue;
        }

        if c == '`' {
            let run = src[i..].iter().take_while(|c| **c == '`').count();
            if !dead.contains(&('`', run)) {
                let ticks = vec!['`'; run];
                let mut k = i + run;
                loop {
                    match find(src, &ticks, k) {
                        None => {
                            dead.insert(('`', run));
                            break;
                        }
                        Some(at) => {
                            let end = at + src[at + run..].iter().take_while(|c| **c == '`').count() + run;
                            if end - at == run {
                                let mut code: String = src[i + run..at].iter().map(|c| if *c == '\n' { ' ' } else { *c }).collect();
                                if code.chars().count() > 2 && code.starts_with(' ') && code.ends_with(' ') {
                                    code = code[1..code.len() - 1].to_owned();
                                }
                                flush(&mut buf, &mut out);
                                out.push(Inline::Code(code));
                                i = end;
                                continue 'scan;
                            }
                            k = end;
                        }
                    }
                }
            }
            buf.extend(std::iter::repeat_n('`', run));
            i += run;
            continue;
        }

        if depth < MAX_DEPTH {
            let image = c == '!' && src.get(i + 1) == Some(&'[');
            if (c == '[' || image) && !in_link {
                let open = if image { i + 1 } else { i };
                if let Some(found) = link_at(src, open) {
                    let href = safe_url(&found.url);
                    let label = inline(&src[found.label.0..found.label.1], depth + 1, true);
                    flush(&mut buf, &mut out);
                    match href {
                        Some(url) => {
                            let children = if label.is_empty() { vec![Inline::Text(url.clone())] } else { label };
                            out.push(Inline::Link { url, children });
                        }
                        None => out.extend(label),
                    }
                    i = found.end;
                    continue;
                }
            }

            let delims: &[usize] = match c {
                '*' | '_' => &[2, 1],
                '~' => &[2],
                _ => &[],
            };
            for &len in delims {
                let d = vec![c; len];
                if src[i..].len() < len || src[i..i + len] != d[..] || dead.contains(&(c, len)) {
                    continue;
                }
                match src.get(i + len) {
                    None => continue,
                    Some(after) if after.is_whitespace() => continue,
                    _ => {}
                }
                if c == '_' && i > 0 && is_word(src[i - 1]) {
                    continue;
                }
                let mut k = i + len;
                loop {
                    let Some(found) = find(src, &d, k) else {
                        dead.insert((c, len));
                        break;
                    };
                    k = found;
                    let run = src[k..].iter().take_while(|x| **x == c).count();
                    // A single marker steps over a longer run; a double one closes on the last two
                    // of it, so `***x***` nests.
                    if len == 1 && run > 1 {
                        k += run;
                        continue;
                    }
                    let at = if len == 2 { k + run - 2 } else { k };
                    let ok = at > i + len
                        && !src[at - 1].is_whitespace()
                        && !(c == '_' && src.get(at + len).is_some_and(|x| is_word(*x)));
                    if ok {
                        let children = inline(&src[i + len..at], depth + 1, in_link);
                        flush(&mut buf, &mut out);
                        out.push(match (c, len) {
                            ('~', _) => Inline::Del(children),
                            (_, 2) => Inline::Strong(children),
                            _ => Inline::Em(children),
                        });
                        i = at + len;
                        continue 'scan;
                    }
                    k += run;
                }
            }
        }

        if !in_link && (c == 'h' || c == 'H') && (i == 0 || !is_word(src[i - 1])) && is_ci_web(src, i) {
            let end = bare_url_end(src, i);
            let raw: String = src[i..end].iter().collect();
            if let Some(url) = safe_url(&raw) {
                flush(&mut buf, &mut out);
                out.push(Inline::Link { url, children: vec![Inline::Text(raw)] });
                i = end;
                continue;
            }
        }

        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut out);
    out
}

/// `text` read for its marks, the way a paragraph, a heading, a list item or a table cell is.
pub fn parse(text: &str) -> Vec<Inline> {
    inline(&text.chars().collect::<Vec<_>>(), 0, false)
}

/// Like [`parse`], at the depth a table cell is read at: one level in already.
pub fn parse_nested(text: &str) -> Vec<Inline> {
    inline(&text.chars().collect::<Vec<_>>(), 1, false)
}

#[cfg(test)]
pub(crate) mod tests;
