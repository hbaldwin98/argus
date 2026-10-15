//! A board's tracker map: who may write it, and who may only propose.
//!
//! The person writes the map, or approves an agent's draft of it; an agent
//! reads it and drafts. Nothing an agent sends changes what the next agent
//! reads as the map, so a wrong guess about how the team's trackers line
//! up stays a proposal until the person has read it.

use argus_protocol::{checked_map, ArtifactScope, MapChange, PaneId, ProjectId, TrackerMap};

use super::*;

impl Daemon {
    /// The map an agent reads, or, with a body, the draft it proposes and
    /// the map as it stands afterwards: the draft beside an unchanged map.
    pub fn tracker_map_for_agent(
        &self,
        pane_id: PaneId,
        draft: &str,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TrackerMap> {
        let address = self.address_for_agent(pane_id, artifact_scope)?;
        let key = address.key.as_str();
        if draft.trim().is_empty() {
            return self.store.tracker_map(key);
        }

        let body = checked_map(draft).map_err(|e| anyhow::anyhow!("{e}"))?;
        let by = format!("pane #{}", pane_id.0);
        self.store.draft_tracker_map(key, &body, &by, super::decisions::now())?;
        self.broadcast_decisions(&address, self.store.decisions(key)?);
        self.store.tracker_map(key)
    }

    /// The person's change to the map, from the view.
    pub fn change_tracker_map_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        change: MapChange,
    ) -> anyhow::Result<()> {
        let address = self.address_for_client(project, checkout)?;
        let key = address.key.as_str();
        let checked = |body: &str| checked_map(body).map_err(|e| anyhow::anyhow!("{e}"));
        match change {
            MapChange::Write(body) => self.store.set_tracker_map(key, &checked(&body)?)?,
            MapChange::Approve(body) => self.store.approve_tracker_map(key, &checked(&body)?)?,
            MapChange::Discard => self.store.discard_tracker_map_draft(key)?,
        }
        self.broadcast_decisions(&address, self.store.decisions(key)?);
        Ok(())
    }
}
