//! The feature boards: features, the decisions filed under them, their
//! tasks, and the tracker map beside them, all keyed by artifact rather
//! than by project.

use super::*;

impl Store {
    // ---- decisions -------------------------------------------------

    /// Records one decision on a project's board and returns its id.
    ///
    /// One transaction, because superseding is two writes: the new row,
    /// and the mark on the row it replaces. A board that had gained the
    /// reversal but not the mark would show two live decisions
    /// contradicting each other.
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
        let filed = Filed {
            project,
            at,
            session,
        };
        let id = insert_decision(&tx, &filed, write, feature, checkout)?;
        tx.commit()?;
        Ok(id)
    }

    /// One project's board, oldest first. Unbounded on purpose: a decision
    /// tree with the old half cut off is a tree with no roots, which is
    /// the part that explains the rest.
    pub fn decisions(&self, project: &str) -> Result<Vec<Decision>> {
        read_decisions(&self.conn(), project)
    }

    /// Marks a decision withdrawn at `at`, or restores it with `None`.
    pub fn set_decision_withdrawn(&self, project: &str, id: i64, at: Option<i64>) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE decision SET withdrawn_at = ?3 WHERE project = ?1 AND id = ?2",
            rusqlite::params![project, id, at],
        )?;
        if changed == 0 {
            anyhow::bail!("there is no decision {id} on this board");
        }
        Ok(())
    }

    /// Answers an open decision: it is accepted, `answer` becomes what was
    /// decided in full, and the options not taken become what it was chosen
    /// over unless it already says.
    ///
    /// Only a question can be answered. A decision already made is replaced
    /// by superseding it, which keeps what was believed; answering it again
    /// would overwrite that in place.
    pub fn resolve_decision(
        &self,
        project: &str,
        id: i64,
        answer: &str,
        because: Option<&str>,
    ) -> Result<()> {
        let answer = answer.trim();
        let because = because.map(str::trim).filter(|b| !b.is_empty());
        if answer.is_empty() {
            anyhow::bail!("resolving a decision has to say what was decided");
        }
        if answer.len().max(because.map_or(0, str::len)) > MAX_DECISION_BYTES {
            anyhow::bail!("a decision is a sentence or two per field, not a document");
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let decision = read_decisions(&tx, project)?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("there is no decision {id} on this board"))?;
        if !decision.open() {
            anyhow::bail!(
                "decision {id} is not an open question; supersede a decision that was made"
            );
        }
        let passed_over = decision
            .options
            .iter()
            .filter(|option| !option.trim().eq_ignore_ascii_case(answer))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("; ");
        let over = decision
            .over
            .or_else(|| (!passed_over.is_empty()).then(|| bounded(passed_over)));
        tx.execute(
            "UPDATE decision
             SET state = ?1, detail = ?2, because = COALESCE(?3, because), over_ = ?4,
                 options = '[]'
             WHERE project = ?5 AND id = ?6",
            rusqlite::params![
                DecisionState::Accepted.as_str(),
                answer,
                because,
                over,
                project,
                id
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Replaces what a decision says, leaving where it hangs, how it stands
    /// and who recorded it alone.
    ///
    /// Options are what an open decision is choosing between, so they are
    /// kept only on one: a made decision says what it was chosen over.
    pub fn rewrite_decision(&self, project: &str, id: i64, text: DecisionText) -> Result<()> {
        let text = text.checked().map_err(anyhow::Error::msg)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let decision = read_decisions(&tx, project)?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("there is no decision {id} on this board"))?;
        let options = if decision.open() { text.options } else { Vec::new() };
        tx.execute(
            "UPDATE decision
             SET chose = ?1, context = ?2, detail = ?3, because = ?4, over_ = ?5,
                 consequences = ?6, options = ?7
             WHERE project = ?8 AND id = ?9",
            rusqlite::params![
                text.chose,
                text.context,
                text.detail,
                text.because,
                text.over,
                encode(&text.consequences)?,
                encode(&options)?,
                project,
                id
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---- features ----------------------------------------------------

    /// Opens a feature and returns it with the slug and number it was given.
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
        let filed = Filed {
            project,
            at,
            session,
        };
        let feature = insert_feature(&tx, &filed, write, origin_checkout, origin_branch)?;
        tx.commit()?;
        Ok(feature)
    }

    /// One artifact scope's features, oldest first.
    pub fn features(&self, project: &str) -> Result<Vec<Feature>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT slug, title, body, origin_checkout, origin_branch, at, session, state, held,
                    number
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
                held: r.get(8)?,
                waits_on: Vec::new(),
                links: Vec::new(),
                number: r.get(9)?,
            })
        })?;
        let mut features = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut stmt = conn.prepare(
            "SELECT slug, waits_on FROM feature_wait WHERE project = ?1 ORDER BY slug, waits_on",
        )?;
        let waits = stmt
            .query_map(rusqlite::params![project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (slug, on) in waits {
            if let Some(feature) = features.iter_mut().find(|f| f.slug == slug) {
                feature.waits_on.push(on);
            }
        }

        let mut stmt = conn.prepare(
            "SELECT slug, tracker, key, url FROM feature_link WHERE project = ?1 ORDER BY slug, place",
        )?;
        let links = stmt
            .query_map(rusqlite::params![project], |r| {
                let link = TrackerLink { tracker: r.get(1)?, key: r.get(2)?, url: r.get(3)? };
                Ok((r.get::<_, String>(0)?, link))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (slug, link) in links {
            if let Some(feature) = features.iter_mut().find(|f| f.slug == slug) {
                feature.links.push(link);
            }
        }

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
    /// Its decisions do not: the board keeps every decision because it records
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
            "DELETE FROM task_note WHERE task IN
                 (SELECT id FROM task WHERE project = ?1 AND feature = ?2)",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM task WHERE project = ?1 AND feature = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM sequence_diagram WHERE project = ?1 AND feature = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM feature_event WHERE project = ?1 AND slug = ?2",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM feature_wait WHERE project = ?1 AND (slug = ?2 OR waits_on = ?2)",
            rusqlite::params![project, slug],
        )?;
        tx.execute(
            "DELETE FROM feature_link WHERE project = ?1 AND slug = ?2",
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
        // The tasks they were raised under went with the feature.
        tx.execute(
            "UPDATE decision SET feature = NULL, task = NULL WHERE project = ?1 AND feature = ?2",
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

    /// Replaces a feature's document outright: how a person fixes a brief,
    /// and how an agent takes back a paragraph it appended.
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

    /// Holds a feature with `reason`, or lifts its hold with `None`.
    pub fn hold_feature(&self, project: &str, slug: &str, reason: Option<&str>) -> Result<()> {
        let reason = reason.map(str::trim);
        if reason.is_some_and(str::is_empty) {
            anyhow::bail!("a hold has to say why");
        }
        if reason.is_some_and(|r| r.len() > argus_protocol::MAX_HOLD_BYTES) {
            anyhow::bail!("a hold says why in a line; the rest belongs in the brief");
        }
        self.update_one(
            "UPDATE feature SET held = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![reason, project, slug],
            || format!("there is no feature {slug} on this project"),
        )
    }

    /// Records that `slug` comes after `on`, both on this board.
    ///
    /// Refused when `on` already comes after `slug`, however far round: a
    /// loop is two features each waiting for the other to be accepted,
    /// which neither ever will be.
    pub fn add_feature_wait(&self, project: &str, slug: &str, on: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for name in [slug, on] {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                rusqlite::params![project, name],
                |r| r.get(0),
            )?;
            if !exists {
                anyhow::bail!("there is no feature {name} on this project");
            }
        }
        if slug == on {
            anyhow::bail!("a feature cannot wait on itself");
        }
        let mut stmt = tx.prepare("SELECT slug, waits_on FROM feature_wait WHERE project = ?1")?;
        let waits = stmt
            .query_map(rusqlite::params![project], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let mut reached = vec![on.to_string()];
        let mut seen = 0;
        while seen < reached.len() {
            let next: Vec<String> = waits
                .iter()
                .filter(|(from, to)| *from == reached[seen] && !reached.contains(to))
                .map(|(_, to)| to.clone())
                .collect();
            if next.iter().any(|to| to == slug) {
                anyhow::bail!("{on} already comes after {slug}, so {slug} cannot wait on it");
            }
            reached.extend(next);
            seen += 1;
        }
        tx.execute(
            "INSERT OR IGNORE INTO feature_wait (project, slug, waits_on) VALUES (?1, ?2, ?3)",
            rusqlite::params![project, slug, on],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Takes back that `slug` comes after `on`.
    pub fn remove_feature_wait(&self, project: &str, slug: &str, on: &str) -> Result<()> {
        self.update_one(
            "DELETE FROM feature_wait WHERE project = ?1 AND slug = ?2 AND waits_on = ?3",
            rusqlite::params![project, slug, on],
            || format!("{slug} does not wait on {on}"),
        )
    }

    /// Links a feature to a ticket. Linking a key it already has replaces
    /// that link's tracker and URL and keeps its place, so correcting a
    /// mistyped URL is the same command as making the link.
    pub fn link_feature(&self, project: &str, slug: &str, link: &TrackerLink) -> Result<()> {
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
        let (count, relinked): (i64, bool) = tx.query_row(
            "SELECT COUNT(*), COALESCE(SUM(key = ?3), 0) > 0
             FROM feature_link WHERE project = ?1 AND slug = ?2",
            rusqlite::params![project, slug, link.key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !relinked && count >= MAX_FEATURE_LINKS as i64 {
            anyhow::bail!("{slug} is linked to {MAX_FEATURE_LINKS} tickets already; unlink one first");
        }
        tx.execute(
            "INSERT INTO feature_link (project, slug, key, tracker, url, place)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     (SELECT COALESCE(MAX(place), 0) + 1 FROM feature_link
                      WHERE project = ?1 AND slug = ?2))
             ON CONFLICT(project, slug, key) DO UPDATE SET tracker = excluded.tracker, url = excluded.url",
            rusqlite::params![project, slug, link.key, link.tracker, link.url],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Takes a feature's link to `key` away. The key is matched without
    /// regard to case, since `eng-88` is how it is often typed.
    pub fn unlink_feature(&self, project: &str, slug: &str, key: &str) -> Result<()> {
        self.update_one(
            "DELETE FROM feature_link WHERE project = ?1 AND slug = ?2 AND key = ?3 COLLATE NOCASE",
            rusqlite::params![project, slug, key.trim()],
            || format!("{slug} is not linked to {}", key.trim()),
        )
    }

    // ---- the tracker map -------------------------------------------

    /// The project's tracker map, empty when nobody has written one.
    pub fn tracker_map(&self, project: &str) -> Result<TrackerMap> {
        let row = self
            .conn()
            .query_row(
                "SELECT body, draft, drafted_by, drafted_at FROM tracker_map WHERE project = ?1",
                [project],
                |r| {
                    let draft = r.get::<_, Option<String>>(1)?.map(|body| -> rusqlite::Result<_> {
                        Ok(MapDraft {
                            body,
                            by: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                            at: r.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                        })
                    });
                    Ok(TrackerMap {
                        body: r.get(0)?,
                        draft: draft.transpose()?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_default())
    }

    /// The map as the person wrote it. A waiting draft is left waiting.
    pub fn set_tracker_map(&self, project: &str, body: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO tracker_map (project, body) VALUES (?1, ?2)
             ON CONFLICT(project) DO UPDATE SET body = excluded.body",
            rusqlite::params![project, body],
        )?;
        Ok(())
    }

    /// An agent's proposed map. It replaces any earlier draft, and the
    /// map itself is not touched.
    pub fn draft_tracker_map(&self, project: &str, body: &str, by: &str, at: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO tracker_map (project, draft, drafted_by, drafted_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project) DO UPDATE SET draft = excluded.draft,
                 drafted_by = excluded.drafted_by, drafted_at = excluded.drafted_at",
            rusqlite::params![project, body, by, at],
        )?;
        Ok(())
    }

    /// The draft approved as `body`, which becomes the map. Refused when
    /// there is no draft: one turned down from another view is not
    /// approved by a key pressed on a stale screen.
    pub fn approve_tracker_map(&self, project: &str, body: &str) -> Result<()> {
        self.update_one(
            "UPDATE tracker_map SET body = ?2, draft = NULL, drafted_by = NULL, drafted_at = NULL
             WHERE project = ?1 AND draft IS NOT NULL",
            rusqlite::params![project, body],
            || "there is no draft of the tracker map to approve".to_string(),
        )
    }

    pub fn discard_tracker_map_draft(&self, project: &str) -> Result<()> {
        self.update_one(
            "UPDATE tracker_map SET draft = NULL, drafted_by = NULL, drafted_at = NULL
             WHERE project = ?1 AND draft IS NOT NULL",
            [project],
            || "there is no draft of the tracker map to turn down".to_string(),
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

    /// Adds a task to the end of its parent's sibling list, or to the end
    /// of the feature's root list when it has no parent. The end is useful
    /// for a list an agent is populating from a tracker; a human can reorder
    /// siblings afterwards without disturbing the rest of the tree.
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
        let filed = Filed {
            project,
            at,
            session,
        };
        let task = insert_task(&tx, &filed, feature, write)?;
        tx.commit()?;
        Ok(task)
    }

    /// One feature's tasks, in depth-first order. The database query is one
    /// read; the borrowed protocol projection then arranges each sibling
    /// list by position without a query per node.
    pub fn tasks(&self, project: &str, feature: &str) -> Result<Vec<Task>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, feature, parent, title, body, state, claimed_by, external, position, at,
                    session, claimed_pane, criteria
             FROM task WHERE project = ?1 AND feature = ?2 ORDER BY position, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project, feature], |r| {
            Ok(Task {
                id: r.get(0)?,
                feature: r.get(1)?,
                parent: r.get(2)?,
                title: r.get(3)?,
                body: r.get(4)?,
                state: TaskState::parse(&r.get::<_, String>(5)?).unwrap_or_default(),
                claimed_by: r.get(6)?,
                external: r.get(7)?,
                position: r.get(8)?,
                at: r.get(9)?,
                session: r.get(10)?,
                claimed_pane: pane_column(r, 11)?,
                criteria: decode_row(&r.get::<_, String>(12)?, 12)?,
                notes: Vec::new(),
            })
        })?;
        let mut tasks = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        // One grouped read rather than a query per task, as the features
        // read their checkouts.
        let mut stmt = conn.prepare(
            "SELECT note.task, note.at, note.session, note.pane, note.text
             FROM task_note note JOIN task ON task.id = note.task
             WHERE task.project = ?1 AND task.feature = ?2
             ORDER BY note.id",
        )?;
        let notes = stmt
            .query_map(rusqlite::params![project, feature], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    TaskNote {
                        at: r.get(1)?,
                        session: r.get(2)?,
                        pane: pane_column(r, 3)?,
                        text: r.get(4)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, note) in notes {
            if let Some(task) = tasks.iter_mut().find(|task| task.id == id) {
                task.notes.push(note);
            }
        }
        Ok(argus_protocol::TaskList {
            project_name: String::new(),
            feature: Some(feature.to_string()),
            tasks,
        }
        .tasks_in_tree_order())
    }

    /// Moves a task to another column.
    ///
    /// Taking it up is what claims it and finishing it is what releases
    /// it, so a `doing` column always says who is on each card without
    /// anyone having to claim anything by hand. The pane goes with the
    /// session and leaves with it: a session id tells a reader who, and
    /// only the pane tells them where to look.
    pub fn move_task(
        &self,
        project: &str,
        id: i64,
        state: TaskState,
        session: Option<&str>,
        pane: Option<PaneId>,
    ) -> Result<()> {
        let (claimed, pane) = match state {
            TaskState::Doing => (session, pane),
            TaskState::Todo | TaskState::Done => (None, None),
        };
        self.update_one(
            "UPDATE task SET state = ?1, claimed_by = ?2, claimed_pane = ?3
             WHERE project = ?4 AND id = ?5",
            rusqlite::params![
                state.as_str(),
                claimed,
                pane.map(|pane| pane.0 as i64),
                project,
                id
            ],
            || format!("there is no task {id} on this project"),
        )
    }

    /// Adds one thing that has to be true before the task is done.
    pub fn add_task_criterion(&self, project: &str, id: i64, text: &str) -> Result<()> {
        let text = checked_task_line(text, MAX_CRITERION_BYTES).map_err(anyhow::Error::msg)?;
        self.change_task_criteria(project, id, |criteria| {
            if criteria.len() >= MAX_TASK_CRITERIA {
                anyhow::bail!(
                    "task {id} already has {MAX_TASK_CRITERIA} criteria; more than that is \
                     several tasks"
                );
            }
            criteria.push(Criterion { text, done: false });
            Ok(())
        })
    }

    /// Checks the criterion at zero-based `index` off, or takes that back.
    pub fn set_task_criterion(&self, project: &str, id: i64, index: usize, done: bool) -> Result<()> {
        self.change_task_criteria(project, id, |criteria| {
            let count = criteria.len();
            let criterion = criteria
                .get_mut(index)
                .ok_or_else(|| no_such_criterion(id, index, count))?;
            criterion.done = done;
            Ok(())
        })
    }

    pub fn remove_task_criterion(&self, project: &str, id: i64, index: usize) -> Result<()> {
        self.change_task_criteria(project, id, |criteria| {
            if index >= criteria.len() {
                return Err(no_such_criterion(id, index, criteria.len()));
            }
            criteria.remove(index);
            Ok(())
        })
    }

    /// One read-modify-write of a task's criteria. A transaction, so two
    /// agents ticking different criteria of one task both land.
    fn change_task_criteria(
        &self,
        project: &str,
        id: i64,
        change: impl FnOnce(&mut Vec<Criterion>) -> Result<()>,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let stored: String = tx
            .query_row(
                "SELECT criteria FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no task {id} on this project"))?;
        let mut criteria: Vec<Criterion> = serde_json::from_str(&stored)?;
        change(&mut criteria)?;
        tx.execute(
            "UPDATE task SET criteria = ?1 WHERE project = ?2 AND id = ?3",
            rusqlite::params![encode(&criteria)?, project, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Leaves a line on a task, dropping the oldest past the bound.
    pub fn add_task_note(
        &self,
        project: &str,
        id: i64,
        text: &str,
        at: i64,
        session: Option<&str>,
        pane: Option<PaneId>,
    ) -> Result<()> {
        let text = checked_task_line(text, MAX_TASK_NOTE_BYTES).map_err(anyhow::Error::msg)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task WHERE project = ?1 AND id = ?2)",
            rusqlite::params![project, id],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no task {id} on this project");
        }
        tx.execute(
            "INSERT INTO task_note (task, at, session, pane, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, at, session, pane.map(|pane| pane.0 as i64), text],
        )?;
        tx.execute(
            "DELETE FROM task_note WHERE task = ?1 AND id NOT IN
                 (SELECT id FROM task_note WHERE task = ?1 ORDER BY id DESC LIMIT ?2)",
            rusqlite::params![id, MAX_TASK_NOTES as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Forgets which pane held each claim and wrote each note.
    ///
    /// Pane ids are handed out fresh on every start, so one read back after
    /// a restart would name whichever pane happened to get the number. The
    /// session beside it still says who.
    pub(super) fn forget_panes(&self) -> Result<()> {
        self.conn().execute_batch(
            "UPDATE task SET claimed_pane = NULL WHERE claimed_pane IS NOT NULL;
             UPDATE task_note SET pane = NULL WHERE pane IS NOT NULL;",
        )?;
        Ok(())
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

    /// Removes a task and its descendants outright.
    ///
    /// Unlike a decision, which is superseded rather than deleted: a
    /// decision is a record of what was believed, and a task is a thing
    /// somebody meant to do. A removed parent takes its newly discovered
    /// work with it; leaving those children behind would create rows with no
    /// context and make the tree harder to trust. Its notes go too.
    pub fn remove_task(&self, project: &str, id: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (feature, parent): (String, Option<i64>) = tx
            .query_row(
                "SELECT feature, parent
                 FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no task {id} on this project"))?;
        let removed: Vec<i64> = tx
            .prepare(
                "WITH RECURSIVE descendants(id) AS (
                     SELECT id FROM task
                     WHERE project = ?1 AND feature = ?2 AND id = ?3
                     UNION
                     SELECT child.id FROM task child
                     JOIN descendants parent ON child.parent = parent.id
                     WHERE child.project = ?1 AND child.feature = ?2
                 )
                 SELECT id FROM descendants",
            )?
            .query_map(rusqlite::params![project, feature, id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for task in &removed {
            tx.execute(
                "DELETE FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, task],
            )?;
            tx.execute("DELETE FROM task_note WHERE task = ?1", [task])?;
            // A decision outlives the task it was raised under: what was
            // decided is still true of the feature, it just hangs from
            // nothing now.
            tx.execute(
                "UPDATE decision SET task = NULL WHERE project = ?1 AND task = ?2",
                rusqlite::params![project, task],
            )?;
        }
        // Keep the remaining sibling positions dense. The client sends a
        // sibling index, not a database-specific position; normalizing the
        // whole list also repairs gaps left by older stores.
        let siblings = {
            let mut stmt = tx.prepare(
                "SELECT id FROM task
                 WHERE project = ?1 AND feature = ?2 AND parent IS ?3
                 ORDER BY position, id",
            )?;
            let rows = stmt.query_map(rusqlite::params![project, feature, parent], |row| {
                row.get::<_, i64>(0)
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (position, sibling) in siblings.into_iter().enumerate() {
            tx.execute(
                "UPDATE task SET position = ?1 WHERE project = ?2 AND id = ?3",
                rusqlite::params![position as i64, project, sibling],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Puts a task at a zero-based index among its siblings. Rewriting the
    /// whole sibling list also repairs gaps left by older stores, while a
    /// child keeps its parent and root tasks remain in their own list.
    pub fn reorder_task(&self, project: &str, id: i64, to: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (feature, parent): (String, Option<i64>) = tx
            .query_row(
                "SELECT feature, parent FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no task {id} on this project"))?;
        if to < 0 {
            anyhow::bail!("task order cannot be negative");
        }
        let mut siblings = tx
            .prepare(
                "SELECT id FROM task
                 WHERE project = ?1 AND feature = ?2 AND parent IS ?3
                 ORDER BY position, id",
            )?
            .query_map(rusqlite::params![project, feature, parent], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let Some(from) = siblings.iter().position(|sibling| *sibling == id) else {
            anyhow::bail!("task {id} is not in its sibling list");
        };
        let to = usize::try_from(to).map_err(|_| anyhow::anyhow!("task order is too large"))?;
        if to >= siblings.len() {
            anyhow::bail!("task order {to} is outside the sibling list");
        }
        if from == to {
            return Ok(());
        }
        let task = siblings.remove(from);
        siblings.insert(to, task);
        for (position, sibling) in siblings.into_iter().enumerate() {
            tx.execute(
                "UPDATE task SET position = ?1 WHERE project = ?2 AND id = ?3",
                rusqlite::params![position as i64, project, sibling],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Moves a task, with every subtask under it, to `place`, and answers
    /// with the feature it is now under.
    ///
    /// A task placed beside another takes that one's parent, so one move
    /// both re-parents and orders. Moved to another feature it arrives at
    /// the top level, since no task there is its parent yet. Both sibling
    /// lists it touches are renumbered dense, as a reorder leaves them.
    pub fn place_task(&self, project: &str, id: i64, place: &TaskPlace) -> Result<String> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (feature, parent): (String, Option<i64>) = tx
            .query_row(
                "SELECT feature, parent FROM task WHERE project = ?1 AND id = ?2",
                rusqlite::params![project, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no task {id} on this project"))?;
        let subtree: Vec<i64> = tx
            .prepare(
                "WITH RECURSIVE descendants(id) AS (
                     SELECT id FROM task WHERE project = ?1 AND id = ?2
                     UNION
                     SELECT child.id FROM task child
                     JOIN descendants parent ON child.parent = parent.id
                     WHERE child.project = ?1 AND child.feature = ?3
                 )
                 SELECT id FROM descendants",
            )?
            .query_map(rusqlite::params![project, id, feature], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        // The task a place is named against: under this feature, and not the
        // moving task or one of its own subtasks, which would hang the
        // subtree from itself.
        let anchor = |other: i64| -> Result<Option<i64>> {
            let found: Option<Option<i64>> = tx
                .query_row(
                    "SELECT parent FROM task WHERE project = ?1 AND feature = ?2 AND id = ?3",
                    rusqlite::params![project, feature, other],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(anchor_parent) = found else {
                anyhow::bail!("there is no task {other} under feature {feature}");
            };
            if subtree.contains(&other) {
                anyhow::bail!("task {id} cannot go under or beside itself or its own subtasks");
            }
            Ok(anchor_parent)
        };
        let (to_feature, to_parent, beside) = match place {
            TaskPlace::Under(other) => {
                anchor(*other)?;
                (feature.clone(), Some(*other), None)
            }
            TaskPlace::Top => (feature.clone(), None, None),
            TaskPlace::Before(other) => (feature.clone(), anchor(*other)?, Some((*other, 0))),
            TaskPlace::After(other) => (feature.clone(), anchor(*other)?, Some((*other, 1))),
            TaskPlace::Feature(slug) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                    rusqlite::params![project, slug],
                    |r| r.get(0),
                )?;
                if !exists {
                    anyhow::bail!("there is no feature {slug} on this project");
                }
                (slug.clone(), None, None)
            }
        };

        let siblings = |feature: &str, parent: Option<i64>| -> Result<Vec<i64>> {
            Ok(tx
                .prepare(
                    "SELECT id FROM task
                     WHERE project = ?1 AND feature = ?2 AND parent IS ?3 AND id <> ?4
                     ORDER BY position, id",
                )?
                .query_map(rusqlite::params![project, feature, parent, id], |r| {
                    r.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<_>>()?)
        };
        let left = siblings(&feature, parent)?;
        let mut joined = siblings(&to_feature, to_parent)?;
        let at = match beside {
            Some((other, after)) => {
                joined.iter().position(|sibling| *sibling == other).unwrap_or(joined.len()) + after
            }
            None => joined.len(),
        };
        joined.insert(at, id);

        for task in &subtree {
            tx.execute(
                "UPDATE task SET feature = ?1 WHERE project = ?2 AND id = ?3",
                rusqlite::params![to_feature, project, task],
            )?;
            // A decision stays on the feature it was made about, and a task
            // on another feature is not one it can be raised under.
            if to_feature != feature {
                tx.execute(
                    "UPDATE decision SET task = NULL WHERE project = ?1 AND task = ?2",
                    rusqlite::params![project, task],
                )?;
            }
        }
        tx.execute(
            "UPDATE task SET parent = ?1 WHERE project = ?2 AND id = ?3",
            rusqlite::params![to_parent, project, id],
        )?;
        for list in [left, joined] {
            for (position, sibling) in list.into_iter().enumerate() {
                tx.execute(
                    "UPDATE task SET position = ?1 WHERE project = ?2 AND id = ?3",
                    rusqlite::params![position as i64, project, sibling],
                )?;
            }
        }
        tx.commit()?;
        Ok(to_feature)
    }

    // ---- sequence diagrams -------------------------------------------

    pub fn add_sequence_diagram(
        &self,
        project: &str,
        feature: &str,
        write: &argus_protocol::DiagramWrite,
        at: i64,
        session: Option<&str>,
    ) -> Result<argus_protocol::SequenceDiagram> {
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
        tx.execute(
            "INSERT INTO sequence_diagram (project, feature, title, body, at, session)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                project,
                feature,
                write.title,
                write.body,
                at,
                session
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(argus_protocol::SequenceDiagram {
            id,
            feature: feature.to_string(),
            title: write.title.clone(),
            body: write.body.clone(),
            at,
            session: session.map(str::to_string),
        })
    }

    pub fn sequence_diagrams(
        &self,
        project: &str,
        feature: &str,
    ) -> Result<Vec<argus_protocol::SequenceDiagram>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, feature, title, body, at, session
             FROM sequence_diagram
             WHERE project = ?1 AND feature = ?2
             ORDER BY id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project, feature], |r| {
            Ok(argus_protocol::SequenceDiagram {
                id: r.get(0)?,
                feature: r.get(1)?,
                title: r.get(2)?,
                body: r.get(3)?,
                at: r.get(4)?,
                session: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Replaces a diagram's title and source, and files it as written now
    /// by whoever wrote it: a rewrite is the flow recorded again.
    pub fn rewrite_sequence_diagram(
        &self,
        project: &str,
        feature: &str,
        id: i64,
        write: &argus_protocol::DiagramWrite,
        at: i64,
        session: Option<&str>,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE sequence_diagram SET title = ?4, body = ?5, at = ?6, session = ?7
             WHERE project = ?1 AND feature = ?2 AND id = ?3",
            rusqlite::params![project, feature, id, write.title, write.body, at, session],
        )?;
        if changed == 0 {
            anyhow::bail!("there is no diagram {id} under feature {feature}");
        }
        Ok(())
    }

    pub fn remove_sequence_diagram(&self, project: &str, feature: &str, id: i64) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM sequence_diagram WHERE project = ?1 AND feature = ?2 AND id = ?3",
            rusqlite::params![project, feature, id],
        )?;
        if changed == 0 {
            anyhow::bail!("there is no diagram {id} under feature {feature}");
        }
        Ok(())
    }
}

/// Who is writing a row, when, and on which board: what every insert under
/// a feature needs and none of them decides.
pub(super) struct Filed<'a> {
    pub project: &'a str,
    pub at: i64,
    pub session: Option<&'a str>,
}

/// The slug is derived from the title and made unique by suffix inside the
/// transaction: two agents opening the same-sounding feature on two
/// branches at the same moment must neither collide on one key nor end up
/// quietly filing decisions on the same board.
pub(super) fn insert_feature(
    tx: &rusqlite::Transaction<'_>,
    filed: &Filed<'_>,
    write: &FeatureWrite,
    origin_checkout: Option<&str>,
    origin_branch: Option<&str>,
) -> Result<Feature> {
    let project = filed.project;
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
    // The counter, not the highest number in use: see `SCHEMA_V19`. The
    // rows are consulted too, so a board whose counter was lost still
    // cannot hand out a number somebody is using.
    let number: i64 = tx.query_row(
        "SELECT MAX(
             COALESCE((SELECT last FROM feature_counter WHERE project = ?1), 0),
             COALESCE((SELECT MAX(number) FROM feature WHERE project = ?1), 0)
         ) + 1",
        rusqlite::params![project],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO feature_counter (project, last) VALUES (?1, ?2)
         ON CONFLICT(project) DO UPDATE SET last = excluded.last",
        rusqlite::params![project, number],
    )?;
    let feature = Feature {
        number,
        slug,
        title: write.title.clone(),
        body: write.body.clone().unwrap_or_default(),
        origin_checkout: origin_checkout.map(str::to_string),
        origin_branch: origin_branch.map(str::to_string),
        at: filed.at,
        session: filed.session.map(str::to_string),
        ..Feature::default()
    };
    tx.execute(
        "INSERT INTO feature
             (project, slug, title, body, origin_checkout, origin_branch, at, session, number)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            project,
            feature.slug,
            feature.title,
            feature.body,
            feature.origin_checkout,
            feature.origin_branch,
            feature.at,
            feature.session,
            feature.number
        ],
    )?;
    Ok(feature)
}

pub(super) fn insert_task(
    tx: &rusqlite::Transaction<'_>,
    filed: &Filed<'_>,
    feature: &str,
    write: &TaskWrite,
) -> Result<Task> {
    let project = filed.project;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
        rusqlite::params![project, feature],
        |r| r.get(0),
    )?;
    if !exists {
        anyhow::bail!("there is no feature {feature} on this project");
    }
    let parent = write.parent;
    if parent.is_some_and(|parent| parent <= 0) {
        anyhow::bail!("a parent task id must be positive");
    }
    if let Some(parent) = parent {
        if !task_is_under(tx, project, feature, parent)? {
            anyhow::bail!("there is no parent task {parent} under feature {feature}");
        }
    }
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1
         FROM task
         WHERE project = ?1 AND feature = ?2 AND parent IS ?3",
        rusqlite::params![project, feature, parent],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO task
             (project, feature, parent, title, state, external, position, at, session)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            project,
            feature,
            parent,
            write.title,
            TaskState::Todo.as_str(),
            write.external,
            position,
            filed.at,
            filed.session
        ],
    )?;
    Ok(Task {
        id: tx.last_insert_rowid(),
        feature: feature.to_string(),
        parent,
        title: write.title.clone(),
        external: write.external.clone(),
        position,
        at: filed.at,
        session: filed.session.map(str::to_string),
        ..Task::default()
    })
}

fn task_is_under(
    tx: &rusqlite::Transaction<'_>,
    project: &str,
    feature: &str,
    id: i64,
) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task WHERE project = ?1 AND feature = ?2 AND id = ?3)",
        rusqlite::params![project, feature, id],
        |r| r.get(0),
    )?)
}

/// A superseding decision takes the place of the one it replaces — its
/// parent, not its children. It is a different answer to the same
/// question, so it belongs where that question was asked.
pub(super) fn insert_decision(
    tx: &rusqlite::Transaction<'_>,
    filed: &Filed<'_>,
    write: &DecisionWrite,
    feature: Option<&str>,
    checkout: Option<&str>,
) -> Result<i64> {
    let project = filed.project;
    // A decision replaces or hangs under one on its own feature: a
    // number from another feature's board is a mistyped number, and
    // taking it would mark that feature's reasoning replaced.
    let named = |id: i64| -> Result<Option<i64>> {
        let (parent, on): (Option<i64>, Option<String>) = tx
            .query_row(
                "SELECT parent, feature FROM decision WHERE id = ?1 AND project = ?2",
                rusqlite::params![id, project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no decision {id} on this board"))?;
        if feature.is_some() && on.as_deref() != feature {
            anyhow::bail!("decision {id} is under another feature");
        }
        Ok(parent)
    };
    let parent = match write.supersedes {
        Some(old) => named(old)?,
        None => write.under,
    };
    if let Some(under) = write.under {
        named(under)?;
    }
    // The same rule for the task it is raised under, for the same reason:
    // the link is what holds a task behind an open question, and it must
    // not reach onto another feature's list.
    if let Some(task) = write.task {
        let under = match feature {
            Some(feature) => task_is_under(tx, project, feature, task)?,
            None => false,
        };
        if !under {
            anyhow::bail!("there is no task {task} under this feature to raise the decision under");
        }
    }
    tx.execute(
        "INSERT INTO decision
             (project, parent, at, session, checkout, feature, chose, over_, because,
              state, task, context, detail, consequences, options)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            project,
            parent,
            filed.at,
            filed.session,
            checkout,
            feature,
            write.chose,
            write.over,
            write.because,
            write.state.as_str(),
            write.task,
            write.context,
            write.detail,
            encode(&write.consequences)?,
            encode(&write.options)?
        ],
    )?;
    let id = tx.last_insert_rowid();
    if let Some(old) = write.supersedes {
        tx.execute(
            "UPDATE decision SET superseded_by = ?1 WHERE id = ?2 AND project = ?3",
            rusqlite::params![id, old, project],
        )?;
    }
    Ok(id)
}

/// Takes a connection so a write can read the board inside its own
/// transaction, and judge a decision by what `Decision::open` says of it
/// rather than by a second reading of the same columns.
fn read_decisions(conn: &Connection, project: &str) -> Result<Vec<Decision>> {
    let mut stmt = conn.prepare(
        "SELECT id, parent, at, session, checkout, feature, chose, over_, because,
                superseded_by, withdrawn_at, state, task, context, detail, consequences, options
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
            withdrawn_at: r.get(10)?,
            // A state this build does not know was written by a newer one;
            // read it as accepted rather than refusing the whole board.
            state: DecisionState::parse(&r.get::<_, String>(11)?).unwrap_or_default(),
            task: r.get(12)?,
            context: r.get(13)?,
            detail: r.get(14)?,
            consequences: decode_row::<Vec<Consequence>>(&r.get::<_, String>(15)?, 15)?,
            options: decode_row(&r.get::<_, String>(16)?, 16)?,
        })
    })?;
    let mut decisions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    // A replacement withdrawn as recorded in error replaces nothing, so
    // what it named stands again; restoring it brings the link back.
    let withdrawn: Vec<i64> = decisions
        .iter()
        .filter(|d| d.withdrawn())
        .map(|d| d.id)
        .collect();
    for decision in &mut decisions {
        if decision.superseded_by.is_some_and(|by| withdrawn.contains(&by)) {
            decision.superseded_by = None;
        }
    }
    Ok(decisions)
}

fn pane_column(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Option<PaneId>> {
    Ok(row
        .get::<_, Option<i64>>(column)?
        .and_then(|pane| u64::try_from(pane).ok())
        .map(PaneId))
}

fn no_such_criterion(task: i64, index: usize, count: usize) -> anyhow::Error {
    anyhow::anyhow!(
        "task {task} has no criterion {}; it has {count}",
        index.saturating_add(1)
    )
}

/// Holds text assembled from several bounded fields to the bound of one,
/// cutting on a character rather than through it.
fn bounded(mut text: String) -> String {
    let mut end = text.len().min(MAX_DECISION_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
