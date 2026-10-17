//! A board's tracker map: who may write it, and who may only propose.
//!
//! The person writes the map, or approves an agent's draft of it; an agent
//! reads it and drafts. Nothing an agent sends changes what the next agent
//! reads as the map, so a wrong guess about how the team's trackers line
//! up stays a proposal until the person has read it.

use argus_protocol::{checked_map, ArtifactScope, MapChange, PaneId, ProjectId, TrackerMap};

use super::asker::{pane_label, Asker};
use super::address::BoardAddress;
use crate::state::*;

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
        let by = Asker::Agent {
            pane: pane_id,
            session: None,
        };
        self.agent_tracker_map(&address, by, draft)
    }

    pub(in crate::state) fn agent_tracker_map(
        &self,
        address: &BoardAddress,
        by: Asker<'_>,
        draft: &str,
    ) -> anyhow::Result<TrackerMap> {
        if !draft.trim().is_empty() {
            self.change_tracker_map(address, by, MapChange::Write(draft.to_string()))?;
        }
        self.store.tracker_map(&address.key)
    }

    /// The person's change to the map, from the view.
    pub fn change_tracker_map_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        change: MapChange,
    ) -> anyhow::Result<()> {
        let address = self.address_for_client(project, checkout)?;
        self.change_tracker_map(&address, Asker::Person, change)
    }

    /// One change to the map, and the push. A person's write is the map;
    /// an agent's is a draft beside it, and what becomes of a draft is the
    /// person's to say.
    pub(in crate::state) fn change_tracker_map(
        &self,
        address: &BoardAddress,
        by: Asker<'_>,
        change: MapChange,
    ) -> anyhow::Result<()> {
        let key = address.key.as_str();
        let checked = |body: &str| checked_map(body).map_err(|e| anyhow::anyhow!("{e}"));
        match (by, change) {
            (Asker::Person, MapChange::Write(body)) => {
                self.store.set_tracker_map(key, &checked(&body)?)?
            }
            (Asker::Agent { pane, .. }, MapChange::Write(body)) => {
                let by = pane_label(pane);
                self.store.draft_tracker_map(key, &checked(&body)?, &by, super::decisions::now())?
            }
            (Asker::Person, MapChange::Approve(body)) => {
                self.store.approve_tracker_map(key, &checked(&body)?)?
            }
            (Asker::Person, MapChange::Discard) => self.store.discard_tracker_map_draft(key)?,
            (Asker::Agent { .. }, MapChange::Approve(_) | MapChange::Discard) => {
                anyhow::bail!("a draft of the map is the person's to approve or discard")
            }
        }
        self.broadcast_decisions(address, self.store.decisions(key)?);
        Ok(())
    }
}
