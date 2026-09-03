//! The decision board: appending to it, and reading it back.
//!
//! Same translation job as `notes` — clients speak in ids, the store
//! speaks in project names — with one difference that shapes the whole
//! module. A note is scoped to the pane that asks for it. A decision is
//! scoped to a *feature*: a tree still has to be read whole, because a
//! node hanging off three others says nothing without them, but the tree
//! that has to be read whole is one feature's, not one project's. The
//! project-wide board is what the client is pushed, since it draws the
//! features alongside it; an agent is answered one feature at a time by
//! `features`.
//!
//! Nothing here is gated on a project flag the way note writes are. A note
//! is the human's document and an agent writing to it needs permission; the
//! board exists for agents to write, is append-only, and attributes every
//! row. There is nothing for a policy to protect.

use argus_protocol::{Decision, DecisionBoard, DecisionWrite};

use super::*;

impl Daemon {
    /// One project's board, by the id a client holds.
    pub fn decision_board(&self, project: ProjectId) -> anyhow::Result<DecisionBoard> {
        let name = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .find(|p| p.id == project)
                .map(|p| p.name.clone())
                .ok_or_else(|| anyhow::anyhow!("no such project"))?
        };
        Ok(DecisionBoard {
            project: Some(project),
            features: self.store.features(&name)?,
            decisions: self.store.decisions(&name)?,
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
    pub fn decisions_for_agent(&self, pane_id: PaneId) -> anyhow::Result<DecisionBoard> {
        let scope = self.agent_scope(pane_id)?;
        let feature = self.feature_for_agent(&scope)?;
        let decisions = self
            .store
            .decisions(&scope.project_name)?
            .into_iter()
            .filter(|d| d.feature == feature)
            .collect();
        Ok(DecisionBoard {
            project: self.project_id_named(&scope.project_name),
            features: self.store.features(&scope.project_name)?,
            name: scope.project_name,
            decisions,
        })
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
    ) -> anyhow::Result<Decision> {
        let scope = self.agent_scope(pane_id)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        // Refused rather than filed loose: a decision nobody can find
        // again is the pile this scoping exists to end.
        let feature = self.feature_for_agent(&scope)?.ok_or_else(|| {
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
            &scope.project_name,
            &write,
            Some(feature.as_str()),
            at,
            session,
            Some(checkout.as_str()),
        )?;
        // Read back rather than assembled from the write: `supersedes`
        // decides the parent inside the transaction, so what the store
        // holds is the only account of where the node actually landed.
        let board = self.store.decisions(&scope.project_name)?;
        let recorded = board
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("the decision was not recorded"))?;
        self.broadcast_decisions(&scope.project_name, board);
        Ok(recorded)
    }

    pub(super) fn project_id_named(&self, name: &str) -> Option<ProjectId> {
        let inner = self.inner.lock().unwrap();
        inner
            .projects
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.id)
    }

    /// Pushes a changed board at every attached client.
    ///
    /// Unlike a note, which is answered only to the client that asked, a
    /// board is meant to be watched: the point of drawing the tree is
    /// seeing it built up while the work happens. A client with another
    /// project open drops it by name.
    pub(super) fn broadcast_decisions(&self, name: &str, decisions: Vec<Decision>) {
        let features = self.store.features(name).unwrap_or_default();
        let _ = self.decisions_tx.send(DecisionBoard {
            project: self.project_id_named(name),
            name: name.to_string(),
            features,
            decisions,
        });
    }
}
