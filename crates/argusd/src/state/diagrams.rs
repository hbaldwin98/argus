//! Sequence diagrams under a feature: stored Mermaid, listed and edited
//! from the feature view like tasks.

use argus_protocol::{DiagramAction, DiagramList};

use super::asker::Asker;
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
            DiagramAction::Rewrite { id, .. } | DiagramAction::Remove { id } => Some(*id),
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
        by: Asker<'_>,
    ) -> anyhow::Result<()> {
        match self {
            DiagramAction::List => {}
            DiagramAction::Add(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                store.add_sequence_diagram(key, feature, &write, at, by.session())?;
            }
            DiagramAction::Rewrite { id, write } => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                store.rewrite_sequence_diagram(key, feature, id, &write, at, by.session())?;
            }
            DiagramAction::Remove { id } => store.remove_sequence_diagram(key, feature, id)?,
        }
        Ok(())
    }

    fn list(daemon: &Daemon, target: &BoardTarget<'_>) -> anyhow::Result<DiagramList> {
        Ok(DiagramList {
            repositories: target.address.repositories.clone(),
            project_name: target.address.project_name.clone(),
            feature: Some(target.feature.clone()),
            diagrams: daemon.store.sequence_diagrams(&target.address.key, &target.feature)?,
        })
    }

    fn empty(project_name: String) -> DiagramList {
        DiagramList {
            repositories: Vec::new(),
            project_name,
            feature: None,
            diagrams: Vec::new(),
        }
    }

    fn broadcast(daemon: &Daemon, target: &BoardTarget<'_>) {
        if !target.address.is_pushed() {
            return;
        }
        if let Ok(list) = Self::list(daemon, target) {
            let _ = daemon.diagrams_tx.send(list);
        }
    }
}
