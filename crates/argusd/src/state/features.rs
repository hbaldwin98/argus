//! Which feature a checkout is working on, and the document that says
//! what that feature is.
//!
//! The decision board used to be one tree per project, which answered
//! "what has this project ever decided" — a question nobody asks. An agent
//! picking up work needs the handful of choices made while building the
//! thing it is about to touch, and everything else on a project-wide board
//! is noise it has to read past. So decisions are filed under a feature,
//! and the feature an agent is on is resolved here.
//!
//! Resolution is deliberately not a flag. A checkout points at a feature
//! and the pointer is durable, so an agent that never mentions features
//! still files its decisions in the right place; the flag exists only for
//! the case the checkout cannot answer, which is several features sharing
//! one checkout.

use argus_protocol::{
    Actor, ArtifactScope, Decision, Feature, FeatureBoard, FeatureMove, FeatureState, FeatureWrite,
    PaneId, ProjectId,
};

use super::agents::AgentScope;
use super::*;

impl Daemon {
    /// Everything an agent needs to know about where it is: the project's
    /// features, which one this checkout is on, and that feature's
    /// decisions.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn feature_board_for_agent(&self, pane_id: PaneId) -> anyhow::Result<FeatureBoard> {
        self.feature_board_for_agent_in_scope(pane_id, ArtifactScope::default())
    }

    pub fn feature_board_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        self.feature_board(&scope, artifact_scope)
    }

    fn feature_board(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<FeatureBoard> {
        let key = scope.artifact_key(artifact_scope);
        let features = self.store.features(key)?;
        let current = self.current_feature(scope, artifact_scope, &features)?;
        let decisions = self.store.decisions(key)?;
        let unfiled = decisions.iter().filter(|d| d.feature.is_none()).count();
        let scoped: Vec<Decision> = match &current {
            Some(slug) => decisions
                .into_iter()
                .filter(|d| d.feature.as_deref() == Some(slug.as_str()))
                .collect(),
            None => Vec::new(),
        };
        Ok(FeatureBoard {
            project: self.project_id_named(&scope.project_name),
            project_name: scope.project_name.clone(),
            features,
            current,
            decisions: scoped,
            unfiled,
        })
    }

    /// The feature a checkout is on: what it was last pointed at, or — if
    /// it was never pointed anywhere — the one feature that originated
    /// there.
    ///
    /// The fallback is what makes worktree-per-feature need no ceremony.
    /// It deliberately gives up when a checkout has more than one feature
    /// to its name: guessing there would file a decision under whichever
    /// happened to be older, which is worse than asking.
    fn current_feature(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
        features: &[Feature],
    ) -> anyhow::Result<Option<String>> {
        if let Some(slug) = self
            .store
            .artifact_feature_scope(&scope.checkout_path, scope.artifact_key(artifact_scope))?
        {
            if features.iter().any(|f| f.slug == slug) {
                return Ok(Some(slug));
            }
        }
        let here = scope.checkout_path.to_string_lossy();
        let mut born_here = features
            .iter()
            .filter(|f| f.origin_checkout.as_deref() == Some(here.as_ref()));
        let only = born_here.next();
        Ok(match (only, born_here.next()) {
            (Some(feature), None) => Some(feature.slug.clone()),
            _ => None,
        })
    }

    /// Opens a feature, points this checkout at it, and answers with the
    /// board as it now stands.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn open_feature_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: FeatureWrite,
    ) -> anyhow::Result<FeatureBoard> {
        self.open_feature_for_agent_in_scope(pane_id, session, write, ArtifactScope::default())
    }

    pub fn open_feature_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: FeatureWrite,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let checkout = scope.checkout_path.to_string_lossy().to_string();
        let feature = self.store.add_feature(
            key,
            &write,
            Some(checkout.as_str()),
            self.branch_of(&scope.checkout_path).as_deref(),
            at,
            session,
        )?;
        self.store
            .set_artifact_feature_scope(&scope.checkout_path, key, &feature.slug)?;
        let board = self.feature_board(&scope, artifact_scope)?;
        self.broadcast_decisions(&scope.project_name, key, self.store.decisions(key)?);
        Ok(board)
    }

    /// Points this checkout at a feature that already exists.
    ///
    /// Pushed, because a feature row names the checkouts pointed at it:
    /// this is the moment a feature somebody wrote down gains a place
    /// where work on it can be seen happening.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn select_feature_for_agent(
        &self,
        pane_id: PaneId,
        slug: &str,
    ) -> anyhow::Result<FeatureBoard> {
        self.select_feature_for_agent_in_scope(pane_id, slug, ArtifactScope::default())
    }

    pub fn select_feature_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        slug: &str,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        self.store.set_artifact_feature_scope(
            &scope.checkout_path,
            scope.artifact_key(artifact_scope),
            slug,
        )?;
        let board = self.feature_board(&scope, artifact_scope)?;
        let key = scope.artifact_key(artifact_scope);
        self.broadcast_decisions(&scope.project_name, key, self.store.decisions(key)?);
        Ok(board)
    }

    /// Adds a paragraph to the current feature's document.
    ///
    /// Refused when the checkout is on no feature, rather than opening one:
    /// what to call a feature is the decision this whole scope hangs off,
    /// and it is not one to make out of a stray note.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn append_to_feature_for_agent(
        &self,
        pane_id: PaneId,
        text: &str,
    ) -> anyhow::Result<FeatureBoard> {
        self.append_to_feature_for_agent_in_scope(pane_id, text, ArtifactScope::default())
    }

    pub fn append_to_feature_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        text: &str,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        let features = self.store.features(key)?;
        let Some(slug) = self.current_feature(&scope, artifact_scope, &features)? else {
            anyhow::bail!("this checkout is not on a feature yet");
        };
        if text.trim().is_empty() {
            anyhow::bail!("there is nothing to add");
        }
        self.store.append_to_feature(key, &slug, text)?;
        let board = self.feature_board(&scope, artifact_scope)?;
        // The brief is drawn beside the decisions, so a paragraph added
        // mid-task shows up where it is being read rather than at whatever
        // point the reader next changes something else.
        self.broadcast_decisions(&scope.project_name, key, self.store.decisions(key)?);
        Ok(board)
    }

    /// Accepts a feature, or reopens one, from the feature view.
    ///
    /// There is no agent-side counterpart. `done` means a person has
    /// looked at the work and taken it, and the agent that did the work is
    /// the one party that cannot make that call — so the state a feature
    /// is in has exactly one writer. It names the feature outright rather
    /// than resolving one from a checkout: a person is looking at the
    /// project's features, and whichever checkout they happen to have
    /// selected has nothing to do with the row under the cursor.
    pub fn move_feature_for_client(
        &self,
        project: ProjectId,
        slug: &str,
        state: FeatureState,
        detail: Option<String>,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project)?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.move_feature(
            &key,
            slug,
            &FeatureMove {
                state,
                detail,
                actor: Actor::Human,
                session: None,
                at,
            },
        )?;
        self.broadcast_decisions(&name, &key, self.store.decisions(&key)?);
        Ok(())
    }

    /// Opens a feature from the board, with no checkout to its name.
    ///
    /// A feature written down by a person is work that has not started
    /// yet, so there is nothing to record as its origin and no checkout to
    /// point at it. Whichever agent picks it up says so with
    /// `argus-hook feature use`, and that is when it gains a home.
    pub fn open_feature_for_client(
        &self,
        project: ProjectId,
        write: FeatureWrite,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.add_feature(&key, &write, None, None, at, None)?;
        self.broadcast_decisions(&name, &key, self.store.decisions(&key)?);
        Ok(())
    }

    pub fn remove_feature_for_client(&self, project: ProjectId, slug: &str) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project)?;
        self.store.remove_feature(&key, slug)?;
        self.broadcast_decisions(&name, &key, self.store.decisions(&key)?);
        Ok(())
    }

    pub fn rename_feature_for_client(
        &self,
        project: ProjectId,
        slug: &str,
        title: &str,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project)?;
        self.store.rename_feature(&key, slug, title)?;
        self.broadcast_decisions(&name, &key, self.store.decisions(&key)?);
        Ok(())
    }

    /// Rewrites a feature's brief from the view.
    pub fn set_feature_body_for_client(
        &self,
        project: ProjectId,
        slug: &str,
        body: String,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project)?;
        self.store.set_feature_body(&key, slug, &body)?;
        self.broadcast_decisions(&name, &key, self.store.decisions(&key)?);
        Ok(())
    }

    /// The feature the next decision from this pane is filed under.
    pub(super) fn feature_for_agent_in_scope(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<Option<String>> {
        let features = self.store.features(scope.artifact_key(artifact_scope))?;
        self.current_feature(scope, artifact_scope, &features)
    }

    /// The branch a checkout is on, as the last git poll saw it.
    fn branch_of(&self, path: &std::path::Path) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner
            .projects
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .find(|c| same_path(&c.path, path))
            .and_then(|c| c.git.as_ref())
            .and_then(|git| git.branch.clone())
    }
}
