//! The decision board: appending to it, and reading it back.
//!
//! Clients speak in ids and the store speaks in artifact keys; this
//! translates between them. A decision is scoped to a *feature*: a tree
//! still has to be read whole, because a node hanging off three others
//! says nothing without them, but the tree that has to be read whole is
//! one feature's, not one project's. The repository board is what the
//! client is pushed, since it draws the features alongside it; an agent is
//! answered one feature at a time by `features`.
//!
//! A decision is not always made yet: an open one is a question, usually
//! the person's to answer, and a rejected one an answer turned down. Both
//! sides write all of it — a person from the feature view, an agent through
//! its pane — and after resolving which feature a request is about the two
//! are one path, so what a change may touch cannot differ by who asked.
//!
//! Nothing here is gated on a project flag. The board exists for agents to
//! write, never loses a row, and attributes every one, so there is nothing
//! for a policy to protect.

use argus_protocol::{Decision, DecisionBoard, DecisionChange, DecisionWrite};

use super::*;

impl Daemon {
    /// One project's board, by the id a client holds.
    pub fn decision_board(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
    ) -> anyhow::Result<DecisionBoard> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        Ok(DecisionBoard {
            project: Some(project),
            features: self.store.features(&key)?,
            tracker_map: self.store.tracker_map(&key)?,
            decisions: self.store.decisions(&key)?,
            name,
        })
    }

    /// The board an agent reads: the decisions of the feature its
    /// checkout is on, and nothing else.
    ///
    /// The read is what makes the board a reference rather than a diary:
    /// an agent picking up a feature reads what was already decided, and
    /// what those decisions were made against, before adding to it. Scoped
    /// to the feature for the same reason — everything decided about some
    /// other feature is noise it has to read past to find the part that
    /// constrains it.
    pub fn decisions_for_agent(
        &self,
        pane_id: PaneId,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<DecisionBoard> {
        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(filing.scope);
        let feature = self.feature_for_agent(&scope, &filing)?;
        // A withdrawn decision stays on the board for people reading its
        // history; an agent reads the rest. That includes a question still
        // open and an answer turned down, each carrying its state: what an
        // agent must not do is read either as decided, and the surest way
        // to have one decide a person's question alone is to hide it.
        let decisions = self
            .store
            .decisions(key)?
            .into_iter()
            .filter(|d| d.feature == feature && !d.withdrawn())
            .collect();
        Ok(DecisionBoard {
            project: self.project_id_named(&scope.project_name),
            features: self.store.features(key)?,
            tracker_map: self.store.tracker_map(key)?,
            name: scope.project_name,
            decisions,
        })
    }

    /// Changes one decision for an agent, and answers with the board as the
    /// agent now reads it.
    pub fn change_decision_for_agent(
        &self,
        pane_id: PaneId,
        change: DecisionChange,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<DecisionBoard> {
        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(filing.scope);
        let feature = self.feature_for_agent(&scope, &filing)?;
        self.change_decision(&scope.project_name, key, feature.as_deref(), change)?;
        self.decisions_for_agent(pane_id, filing)
    }

    /// Changes one decision from the feature view, which names its feature
    /// outright.
    pub fn change_decision_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        change: DecisionChange,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.change_decision(&name, &key, Some(feature), change)
    }

    /// Answers, rewords, withdraws or restores one decision, and pushes the
    /// board.
    ///
    /// The decision has to be under the feature the request is about, as
    /// every write naming a row does: an id from a list that has since
    /// moved on must not reach into some other feature's history.
    fn change_decision(
        &self,
        name: &str,
        key: &str,
        feature: Option<&str>,
        change: DecisionChange,
    ) -> anyhow::Result<()> {
        let id = match &change {
            DecisionChange::Withdraw { id }
            | DecisionChange::Restore { id }
            | DecisionChange::Resolve { id, .. }
            | DecisionChange::Rewrite { id, .. } => *id,
        };
        let under = self
            .store
            .decisions(key)?
            .into_iter()
            .any(|d| d.id == id && d.feature.is_some() && d.feature.as_deref() == feature);
        if !under {
            anyhow::bail!("decision {id} is not under this feature");
        }
        match change {
            DecisionChange::Withdraw { id } => {
                self.store.set_decision_withdrawn(key, id, Some(now()))?
            }
            DecisionChange::Restore { id } => self.store.set_decision_withdrawn(key, id, None)?,
            DecisionChange::Resolve {
                id,
                answer,
                because,
            } => self
                .store
                .resolve_decision(key, id, &answer, because.as_deref())?,
            DecisionChange::Rewrite { id, text } => self.store.rewrite_decision(key, id, text)?,
        }
        self.broadcast_decisions(name, key, self.store.decisions(key)?);
        Ok(())
    }

    /// Appends one decision, and returns it as the board now holds it.
    ///
    /// The caller must be a live agent pane, and the decision must say
    /// what was chosen; both refusals come back as text the agent can put
    /// in front of the user. The id in the answer is what the next
    /// decision hangs off, which is the only reason a write answers with
    /// more than an acknowledgement.
    pub fn record_agent_decision(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: DecisionWrite,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<Decision> {
        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(filing.scope);
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        // Refused rather than filed loose: a decision nobody can find
        // again is the pile this scoping exists to end.
        let feature = self
            .feature_for_agent(&scope, &filing)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "this checkout is not on a feature yet — open one with \
                 `argus-hook feature open \"<title>\"`, or point it at an \
                 existing one with `argus-hook feature <slug>`"
                )
            })?;
        let checkout = scope.checkout_path.to_string_lossy().to_string();
        let id = self.store.add_decision(
            key,
            &write,
            Some(feature.as_str()),
            now(),
            session,
            Some(checkout.as_str()),
        )?;
        self.recorded(&scope.project_name, key, id)
    }

    /// Records a decision from the feature view, under the feature it
    /// names. A person's decision has no session and no checkout: it was
    /// made looking at the board, not from inside a working tree.
    pub fn decide_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        write: DecisionWrite,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        // The store files a decision under whatever slug it is handed, so
        // a feature removed since the view last drew it is refused here.
        if !self.store.features(&key)?.iter().any(|f| f.slug == feature) {
            anyhow::bail!("there is no feature {feature} on this project");
        }
        let id = self
            .store
            .add_decision(&key, &write, Some(feature), now(), None, None)?;
        self.recorded(&name, &key, id).map(|_| ())
    }

    /// The decision just written, read back rather than assembled from the
    /// write: `supersedes` decides the parent inside the transaction, so
    /// what the store holds is the only account of where the node landed.
    fn recorded(&self, name: &str, key: &str, id: i64) -> anyhow::Result<Decision> {
        let board = self.store.decisions(key)?;
        let recorded = board
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("the decision was not recorded"))?;
        self.broadcast_decisions(name, key, board);
        Ok(recorded)
    }

    pub(super) fn project_id_named(&self, name: &str) -> Option<ProjectId> {
        let inner = self.inner.lock().unwrap();
        inner.projects.iter().find(|p| p.name == name).map(|p| p.id)
    }

    /// Pushes a changed board at every attached client.
    ///
    /// Pushed rather than answered to one client, because a board is
    /// meant to be watched: the point of drawing the tree is
    /// seeing it built up while the work happens. A client with another
    /// project open drops it by name.
    pub(super) fn broadcast_decisions(&self, name: &str, key: &str, decisions: Vec<Decision>) {
        let features = self.store.features(key).unwrap_or_default();
        let _ = self.decisions_tx.send(DecisionBoard {
            project: self.project_id_named(name),
            name: name.to_string(),
            features,
            decisions,
            tracker_map: self.store.tracker_map(key).unwrap_or_default(),
        });
    }
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}
