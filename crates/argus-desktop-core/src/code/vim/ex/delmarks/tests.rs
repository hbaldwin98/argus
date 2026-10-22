use crate::code::vim::tests::Ed;

fn marks_after(commands: &str) -> String {
    let mut ed = Ed::new("|a");
    ed.keys("mamb mc mD".replace(' ', "").as_str());
    ed.keys(commands);
    ed.keys(":marks<CR>");
    ed.vim.message().unwrap().to_owned()
}

#[test]
fn a_letter_a_list_of_them_and_a_range_remove_marks() {
    let say = marks_after(":delm a<CR>");
    assert!(!say.contains("\na\t") && say.contains("\nb\t"), "{say}");
    let say = marks_after(":delm a b<CR>");
    assert!(!say.contains("\na\t") && !say.contains("\nb\t") && say.contains("\nc\t"), "{say}");
    let say = marks_after(":delm a-c<CR>");
    assert!(!say.contains("\na\t") && !say.contains("\nc\t") && say.contains("\nD\t"), "{say}");
    let say = marks_after(":delm ac<CR>");
    assert!(!say.contains("\na\t") && say.contains("\nb\t") && !say.contains("\nc\t"), "{say}");
}

#[test]
fn a_range_of_capitals_is_a_range_and_one_of_both_cases_is_not() {
    let say = marks_after(":delm A-D<CR>");
    assert!(!say.contains("\nD\t") && say.contains("\na\t"), "{say}");
    let mut ed = Ed::new("|a");
    ed.keys(":delm a-D<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: a-"));
}

#[test]
fn an_empty_range_or_a_reversed_one_or_a_non_letter_is_invalid_and_stops_there() {
    for (arg, said) in [("b-a", "Invalid argument: b-a"), ("a-a", "Invalid argument: a-a"), ("1", "Invalid argument: 1"), ("a-", "Invalid argument: a-")] {
        let mut ed = Ed::new("|a");
        ed.keys(&format!(":delm {arg}<CR>"));
        assert_eq!(ed.vim.message(), Some(said), "{arg}");
    }
    let mut ed = Ed::new("|a");
    ed.keys("ma:delm a 1<CR>:marks<CR>");
    assert!(!ed.vim.message().unwrap().contains("\na\t"), "the letter before the bad one went");
}

#[test]
fn nothing_after_it_asks_for_an_argument() {
    let mut ed = Ed::new("|a");
    ed.keys(":delm<CR>");
    assert_eq!(ed.vim.message(), Some("Argument required"));
    ed.keys(":delm   <CR>");
    assert_eq!(ed.vim.message(), Some("Argument required"));
}
