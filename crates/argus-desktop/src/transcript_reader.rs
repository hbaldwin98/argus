//! The full-transcript reader: one session read whole before it is
//! resumed — the person's turns as a jump list and the one it is on, what
//! find looks for and the match it is on — and the page it draws.
//!
//! What was said is the daemon's, held by `sessions`, and drawn as the
//! dialog's preview draws it; the reader only says where in it the person
//! is. The recent-sessions dialog opens it on the session under its cursor
//! and has the keys back when it closes.

use argus_protocol::agents::sessions::Session;
use argus_protocol::agents::transcript::Entry;

use crate::connection::page::{ReaderPage, TranscriptLine, TurnJump};
use crate::sessions_dialog::SessionPreview;

pub struct TranscriptReader {
    session_id: String,
    /// The turn the jump list is on, by its place among the person's
    /// turns; none until one is jumped to, as the reader opens on how the
    /// session ended.
    turn: Option<usize>,
    find: String,
    /// The match find is on, by its place among the matches.
    found: usize,
    /// What the last move was to, which the page scrolls to.
    went: Went,
    /// Counts the moves, so a move to where the page already is still
    /// scrolls it back there.
    moves: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Went {
    Nowhere,
    Turn,
    Match,
}

impl TranscriptReader {
    pub fn new(session_id: String) -> TranscriptReader {
        TranscriptReader { session_id, turn: None, find: String::new(), found: 0, went: Went::Nowhere, moves: 0 }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// `j` and `k`: the next or previous of the person's turns, held at
    /// either end. The first move goes to the first turn down, the last up.
    pub fn move_turn(&mut self, delta: isize, lines: &[TranscriptLine]) {
        let count = turns(lines).len();
        if count == 0 {
            return;
        }
        let to = match self.turn {
            Some(at) => at.saturating_add_signed(delta).min(count - 1),
            None if delta < 0 => count - 1,
            None => 0,
        };
        self.go_to_turn(to);
    }

    pub fn ends(&mut self, last: bool, lines: &[TranscriptLine]) {
        let count = turns(lines).len();
        if count > 0 {
            self.go_to_turn(if last { count - 1 } else { 0 });
        }
    }

    /// A click on the jump list; a turn not in it is not there to click.
    pub fn jump(&mut self, turn: usize, lines: &[TranscriptLine]) {
        if turn < turns(lines).len() {
            self.go_to_turn(turn);
        }
    }

    fn go_to_turn(&mut self, turn: usize) {
        self.turn = Some(turn);
        self.went = Went::Turn;
        self.moves += 1;
    }

    /// What the find field says now: it goes to the first match.
    pub fn set_find(&mut self, find: String, lines: &[TranscriptLine]) {
        self.find = find;
        self.found = 0;
        if matches(&self.find, lines).is_empty() {
            self.went = Went::Nowhere;
        } else {
            self.went = Went::Match;
            self.moves += 1;
        }
    }

    /// `n` and `N`: the next or previous match, round from the last to the
    /// first as find does anywhere.
    pub fn move_match(&mut self, delta: isize, lines: &[TranscriptLine]) {
        let count = matches(&self.find, lines).len();
        if count == 0 {
            return;
        }
        self.found = (self.found.min(count - 1) as isize + delta).rem_euclid(count as isize) as usize;
        self.went = Went::Match;
        self.moves += 1;
    }

    /// The reader over the dialog's preview of the same session, which
    /// holds what it said and what it offers.
    pub fn page(&self, session: &Session, repository: &str, preview: SessionPreview) -> ReaderPage {
        let turns = turns(&preview.entries);
        let matches = matches(&self.find, &preview.entries);
        let turn = self.turn.filter(|&t| t < turns.len());
        let found = (!matches.is_empty()).then(|| self.found.min(matches.len() - 1));
        let target = match self.went {
            Went::Nowhere => None,
            Went::Turn => turn.map(|t| turns[t]),
            Went::Match => found.map(|f| matches[f]),
        };
        ReaderPage {
            repository: repository.to_owned(),
            ended: session.ended.clone(),
            turns: session.turns,
            jumps: turns
                .iter()
                .enumerate()
                .map(|(i, &entry)| TurnJump {
                    entry,
                    text: first_line(&preview.entries[entry].text),
                    at: preview.entries[entry].at.clone(),
                    on: turn == Some(i),
                })
                .collect(),
            find: self.find.clone(),
            found: found.map(|f| matches[f]),
            matches,
            target,
            moves: self.moves,
            session: preview,
        }
    }
}

/// One thing said or done, as the preview and the reader draw it.
pub fn line(entry: &Entry) -> Option<TranscriptLine> {
    let (kind, text, tool, failed, at) = match entry {
        Entry::User { text, at } => ("user", text.clone(), String::new(), false, at),
        Entry::Agent { text, at } => ("agent", text.clone(), String::new(), false, at),
        Entry::Tool { tool, summary, failed, at } => ("tool", summary.clone(), tool.clone(), *failed, at),
        // A kind from a newer daemon: passed over, not shown as nothing.
        Entry::Unknown => return None,
    };
    Some(TranscriptLine { kind, text, tool, failed, at: at.clone() })
}

/// The lines that are the person's turns, by their place in `lines`.
fn turns(lines: &[TranscriptLine]) -> Vec<usize> {
    lines.iter().enumerate().filter(|(_, l)| l.kind == "user").map(|(i, _)| i).collect()
}

/// The lines whose text or tool holds `find`, case aside; none for an
/// empty find.
fn matches(find: &str, lines: &[TranscriptLine]) -> Vec<usize> {
    let find = find.trim().to_lowercase();
    if find.is_empty() {
        return Vec::new();
    }
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.text.to_lowercase().contains(&find) || l.tool.to_lowercase().contains(&find))
        .map(|(i, _)| i)
        .collect()
}

/// A turn as the jump list names it: its first line that says anything.
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(kind: &'static str, text: &str) -> TranscriptLine {
        TranscriptLine { kind, text: text.into(), tool: String::new(), failed: false, at: None }
    }

    /// Three turns, each answered, with a tool call between.
    fn lines() -> Vec<TranscriptLine> {
        vec![
            said("user", "The phone client is gone.\nRemove it."),
            said("agent", "Removing the outbox."),
            TranscriptLine { tool: "Grep".into(), ..said("tool", "notify(") },
            said("user", "Keep the review route"),
            said("agent", "Then notify moves to state/inbox."),
            said("user", "Commit it on main."),
        ]
    }

    fn session() -> Session {
        Session {
            harness: "claude".into(),
            session_id: "s1".into(),
            checkout: argus_protocol::CheckoutId(1),
            removed_worktree: None,
            title: Some("Remove the outbox".into()),
            started: Some("2026-10-14T12:40:00Z".into()),
            ended: Some("2026-10-14T13:52:00Z".into()),
            modified: None,
            turns: Some(48),
            pane: None,
        }
    }

    fn page(reader: &TranscriptReader) -> ReaderPage {
        let preview = SessionPreview {
            title: "Remove the outbox".into(),
            harness: "claude".into(),
            checkout: "main".into(),
            session_id: "s1".into(),
            started: None,
            loading: false,
            notice: None,
            entries: lines(),
            earlier: 0,
            offer: "resume",
            command: None,
            why: None,
            readable: true,
        };
        reader.page(&session(), "argus", preview)
    }

    #[test]
    fn the_jump_list_is_the_persons_turns_by_their_first_line() {
        let page = page(&TranscriptReader::new("s1".into()));
        let jumps: Vec<_> = page.jumps.iter().map(|j| (j.entry, j.text.as_str())).collect();
        assert_eq!(jumps, [(0, "The phone client is gone."), (3, "Keep the review route"), (5, "Commit it on main.")]);
        assert!(page.jumps.iter().all(|j| !j.on), "it opens on how the session ended");
        assert_eq!((page.target, page.turns, page.ended.as_deref()), (None, Some(48), Some("2026-10-14T13:52:00Z")));
    }

    #[test]
    fn a_turn_is_jumped_to_held_at_either_end() {
        let mut reader = TranscriptReader::new("s1".into());
        reader.move_turn(1, &lines());
        assert_eq!(page(&reader).target, Some(0), "down goes to the first");
        reader.move_turn(1, &lines());
        reader.move_turn(5, &lines());
        let on = page(&reader);
        assert_eq!(on.target, Some(5), "held at the last");
        assert!(on.jumps[2].on);
        reader.ends(false, &lines());
        assert_eq!(page(&reader).target, Some(0));
        reader.jump(1, &lines());
        assert_eq!(page(&reader).target, Some(3));
        reader.jump(9, &lines());
        assert_eq!(page(&reader).target, Some(3), "no such turn to click");

        let mut fresh = TranscriptReader::new("s1".into());
        fresh.move_turn(-1, &lines());
        assert_eq!(page(&fresh).target, Some(5), "up from the end goes to the last");
    }

    #[test]
    fn find_goes_to_each_match_in_turn_and_round_again() {
        let mut reader = TranscriptReader::new("s1".into());
        reader.set_find("NOTIFY".into(), &lines());
        let found = page(&reader);
        assert_eq!(found.matches, [2, 4], "a tool's summary and a reply, case aside");
        assert_eq!((found.found, found.target), (Some(2), Some(2)));
        reader.move_match(1, &lines());
        assert_eq!(page(&reader).target, Some(4));
        reader.move_match(1, &lines());
        assert_eq!(page(&reader).target, Some(2), "round from the last to the first");
        reader.move_match(-1, &lines());
        assert_eq!(page(&reader).target, Some(4));
        reader.set_find("grep".into(), &lines());
        assert_eq!(page(&reader).matches, [2], "the tool's name too");
        reader.set_find("nothing like it".into(), &lines());
        let none = page(&reader);
        assert!(none.matches.is_empty() && none.found.is_none() && none.target.is_none());
    }

    #[test]
    fn each_move_is_counted_so_the_page_scrolls_back_to_the_same_place() {
        let mut reader = TranscriptReader::new("s1".into());
        reader.ends(false, &lines());
        let first = page(&reader).moves;
        reader.ends(false, &lines());
        assert_eq!(page(&reader).moves, first + 1);
        assert_eq!(page(&reader).target, Some(0));
    }

    #[test]
    #[ignore]
    fn dump_transcript_reader() {
        // The repo's dump-test convention, as text: there is no screen to
        // render a webview to here.
        let mut reader = TranscriptReader::new("s1".into());
        reader.jump(1, &lines());
        reader.set_find("notify".into(), &lines());
        reader.jump(1, &lines());
        let page = page(&reader);
        let s = &page.session;
        println!("‹ Recent sessions [Esc]   {}  [{}]", s.title, s.harness);
        println!(
            "  {} · {} · {} – {} · {} turns",
            page.repository,
            s.checkout,
            session().started.unwrap(),
            page.ended.as_deref().unwrap_or("?"),
            page.turns.map_or("?".into(), |t| t.to_string())
        );
        let found = page.matches.iter().position(|&m| Some(m) == page.found).map_or(0, |i| i + 1);
        println!(
            "  [find: {}  {found}/{}]   [Copy session id]   [{}]",
            page.find,
            page.matches.len(),
            if s.offer == "resume" { "Resume in a new pane" } else { "Go to its pane" }
        );
        println!("  ─────────────────────────────────────────────");
        let mut jumps = page.jumps.iter();
        for (i, entry) in s.entries.iter().enumerate() {
            let jump = jumps.next().map_or(String::new(), |j| format!("{} {}", if j.on { "▌" } else { " " }, j.text));
            let mark = if Some(i) == page.target { "→" } else if page.matches.contains(&i) { "·" } else { " " };
            let said = match entry.kind {
                "user" => format!("You    {}", entry.text.replace('\n', " / ")),
                "agent" => format!("Agent  {}", entry.text),
                _ => format!("  ● {} {}", entry.tool, entry.text),
            };
            println!("  {jump:<32}│{mark} {said}");
        }
        println!("  (scrolls to entry {:?} on move {})", page.target, page.moves);
    }

    #[test]
    fn a_transcript_with_no_turns_moves_nowhere() {
        let mut reader = TranscriptReader::new("s1".into());
        let agent_only = [said("agent", "hello")];
        reader.move_turn(1, &agent_only);
        reader.ends(true, &agent_only);
        reader.move_match(1, &agent_only);
        assert_eq!(reader.moves, 0);
    }
}
