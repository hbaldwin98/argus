//! Review comments: feedback left against a checkout's changes, kept
//! until the agent working there reads it.

use super::*;

impl Store {
    pub fn add_review_comment(
        &self,
        checkout_path: &Path,
        anchor: ReviewAnchor,
        body: String,
    ) -> Result<ReviewComment> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO review_comment (checkout_path, anchor, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![path_text(checkout_path), encode(&anchor)?, body],
        )?;
        Ok(ReviewComment {
            id: conn.last_insert_rowid() as u64,
            anchor,
            body,
        })
    }

    /// The newest bounded window, returned oldest-first so it reads as a
    /// conversation rather than in reverse database order.
    pub fn review_comments(&self, checkout_path: &Path) -> Result<Vec<ReviewComment>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, anchor, body FROM (
                 SELECT id, anchor, body FROM review_comment
                  WHERE checkout_path = ?1 ORDER BY id DESC LIMIT ?2
             ) ORDER BY id",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![path_text(checkout_path), MAX_REVIEW_COMMENTS as i64],
            |r| {
                Ok(ReviewComment {
                    id: r.get::<_, i64>(0)? as u64,
                    anchor: decode_row(&r.get::<_, String>(1)?, 1)?,
                    body: r.get(2)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
