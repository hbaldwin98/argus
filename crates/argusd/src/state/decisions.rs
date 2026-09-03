//! The decision board: appending to it, and reading it back.
//!
//! Same translation job as `notes` — clients speak in ids, the store
//! speaks in project names — with one difference that shapes the whole
//! module. A note is scoped to the pane that asks for it. A decision tree
//! is not, and cannot be: a decision that hangs off three others means
//! nothing without them, so an agent reads the project's whole board and
//! not just the part it wrote.
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
            decisions: self.store.decisions(&name)?,
            name,
        })
    }

    /// The board of the project a live agent pane belongs to.
    ///
    /// The read is what makes the board a reference rather than a diary:
    /// an agent picking up a feature reads what was already decided, and
    /// what those decisions were made against, before adding to it.
    pub fn decisions_for_agent(&self, pane_id: PaneId) -> anyhow::Result<DecisionBoard> {
        let scope = self.agent_scope(pane_id)?;
        let decisions = self.store.decisions(&scope.project_name)?;
        Ok(DecisionBoard {
            project: self.project_id_named(&scope.project_name),
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
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let checkout = scope.checkout_path.to_string_lossy().to_string();
        let id = self.store.add_decision(
            &scope.project_name,
            &write,
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

    fn project_id_named(&self, name: &str) -> Option<ProjectId> {
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
    fn broadcast_decisions(&self, name: &str, decisions: Vec<Decision>) {
        let _ = self.decisions_tx.send(DecisionBoard {
            project: self.project_id_named(name),
            name: name.to_string(),
            decisions,
        });
    }
}
