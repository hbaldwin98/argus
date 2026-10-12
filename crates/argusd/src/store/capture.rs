//! Writing a whole note down as a feature, in one transaction.
//!
//! A note is a title, a brief, the tasks under it and the decisions it
//! already holds. Written a row at a time, a refusal halfway — a task too
//! long, a ninth option too many — would leave half a feature on the board
//! with nothing saying the other half was meant to be there.

use super::boards::{insert_decision, insert_feature, insert_task, Filed};
use super::*;

impl Store {
    /// Opens a feature from `capture`, or with `into` adds the note's tasks
    /// and decisions to that feature and leaves its title and brief alone.
    /// Answers with the slug of the feature written to.
    pub fn capture_feature(
        &self,
        project: &str,
        into: Option<&str>,
        capture: &Capture,
        at: i64,
    ) -> Result<String> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // A note is a person's: no session wrote it and no checkout is its
        // origin, as with a feature opened from the board.
        let filed = Filed {
            project,
            at,
            session: None,
        };
        let slug = match into {
            Some(slug) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                    rusqlite::params![project, slug],
                    |r| r.get(0),
                )?;
                if !exists {
                    anyhow::bail!("there is no feature {slug} on this project");
                }
                slug.to_string()
            }
            None => {
                let write = FeatureWrite {
                    title: capture.title.clone(),
                    body: capture.body.clone(),
                }
                .checked()
                .map_err(anyhow::Error::msg)?;
                insert_feature(&tx, &filed, &write, None, None)?.slug
            }
        };
        for task in &capture.tasks {
            capture_task(&tx, &filed, &slug, None, task)?;
        }
        for decision in &capture.decisions {
            capture_decision(&tx, &filed, &slug, None, decision)?;
        }
        tx.commit()?;
        Ok(slug)
    }
}

fn capture_task(
    tx: &rusqlite::Transaction<'_>,
    filed: &Filed<'_>,
    feature: &str,
    parent: Option<i64>,
    task: &CapturedTask,
) -> Result<()> {
    let write = TaskWrite {
        title: task.title.clone(),
        external: None,
        parent,
    }
    .checked()
    .map_err(anyhow::Error::msg)?;
    let id = insert_task(tx, filed, feature, &write)?.id;
    for decision in &task.decisions {
        capture_decision(tx, filed, feature, Some(id), decision)?;
    }
    for subtask in &task.subtasks {
        capture_task(tx, filed, feature, Some(id), subtask)?;
    }
    Ok(())
}

/// A `?` line is a question with its options; a `!` line was made already
/// and carries its reason.
fn capture_decision(
    tx: &rusqlite::Transaction<'_>,
    filed: &Filed<'_>,
    feature: &str,
    task: Option<i64>,
    decision: &CapturedDecision,
) -> Result<()> {
    let write = if decision.open {
        DecisionWrite {
            chose: decision.chose.clone(),
            state: DecisionState::Open,
            options: decision.options.clone(),
            task,
            ..DecisionWrite::default()
        }
    } else {
        DecisionWrite {
            chose: decision.chose.clone(),
            because: decision.because.clone(),
            task,
            ..DecisionWrite::default()
        }
    };
    let write = write.checked().map_err(anyhow::Error::msg)?;
    insert_decision(tx, filed, &write, Some(feature), None)?;
    Ok(())
}
