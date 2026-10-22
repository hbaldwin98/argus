//! Where a link in a rendered markdown file goes: the system browser, another file of the checkout,
//! or nowhere.
//!
//! The file's text is whatever its author wrote, so only two things are ever acted on: an http(s)
//! address, which Rust opens (`Call::OpenUrl` refuses anything else too), and a path that stays
//! inside the checkout. Every other scheme and an anchor in the page do nothing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Goes {
    Browser(String),
    /// A file of the checkout, by its path from the root.
    File(String),
    Nowhere,
}

/// What a press on `href` in the file at `from` does.
pub fn goes(from: &str, href: &str) -> Goes {
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Goes::Browser(href.to_owned());
    }
    // A scheme (`mailto:`, `file:`, `javascript:`) is a colon before any slash, and `//host` is a
    // network address with the scheme left off.
    let scheme = href.find(':').is_some_and(|colon| !href[..colon].contains(['/', '?', '#']));
    if href.is_empty() || href.starts_with('#') || href.starts_with("//") || scheme {
        return Goes::Nowhere;
    }
    let target = href.split(['#', '?']).next().unwrap_or_default();
    // Only a file is read; `docs/` names a folder, which the tree opens and a link should not.
    if target.is_empty() || target.ends_with('/') {
        return Goes::Nowhere;
    }
    let target = unescape(target);
    let mut parts: Vec<&str> = match target.strip_prefix('/') {
        Some(_) => Vec::new(),
        None => from.rsplit_once('/').map_or(Vec::new(), |(dir, _)| dir.split('/').collect()),
    };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.pop().is_none() => return Goes::Nowhere,
            ".." => {}
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Goes::Nowhere;
    }
    Goes::File(parts.join("/"))
}

/// `%20` and its kind, as a path in a link spells a space; what is not valid UTF-8 stays as written.
fn unescape(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: &u8| (*b as char).to_digit(16);
        match (bytes[at], bytes.get(at + 1).and_then(hex), bytes.get(at + 2).and_then(hex)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            (byte, ..) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> Goes {
        Goes::File(path.into())
    }

    #[test]
    fn a_web_address_goes_to_the_browser() {
        assert_eq!(goes("README.md", "https://example.test/a?b=1#c"), Goes::Browser("https://example.test/a?b=1#c".into()));
        assert_eq!(goes("README.md", "HTTP://example.test"), Goes::Browser("HTTP://example.test".into()));
    }

    #[test]
    fn a_relative_path_is_read_from_the_folder_of_the_file_it_is_in() {
        assert_eq!(goes("README.md", "docs/guide.md"), file("docs/guide.md"));
        assert_eq!(goes("docs/guide.md", "./intro.md#top"), file("docs/intro.md"));
        assert_eq!(goes("docs/guide.md", "../src/lib.rs"), file("src/lib.rs"));
        assert_eq!(goes("docs/guide.md", "/Cargo.toml"), file("Cargo.toml"));
        assert_eq!(goes("docs/guide.md", "my%20notes.md"), file("docs/my notes.md"));
    }

    #[test]
    fn a_path_that_leaves_the_checkout_goes_nowhere() {
        assert_eq!(goes("README.md", "../secret"), Goes::Nowhere);
        assert_eq!(goes("docs/guide.md", "../../secret"), Goes::Nowhere);
    }

    #[test]
    fn an_anchor_a_folder_and_other_schemes_go_nowhere() {
        for href in ["#install", "", "docs/", "mailto:a@b.test", "javascript:alert(1)", "file:///etc/passwd", "//evil.test/x", "C:/Windows", "?q=1"] {
            assert_eq!(goes("README.md", href), Goes::Nowhere, "{href}");
        }
    }
}
