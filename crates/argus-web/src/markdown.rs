//! A reply's markdown as HTML that can run nothing.
//!
//! A transcript carries whatever an agent read — a web page, a file someone
//! else wrote — and the page showing it holds a token that can drive
//! agents. So nothing an agent says may become markup of its own choosing:
//! raw HTML is shown as text, a link keeps only a scheme that navigates,
//! and an image becomes a link, because loading one is a request to
//! wherever it points. The page's CSP stands behind this, not instead of
//! it.

use pulldown_cmark::{html, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

/// `text` as HTML safe to set as an element's content.
pub fn to_html(text: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut in_image = false;
    let events = Parser::new_ext(text, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: navigable(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            dest_url, title, id, ..
        }) => {
            in_image = true;
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: navigable(dest_url),
                title,
                id,
            })
        }
        Event::End(TagEnd::Image) if in_image => {
            in_image = false;
            Event::End(TagEnd::Link)
        }
        other => other,
    });
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, events);
    out
}

/// A link target, kept only when following it can do nothing but go
/// somewhere. Anything else — `javascript:`, `data:`, a relative path into
/// this server — becomes an inert fragment.
fn navigable(url: CowStr<'_>) -> CowStr<'_> {
    let lower = url.trim().to_ascii_lowercase();
    let allowed = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    if allowed {
        url
    } else {
        CowStr::Borrowed("#")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_becomes_html() {
        let html = to_html("**bold** and `code`\n\n- one\n- two");
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(html.contains("<code>code</code>"), "{html}");
        assert!(html.contains("<li>one</li>"), "{html}");
    }

    #[test]
    fn raw_html_is_shown_as_text() {
        let html = to_html("hi <script>alert(1)</script> <img src=x onerror=alert(1)>");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn a_block_of_raw_html_is_shown_as_text() {
        let html = to_html("<div onclick=\"steal()\">\nclick\n</div>");
        assert!(!html.contains("<div"), "{html}");
    }

    #[test]
    fn only_links_that_navigate_survive() {
        let html = to_html("[a](javascript:alert(1)) [b](https://example.com) [c](data:text/html,x)");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("data:"), "{html}");
        assert!(html.contains("href=\"https://example.com\""), "{html}");
    }

    #[test]
    fn an_image_becomes_a_link_rather_than_a_request() {
        let html = to_html("![chart](https://tracker.example/pixel.png)");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("<a href=\"https://tracker.example/pixel.png\">chart</a>"), "{html}");
    }
}
