//! Sequence diagrams under a feature: stored Mermaid, listed and edited
//! from the feature view like tasks.

use argus_protocol::{
    ArtifactScope, DiagramAction, DiagramList, PaneId, ProjectId, SequenceDiagram,
};

use super::*;

struct DiagramTarget {
    project_name: String,
    key: String,
    feature: String,
}

impl Daemon {
    /// A diagram change from an agent, applied to the feature its checkout
    /// is on, answered with the list as it stands afterwards.
    pub fn diagram_action_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        action: DiagramAction,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<DiagramList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope).to_string();
        let Some(feature) = self.feature_for_agent(&scope, artifact_scope)? else {
            return match action {
                DiagramAction::List => Ok(DiagramList {
                    project_name: scope.project_name,
                    feature: None,
                    diagrams: Vec::new(),
                }),
                DiagramAction::Add(_) => anyhow::bail!(
                    "this checkout is not on a feature yet — open one with \
                     `argus-hook feature open` before adding diagrams to it"
                ),
                DiagramAction::Remove { .. } => {
                    anyhow::bail!("this checkout is not on a feature yet")
                }
            };
        };
        if let DiagramAction::Remove { id } = &action {
            self.guard_diagram(&key, &feature, *id)?;
        }
        let target = DiagramTarget {
            project_name: scope.project_name,
            key,
            feature,
        };
        self.apply_diagram(&target, action, session)
    }

    pub fn diagram_action_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        action: DiagramAction,
    ) -> anyhow::Result<DiagramList> {
        let (project_name, key) = self.client_artifact_scope(project, checkout)?;
        if let DiagramAction::Remove { id } = &action {
            self.guard_diagram(&key, feature, *id)?;
        }
        let target = DiagramTarget {
            project_name,
            key,
            feature: feature.to_string(),
        };
        self.apply_diagram(&target, action, None)
    }

    fn apply_diagram(
        &self,
        target: &DiagramTarget,
        action: DiagramAction,
        session: Option<&str>,
    ) -> anyhow::Result<DiagramList> {
        let DiagramTarget { key, feature, .. } = target;
        let changes = !matches!(action, DiagramAction::List);
        match action {
            DiagramAction::List => {}
            DiagramAction::Add(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or_default();
                self.store
                    .add_sequence_diagram(key, feature, &write, at, session)?;
            }
            DiagramAction::Remove { id } => {
                self.guard_diagram(key, feature, id)?;
                self.store.remove_sequence_diagram(key, feature, id)?;
            }
        }
        if changes {
            self.broadcast_diagrams(&target.project_name, key, feature);
        }
        Ok(DiagramList {
            project_name: target.project_name.clone(),
            feature: Some(feature.clone()),
            diagrams: self.store.sequence_diagrams(key, feature)?,
        })
    }

    fn guard_diagram(&self, key: &str, feature: &str, id: i64) -> anyhow::Result<()> {
        let mine = self
            .store
            .sequence_diagrams(key, feature)?
            .iter()
            .any(|d: &SequenceDiagram| d.id == id);
        if !mine {
            anyhow::bail!("diagram {id} is not under this feature");
        }
        Ok(())
    }

    fn broadcast_diagrams(&self, name: &str, key: &str, feature: &str) {
        let diagrams = self.store.sequence_diagrams(key, feature).unwrap_or_default();
        let _ = self.diagrams_tx.send(DiagramList {
            project_name: name.to_string(),
            feature: Some(feature.to_string()),
            diagrams,
        });
    }
}
