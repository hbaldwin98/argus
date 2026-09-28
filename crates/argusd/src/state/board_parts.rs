//! The parts filed under a feature — its tasks and its sequence diagrams:
//! which feature a request for one lands in, which rows it may touch, and
//! who hears about the change.
//!
//! An agent is on whichever feature its checkout points at, and a client
//! names one outright. After that the two are one path, so the id guard
//! and the push cannot differ by who asked or by which part was asked for;
//! a part says only how its rows are stored.

use argus_protocol::{ArtifactScope, PaneId, ProjectId};

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
    /// Why no agent may make this change, when none may.
    fn refused_to_agents(&self) -> Option<&'static str> {
        None
    }

    fn ids(store: &Store, key: &str, feature: &str) -> anyhow::Result<Vec<i64>>;
    fn apply(
        self,
        store: &Store,
        key: &str,
        feature: &str,
        at: i64,
        session: Option<&str>,
    ) -> anyhow::Result<()>;
    fn list(store: &Store, target: &BoardTarget) -> anyhow::Result<Self::List>;
    /// The answer on a checkout that is on no feature.
    fn empty(project_name: String) -> Self::List;
    fn broadcast(daemon: &Daemon, target: &BoardTarget);
}

/// The one feature a part request lands in, however the caller named it.
pub struct BoardTarget {
    pub project_name: String,
    pub key: String,
    pub feature: String,
}

impl Daemon {
    /// A change from an agent, applied to the feature its checkout is on,
    /// answered with the list as it stands afterwards — an agent that has
    /// just added three tasks needs the ids they were given before it can
    /// take one up.
    ///
    /// Refused when the checkout is on no feature, for the same reason a
    /// decision is: a row with nothing to be under is the pile all of this
    /// exists to end. A read there is an empty list rather than an error.
    pub fn part_action_for_agent<P: FeaturePart>(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        action: P,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<P::List> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope).to_string();
        let Some(feature) = self.feature_for_agent(&scope, artifact_scope)? else {
            if action.is_read() {
                return Ok(P::empty(scope.project_name));
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
        if let Some(reason) = action.refused_to_agents() {
            anyhow::bail!("{reason}");
        }
        let target = BoardTarget {
            project_name: scope.project_name,
            key,
            feature,
        };
        self.apply_part(&target, action, session)
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
        let (project_name, key) = self.client_artifact_scope(project, checkout)?;
        let target = BoardTarget {
            project_name,
            key,
            feature: feature.to_string(),
        };
        self.apply_part(&target, action, None)
    }

    /// Applies one change and answers with the list afterwards. Every
    /// change is also pushed, so a client that did not ask still sees it.
    fn apply_part<P: FeaturePart>(
        &self,
        target: &BoardTarget,
        action: P,
        session: Option<&str>,
    ) -> anyhow::Result<P::List> {
        if let Some(id) = action.id() {
            self.guard_part::<P>(target, id)?;
        }
        if !action.is_read() {
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default();
            action.apply(&self.store, &target.key, &target.feature, at, session)?;
            P::broadcast(self, target);
        }
        P::list(&self.store, target)
    }

    /// Refuses a row that is not under the target's feature.
    ///
    /// Ids are database-wide and a writer numbers rows from what it last
    /// read, so a stale id would otherwise let one feature's agent tick off
    /// another's work by arithmetic — or a client holding a list the tree
    /// has since moved on from rewrite a row it is no longer showing.
    fn guard_part<P: FeaturePart>(&self, target: &BoardTarget, id: i64) -> anyhow::Result<()> {
        if !P::ids(&self.store, &target.key, &target.feature)?.contains(&id) {
            anyhow::bail!("{} {id} is not under this feature", P::NOUN);
        }
        Ok(())
    }
}
