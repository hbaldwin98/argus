//! The Markdown of an agent's view, which is text from somewhere nobody controls. Whatever it is,
//! it reads without panicking, a link that comes out is a web address, and the text a window cuts
//! into runs is cut at whole characters and covered once.

#![no_main]

use argus_desktop_core::views::markdown::blocks::parse;
use argus_desktop_core::views::markdown::leaves::leaves;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let blocks = parse(text);
    for leaf in leaves(&blocks) {
        let styled = &leaf.styled;
        for url in &styled.links {
            assert!(url.starts_with("http://") || url.starts_with("https://"), "{url}");
        }
        let mut at = 0;
        for (range, marks) in &styled.spans {
            assert!(range.start >= at && range.start < range.end && range.end <= styled.text.len());
            assert!(styled.text.is_char_boundary(range.start) && styled.text.is_char_boundary(range.end));
            at = range.end;
            if let Some(link) = marks.link {
                assert!(link < styled.links.len());
            }
        }
        let cuts = styled.cuts(Some(0..styled.text.len() / 2));
        assert_eq!(cuts.iter().map(|cut| cut.range.len()).sum::<usize>(), styled.text.len());
    }
});
