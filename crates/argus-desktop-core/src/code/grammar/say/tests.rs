use super::*;
use crate::code::grammar::fetch::Remote;

fn offer(pinned: bool, publisher: Option<&str>, bytes: (u64, u64)) -> Offer {
    Offer {
        name: "kotlin".into(),
        label: "Kotlin".into(),
        source: "tree-sitter-kotlin@1.2.3".into(),
        symbol: "kotlin".into(),
        wasm: Remote { url: String::new(), size: bytes.0, sha256: None },
        queries: vec![Remote { url: String::new(), size: bytes.1, sha256: None }],
        pinned,
        publisher: publisher.map(str::to_owned),
    }
}

#[test]
fn a_size_is_kilobytes_rounded_up_until_a_megabyte() {
    assert_eq!(size(0), "0 KB");
    assert_eq!(size(1), "1 KB");
    assert_eq!(size(1024), "1 KB");
    assert_eq!(size(1025), "2 KB");
    assert_eq!(size(1024 * 1024 - 1), "1024 KB");
    assert_eq!(size(1024 * 1024), "1.0 MB");
    assert_eq!(size(1102547 + 3527), "1.1 MB");
    assert_eq!(size(1536 * 1024), "1.5 MB");
}

#[test]
fn a_pinned_grammar_is_offered_with_where_it_comes_from_and_what_it_costs() {
    let ask = ask(&offer(true, None, (500 * 1024, 0)));
    assert_eq!(ask.question, "No colours for Kotlin yet. Install its tree-sitter grammar? ");
    assert_eq!(ask.from, "tree-sitter-kotlin@1.2.3 from npm, 500 KB");
}

#[test]
fn a_found_grammar_says_who_published_it_and_that_nothing_checked_it() {
    let found = ask(&offer(false, Some("fwcd"), (3000, 100)));
    assert_eq!(found.from, "tree-sitter-kotlin@1.2.3 from npm by fwcd, 4 KB; found by searching, not checked by Argus");
    let unknown = ask(&offer(false, None, (3000, 100)));
    assert_eq!(unknown.from, "tree-sitter-kotlin@1.2.3 from npm, 4 KB; found by searching, not checked by Argus");
}

#[test]
fn what_became_of_installing_is_said_in_the_pages_words() {
    let offer = offer(true, None, (1, 1));
    assert_eq!(installing(&offer), "Installing the Kotlin grammar…");
    assert_eq!(could_not_install(&offer, "boom"), "Could not install tree-sitter-kotlin@1.2.3: boom");
    assert_eq!(could_not_load("boom"), "Could not load the grammar: boom");
}
