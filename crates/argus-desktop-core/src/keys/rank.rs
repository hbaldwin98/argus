//! How the command palette searches: which rows a query keeps, in what order, and which of their
//! characters it marks (the page's `keys/palette.ts`, `fuzzy` and `drawResults`).
//!
//! It knows nothing of what a row is: a row is the group it is in and the text the search matches,
//! and the answer is which rows, by their place in what was given, with the positions to mark.

/// The palette's groups, in the order it draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Commands,
    Panes,
    Features,
    Checkouts,
}

pub const GROUPS: [Group; 4] = [Group::Commands, Group::Panes, Group::Features, Group::Checkouts];

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Commands => "Commands",
            Group::Panes => "Panes",
            Group::Features => "Features",
            Group::Checkouts => "Checkouts",
        }
    }
}

/// Rows a group shows before the search narrows it.
pub const SHOWN: usize = 6;

/// What the search sees of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub group: Group,
    pub text: String,
}

/// A row that stayed, and the characters of its text to mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Its place in the candidates given.
    pub at: usize,
    /// Positions in characters, into the row's whole text.
    pub marks: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// The group a leading `>` or `#` limited the search to.
    pub scope: Option<Group>,
    /// The groups that have rows, in the palette's order.
    pub groups: Vec<(Group, Vec<Hit>)>,
}

impl Ranked {
    /// Every row shown, top to bottom: what the highlight moves through.
    pub fn flat(&self) -> Vec<&Hit> {
        self.groups.iter().flat_map(|(_, hits)| hits).collect()
    }
}

/// Where `needle`'s characters fall in order in `hay`, ignoring case, and how well: runs and word
/// starts score, a late start costs. `None` when they do not all fall.
pub fn fuzzy(needle: &str, hay: &str) -> Option<(f32, Vec<usize>)> {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle: Vec<char> = needle.chars().filter(|c| !c.is_whitespace()).map(fold).collect();
    let hay: Vec<char> = hay.chars().map(fold).collect();
    if needle.is_empty() {
        return Some((0.0, Vec::new()));
    }
    let mut hits: Vec<usize> = Vec::new();
    let mut score = 0.0_f32;
    let mut from = 0;
    for ch in needle {
        let at = from + hay.get(from..)?.iter().position(|&c| c == ch)?;
        if hits.last().is_some_and(|&last| at == last + 1) {
            score += 5.0;
        }
        if at == 0 || hay.get(at - 1).is_some_and(|c| c.is_whitespace() || "/#-_.".contains(*c)) {
            score += 3.0;
        }
        hits.push(at);
        from = at + 1;
    }
    Some((score - hits[0] as f32 * 0.1 - hay.len() as f32 * 0.01, hits))
}

/// What `query` keeps of `rows`. It is trimmed; a leading `>` limits it to Commands and `#` to
/// Features, and the character goes. With no needle each group keeps its own order and shows only
/// its first [`SHOWN`] rows, unless a scope asked for the group, which shows all; with a needle
/// every row that matches shows, best first, ties in their order.
pub fn rank(query: &str, rows: &[Candidate]) -> Ranked {
    let mut needle = query.trim();
    let scope = if needle.starts_with('>') {
        Some(Group::Commands)
    } else if needle.starts_with('#') {
        Some(Group::Features)
    } else {
        None
    };
    if scope.is_some() {
        needle = needle[1..].trim();
    }
    let groups = GROUPS
        .iter()
        .filter(|group| scope.is_none_or(|only| only == **group))
        .filter_map(|&group| {
            let mut found: Vec<(f32, Hit)> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.group == group)
                .filter_map(|(at, row)| {
                    // A feature's text is `#<number> <title>`: under `#` the search starts after it,
                    // and its marks are counted back from there.
                    let past = usize::from(scope == Some(Group::Features) && group == Group::Features);
                    let text: String = row.text.chars().skip(past).collect();
                    let (score, hits) = fuzzy(needle, &text)?;
                    Some((score, Hit { at, marks: hits.into_iter().map(|mark| mark + past).collect() }))
                })
                .collect();
            if !needle.is_empty() {
                found.sort_by(|a, b| b.0.total_cmp(&a.0));
            } else if scope.is_none() {
                found.truncate(SHOWN);
            }
            (!found.is_empty()).then(|| (group, found.into_iter().map(|(_, hit)| hit).collect()))
        })
        .collect();
    Ranked { scope, groups }
}

#[cfg(test)]
mod tests;
