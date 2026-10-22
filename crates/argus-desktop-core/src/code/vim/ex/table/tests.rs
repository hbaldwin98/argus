use super::*;

fn name(typed: &str) -> Option<&'static str> {
    find(typed).map(|e| e.name)
}

#[test]
fn a_name_may_be_the_whole_of_it_or_begin_with_its_short_name() {
    assert_eq!(name("write"), Some("write"));
    assert_eq!(name("w"), Some("write"));
    assert_eq!(name("wri"), Some("write"));
    assert_eq!(name("substitute"), Some("substitute"));
    assert_eq!(name("s"), Some("substitute"));
    assert_eq!(name("subst"), Some("substitute"));
    assert_eq!(name("normal"), Some("normal"));
    assert_eq!(name("norm"), Some("normal"));
    assert_eq!(name("noh"), Some("nohlsearch"));
    assert_eq!(name("nohl"), Some("nohlsearch"));
}

#[test]
fn a_name_that_no_short_name_begins_is_none() {
    for typed in ["wq", "x", "q", "e", "quit", "so", "bd", "xyz", "", "wx", "normalx"] {
        assert_eq!(name(typed), None, "{typed}");
    }
}

#[test]
fn the_longest_short_name_that_begins_it_is_the_one() {
    assert_eq!(name("no"), Some("noremap"));
    assert_eq!(name("nm"), Some("nmap"));
    assert_eq!(name("nn"), Some("nnoremap"));
    assert_eq!(name("se"), Some("set"));
    assert_eq!(name("setl"), Some("setlocal"));
    assert_eq!(name("setg"), Some("setglobal"));
    assert_eq!(name("sor"), Some("sort"));
    assert_eq!(name("g"), Some("global"));
    assert_eq!(name("v"), Some("vglobal"));
    assert_eq!(name("d"), Some("delete"));
    assert_eq!(name("delm"), Some("delmarks"));
    assert_eq!(name("reg"), Some("registers"));
    assert_eq!(name("ve"), Some("version"));
    assert_eq!(name("colo"), Some("colorscheme"));
}

#[test]
fn only_the_listings_leave_the_colon_register_alone() {
    let quiet: Vec<_> = TABLE.iter().filter(|e| e.quiet).map(|e| e.name).collect();
    assert_eq!(quiet, vec!["marks", "registers"]);
}

#[test]
fn a_command_is_not_in_the_table_twice() {
    let mut names: Vec<_> = TABLE.iter().map(|e| e.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), TABLE.len());
    // And a short name is no other command's name, which would make it ambiguous.
    for e in TABLE {
        assert!(e.name.starts_with(e.short), "{} is not {}'s start", e.short, e.name);
    }
}
