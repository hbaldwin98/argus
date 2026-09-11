//! The panes that were open when the daemon last stopped, so the next
//! run can bring them back.

use super::*;

impl Store {
    /// Replaces the recorded panes with exactly these, in this order.
    ///
    /// A whole-table swap rather than a diff because the tree is the truth
    /// and this follows it: a pane that closed has no row to update.
    pub fn save_panes(&self, panes: &[SessionPane]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM pane", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO pane
                   (seq, checkout_path, kind, title, template, status, note, harness, harness_session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (seq, p) in panes.iter().enumerate() {
                stmt.execute(rusqlite::params![
                    seq as i64,
                    path_text(&p.checkout_path),
                    encode(&p.kind)?,
                    p.title,
                    p.template,
                    encode(&p.status)?,
                    p.note,
                    p.harness,
                    p.harness_session_id,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn panes(&self) -> Result<Vec<SessionPane>> {
        if std::env::var_os(NO_RESTORE).is_some() {
            tracing::info!("{NO_RESTORE} is set; starting with nothing running");
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT checkout_path, kind, title, template, status, note, harness, harness_session_id
               FROM pane ORDER BY seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionPane {
                checkout_path: PathBuf::from(r.get::<_, String>(0)?),
                kind: decode_row(&r.get::<_, String>(1)?, 1)?,
                title: r.get(2)?,
                template: r.get(3)?,
                status: decode_row(&r.get::<_, String>(4)?, 4)?,
                note: r.get(5)?,
                harness: r.get(6)?,
                harness_session_id: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
