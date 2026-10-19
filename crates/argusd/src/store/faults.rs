//! Faults injected into the store's one connection, for tests that hold a
//! write to leaving either all of itself or none of it.
//!
//! Every fault goes through SQLite's own hooks on the connection the store
//! already serializes every write through, so the code under test runs
//! unchanged and a fault lands where a real one would: between two
//! statements or two rows, at the commit, on a full disk, or behind another
//! writer's lock. Each is decided by its parameters alone, so a failing case
//! replays exactly.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
use rusqlite::Connection;

use super::Store;

/// What goes wrong, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The write fails once it has taken this many steps, a step being a
    /// statement that writes being prepared or a row being written: zero
    /// fails it before it starts, and a count past its last step fails
    /// whatever it runs next, usually the commit. SQLite rolls back an
    /// explicit transaction a write failed in, as it does after a full disk
    /// or an I/O error.
    AfterSteps(usize),
    /// The first commit is turned into a rollback.
    Commit,
    /// The disk fills: the file may grow by `pages` more pages.
    Full { pages: u32 },
    /// Another connection holds the write lock, and no wait outlasts it.
    /// Needs a store in a file: an in-memory one has no other connection.
    Busy,
}

/// A fault in place until this is dropped.
pub struct Armed<'a> {
    store: &'a Store,
    /// The connection holding the write lock, for [`Fault::Busy`].
    holder: Option<Connection>,
}

/// Counts a write's steps on `conn`, and once `fail_at` of them have been
/// taken fails the next thing it does, once.
///
/// Both counts are needed. Rows alone miss the gap between two statements
/// when the first writes none the update hook sees: `DELETE FROM pane`
/// empties the table without a row each. Statements alone miss the rows
/// of one statement run many times, which is prepared only once.
fn count_steps(conn: &Connection, taken: &Arc<AtomicUsize>, fail_at: Option<usize>) {
    let fired = Arc::new(AtomicBool::new(false));
    let due = {
        let (taken, fired) = (Arc::clone(taken), Arc::clone(&fired));
        move || fail_at.is_some_and(|at| taken.load(Ordering::SeqCst) >= at) && !fired.swap(true, Ordering::SeqCst)
    };
    conn.update_hook(Some({
        let taken = Arc::clone(taken);
        move |_, _: &str, _: &str, _| {
            taken.fetch_add(1, Ordering::SeqCst);
        }
    }))
    .unwrap();
    conn.authorizer(Some({
        let (taken, due) = (Arc::clone(taken), due.clone());
        move |ctx: AuthContext<'_>| match ctx.action {
            // The rollback a failure needs always runs.
            AuthAction::Transaction { operation: TransactionOperation::Rollback } => Authorization::Allow,
            _ if due() => Authorization::Deny,
            AuthAction::Insert { .. } | AuthAction::Update { .. } | AuthAction::Delete { .. } => {
                taken.fetch_add(1, Ordering::SeqCst);
                Authorization::Allow
            }
            _ => Authorization::Allow,
        }
    }))
    .unwrap();
    // Asked between virtual machine steps, which is how a fault lands
    // between two rows of one statement.
    conn.progress_handler(1, Some(due)).unwrap();
}

fn stop_counting(conn: &Connection) {
    let _ = conn.update_hook(None::<fn(rusqlite::hooks::Action, &str, &str, i64)>);
    let _ = conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    let _ = conn.progress_handler(0, None::<fn() -> bool>);
}

impl Store {
    /// Puts `fault` in place on this store's connection.
    pub(crate) fn inject(&self, fault: Fault) -> Armed<'_> {
        let conn = self.conn();
        let mut holder = None;
        match fault {
            Fault::AfterSteps(steps) => count_steps(&conn, &Arc::new(AtomicUsize::new(0)), Some(steps)),
            Fault::Commit => {
                let fired = AtomicBool::new(false);
                conn.commit_hook(Some(move || !fired.swap(true, Ordering::SeqCst))).unwrap();
            }
            Fault::Full { pages } => {
                let used: u32 = conn.pragma_query_value(None, "page_count", |r| r.get(0)).unwrap();
                conn.pragma_update(None, "max_page_count", used + pages).unwrap();
            }
            Fault::Busy => {
                let path: String = conn
                    .query_row("SELECT file FROM pragma_database_list WHERE name = 'main'", [], |r| r.get(0))
                    .unwrap();
                assert!(!path.is_empty(), "a busy fault needs a store in a file");
                let other = Connection::open(&path).unwrap();
                other.execute_batch("BEGIN IMMEDIATE").unwrap();
                conn.busy_timeout(std::time::Duration::ZERO).unwrap();
                holder = Some(other);
            }
        }
        drop(conn);
        Armed { store: self, holder }
    }

    /// Runs `write` and counts its steps, which is how many places
    /// [`Fault::AfterSteps`] can fail it.
    pub(crate) fn steps_taken<R>(&self, write: impl FnOnce(&Store) -> R) -> (R, usize) {
        let taken = Arc::new(AtomicUsize::new(0));
        count_steps(&self.conn(), &taken, None);
        let out = write(self);
        stop_counting(&self.conn());
        (out, taken.load(Ordering::SeqCst))
    }
}

impl Drop for Armed<'_> {
    fn drop(&mut self) {
        if let Some(holder) = self.holder.take() {
            let _ = holder.execute_batch("ROLLBACK");
        }
        let conn = self.store.conn();
        stop_counting(&conn);
        let _ = conn.commit_hook(None::<fn() -> bool>);
        // As large as SQLite allows: the limit only ever stood in for a
        // full disk.
        let _ = conn.pragma_update(None, "max_page_count", 0xFFFF_FFFEu32);
        let _ = conn.busy_timeout(super::BUSY_TIMEOUT);
    }
}
