//! Sequence diagrams under a feature: stored Mermaid, listed and edited
//! from the feature view like tasks.

use argus_protocol::{DiagramAction, DiagramList};

use super::board_parts::{BoardTarget, FeaturePart};
use super::*;
use crate::store::Store;

impl FeaturePart for DiagramAction {
    type List = DiagramList;
    const NOUN: &'static str = "diagram";

    fn is_read(&self) -> bool {
        matches!(self, DiagramAction::List)
    }

    fn is_add(&self) -> bool {
        matches!(self, DiagramAction::Add(_))
    }

    fn id(&self) -> Option<i64> {
        match self {
            DiagramAction::List | DiagramAction::Add(_) => None,
            DiagramAction::Remove { id } => Some(*id),
        }
    }

    fn ids(store: &Store, key: &str, feature: &str) -> anyhow::Result<Vec<i64>> {
        Ok(store
            .sequence_diagrams(key, feature)?
            .iter()
            .map(|d| d.id)
            .collect())
    }

    fn apply(
        self,
        store: &Store,
        key: &str,
        feature: &str,
        at: i64,
        session: Option<&str>,
    ) -> anyhow::Result<()> {
        match self {
            DiagramAction::List => {}
            DiagramAction::Add(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                store.add_sequence_diagram(key, feature, &write, at, session)?;
            }
            DiagramAction::Remove { id } => store.remove_sequence_diagram(key, feature, id)?,
        }
        Ok(())
    }

    fn list(store: &Store, target: &BoardTarget) -> anyhow::Result<DiagramList> {
        Ok(DiagramList {
            project_name: target.project_name.clone(),
            feature: Some(target.feature.clone()),
            diagrams: store.sequence_diagrams(&target.key, &target.feature)?,
        })
    }

    fn empty(project_name: String) -> DiagramList {
        DiagramList {
            project_name,
            feature: None,
            diagrams: Vec::new(),
        }
    }

    fn broadcast(daemon: &Daemon, target: &BoardTarget) {
        let diagrams = daemon
            .store
            .sequence_diagrams(&target.key, &target.feature)
            .unwrap_or_default();
        let _ = daemon.diagrams_tx.send(DiagramList {
            project_name: target.project_name.clone(),
            feature: Some(target.feature.clone()),
            diagrams,
        });
    }
}
