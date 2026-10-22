//! What a motion needs of the editor beyond the text: the column `j` and `k`
//! aim for, the last find, the window, the marks and the search.

use super::super::marks::Marks;
use super::super::screen::View;
use super::super::search::pattern::Query;
use super::Find;

/// What a motion needs of the editor beyond the text.
pub struct Aim<'a> {
    /// The column `j` and `k` are aiming for.
    pub want: usize,
    /// The last `f`, `F`, `t` or `T`, which `;` and `,` repeat.
    pub last_find: &'a mut Option<(Find, char)>,
    /// An operator is waiting: `l` may then reach the line's end, which a
    /// bare cursor cannot rest on.
    pub operator: bool,
    /// What the window shows, within the text.
    pub view: View,
    /// How many characters a row of a wrapped line holds, if it wraps.
    pub wrap: Option<usize>,
    pub marks: &'a Marks,
    /// The search `n` repeats, and whether `?` made it run backward.
    pub search: Option<&'a Query>,
    pub reversed: bool,
    /// Whether the search is a JavaScript pattern, for what a miss says.
    pub pcre: bool,
    /// What a motion that failed has to say.
    pub message: &'a mut Option<String>,
    /// A count was typed.
    pub explicit: bool,
}

impl<'a> Aim<'a> {
    /// What a motion is aimed with when the window, the marks and the search
    /// are not in it: the whole text in view, no wrap, no search.
    pub fn new(
        want: usize,
        last_find: &'a mut Option<(Find, char)>,
        operator: bool,
        marks: &'a Marks,
        message: &'a mut Option<String>,
    ) -> Aim<'a> {
        Aim {
            want,
            last_find,
            operator,
            view: View::default(),
            wrap: None,
            marks,
            search: None,
            reversed: false,
            pcre: true,
            message,
            explicit: false,
        }
    }
}
