//! The sessions Argus saw agents run under a harness that keeps no
//! transcript: its title, where and when, which is all a listing has of
//! one once its pane is gone.

use super::*;

/// The most sessions kept. A listing shows a repository's newest hundred,
/// and an old one of a harness that keeps nothing has nothing to preview.
const MAX_SEEN: i64 = 1000;

/// A session running in an agent pane, as `saw_sessions` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub harness: String,
    pub session_id: String,
    pub checkout_path: PathBuf,
    /// The pane's title; empty keeps the one recorded before.
    pub title: String,
}

/// One session as Argus last saw it. Times are seconds since the epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenSession {
    pub harness: String,
    pub session_id: String,
    pub checkout_path: PathBuf,
    pub title: String,
    pub started: u64,
    pub ended: u64,
}

impl Store {
    /// Records that each of `sessions` is running at `at`: a session seen
    /// for the first time starts then, and every one ends then until it is
    /// seen again.
    pub fn saw_sessions(&self, sessions: &[Sighting], at: u64) -> Result<()> {
        if sessions.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO seen_session (harness, session_id, checkout_path, title, started, ended)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(harness, session_id) DO UPDATE SET
                   checkout_path = excluded.checkout_path,
                   title = CASE WHEN excluded.title = '' THEN title ELSE excluded.title END,
                   ended = excluded.ended",
            )?;
            for s in sessions {
                stmt.execute(rusqlite::params![s.harness, s.session_id, path_text(&s.checkout_path), s.title, at as i64])?;
            }
        }
        tx.execute(
            "DELETE FROM seen_session WHERE ended < (
               SELECT ended FROM seen_session ORDER BY ended DESC LIMIT 1 OFFSET ?1)",
            [MAX_SEEN - 1],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every session seen, newest first.
    pub fn seen_sessions(&self) -> Result<Vec<SeenSession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT harness, session_id, checkout_path, title, started, ended
               FROM seen_session ORDER BY ended DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SeenSession {
                harness: r.get(0)?,
                session_id: r.get(1)?,
                checkout_path: PathBuf::from(r.get::<_, String>(2)?),
                title: r.get(3)?,
                started: r.get::<_, i64>(4)? as u64,
                ended: r.get::<_, i64>(5)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saw(id: &str, title: &str) -> Sighting {
        Sighting {
            harness: "opencode".into(),
            session_id: id.into(),
            checkout_path: PathBuf::from(r"C:\src\app"),
            title: title.into(),
        }
    }

    #[test]
    fn a_session_keeps_when_it_started_and_moves_its_end() {
        let store = Store::in_memory().unwrap();
        store.saw_sessions(&[saw("s1", "Fix the build")], 100).unwrap();
        store.saw_sessions(&[saw("s1", "")], 160).unwrap();
        let seen = store.seen_sessions().unwrap();
        assert_eq!(
            seen,
            [SeenSession {
                harness: "opencode".into(),
                session_id: "s1".into(),
                checkout_path: PathBuf::from("C:/src/app"),
                title: "Fix the build".into(),
                started: 100,
                ended: 160,
            }]
        );
    }

    #[test]
    fn only_the_newest_sessions_are_kept() {
        let store = Store::in_memory().unwrap();
        for at in 0..(MAX_SEEN as u64 + 5) {
            store.saw_sessions(&[saw(&format!("s{at}"), "t")], at).unwrap();
        }
        let seen = store.seen_sessions().unwrap();
        assert_eq!(seen.len(), MAX_SEEN as usize);
        assert_eq!(seen[0].session_id, format!("s{}", MAX_SEEN + 4));
        assert_eq!(seen.last().unwrap().session_id, "s5");
    }
}
