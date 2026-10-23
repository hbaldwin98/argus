//! A review being written: drafts kept against a checkout until the
//! person sends them, when they become its review comments in one step.

use argus_protocol::{ReviewDraft, MAX_REVIEW_DRAFTS};

use super::*;

impl Store {
    /// Refused once the checkout holds as many drafts as are ever shown.
    pub fn add_review_draft(&self, checkout_path: &Path, anchor: &ReviewAnchor, body: &str) -> Result<u64> {
        let conn = self.conn();
        let held: i64 = conn.query_row(
            "SELECT COUNT(*) FROM review_draft WHERE checkout_path = ?1",
            [path_text(checkout_path)],
            |r| r.get(0),
        )?;
        if held as usize >= MAX_REVIEW_DRAFTS {
            anyhow::bail!("a review holds at most {MAX_REVIEW_DRAFTS} comments; send it first");
        }
        conn.execute(
            "INSERT INTO review_draft (checkout_path, anchor, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![path_text(checkout_path), encode(anchor)?, body],
        )?;
        Ok(conn.last_insert_rowid() as u64)
    }

    /// Whether a draft of this checkout's had that id.
    pub fn edit_review_draft(&self, checkout_path: &Path, id: u64, body: &str) -> Result<bool> {
        let changed = self.conn().execute(
            "UPDATE review_draft SET body = ?3 WHERE checkout_path = ?1 AND id = ?2",
            rusqlite::params![path_text(checkout_path), id as i64, body],
        )?;
        Ok(changed > 0)
    }

    /// Whether a draft of this checkout's had that id.
    pub fn delete_review_draft(&self, checkout_path: &Path, id: u64) -> Result<bool> {
        let changed = self.conn().execute(
            "DELETE FROM review_draft WHERE checkout_path = ?1 AND id = ?2",
            rusqlite::params![path_text(checkout_path), id as i64],
        )?;
        Ok(changed > 0)
    }

    pub fn discard_review_drafts(&self, checkout_path: &Path) -> Result<()> {
        self.conn()
            .execute("DELETE FROM review_draft WHERE checkout_path = ?1", [path_text(checkout_path)])?;
        Ok(())
    }

    /// Oldest first: the order they were written in, as the review reads.
    pub fn review_drafts(&self, checkout_path: &Path) -> Result<Vec<ReviewDraft>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT id, anchor, body FROM review_draft WHERE checkout_path = ?1 ORDER BY id")?;
        let rows = stmt.query_map([path_text(checkout_path)], |r| {
            Ok(ReviewDraft {
                id: r.get::<_, i64>(0)? as u64,
                anchor: decode_row(&r.get::<_, String>(1)?, 1)?,
                body: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every draft made a review comment and dropped, in one transaction,
    /// so a review is sent whole or not at all. The comments come back in
    /// the drafts' order.
    pub fn send_review_drafts(&self, checkout_path: &Path) -> Result<Vec<ReviewComment>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let key = path_text(checkout_path);
        let drafts: Vec<(i64, String, String)> = {
            let mut stmt =
                tx.prepare("SELECT id, anchor, body FROM review_draft WHERE checkout_path = ?1 ORDER BY id")?;
            let rows = stmt.query_map([&key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut sent = Vec::with_capacity(drafts.len());
        for (_, anchor, body) in drafts {
            tx.execute(
                "INSERT INTO review_comment (checkout_path, anchor, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![key, anchor, body],
            )?;
            sent.push(ReviewComment {
                id: tx.last_insert_rowid() as u64,
                anchor: decode_row(&anchor, 1)?,
                body,
            });
        }
        tx.execute("DELETE FROM review_draft WHERE checkout_path = ?1", [&key])?;
        tx.commit()?;
        Ok(sent)
    }
}
