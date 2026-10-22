//! What the one field over a checkout's files means by what is typed in it: a name goes to a
//! file, and a leading `#` turns the rest into a search of the checkout's text. The Files panel's
//! filter and the Code stage's find field both take it, and the stage's two lists hand a field
//! over to each other by it, so the rule is one type and not a `starts_with('#')` in each.

/// The text of the field, read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query<'a> {
    /// A name to go to, or to filter names by.
    Name(&'a str),
    /// The text after the `#`, to search the checkout for.
    Text(&'a str),
}

impl<'a> Query<'a> {
    pub fn of(field: &'a str) -> Self {
        match field.strip_prefix('#') {
            Some(text) => Query::Text(text),
            None => Query::Name(field),
        }
    }

    /// What names are filtered by: a search is not a filter, so it filters nothing meanwhile.
    pub fn filter(self) -> &'a str {
        match self {
            Query::Name(name) => name,
            Query::Text(_) => "",
        }
    }

    /// The text Enter would search for: after a `#`, and not blank.
    pub fn to_search(self) -> Option<&'a str> {
        match self {
            Query::Text(text) if !text.trim().is_empty() => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_hash_makes_the_rest_a_search_and_nothing_else_does() {
        assert_eq!(Query::of("#Dragged"), Query::Text("Dragged"));
        assert_eq!(Query::of("Dragged#"), Query::Name("Dragged#"));
        assert_eq!(Query::of(""), Query::Name(""));
        assert_eq!(Query::of("##x"), Query::Text("#x"), "only the first is the mark");
    }

    #[test]
    fn a_search_filters_no_names() {
        assert_eq!(Query::of("#abc").filter(), "");
        assert_eq!(Query::of("abc").filter(), "abc");
    }

    #[test]
    fn enter_searches_only_a_hash_with_something_after_it() {
        assert_eq!(Query::of("#abc").to_search(), Some("abc"));
        assert_eq!(Query::of("#  ").to_search(), None);
        assert_eq!(Query::of("#").to_search(), None);
        assert_eq!(Query::of("abc").to_search(), None);
        assert_eq!(Query::of("# a b").to_search(), Some(" a b"), "the text is not trimmed");
    }
}
