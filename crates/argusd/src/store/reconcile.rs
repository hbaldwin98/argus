//! A feature's reconcile with its tickets, as stored: the one under way
//! and the last one carried out, each kept whole.

use argus_protocol::{Reconcile, Reconciled};

use super::*;

/// Puts each feature's reconcile on it, as `features` reads them.
pub(super) fn attach(conn: &Connection, project: &str, features: &mut [Feature]) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT slug, reconcile, reconciled FROM feature_reconcile WHERE project = ?1",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![project], |r| {
            let under_way: Option<String> = r.get(1)?;
            let carried: Option<String> = r.get(2)?;
            Ok((
                r.get::<_, String>(0)?,
                under_way.as_deref().map(|raw| decode_row::<Reconcile>(raw, 1)).transpose()?,
                carried.as_deref().map(|raw| decode_row::<Reconciled>(raw, 2)).transpose()?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (slug, reconcile, reconciled) in rows {
        if let Some(feature) = features.iter_mut().find(|f| f.slug == slug) {
            feature.reconcile = reconcile;
            feature.reconciled = reconciled;
        }
    }
    Ok(())
}

impl Store {
    /// One feature's reconcile under way, and the last one carried out.
    pub fn reconcile(&self, project: &str, slug: &str) -> Result<(Option<Reconcile>, Option<Reconciled>)> {
        let row: Option<(Option<String>, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT reconcile, reconciled FROM feature_reconcile WHERE project = ?1 AND slug = ?2",
                rusqlite::params![project, slug],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((under_way, carried)) = row else {
            return Ok((None, None));
        };
        Ok((
            under_way.map(|raw| serde_json::from_str(&raw)).transpose()?,
            carried.map(|raw| serde_json::from_str(&raw)).transpose()?,
        ))
    }

    /// Puts a reconcile under way on a feature, or with `None` takes it
    /// away, leaving the last one carried out as it was.
    pub fn set_reconcile(&self, project: &str, slug: &str, reconcile: Option<&Reconcile>) -> Result<()> {
        let under_way = reconcile.map(encode).transpose()?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        feature_exists(&tx, project, slug)?;
        tx.execute(
            "INSERT INTO feature_reconcile (project, slug, reconcile) VALUES (?1, ?2, ?3)
             ON CONFLICT(project, slug) DO UPDATE SET reconcile = excluded.reconcile",
            rusqlite::params![project, slug, under_way],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Records a reconcile carried out, and what is left of it under way:
    /// the lines that failed, or nothing.
    pub fn finish_reconcile(
        &self,
        project: &str,
        slug: &str,
        left: Option<&Reconcile>,
        reconciled: &Reconciled,
    ) -> Result<()> {
        let (left, reconciled) = (left.map(encode).transpose()?, encode(reconciled)?);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        feature_exists(&tx, project, slug)?;
        tx.execute(
            "INSERT INTO feature_reconcile (project, slug, reconcile, reconciled) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project, slug) DO UPDATE
                 SET reconcile = excluded.reconcile, reconciled = excluded.reconciled",
            rusqlite::params![project, slug, left, reconciled],
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn feature_exists(tx: &rusqlite::Transaction<'_>, project: &str, slug: &str) -> Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
        rusqlite::params![project, slug],
        |r| r.get(0),
    )?;
    if !exists {
        anyhow::bail!("there is no feature {slug} on this project");
    }
    Ok(())
}
