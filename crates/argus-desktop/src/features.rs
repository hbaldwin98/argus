//! What the Feature stage's index shows from a repository's board: which
//! features it lists and in what order, each row, the places a person may move
//! each, and the peek at the one under the cursor.
//!
//! How a feature stands is `argus_protocol::board::standing`'s to say, so this
//! window files a feature under the heading the terminal client does; this
//! only turns those rows into what the page draws. A feature opened from
//! the index is `feature_view`'s.

use argus_protocol::board::standing::{self, FeatureRow, Standing};
use argus_protocol::{DecisionBoard, DiagramList, Feature, FeatureListing, FeatureState, PaneInfo, ProjectInfo, TaskList, TaskState};

use argus_protocol::board::outline::WorkRef;

use crate::feature_view::{self, Opened};
use crate::flows_lens::FlowCursor;
use crate::page::{self, AfterChoice, Agent, FeatureViewLink, Features, Lens, Line, Link, Move, Peek};

/// The repository the index is about and how it is shown.
pub struct Index<'a> {
    pub repository: &'a str,
    pub board: Option<&'a DecisionBoard>,
    pub tree: &'a [ProjectInfo],
    pub listing: &'a Listing,
    /// The feature under the cursor, and its tasks and flows once they
    /// arrived.
    pub peek: Option<&'a str>,
    pub peek_tasks: Option<&'a TaskList>,
    pub peek_diagrams: Option<&'a DiagramList>,
    /// Whether the peeked feature is opened, the lens it is read through,
    /// and where each lens's cursor is.
    pub opened: bool,
    pub lens: Lens,
    pub work: Option<WorkRef>,
    pub record: Option<i64>,
    pub flow: FlowCursor,
    /// The views agents showed in this repository, each with the feature
    /// number it names.
    pub views: &'a [(i64, FeatureViewLink)],
    /// Seconds since the epoch, for how long ago a card last changed.
    pub now: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    #[default]
    List,
    Board,
}

impl Layout {
    pub fn word(self) -> &'static str {
        match self {
            Layout::List => "list",
            Layout::Board => "board",
        }
    }

    pub fn from_word(word: &str) -> Option<Layout> {
        match word {
            "list" => Some(Layout::List),
            "board" => Some(Layout::Board),
            _ => None,
        }
    }
}

/// The board's columns in the order they are drawn and read: the three that
/// follow the work, then the side's Archived over Held.
const COLUMNS: [Standing; 5] = [Standing::Queued, Standing::Active, Standing::Done, Standing::Archived, Standing::Held];

/// Which features the index lists and in what order: the fold, the filter and
/// the layout's order, and the feature opened from it, which is listed whatever
/// they say, so a hold pushed while it is read does not swap it for another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub layout: Layout,
    pub show_folded: bool,
    /// What the filter field says, as typed.
    pub filter: String,
    pub opened: Option<String>,
}

impl Listing {
    /// Whether the fold toggle covers a feature standing so: on the list the
    /// held and archived, on the board only the archived, since Held is a
    /// column a card is dropped on.
    pub fn folds(&self, standing: Standing) -> bool {
        match self.layout {
            Layout::List => standing.folded(),
            Layout::Board => standing.group() == Standing::Archived,
        }
    }

    /// Whether the filter keeps a row: its title holds the words, or its
    /// number is the one typed, with or without `#`. An empty filter keeps all.
    pub fn matches(&self, row: &FeatureRow) -> bool {
        let query = self.filter.trim().to_lowercase();
        let query = query.strip_prefix('#').unwrap_or(&query);
        query.is_empty() || row.title.to_lowercase().contains(query) || row.number.to_string() == query
    }

    /// Whether the index lists the row now.
    pub fn lists(&self, row: &FeatureRow) -> bool {
        self.opened.as_deref() == Some(row.slug.as_str())
            || (!(self.folds(row.standing) && !self.show_folded) && self.matches(row))
    }
}

impl FeatureListing for Listing {
    fn list(&self, rows: Vec<FeatureRow>) -> Vec<FeatureRow> {
        let mut rows: Vec<FeatureRow> = rows.into_iter().filter(|row| self.lists(row)).collect();
        if self.layout == Layout::Board {
            // Stable: within a column the board's own order stays.
            rows.sort_by_key(|row| COLUMNS.iter().position(|c| *c == row.standing.group()));
        }
        rows
    }
}

pub fn build(index: &Index) -> Features {
    let mut features = Features {
        repository: index.repository.to_string(),
        layout: index.listing.layout.word(),
        show_folded: index.listing.show_folded,
        filter: index.listing.filter.clone(),
        loaded: index.board.is_some(),
        ..Features::default()
    };
    let Some(board) = index.board else {
        return features;
    };
    let rows = standing::feature_rows(board, index.tree);
    for row in &rows {
        match row.standing.group() {
            Standing::Active => features.active += 1,
            Standing::Queued => features.queued += 1,
            Standing::Done => features.done += 1,
            Standing::Held => features.held += 1,
            Standing::Archived => features.archived += 1,
            Standing::New => {}
        }
    }
    features.folded = rows.iter().filter(|row| index.listing.folds(row.standing) && index.listing.matches(row)).count();
    features.rows = index
        .listing
        .list(rows.clone())
        .iter()
        .map(|row| {
            let feature = board.features.iter().find(|f| f.slug == row.slug);
            page_row(row, feature, index)
        })
        .collect();
    features.peek = index
        .peek
        .and_then(|slug| rows.iter().find(|row| row.slug == slug))
        .and_then(|row| {
            let feature = board.features.iter().find(|f| f.slug == row.slug)?;
            Some(peek(row, feature, board, index))
        });
    features.open = index
        .peek
        .filter(|_| index.opened)
        .and_then(|slug| Some((rows.iter().find(|row| row.slug == slug)?, board.features.iter().find(|f| f.slug == slug)?)))
        .map(|(row, feature)| {
            feature_view::build(&Opened {
                feature,
                row,
                board,
                tasks: index.peek_tasks,
                diagrams: index.peek_diagrams,
                tree: index.tree,
                lens: index.lens,
                work: index.work,
                record: index.record,
                flow: index.flow,
                views: index.views,
                now: index.now,
            })
        });
    features
}

fn page_row(row: &FeatureRow, feature: Option<&Feature>, index: &Index) -> page::FeatureRow {
    let agent = feature.and_then(|f| loudest_agent(&standing::panes_on(f, index.tree)));
    let peeked = index.peek == Some(row.slug.as_str());
    let footer = footer(row, feature, agent.as_ref(), index);
    let next = index
        .peek_tasks
        .filter(|_| peeked)
        .and_then(|list| standing::next_task(&list.tree_rows()).map(|task| Line { label: task.label(), text: task.task.title.clone(), aside: None }));
    page::FeatureRow {
        slug: row.slug.clone(),
        number: row.number,
        title: row.title.clone(),
        standing: word(row.standing),
        group: word(row.standing.group()),
        tasks_done: row.tasks.done,
        tasks_total: row.tasks.total(),
        decided: row.decided,
        open: row.open,
        branch: row.branch.clone(),
        agent,
        after: row.after.clone(),
        held: feature.and_then(|f| f.held.clone()),
        peeked,
        moves: moves(row),
        footer,
        next,
    }
}

/// A card's footer: how it came to stand where it does, read off the same
/// things its standing is, and how long ago the feature or a decision on
/// it last changed — the board keeps no history of moves, so that is the
/// latest moment it can name.
fn footer(row: &FeatureRow, feature: Option<&Feature>, agent: Option<&Agent>, index: &Index) -> String {
    let how = match row.standing {
        Standing::Archived => "accepted".to_string(),
        Standing::Held => "held".to_string(),
        Standing::Done => "every task done".to_string(),
        Standing::Active => match agent {
            Some(agent) => format!("{} is on it", agent.title),
            None => "a task started".to_string(),
        },
        Standing::Queued if !row.after.is_empty() => {
            let after: Vec<String> = row.after.iter().map(|n| format!("#{n}")).collect();
            format!("after {}", after.join(", "))
        }
        Standing::Queued => "written down".to_string(),
        Standing::New => "written down, nothing in it yet".to_string(),
    };
    let decided = index
        .board
        .into_iter()
        .flat_map(|board| board.decisions.iter())
        .filter(|d| d.feature.as_deref() == Some(row.slug.as_str()))
        .map(|d| d.at);
    let Some(last) = feature.map(|f| f.at).into_iter().chain(decided).max().filter(|at| *at > 0) else {
        return how;
    };
    format!("{how} · {} ago", argus_protocol::ago::ago(Some(last as u64), index.now))
}

pub fn loudest_agent(panes: &[&PaneInfo]) -> Option<Agent> {
    let pane = panes.iter().max_by_key(|pane| pane.loudest_state().status.urgency())?;
    let status = pane.loudest_state().status;
    Some(Agent {
        pane: pane.id.0,
        title: pane.title.clone(),
        status: status.word(),
        needs_you: status.needs_you(),
    })
}

/// The board's places as a card's Move to… menu offers them. Queued,
/// Active and Done follow the work, so they are listed with why they
/// cannot be chosen; Held and Archived are a person's to set, and each
/// becomes its undoing once it is where the feature is.
fn moves(row: &FeatureRow) -> Vec<Move> {
    let here = row.standing.group();
    let derived = |place: Standing, label: &str, reason: String| Move {
        to: word(place),
        label: label.into(),
        disabled: Some(if here == place { "here now".into() } else { reason }),
    };
    let left = row.tasks.total() - row.tasks.done;
    let mut moves = vec![
        derived(Standing::Queued, "Queued", "follows the work".into()),
        derived(Standing::Active, "Active", "when a task starts or an agent arrives".into()),
        derived(
            Standing::Done,
            "Done",
            match left {
                0 if row.tasks.total() == 0 => "when it has tasks and every one is done".into(),
                1 => "when its last task is done".into(),
                n => format!("when its {n} open tasks are done"),
            },
        ),
    ];
    moves.push(match here {
        Standing::Held => Move { to: "unhold", label: "Lift the hold".into(), disabled: None },
        _ => Move { to: "held", label: "Held…".into(), disabled: None },
    });
    moves.push(match here {
        Standing::Archived => Move { to: "reopen", label: "Reopen".into(), disabled: None },
        _ => Move { to: "archived", label: "Archived".into(), disabled: None },
    });
    moves
}

fn peek(row: &FeatureRow, feature: &Feature, board: &DecisionBoard, index: &Index) -> Peek {
    let scoped = board.scoped(Some(&feature.slug));
    let decisions = scoped.tree_rows();
    let panes = standing::panes_on(feature, index.tree);

    let mut needs_you: Vec<Line> = decisions
        .iter()
        .filter(|row| row.decision.open() && !row.decision.withdrawn())
        .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: None })
        .collect();
    needs_you.extend(
        panes
            .iter()
            .filter(|pane| pane.loudest_state().status.needs_you())
            .map(|pane| Line {
                label: "◆".into(),
                text: pane.title.clone(),
                aside: Some(pane.loudest_state().status.word().into()),
            }),
    );

    let tasks = index.peek_tasks.map(TaskList::tree_rows).unwrap_or_default();
    let leaf = |row: &&argus_protocol::TaskTreeRow<'_>| !row.has_children;
    let in_flight = tasks
        .iter()
        .filter(leaf)
        .filter(|row| row.task.state == TaskState::Doing)
        .map(|row| Line {
            label: row.label(),
            text: row.task.title.clone(),
            aside: row
                .task
                .claimed_pane
                .and_then(|id| panes.iter().find(|pane| pane.id == id))
                .map(|pane| pane.title.clone()),
        })
        .collect();
    let next_up = tasks
        .iter()
        .filter(leaf)
        .find(|row| row.task.state == TaskState::Todo)
        .map(|row| Line { label: row.label(), text: row.task.title.clone(), aside: None });
    let last_decided = decisions
        .iter()
        .filter(|row| row.decision.stands())
        .max_by_key(|row| (row.decision.at, row.decision.id))
        .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: None });

    Peek {
        slug: feature.slug.clone(),
        number: feature.number,
        title: feature.title.clone(),
        standing: word(row.standing),
        branch: row.branch.clone(),
        brief: feature.body.clone(),
        held: feature.held.clone(),
        links: feature
            .links
            .iter()
            .map(Link::of)
            .collect(),
        needs_you,
        in_flight,
        next_up,
        last_decided,
        pane: row.pane.map(|pane| pane.0),
        after: after_choices(feature, board),
    }
}

/// What a feature may come after, as the terminal client's `w` offers it:
/// the other open features, and any it already waits on, which choosing
/// takes back.
fn after_choices(feature: &Feature, board: &DecisionBoard) -> Vec<AfterChoice> {
    board
        .features
        .iter()
        .filter(|other| other.slug != feature.slug)
        .map(|other| (other, feature.waits_on.contains(&other.slug)))
        .filter(|(other, waits)| *waits || other.state != FeatureState::Done)
        .map(|(other, waits)| AfterChoice { slug: other.slug.clone(), number: other.number, title: other.title.clone(), waits })
        .collect()
}

pub fn word(standing: Standing) -> &'static str {
    match standing {
        Standing::New => "new",
        Standing::Queued => "queued",
        Standing::Active => "active",
        Standing::Done => "done",
        Standing::Held => "held",
        Standing::Archived => "archived",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    /// Every feature in `standing`'s order, as the list draws them with the
    /// folded shown.
    static SHOWN: Listing = Listing { layout: Layout::List, show_folded: true, filter: String::new(), opened: None };

    fn index<'a>(board: &'a DecisionBoard, tree: &'a [ProjectInfo], peek: Option<&'a str>) -> Index<'a> {
        Index {
            repository: "argus",
            board: Some(board),
            tree,
            listing: &SHOWN,
            peek,
            peek_tasks: None,
            peek_diagrams: None,
            opened: false,
            lens: Lens::Work,
            work: None,
            record: None,
            flow: FlowCursor::default(),
            views: &[],
            now: 3 * 86_400 + 60,
        }
    }

    #[test]
    fn a_card_says_how_it_came_to_stand_there_and_how_long_ago_it_changed() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let features = build(&index(&board, &tree, None));
        let footers: Vec<_> = features.rows.iter().map(|r| (r.number, r.footer.as_str())).collect();
        assert_eq!(
            footers,
            [
                (7, "claude is on it · 3d ago"),
                (8, "after #7 · 3d ago"),
                (10, "written down, nothing in it yet · 3d ago"),
                (6, "every task done · 3d ago"),
                (5, "held · 3d ago"),
                (4, "accepted · 3d ago"),
            ]
        );
    }

    #[test]
    fn only_the_peeked_row_says_what_comes_next_once_its_tasks_arrive() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let tasks = fixtures::tasks("trim");
        let mut idx = index(&board, &tree, Some("trim"));
        assert!(build(&idx).rows[0].next.is_none(), "not yet arrived");
        idx.peek_tasks = Some(&tasks);
        let features = build(&idx);
        let next = features.rows[0].next.as_ref().unwrap();
        assert_eq!((next.label.as_str(), next.text.as_str()), ("1.2", "The hook reads the brief"), "the one being done");
        assert!(features.rows[1..].iter().all(|r| r.next.is_none()));
    }

    #[test]
    fn a_feature_may_come_after_the_open_ones_and_takes_back_one_it_waits_on() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let peek = build(&index(&board, &tree, Some("bump"))).peek.unwrap();
        let offered: Vec<_> = peek.after.iter().map(|c| (c.number, c.waits)).collect();
        assert_eq!(offered, [(5, false), (6, false), (7, true), (10, false)], "not #4, accepted, nor itself");
    }

    #[test]
    fn rows_come_grouped_and_counted_with_held_and_archived_included() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let features = build(&index(&board, &tree, None));
        let order: Vec<_> = features.rows.iter().map(|r| (r.number, r.group)).collect();
        assert_eq!(order, [(7, "active"), (8, "queued"), (10, "queued"), (6, "done"), (5, "held"), (4, "archived")]);
        assert_eq!(
            (features.active, features.queued, features.done, features.held, features.archived),
            (1, 2, 1, 1, 1)
        );
        assert_eq!(features.rows[2].standing, "new");
        assert_eq!(features.rows[1].after, [7]);
        assert_eq!(features.rows[4].held.as_deref(), Some("until the bump lands"));
    }

    #[test]
    fn the_active_feature_names_its_agent() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let features = build(&index(&board, &tree, None));
        let agent = features.rows[0].agent.as_ref().unwrap();
        assert_eq!((agent.title.as_str(), agent.status, agent.needs_you), ("claude", "working", false));
    }

    #[test]
    fn derived_places_cannot_be_chosen_and_say_why() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let features = build(&index(&board, &tree, None));
        let active = &features.rows[0].moves;
        let places: Vec<_> = active.iter().map(|m| (m.to, m.disabled.as_deref())).collect();
        assert_eq!(
            places,
            [
                ("queued", Some("follows the work")),
                ("active", Some("here now")),
                ("done", Some("when its 4 open tasks are done")),
                ("held", None),
                ("archived", None),
            ]
        );
        let held: Vec<_> = features.rows[4].moves.iter().map(|m| m.to).collect();
        assert!(held.contains(&"unhold") && !held.contains(&"held"));
        let archived: Vec<_> = features.rows[5].moves.iter().map(|m| m.to).collect();
        assert!(archived.contains(&"reopen") && !archived.contains(&"archived"));
    }

    #[test]
    fn the_peek_says_what_needs_you_what_is_in_flight_and_what_is_next() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let tasks = fixtures::tasks("trim");
        let mut idx = index(&board, &tree, Some("trim"));
        idx.peek_tasks = Some(&tasks);
        let peek = build(&idx).peek.unwrap();
        assert_eq!(peek.number, 7);
        let needs: Vec<_> = peek.needs_you.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(needs, ["D2"], "the open question");
        assert_eq!(peek.in_flight[0].label, "1.2");
        assert_eq!(peek.in_flight[0].aside.as_deref(), Some("claude"));
        assert_eq!(peek.next_up.as_ref().map(|l| l.label.as_str()), Some("2"));
        assert_eq!(peek.last_decided.as_ref().map(|l| l.label.as_str()), Some("D1"));
        assert_eq!(peek.links[0].key, "ENG-88");
    }

    #[test]
    fn an_opened_feature_is_drawn_in_place_of_the_index() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let tasks = fixtures::tasks("trim");
        let mut idx = index(&board, &tree, Some("trim"));
        idx.peek_tasks = Some(&tasks);
        assert!(build(&idx).open.is_none(), "peeked, not opened");
        idx.opened = true;
        idx.lens = Lens::Record;
        let open = build(&idx).open.unwrap();
        assert_eq!((open.number, open.rows.len(), open.lens), (7, 6, Lens::Record));
        assert!(open.record.is_some());
    }

    #[test]
    fn a_board_not_yet_arrived_is_not_loaded() {
        let tree = fixtures::feature_tree();
        let empty = DecisionBoard::default();
        let mut idx = index(&empty, &tree, None);
        idx.board = None;
        let features = build(&idx);
        assert!(!features.loaded && features.rows.is_empty());
    }

    fn listing(layout: Layout, show_folded: bool, filter: &str, opened: Option<&str>) -> Listing {
        Listing { layout, show_folded, filter: filter.into(), opened: opened.map(str::to_string) }
    }

    /// The fixture's index as `listing` lists it.
    fn built(listing: &Listing) -> Features {
        let (board, tree) = (fixtures::board(), fixtures::feature_tree());
        let mut idx = index(&board, &tree, None);
        idx.listing = listing;
        build(&idx)
    }

    fn listed(listing: &Listing) -> Vec<String> {
        built(listing).rows.into_iter().map(|row| row.slug).collect()
    }

    #[test]
    fn the_list_folds_the_held_and_archived_and_the_board_only_the_archived() {
        let list = listing(Layout::List, false, "", None);
        let board = listing(Layout::Board, false, "", None);
        assert!(list.folds(Standing::Held) && list.folds(Standing::Archived) && !list.folds(Standing::Done));
        assert!(!board.folds(Standing::Held) && board.folds(Standing::Archived) && !board.folds(Standing::New));
        assert_eq!(listed(&list), ["trim", "bump", "desktop", "ctrlc"]);
        assert_eq!(listed(&board), ["bump", "desktop", "trim", "ctrlc", "mouse"], "Held is a column a card is dropped on");
    }

    #[test]
    fn the_board_lists_its_features_column_by_column_as_it_draws_them() {
        assert_eq!(listed(&listing(Layout::Board, true, "", None)), ["bump", "desktop", "trim", "ctrlc", "old", "mouse"]);
        assert!(!listed(&listing(Layout::Board, false, "", None)).contains(&"old".to_string()));
    }

    #[test]
    fn the_feature_opened_is_listed_whatever_the_fold_and_the_filter_say() {
        assert_eq!(listed(&listing(Layout::List, false, "", Some("mouse"))), ["trim", "bump", "desktop", "ctrlc", "mouse"]);
        assert_eq!(listed(&listing(Layout::Board, false, "zz", Some("old"))), ["old"]);
    }

    #[test]
    fn the_fold_counts_what_it_covers_that_the_filter_matches() {
        let folded = |layout, show_folded, filter| built(&listing(layout, show_folded, filter, None)).folded;
        assert_eq!((folded(Layout::List, false, ""), folded(Layout::List, true, "")), (2, 2), "shown or not");
        assert_eq!(folded(Layout::Board, false, ""), 1, "only the archived");
        assert_eq!((folded(Layout::List, false, "mouse"), folded(Layout::Board, false, "mouse")), (1, 0));
        let index = built(&listing(Layout::List, false, "mouse", None));
        assert_eq!((index.held, index.archived, index.active), (1, 1, 1), "the heading counts the whole board");
    }

    #[test]
    fn the_filter_keeps_titles_holding_its_words_and_the_number_typed_with_or_without_a_hash() {
        let filtered = |filter| listed(&listing(Layout::List, true, filter, None));
        assert_eq!(filtered("RATATUI"), ["bump"]);
        assert_eq!((filtered("#6"), filtered("6")), (vec!["ctrlc".to_string()], vec!["ctrlc".to_string()]));
        assert_eq!(filtered("  ").len(), 6, "a blank filter keeps every row");
    }

    #[test]
    #[ignore]
    fn dump_feature_index() {
        let board = fixtures::board();
        let tree = fixtures::feature_tree();
        let tasks = fixtures::tasks("trim");
        let mut idx = index(&board, &tree, Some("trim"));
        idx.peek_tasks = Some(&tasks);
        let f = build(&idx);
        println!("Features in {}  {} active · {} queued · {} done", f.repository, f.active, f.queued, f.done);
        let mut group = "";
        for r in &f.rows {
            if r.group != group {
                group = r.group;
                println!("  {}", group.to_uppercase());
            }
            let agent = r.agent.as_ref().map(|a| format!(" ◆ {} {}", a.title, a.status)).unwrap_or_default();
            println!("    #{} {}  {}/{}  {}{}", r.number, r.title, r.tasks_done, r.tasks_total, r.branch.clone().unwrap_or_default(), agent);
        }
        let p = f.peek.unwrap();
        println!("peek #{} {}: needs you {:?}, in flight {:?}, next {:?}", p.number, p.title,
            p.needs_you.iter().map(|l| &l.label).collect::<Vec<_>>(),
            p.in_flight.iter().map(|l| &l.label).collect::<Vec<_>>(),
            p.next_up.map(|l| l.label));
    }
}
