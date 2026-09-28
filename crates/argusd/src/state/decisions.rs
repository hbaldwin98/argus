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
//! Nothing here is gated on a project flag. The board exists for agents to
//! write, is append-only, and attributes every row, so there is nothing for
//! a policy to protect.

use argus_protocol::{Decision, DecisionBoard, DecisionWrite};

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
        // history; an agent reads the ones that still stand.
        let decisions = self
            .store
            .decisions(key)?
            .into_iter()
            .filter(|d| d.feature == feature && !d.withdrawn())
            .collect();
        Ok(DecisionBoard {
            project: self.project_id_named(&scope.project_name),
            features: self.store.features(key)?,
            name: scope.project_name,
            decisions,
        })
    }

    /// Withdraws a decision, or restores one withdrawn, for an agent, and
    /// answers with the board as the agent now reads it.
    ///
    /// The decision has to be under the feature the request is about, as
    /// every write naming a row does: an id from a list that has since
    /// moved on must not reach into some other feature's history.
    pub fn change_decision_for_agent(
        &self,
        pane_id: PaneId,
        change: argus_protocol::DecisionChange,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<DecisionBoard> {
        use argus_protocol::DecisionChange;

        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(filing.scope);
        let feature = self.feature_for_agent(&scope, &filing)?;
        let (id, at) = match change {
            DecisionChange::Withdraw { id } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or_default();
                (id, Some(now))
            }
            DecisionChange::Restore { id } => (id, None),
        };
        let under = self
            .store
            .decisions(key)?
            .into_iter()
            .any(|d| d.id == id && d.feature.is_some() && d.feature == feature);
        if !under {
            anyhow::bail!("decision {id} is not under this feature");
        }
        self.store.set_decision_withdrawn(key, id, at)?;
        self.broadcast_decisions(&scope.project_name, key, self.store.decisions(key)?);
        self.decisions_for_agent(pane_id, filing)
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
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let checkout = scope.checkout_path.to_string_lossy().to_string();
        let id = self.store.add_decision(
            key,
            &write,
            Some(feature.as_str()),
            at,
            session,
            Some(checkout.as_str()),
        )?;
        // Read back rather than assembled from the write: `supersedes`
        // decides the parent inside the transaction, so what the store
        // holds is the only account of where the node actually landed.
        let board = self.store.decisions(key)?;
        let recorded = board
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("the decision was not recorded"))?;
        self.broadcast_decisions(&scope.project_name, key, board);
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
        });
    }
}
