//! What the address field turns typed text into (SHELL-BROWSER-8).
//!
//! The daemon opens only `http`, `https` and `about:blank`, so this only
//! decides what the person meant; it does not vouch for the result.

/// The address typed text means, or `None` for text that is blank (Enter on
/// an empty field sends nothing). Trimmed, then:
/// - text that starts with a scheme (`^[a-z][a-z0-9+.-]*:`) is kept, unless
///   what follows its colon is a port: that is a host, `localhost:3000`;
/// - a bare host, with or without a port, gets `https://`, except a
///   loopback host (`localhost`, `127.0.0.1`, `[::1]`, `*.localhost`),
///   which gets `http://`: a server on this machine has no certificate, so
///   the page's `https://localhost` would never load. The page kept
///   `localhost:3000` as it came, which the daemon refused (decided, audit
///   item 84).
pub fn address(typed: &str) -> Option<String> {
    let text = typed.trim();
    if text.is_empty() {
        return None;
    }
    if has_scheme(text) && !is_host_port(text) {
        return Some(text.to_string());
    }
    let scheme = if is_loopback(host(text)) { "http" } else { "https" };
    Some(format!("{scheme}://{text}"))
}

fn has_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    for c in chars {
        match c {
            ':' => return true,
            c if c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-') => {}
            _ => return false,
        }
    }
    false
}

/// Whether the text after a scheme-shaped word's colon is a port, then the
/// end or a path, query or fragment.
fn is_host_port(text: &str) -> bool {
    let Some((_, after)) = text.split_once(':') else { return false };
    let digits = after.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && matches!(after[digits..].chars().next(), None | Some('/' | '?' | '#'))
}

/// The host of text with no scheme: up to the first path, query, fragment
/// or, outside an IPv6 literal, port.
fn host(text: &str) -> &str {
    let end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let authority = &text[..end];
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if authority.starts_with('[') {
        return authority.find(']').map_or(authority, |close| &authority[..=close]);
    }
    authority.split(':').next().unwrap_or(authority)
}

fn is_loopback(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost" || host.ends_with(".localhost") || host == "[::1]" || (host.starts_with("127.") && host.split('.').count() == 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_host_is_the_web() {
        assert_eq!(address("  docs.rs/tokio ").as_deref(), Some("https://docs.rs/tokio"));
        assert_eq!(address("example.com:8443/x").as_deref(), Some("https://example.com:8443/x"), "a host with a port is a host");
    }

    #[test]
    fn a_scheme_is_kept_as_typed() {
        for kept in ["https://a.test", "HTTP://A.TEST", "about:blank", "file:///etc/passwd", "view-source:https://a.test"] {
            assert_eq!(address(kept).as_deref(), Some(kept));
        }
    }

    #[test]
    fn a_server_on_this_machine_is_http_with_or_without_a_port() {
        assert_eq!(address("localhost:3000").as_deref(), Some("http://localhost:3000"));
        assert_eq!(address("localhost:3000/a?b#c").as_deref(), Some("http://localhost:3000/a?b#c"));
        assert_eq!(address("localhost").as_deref(), Some("http://localhost"));
        assert_eq!(address("127.0.0.1:8080").as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(address("[::1]:8080/x").as_deref(), Some("http://[::1]:8080/x"));
        assert_eq!(address("app.localhost:5173").as_deref(), Some("http://app.localhost:5173"));
        assert_eq!(address("localhost.example.com:80").as_deref(), Some("https://localhost.example.com:80"));
        assert_eq!(address("http://localhost:3000").as_deref(), Some("http://localhost:3000"), "a typed scheme wins");
    }

    #[test]
    fn blank_text_is_no_address() {
        assert_eq!(address(""), None);
        assert_eq!(address("  \t "), None);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // SHELL-BROWSER-8: whatever is typed, what is sent starts with a scheme.
            #[test]
            fn an_address_always_has_a_scheme(typed in "\\PC{0,30}") {
                if let Some(url) = address(&typed) {
                    prop_assert!(has_scheme(&url), "{url:?}");
                }
            }

            // SHELL-BROWSER-8: text with an http(s) scheme is kept, trimmed.
            #[test]
            fn an_http_address_is_kept(rest in "[a-z0-9./?#=&-]{0,20}", secure: bool, pad in " {0,3}") {
                let url = format!("{}://h{rest}", if secure { "https" } else { "http" });
                prop_assert_eq!(address(&format!("{pad}{url}{pad}")), Some(url));
            }
        }
    }
}
