use super::*;

/// The marks as tags, to compare in one line: `<b>`, `<i>`, `<s>`, `<c>`, `<a href>` and `<br>`.
pub(crate) fn show(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) => text.clone(),
            Inline::Code(text) => format!("<c>{text}</c>"),
            Inline::Strong(inner) => format!("<b>{}</b>", show(inner)),
            Inline::Em(inner) => format!("<i>{}</i>", show(inner)),
            Inline::Del(inner) => format!("<s>{}</s>", show(inner)),
            Inline::Link { url, children } => format!("<a {url}>{}</a>", show(children)),
            Inline::Break => "<br>".to_owned(),
        })
        .collect()
}

fn read(src: &str) -> String {
    show(&parse(src))
}

// MARKDOWN-6

#[test]
fn a_backslash_before_punctuation_makes_it_literal() {
    assert_eq!(read(r"\*not\* \`x\` \a"), r"*not* `x` \a");
}

#[test]
fn a_code_span_closes_on_a_run_of_the_same_length() {
    assert_eq!(read("`a`"), "<c>a</c>");
    assert_eq!(read("``a ` b``"), "<c>a ` b</c>");
    assert_eq!(read("`a``"), "`a``", "a longer run does not close it");
    assert_eq!(read("``a`"), "``a`", "nor a shorter one");
}

#[test]
fn an_unclosed_run_of_ticks_is_literal_and_not_scanned_again() {
    assert_eq!(read("`a **b** c"), "`a <b>b</b> c");
}

#[test]
fn a_space_padded_span_loses_one_space_each_side_and_a_newline_is_a_space() {
    assert_eq!(read("` a `"), "<c>a</c>");
    assert_eq!(read("`  a  `"), "<c> a </c>");
    assert_eq!(read("` `"), "<c> </c>", "two characters or fewer are kept as written");
    assert_eq!(read("`a\nb`"), "<c>a b</c>");
}

// MARKDOWN-7

#[test]
fn strong_emphasis_and_strikethrough() {
    assert_eq!(read("**a** __b__ *c* _d_ ~~e~~"), "<b>a</b> <b>b</b> <i>c</i> <i>d</i> <s>e</s>");
}

#[test]
fn an_opener_wants_a_word_after_it_and_a_closer_one_before_it() {
    assert_eq!(read("** a**"), "** a**");
    assert_eq!(read("**a **"), "**a **");
    assert_eq!(read("2 * 3 * 4"), "2 * 3 * 4");
}

#[test]
fn an_underscore_does_not_open_in_a_word_or_close_before_one() {
    assert_eq!(read("snake_case_name"), "snake_case_name");
    assert_eq!(read("_a_b"), "_a_b");
    assert_eq!(read("_a_ b"), "<i>a</i> b");
}

#[test]
fn a_run_of_three_nests_emphasis_inside_strong() {
    assert_eq!(read("***x***"), "<b><i>x</i></b>");
}

#[test]
fn a_single_marker_steps_over_a_longer_run() {
    assert_eq!(read("*a **b** c*"), "<i>a <b>b</b> c</i>");
}

#[test]
fn nesting_stops_at_the_cap_and_the_rest_is_text() {
    let deep = (0..8).fold("x".to_owned(), |inner, level| {
        let mark = ["*", "_", "~~"][level % 3];
        format!("{mark}a {inner} a{mark}")
    });
    let out = read(&deep);
    assert!(out.contains('x'));
    let mut depth = 0;
    let mut deepest = 0;
    for tag in out.split('<').skip(1) {
        if tag.starts_with('/') {
            depth -= 1;
        } else {
            depth += 1;
            deepest = deepest.max(depth);
        }
    }
    assert!(deepest <= MAX_DEPTH, "{deepest} levels in {out}");
}

#[test]
fn a_long_run_of_unclosed_markers_is_read_in_a_blink() {
    let started = std::time::Instant::now();
    for marker in ["*a ", "_a ", "~~a ", "`a ", "[a ", "**a "] {
        let _ = parse(&marker.repeat(20_000));
    }
    assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
}

// MARKDOWN-8

#[test]
fn a_web_link_keeps_its_label_parsed_and_its_address() {
    assert_eq!(read("[the **docs**](https://a.b/c)"), "<a https://a.b/c>the <b>docs</b></a>");
}

#[test]
fn any_other_address_drops_the_link_and_shows_the_label() {
    assert_eq!(read("[x](javascript:alert(1))"), "x");
    assert_eq!(read("[x](data:text/html,hi)"), "x");
    assert_eq!(read("[x](/relative)"), "x");
    assert_eq!(read("[**x**](ftp://h/f)"), "<b>x</b>");
}

#[test]
fn an_image_is_a_link_and_never_an_image() {
    assert_eq!(read("![alt text](https://a.b/i.png)"), "<a https://a.b/i.png>alt text</a>");
    assert_eq!(read("![alt](javascript:x)"), "alt");
}

#[test]
fn a_link_with_no_label_shows_its_address() {
    assert_eq!(read("[](https://a.b/c)"), "<a https://a.b/c>https://a.b/c</a>");
}

#[test]
fn a_link_is_not_inside_another() {
    assert_eq!(read("[a [b](https://x.y) c](https://z.w)"), "<a https://z.w/>a [b](https://x.y) c</a>");
}

#[test]
fn a_bare_address_is_a_link_whose_text_is_as_written() {
    assert_eq!(read("see https://a.b/c, and HTTP://X.Y/Z."), "see <a https://a.b/c>https://a.b/c</a>, and <a http://x.y/Z>HTTP://X.Y/Z</a>.");
    assert_eq!(read("(https://a.b/c)"), "(<a https://a.b/c>https://a.b/c</a>)");
    assert_eq!(read("xhttps://a.b"), "xhttps://a.b", "only at the start of a word");
}

#[test]
fn raw_html_is_the_text_it_is() {
    assert_eq!(read("<script>alert(1)</script> <b>x</b>"), "<script>alert(1)</script> <b>x</b>");
    assert_eq!(read("<a href=\"javascript:x\">y</a>"), "<a href=\"javascript:x\">y</a>");
}

#[test]
fn a_newline_is_a_hard_break_and_the_spaces_before_it_go() {
    assert_eq!(read("a  \nb"), "a<br>b");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    fn nesting(inlines: &[Inline]) -> usize {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Strong(inner) | Inline::Em(inner) | Inline::Del(inner) => 1 + nesting(inner),
                Inline::Link { children, .. } => 1 + nesting(children),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    fn links(inlines: &[Inline], out: &mut Vec<String>) {
        for inline in inlines {
            match inline {
                Inline::Strong(inner) | Inline::Em(inner) | Inline::Del(inner) => links(inner, out),
                Inline::Link { url, children } => {
                    out.push(url.clone());
                    links(children, out);
                }
                _ => {}
            }
        }
    }

    /// Text made of what the grammar reads: marks, brackets, quotes, addresses and breaks.
    fn text() -> impl Strategy<Value = String> {
        "[a-z0-9 *_~`\\[\\]()!<>\"'\\\\:/.\\n-]{0,160}|(https?://[a-z.]{1,8}/?[a-z)(.]{0,6}[ ]?){0,6}"
    }

    proptest! {
        // MARKDOWN-8: whatever an agent writes, the only link that comes out is a web address with a host.
        #[test]
        fn every_link_is_a_web_address_with_a_host(src in text()) {
            let mut all = Vec::new();
            links(&parse(&src), &mut all);
            for url in all {
                let parsed = url::Url::parse(&url);
                prop_assert!(parsed.is_ok(), "{url}");
                let parsed = parsed.unwrap();
                prop_assert!(matches!(parsed.scheme(), "http" | "https"), "{url}");
                prop_assert!(parsed.host_str().is_some_and(|host| !host.is_empty()), "{url}");
            }
        }

        // MARKDOWN-7, MARKDOWN-9: no input asks for more than `MAX_DEPTH` levels of marks.
        #[test]
        fn marks_never_nest_past_the_cap(src in text()) {
            prop_assert!(nesting(&parse(&src)) <= MAX_DEPTH);
        }

        // MARKDOWN-6: text with no mark in it is read back as it was written, in one piece.
        #[test]
        fn plain_text_is_read_back_unchanged(src in "[a-z0-9][a-z0-9 .,]{0,60}") {
            prop_assert_eq!(parse(&src), vec![Inline::Text(src.clone())]);
        }

        // The windows cut a run by byte ranges: they must be whole characters, in order and apart.
        #[test]
        fn the_stretches_of_a_run_are_whole_characters_in_order(src in text()) {
            let styled = crate::views::markdown::runs::Styled::of(&parse(&src));
            let mut at = 0;
            for (range, _) in &styled.spans {
                prop_assert!(range.start >= at && range.start < range.end && range.end <= styled.text.len());
                prop_assert!(styled.text.is_char_boundary(range.start) && styled.text.is_char_boundary(range.end));
                at = range.end;
            }
        }
    }
}
