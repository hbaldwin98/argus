//! Which links in an agent's Markdown are links at all, and where one ends.
//!
//! Only an http or https URL with a host becomes a link; `javascript:`, `data:`, a relative path
//! and the rest drop the link and keep its words. Every scan is capped (`MAX_LABEL`, `MAX_URL`) so
//! a long run of brackets stays linear.

use url::Url;

use super::model::{MAX_LABEL, MAX_URL};

/// The URL itself when it is http(s) with a host, normalised as the page's `URL` does; none for
/// anything else.
pub fn safe_url(raw: &str) -> Option<String> {
    let head = raw.chars().take(9).collect::<String>().to_ascii_lowercase();
    let rest = head.strip_prefix("https://").or_else(|| head.strip_prefix("http://"))?;
    if rest.chars().next().is_none_or(char::is_whitespace) {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|host| !host.is_empty())).then(|| url.to_string())
}

/// `[label](url "title")`, found by [`link_at`].
#[derive(Debug, PartialEq, Eq)]
pub struct Found {
    /// The character after the closing parenthesis.
    pub end: usize,
    pub label: (usize, usize),
    pub url: String,
}

fn at(src: &[char], i: usize) -> Option<char> {
    src.get(i).copied()
}

fn find(src: &[char], needle: char, from: usize) -> Option<usize> {
    src.get(from..)?.iter().position(|c| *c == needle).map(|p| p + from)
}

/// The link whose `[` is at `open`: a label of balanced brackets up to `MAX_LABEL` long, then a URL
/// in `<>` or a run without whitespace and with balanced parentheses up to `MAX_URL` long, then an
/// optional quoted title up to `MAX_LABEL` long.
pub fn link_at(src: &[char], open: usize) -> Option<Found> {
    let mut depth = 0usize;
    let mut close = None;
    let limit = src.len().min(open + MAX_LABEL);
    let mut k = open + 1;
    while k < limit {
        match src[k] {
            '\\' => k += 1,
            '[' => depth += 1,
            ']' => {
                if depth == 0 {
                    close = Some(k);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
        k += 1;
    }
    let close = close?;
    if at(src, close + 1) != Some('(') {
        return None;
    }
    let mut k = close + 2;
    while at(src, k) == Some(' ') {
        k += 1;
    }
    let from = k;
    let url: String;
    if at(src, k) == Some('<') {
        let gt = find(src, '>', k)?;
        if gt - k > MAX_URL || src[k..gt].contains(&'\n') {
            return None;
        }
        url = src[k + 1..gt].iter().collect();
        k = gt + 1;
    } else {
        let mut parens = 0usize;
        let cap = src.len().min(from + MAX_URL);
        while k < cap && !src[k].is_whitespace() {
            match src[k] {
                '(' => parens += 1,
                ')' => {
                    if parens == 0 {
                        break;
                    }
                    parens -= 1;
                }
                _ => {}
            }
            k += 1;
        }
        url = src[from..k].iter().collect();
    }
    while at(src, k) == Some(' ') {
        k += 1;
    }
    if let Some(quote @ ('"' | '\'')) = at(src, k) {
        let end = find(src, quote, k + 1)?;
        if end - k > MAX_LABEL {
            return None;
        }
        k = end + 1;
        while at(src, k) == Some(' ') {
            k += 1;
        }
    }
    (at(src, k) == Some(')')).then(|| Found { end: k + 1, label: (open + 1, close), url })
}

/// Where a bare URL beginning at `from` ends: at whitespace or `<`, with the punctuation a
/// sentence puts after one, and a `)` nothing opened, taken off.
pub fn bare_url_end(src: &[char], from: usize) -> usize {
    let mut end = from;
    let cap = src.len().min(from + MAX_URL);
    while end < cap && !(src[end].is_whitespace() || src[end] == '<') {
        end += 1;
    }
    let count = |end: usize, ch: char| src[from..end].iter().filter(|c| **c == ch).count();
    while end > from {
        let c = src[end - 1];
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '*' | '_' | '~') || (c == ')' && count(end, ')') > count(end, '(')) {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn only_a_web_address_with_a_host_is_a_link() {
        assert_eq!(safe_url("https://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(safe_url("HTTP://Example.COM").as_deref(), Some("http://example.com/"), "normalised as the page's URL does");
        for not in ["javascript:alert(1)", "data:text/html,x", "/relative", "ftp://host/x", "http://", "https:// x", "mailto:a@b.c", "", "//host/x"] {
            assert_eq!(safe_url(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_link_has_a_label_a_url_and_an_optional_title() {
        let src = chars("[the docs](https://a.b/c \"title\") after");
        let found = link_at(&src, 0).expect("a link");
        assert_eq!(src[found.label.0..found.label.1].iter().collect::<String>(), "the docs");
        assert_eq!(found.url, "https://a.b/c");
        assert_eq!(src[found.end..].iter().collect::<String>(), " after");
    }

    #[test]
    fn an_angle_url_may_hold_spaces_and_a_bare_one_balances_its_parentheses() {
        let angle = chars("[x](<https://a.b/c d>)");
        assert_eq!(link_at(&angle, 0).unwrap().url, "https://a.b/c d");
        let parens = chars("[x](https://a.b/(c)d)e");
        let found = link_at(&parens, 0).unwrap();
        assert_eq!(found.url, "https://a.b/(c)d");
        assert_eq!(parens[found.end], 'e');
    }

    #[test]
    fn a_label_balances_its_brackets_and_an_escaped_one_does_not_count() {
        assert!(link_at(&chars("[a [b] c](https://x.y)"), 0).is_some());
        assert!(link_at(&chars("[a \\] b](https://x.y)"), 0).is_some());
        assert!(link_at(&chars("[a](https://x.y"), 0).is_none(), "no closing parenthesis");
        assert!(link_at(&chars("[a] (https://x.y)"), 0).is_none(), "a blank between is not a link");
    }

    #[test]
    fn a_label_and_a_url_have_limits() {
        let long_label = format!("[{}](https://x.y)", "a".repeat(MAX_LABEL + 5));
        assert!(link_at(&chars(&long_label), 0).is_none());
        let long_url = format!("[a](https://x.y/{})", "b".repeat(MAX_URL + 5));
        assert!(link_at(&chars(&long_url), 0).is_none());
        let long_angle = format!("[a](<https://x.y/{}>)", "b".repeat(MAX_URL + 5));
        assert!(link_at(&chars(&long_angle), 0).is_none());
    }

    #[test]
    fn a_bare_url_stops_before_the_punctuation_that_ends_a_sentence() {
        let end = |s: &str| {
            let c = chars(s);
            c[..bare_url_end(&c, 0)].iter().collect::<String>()
        };
        assert_eq!(end("https://a.b/c."), "https://a.b/c");
        assert_eq!(end("https://a.b/c?!"), "https://a.b/c");
        assert_eq!(end("https://a.b/c) and"), "https://a.b/c", "a parenthesis nothing opened");
        assert_eq!(end("https://a.b/(c) and"), "https://a.b/(c)", "one something opened");
        assert_eq!(end("https://a.b/c<b>"), "https://a.b/c");
    }
}
