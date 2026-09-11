//! The feature boards: features, the decisions filed under them, and
//! their tasks, all keyed by artifact rather than by project.

use super::*;

impl Store {
    // ---- decisions -------------------------------------------------

    /// Records one decision on a project's board and returns its id.
    ///
    /// One transaction, because superseding is two writes: the new row,
    /// and the mark on the row it replaces. A board that had gained the
    /// reversal but not the mark would show two live decisions
    /// contradicting each other.
    ///
    /// A superseding decision takes the place of the one it replaces —
    /// its parent, not its children. It is a different answer to the same
    /// question, so it belongs where that question was asked.
    pub fn add_decision(
        &self,
        project: &str,
        write: &DecisionWrite,
        feature: Option<&str>,
        at: i64,
        session: Option<&str>,
        checkout: Option<&str>,
    ) -> Result<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let parent = match write.supersedes {
            Some(old) => tx
                .query_row(
                    "SELECT parent FROM decision WHERE id = ?1 AND project = ?2",
                    rusqlite::params![old, project],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("there is no decision {old} on this board"))?,
            None => write.under,
        };
        if let Some(under) = write.under {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM decision WHERE id = ?1 AND project = ?2)",
                rusqlite::params![under, project],
                |r| r.get(0),
            )?;
            if !exists {
                anyhow::bail!("there is no decision {under} on this board");
            }
        }
        tx.execute(
            "INSERT INTO decision
                 (project, parent, at, session, checkout, feature, chose, over_, because)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                project,
                parent,
                at,
                session,
                checkout,
                feature,
                write.chose,
                write.over,
                write.because
            ],
        )?;
        let id = tx.last_insert_rowid();
        if let Some(old) = write.supersedes {
            tx.execute(
                "UPDATE decision SET superseded_by = ?1 WHERE id = ?2 AND project = ?3",
                rusqlite::params![id, old, project],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// One project's board, oldest first. Unbounded on purpose: a decision
    /// tree with the old half cut off is a tree with no roots, which is
    /// the part that explains the rest.
    pub fn decisions(&self, project: &str) -> Result<Vec<Decision>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent, at, session, checkout, feature, chose, over_, because,
                    superseded_by
             FROM decision WHERE project = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project], |r| {
            Ok(Decision {
                id: r.get(0)?,
                parent: r.get(1)?,
                at: r.get(2)?,
                session: r.get(3)?,
                checkout: r.get(4)?,
                feature: r.get(5)?,
                chose: r.get(6)?,
                over: r.get(7)?,
                because: r.get(8)?,
                superseded_by: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- features ----------------------------------------------------

    /// Opens a feature and returns it with the slug it was given.
    ///
    /// The slug is derived from the title and made unique by suffix inside
    /// the transaction: two agents opening the same-sounding feature on two
    /// branches at the same moment must neither collide on one key nor end
    /// up quietly filing decisions on the same board.
    pub fn add_feature(
        &self,
        project: &str,
        write: &FeatureWrite,
        origin_checkout: Option<&str>,
        origin_branch: Option<&str>,
        at: i64,
        session: Option<&str>,
    ) -> Result<Feature> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let base = slugify(&write.title);
        let mut slug = base.clone();
        for n in 2.. {
            let taken: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                rusqlite::params![project, slug],
                |r| r.get(0),
            )?;
            if !taken {
                break;
            }
            slug = format!("{base}-{n}");
        }
        let feature = Feature {
            slug,
            title: write.title.clone(),
            body: write.body.clone().unwrap_or_default(),
            origin_checkout: origin_checkout.map(str::to_string),
            origin_branch: origin_branch.map(str::to_string),
            at,
            session: session.map(str::to_string),
            state: FeatureState::default(),
            checkouts: Vec::new(),
            tasks: TaskCounts::default(),
        };
        tx.execute(
            "INSERT INTO feature
                 (project, slug, title, body, origin_checkout, origin_branch, at, session)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                project,
                feature.slug,
                feature.title,
                feature.body,
                feature.origin_checkout,
                feature.origin_branch,
                feature.at,
                feature.session
            ],
        )?;
        tx.commit()?;
        Ok(feature)
    }

    /// One artifact scope's features, oldest first.
    pub fn features(&self, project: &str) -> Result<Vec<Feature>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT slug, title, body, origin_checkout, origin_branch, at, session, state
             FROM feature WHERE project = ?1 ORDER BY at, slug",
        )?;
        let rows = stmt.query_map(rusqlite::params![project], |r| {
            let origin_checkout: Option<String> = r.get(3)?;
            Ok(Feature {
                slug: r.get(0)?,
                title: r.get(1)?,
                body: r.get(2)?,
                origin_branch: r.get(4)?,
                at: r.get(5)?,
                session: r.get(6)?,
                // An unrecognised state means a newer Argus wrote this row.
                // Show it as open rather than refusing the whole list.
                state: FeatureState::parse(&r.get::<_, String>(7)?).unwrap_or_default(),
                checkouts: Vec::new(),
                origin_checkout,
                tasks: TaskCounts::default(),
            })
        })?;
        let mut features = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        // Two grouped reads rather than a join per row. Both are what makes
        // a feature list say anything without being opened: the checkouts
        // are how a reader connects a feature to the panes running on it,
        // and the counts are how a row says how far along it is.
        let mut stmt = conn.prepare(
            "SELECT slug, checkout_path FROM artifact_feature_scope WHERE artifact_scope = ?1",
        )?;
        let scopes = stmt
            .query_map(rusqlite::params![project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (slug, path) in scopes {
            if let Some(feature) = features.iter_mut().find(|f| f.slug == slug) {
                if !feature
                    .checkouts
                    .iter()
                    .any(|c| same_path(Path::new(c), Path::new(&path)))
                {
                    feature.checkouts.push(path);
                }
            }
        }

        let mut stmt = conn.prepare(
            "SELECT feature, state, COUNT(*) FROM task WHERE project = ?1 GROUP BY feature, state",
        )?;
        let counts = stmt
            .query_map(rusqlite::params![project], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?.max(0) as usize,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (slug, state, count) in counts {
            let (Some(feature), Some(state)) = (
                features.iter_mut().find(|f| f.slug == slug),
                TaskState::parse(&state),
            ) else {
                continue;
            };
            for _ in 0..count {
                feature.tasks.add(state);
            }
        }
        Ok(features)
    }

    /// Adds a paragraph to a feature's document, and answers with the
    /// document as it now stands.
    ///
    /// The document is the one part of a feature that is not append-only in
    /// the decision-board sense: it is prose both sides write, so it grows
    /// rather than being superseded. Bounded, because a brief that has
    /// grown past a screen has become the design document it was meant to
    /// point at.
    pub fn append_to_feature(&self, project: &str, slug: &str, text: &str) -> Result<String> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let body: String = tx
            .query_row(
                "SELECT body FROM feature WHERE project = ?1 AND slug = ?2",
                rusqlite::params![project, slug],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no feature {slug} on this project"))?;
        let body = if body.trim().is_empty() {
            text.trim().to_string()
        } else {
            format!("{}\n\n{}", body.trim_end(), text.trim())
        };
        if body.len() > argus_protocol::MAX_FEATURE_BODY_BYTES {
            anyhow::bail!("this feature's document is full; what is left belongs in the checkout");
        }
        tx.execute(
            "UPDATE feature SET body = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![body, project, slug],
        )?;
        tx.commit()?;
        Ok(body)
    }

    /// Removes a feature and everything that only makes sense under it.
    ///
    /// Its tasks go with it — a task is a thing somebody meant to do, and
    /// one under a feature that no longer exists is not owed to anyone.
    /// Its decisions do not: the board is append-only because it records
    /// what was believed at the time, which outlives the feature it was
    /// believed about, so they are unfiled and show up on the board's
    /// "before features" row. Any checkout pointed here is unpointed, or
    /// its next `decide` would be refused with no way to see why.
    pub fn remove_feature(&self, project: &str, slug: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "DELETE FROM feature WHERE project = ?1 AND slug = ?2",
            rusqlite::params![project, slug],
        )?;
        if changed == 0 {
            anyhow::bail!("there is no feature {slug} on this project");
        }
        tx.execute(
            "DELETE FROM task WHERE project = ?1 AND feature = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM feature_event WHERE project = ?1 AND slug = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM feature_scope WHERE project = ?1 AND slug = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM artifact_feature_scope WHERE artifact_scope = ?1 AND slug = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "UPDATE decision SET feature = NULL WHERE project = ?1 AND feature = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Renames a feature, leaving its slug alone.
    ///
    /// The slug is what every decision row, task row and `feature_scope`
    /// entry points at, so re-deriving it from the new title would orphan
    /// exactly the work the feature is about. A title is what it is called;
    /// the slug is what it is.
    pub fn rename_feature(&self, project: &str, slug: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("a feature has to have a title");
        }
        if title.len() > argus_protocol::MAX_FEATURE_TITLE_BYTES {
            anyhow::bail!("a feature title is a short noun phrase, not a paragraph");
        }
        self.update_one(
            "UPDATE feature SET title = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![title, project, slug],
            || format!("there is no feature {slug} on this project"),
        )
    }

    /// Replaces a feature's document outright.
    ///
    /// The append path above is what an agent has, because an agent adding
    /// to a brief mid-task should not be able to erase what it is working
    /// from. A human reading the same document needs to be able to fix it.
    pub fn set_feature_body(&self, project: &str, slug: &str, body: &str) -> Result<()> {
        if body.len() > argus_protocol::MAX_FEATURE_BODY_BYTES {
            anyhow::bail!("this feature's document is full; what is left belongs in the checkout");
        }
        self.update_one(
            "UPDATE feature SET body = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![body.trim_end(), project, slug],
            || format!("there is no feature {slug} on this project"),
        )
    }

    /// Points a checkout at a feature, which is what decisions recorded
    /// from it are filed under afterwards.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_feature_scope(&self, checkout: &Path, project: &str, slug: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![project, slug],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {slug} on this project");
        }
        tx.execute(
            "INSERT INTO feature_scope (checkout_path, project, slug) VALUES (?1, ?2, ?3)
             ON CONFLICT(checkout_path) DO UPDATE SET project = excluded.project,
                                                      slug = excluded.slug",
            rusqlite::params![path_text(checkout), project, slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The feature a checkout was last pointed at, if it is still one of
    /// this project's.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn feature_scope(&self, checkout: &Path, project: &str) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT slug FROM feature_scope WHERE checkout_path = ?1 AND project = ?2",
                rusqlite::params![path_text(checkout), project],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Points a checkout at a feature within one durable artifact scope.
    pub fn set_artifact_feature_scope(
        &self,
        checkout: &Path,
        artifact_scope: &str,
        slug: &str,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![artifact_scope, slug],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {slug} in this artifact scope");
        }
        tx.execute(
            "INSERT INTO artifact_feature_scope (artifact_scope, checkout_path, slug)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(artifact_scope, checkout_path) DO UPDATE SET slug = excluded.slug",
            rusqlite::params![artifact_scope, path_text(checkout), slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn artifact_feature_scope(
        &self,
        checkout: &Path,
        artifact_scope: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT slug FROM artifact_feature_scope
                 WHERE artifact_scope = ?1 AND checkout_path = ?2",
                rusqlite::params![artifact_scope, path_text(checkout)],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Moves one repository feature assignment without changing its origin.
    pub fn transfer_artifact_feature_scope(
        &self,
        artifact_scope: &str,
        slug: &str,
        source: &Path,
        destination: &Path,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![artifact_scope, slug],
            |row| row.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {slug} in this artifact scope");
        }
        let removed = tx.execute(
            "DELETE FROM artifact_feature_scope
             WHERE artifact_scope = ?1 AND checkout_path = ?2 AND slug = ?3",
            rusqlite::params![artifact_scope, path_text(source), slug],
        )?;
        if removed == 0 {
            anyhow::bail!("the source checkout is not assigned to feature {slug}");
        }
        tx.execute(
            "INSERT INTO artifact_feature_scope (artifact_scope, checkout_path, slug)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(artifact_scope, checkout_path) DO UPDATE SET slug = excluded.slug",
            rusqlite::params![artifact_scope, path_text(destination), slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Accepts a feature or reopens it, and records who did.
    ///
    /// The state and the event are written in one transaction: a feature
    /// reading `done` with no event saying who accepted it is worse than
    /// either alone, because it looks like an answer.
    pub fn move_feature(&self, project: &str, slug: &str, mv: &FeatureMove) -> Result<Feature> {
        let FeatureMove {
            state,
            detail,
            actor,
            session,
            at,
        } = mv;
        let (state, at) = (*state, *at);
        let detail = detail.as_deref();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![project, slug],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {slug} on this project");
        }
        tx.execute(
            "UPDATE feature SET state = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![state.as_str(), project, slug],
        )?;
        tx.execute(
            "INSERT INTO feature_event (at, project, slug, state, actor, session, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                at,
                project,
                slug,
                state.as_str(),
                actor.as_str(),
                session,
                detail
            ],
        )?;
        tx.commit()?;
        drop(conn);
        self.features(project)?
            .into_iter()
            .find(|f| f.slug == slug)
            .ok_or_else(|| anyhow::anyhow!("there is no feature {slug} on this project"))
    }

    /// Every move a feature has made, oldest first.
    ///
    /// Not read outside tests yet: the events are written now so a board
    /// built on this schema has a history to show, rather than starting
    /// its history the day the view lands.
    #[allow(dead_code)]
    pub fn feature_events(&self, project: &str, slug: &str) -> Result<Vec<FeatureEvent>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, at, slug, state, actor, session, detail
             FROM feature_event WHERE project = ?1 AND slug = ?2 ORDER BY id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project, slug], |r| {
            Ok(FeatureEvent {
                id: r.get(0)?,
                at: r.get(1)?,
                slug: r.get(2)?,
                state: FeatureState::parse(&r.get::<_, String>(3)?).unwrap_or_default(),
                actor: r.get(4)?,
                session: r.get(5)?,
                detail: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- tasks --------------------------------------------------------

    /// Adds a task to the end of a feature's `todo` column.
    ///
    /// The end, because a list an agent is populating from a tracker has
    /// to arrive in the order it was read; a human reorders afterwards.
    pub fn add_task(
        &self,
        project: &str,
        feature: &str,
        write: &TaskWrite,
        at: i64,
        session: Option<&str>,
    ) -> Result<Task> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![project, feature],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {feature} on this project");
        }
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM task WHERE project = ?1 AND feature = ?2",
            rusqlite::params![project, feature],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO task (project, feature, title, state, external, position, at, session)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                project,
                feature,
                write.title,
                TaskState::Todo.as_str(),
                write.external,
                position,
                at,
                session
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Task {
            id,
            feature: feature.to_string(),
            title: write.title.clone(),
            body: None,
            state: TaskState::Todo,
            claimed_by: None,
            external: write.external.clone(),
            position,
            at,
            session: session.map(str::to_string),
        })
    }

    /// One feature's tasks, in the order a human put them in.
    pub fn tasks(&self, project: &str, feature: &str) -> Result<Vec<Task>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, feature, title, body, state, claimed_by, external, position, at, session
             FROM task WHERE project = ?1 AND feature = ?2 ORDER BY position, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project, feature], |r| {
            Ok(Task {
                id: r.get(0)?,
                feature: r.get(1)?,
                title: r.get(2)?,
                body: r.get(3)?,
                state: TaskState::parse(&r.get::<_, String>(4)?).unwrap_or_default(),
                claimed_by: r.get(5)?,
                external: r.get(6)?,
                position: r.get(7)?,
                at: r.get(8)?,
                session: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Moves a task to another column.
    ///
    /// Taking it up is what claims it and finishing it is what releases
    /// it, so a `doing` column always says who is on each card without
    /// anyone having to claim anything by hand.
    pub fn move_task(
        &self,
        project: &str,
        id: i64,
        state: TaskState,
        session: Option<&str>,
    ) -> Result<()> {
        let claimed = match state {
            TaskState::Doing => session,
            TaskState::Todo | TaskState::Done => None,
        };
        self.update_one(
            "UPDATE task SET state = ?1, claimed_by = ?2 WHERE project = ?3 AND id = ?4",
            rusqlite::params![state.as_str(), claimed, project, id],
            || format!("there is no task {id} on this project"),
        )
    }

    /// Rewrites a task's text, leaving where it is and who has it alone.
    pub fn retitle_task(&self, project: &str, id: i64, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("a task has to say what it is");
        }
        if title.len() > argus_protocol::MAX_TASK_TITLE_BYTES {
            anyhow::bail!("a task is a line, not a brief — that belongs in the feature document");
        }
        self.update_one(
            "UPDATE task SET title = ?1 WHERE project = ?2 AND id = ?3",
            rusqlite::params![title, project, id],
            || format!("there is no task {id} on this project"),
        )
    }

    /// Replaces the task's brief without changing its compact board title.
    pub fn set_task_body(&self, project: &str, id: i64, body: String) -> Result<()> {
        let body = checked_task_body(body).map_err(anyhow::Error::msg)?;
        self.update_one(
            "UPDATE task SET body = ?1 WHERE project = ?2 AND id = ?3",
            rusqlite::params![body, project, id],
            || format!("there is no task {id} on this project"),
        )
    }

    /// Removes a task outright.
    ///
    /// Unlike a decision, which is superseded rather than deleted: a
    /// decision is a record of what was believed, and a task is a thing
    /// somebody meant to do. One that was added by mistake should leave
    /// no trace, or a board populated from a tracker fills with apologies.
    pub fn remove_task(&self, project: &str, id: i64) -> Result<()> {
        self.update_one(
            "DELETE FROM task WHERE project = ?1 AND id = ?2",
            rusqlite::params![project, id],
            || format!("there is no task {id} on this project"),
        )
    }

    /// Puts a task at a place in the list, shifting whatever is there
    /// down. Ordering is over the whole feature rather than per column, so
    /// a card keeps its place in the list when it changes column.
    pub fn reorder_task(&self, project: &str, id: i64, to: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let from: i64 = tx
            .query_row(
                "SELECT position FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no task {id} on this project"))?;
        if from == to {
            return Ok(());
        }
        // Everything between the two places shifts one the other way,
        // which is the whole of a move in a dense ordering.
        if to < from {
            tx.execute(
                "UPDATE task SET position = position + 1
                 WHERE project = ?1 AND position >= ?2 AND position < ?3",
                rusqlite::params![project, to, from],
            )?;
        } else {
            tx.execute(
                "UPDATE task SET position = position - 1
                 WHERE project = ?1 AND position > ?2 AND position <= ?3",
                rusqlite::params![project, from, to],
            )?;
        }
        tx.execute(
            "UPDATE task SET position = ?1 WHERE project = ?2 AND id = ?3",
            rusqlite::params![to, project, id],
        )?;
        tx.commit()?;
        Ok(())
    }
}
