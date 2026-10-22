//! The sentences the Checkouts stage says around its table: what the body is before there is a
//! table (loading, no repository, nothing to list), the strip's counts, and the key bar. The
//! singular and plural of each is settled here, once, for every window.

use crate::checkouts::table::Checkouts;

/// What fills the stage under the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// No snapshot has been heard: five loading bars.
    Loading,
    /// No repository is chosen.
    NoRepository,
    /// A repository with nothing to list, and why.
    Nothing(String),
    Table,
}

pub fn body(table: Option<&Checkouts>) -> Body {
    let Some(table) = table else { return Body::Loading };
    if table.repository.is_none() {
        return Body::NoRepository;
    }
    if !table.rows.is_empty() {
        return Body::Table;
    }
    Body::Nothing(if table.filter.is_empty() { "No checkouts.".to_owned() } else { format!("Nothing matches “{}”.", table.filter) })
}

/// The widths of the five loading bars, as shares of the row.
pub const LOADING: [f32; 5] = [0.3, 0.9, 0.85, 0.9, 0.6];

/// The strip's counts beside the title. They are totals: every checkout and every branch no
/// checkout is on, listed or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counts {
    /// `4 checkouts · 9 other branches`.
    pub totals: String,
    /// The repository's main branch, named after ` · primary `.
    pub main: Option<String>,
}

pub fn counts(table: &Checkouts) -> Counts {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    Counts {
        totals: format!("{} · {}", plural(table.checkouts, "checkout", "checkouts"), plural(table.branches, "other branch", "other branches")),
        main: table.main.clone(),
    }
}

/// What the counts' tooltip says.
pub const COUNTS_TIP: &str = "Enter open · D delete";

/// The keys the table answers to, quietly under it: a key and what it does. `E` is left out unless
/// the daemon lists a checkout's folders.
pub fn key_bar(files_offered: bool) -> Vec<(&'static str, &'static str)> {
    [("Enter", "open"), ("R", "review"), ("H", "history"), ("E", "files"), ("P", "pull"), ("N", "new worktree"), ("D", "delete")]
        .into_iter()
        .filter(|(key, _)| *key != "E" || files_offered)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkouts::says::tests::row;

    fn table() -> Checkouts {
        Checkouts { repository: Some("argus".into()), checkouts: 4, branches: 9, main: Some("main".into()), rows: vec![row()], ..Checkouts::default() }
    }

    #[test]
    fn before_a_snapshot_the_body_is_loading_and_with_no_repository_it_says_so() {
        assert_eq!(body(None), Body::Loading);
        assert_eq!(body(Some(&Checkouts::default())), Body::NoRepository);
    }

    #[test]
    fn a_repository_with_no_rows_says_why_by_the_filter() {
        let empty = Checkouts { rows: Vec::new(), ..table() };
        assert_eq!(body(Some(&empty)), Body::Nothing("No checkouts.".into()));
        assert_eq!(body(Some(&Checkouts { filter: "zzz".into(), ..empty })), Body::Nothing("Nothing matches “zzz”.".into()));
        assert_eq!(body(Some(&table())), Body::Table);
    }

    #[test]
    fn the_counts_are_unfiltered_totals_singular_for_one() {
        assert_eq!(counts(&table()), Counts { totals: "4 checkouts · 9 other branches".into(), main: Some("main".into()) });
        let one = Checkouts { checkouts: 1, branches: 1, main: None, ..table() };
        assert_eq!(counts(&one), Counts { totals: "1 checkout · 1 other branch".into(), main: None });
    }

    #[test]
    fn the_key_bar_leaves_files_out_until_the_daemon_offers_them() {
        let keys = |offered| key_bar(offered).into_iter().map(|(key, _)| key).collect::<Vec<_>>();
        assert_eq!(keys(true), ["Enter", "R", "H", "E", "P", "N", "D"]);
        assert_eq!(keys(false), ["Enter", "R", "H", "P", "N", "D"]);
    }
}
