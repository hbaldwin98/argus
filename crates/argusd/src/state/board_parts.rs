//! The parts filed under a feature — its tasks and its sequence diagrams:
//! which feature a request for one lands in, which rows it may touch, and
//! which lists its change pushes.
//!
//! An agent is on whichever feature its checkout points at, and a client
//! names one outright. After that the two are one path, so the id guard
//! and the push cannot differ by who asked or by which part was asked for;
//! a part says only how its rows are stored.

use argus_protocol::{PaneId, ProjectId};

use super::asker::Asker;
use super::board_address::BoardAddress;
use super::*;
use crate::store::Store;

/// One kind of row filed under a feature, spoken to through the action
/// that changes it.
pub trait FeaturePart: Sized {
    /// What every request is answered with: the part's rows under the
    /// feature as they stand afterwards.
    type List;
    /// The part's name in a refusal: `task`, `diagram`.
    const NOUN: &'static str;

    /// Whether this only reads, so nothing is written or pushed.
    fn is_read(&self) -> bool;
    /// Whether this adds a row: the change an agent on no feature is told
    /// how to make possible.
    fn is_add(&self) -> bool;
    /// The existing row this touches, when it touches one.
    fn id(&self) -> Option<i64>;
    /// Another feature on the board whose rows this changes too: where a
    /// row it moves lands.
    fn also_changes(&self) -> Option<&str> {
        None
    }

    fn ids(store: &Store, key: &str, feature: &str) -> anyhow::Result<Vec<i64>>;
    fn apply(
        self,
        store: &Store,
        key: &str,
        feature: &str,
        at: i64,
        by: Asker<'_>,
    ) -> anyhow::Result<()>;
    /// Names the repositories whose board this is, as a push does: a read
    /// answered without them would be dropped by a client that keeps only
    /// its own repository's lists.
    fn list(daemon: &Daemon, target: &BoardTarget<'_>) -> anyhow::Result<Self::List>;
    /// The answer on a checkout that is on no feature.
    fn empty(project_name: String) -> Self::List;
    fn broadcast(daemon: &Daemon, target: &BoardTarget<'_>);
}

/// The one feature a part request lands in, however the caller named it.
#[derive(Clone)]
pub struct BoardTarget<'a> {
    pub address: &'a BoardAddress,
    pub feature: String,
}

impl Daemon {
    /// A change from an agent, answered with the list as it stands
    /// afterwards — an agent that has just added three tasks needs the ids
    /// they were given before it can take one up.
    pub fn part_action_for_agent<P: FeaturePart>(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        action: P,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<P::List> {
        let filing = filing.into();
        let address = self.address_for_agent(pane_id, filing.scope)?;
        let by = Asker::Agent {
            pane: pane_id,
            session,
        };
        self.agent_part_action(&address, by, filing.feature.as_deref(), action)
    }

    /// An agent's change, applied to the feature it names or else the one
    /// its checkout is on.
    ///
    /// Refused when the checkout is on no feature, for the same reason a
    /// decision is: a row with nothing to be under is the pile all of this
    /// exists to end. A read there is an empty list rather than an error.
    pub(in crate::state) fn agent_part_action<P: FeaturePart>(
        &self,
        address: &BoardAddress,
        by: Asker<'_>,
        named: Option<&str>,
        action: P,
    ) -> anyhow::Result<P::List> {
        let Some(feature) = self.feature_for_agent(address, named)? else {
            if action.is_read() {
                return Ok(P::empty(address.project_name.clone()));
            }
            if action.is_add() {
                anyhow::bail!(
                    "this checkout is not on a feature yet — open one with \
                     `argus-hook feature open` before adding {}s to it",
                    P::NOUN
                );
            }
            anyhow::bail!("this checkout is not on a feature yet");
        };
        let target = BoardTarget { address, feature };
        self.apply_part(&target, action, by)
    }

    /// A change from the feature view, which names its feature outright
    /// rather than resolving one from a checkout.
    pub fn part_action_for_client<P: FeaturePart>(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        action: P,
    ) -> anyhow::Result<P::List> {
        let address = self.address_for_client(project, checkout)?;
        let target = BoardTarget {
            address: &address,
            feature: feature.to_string(),
        };
        self.apply_part(&target, action, Asker::Person)
    }

    /// Applies one change and answers with the list afterwards. Every
    /// change is also pushed, so a client that did not ask still sees it.
    pub(in crate::state) fn apply_part<P: FeaturePart>(
        &self,
        target: &BoardTarget<'_>,
        action: P,
        by: Asker<'_>,
    ) -> anyhow::Result<P::List> {
        if let Some(id) = action.id() {
            self.guard_part::<P>(target, id)?;
        }
        if !action.is_read() {
            let at = super::decisions::now();
            // A client watching the feature a row moved into is owed the
            // push as much as one watching the feature it left.
            let landed = action.also_changes().map(|feature| BoardTarget {
                feature: feature.to_string(),
                ..target.clone()
            });
            action.apply(&self.store, &target.address.key, &target.feature, at, by)?;
            P::broadcast(self, target);
            if let Some(landed) = landed.filter(|landed| landed.feature != target.feature) {
                P::broadcast(self, &landed);
            }
        }
        P::list(self, target)
    }

    /// Refuses a row that is not under the target's feature.
    ///
    /// Ids are database-wide and a writer numbers rows from what it last
    /// read, so a stale id would otherwise let one feature's agent tick off
    /// another's work by arithmetic — or a client holding a list the tree
    /// has since moved on from rewrite a row it is no longer showing.
    fn guard_part<P: FeaturePart>(&self, target: &BoardTarget<'_>, id: i64) -> anyhow::Result<()> {
        if !P::ids(&self.store, &target.address.key, &target.feature)?.contains(&id) {
            anyhow::bail!("{} {id} is not under this feature", P::NOUN);
        }
        Ok(())
    }
}
