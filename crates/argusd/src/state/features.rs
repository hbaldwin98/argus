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
    Actor, Decision, Feature, FeatureBoard, FeatureMove, FeatureState, FeatureWrite, PaneId,
    ProjectId,
};

use super::agents::AgentScope;
use super::*;

impl Daemon {
    /// Everything an agent needs to know about where it is: the project's
    /// features, which one this checkout is on, and that feature's
    /// decisions.
    pub fn feature_board_for_agent(&self, pane_id: PaneId) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        self.feature_board(&scope)
    }

    fn feature_board(&self, scope: &AgentScope) -> anyhow::Result<FeatureBoard> {
        let features = self.store.features(&scope.project_name)?;
        let current = self.current_feature(scope, &features)?;
        let decisions = self.store.decisions(&scope.project_name)?;
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
        features: &[Feature],
    ) -> anyhow::Result<Option<String>> {
        if let Some(slug) = self
            .store
            .feature_scope(&scope.checkout_path, &scope.project_name)?
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
    pub fn open_feature_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: FeatureWrite,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let checkout = scope.checkout_path.to_string_lossy().to_string();
        let feature = self.store.add_feature(
            &scope.project_name,
            &write,
            Some(checkout.as_str()),
            self.branch_of(&scope.checkout_path).as_deref(),
            at,
            session,
        )?;
        self.store
            .set_feature_scope(&scope.checkout_path, &scope.project_name, &feature.slug)?;
        let board = self.feature_board(&scope)?;
        self.broadcast_decisions(
            &scope.project_name,
            self.store.decisions(&scope.project_name)?,
        );
        Ok(board)
    }

    /// Points this checkout at a feature that already exists.
    pub fn select_feature_for_agent(
        &self,
        pane_id: PaneId,
        slug: &str,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        self.store
            .set_feature_scope(&scope.checkout_path, &scope.project_name, slug)?;
        self.feature_board(&scope)
    }

    /// Adds a paragraph to the current feature's document.
    ///
    /// Refused when the checkout is on no feature, rather than opening one:
    /// what to call a feature is the decision this whole scope hangs off,
    /// and it is not one to make out of a stray note.
    pub fn append_to_feature_for_agent(
        &self,
        pane_id: PaneId,
        text: &str,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        let features = self.store.features(&scope.project_name)?;
        let Some(slug) = self.current_feature(&scope, &features)? else {
            anyhow::bail!("this checkout is not on a feature yet");
        };
        if text.trim().is_empty() {
            anyhow::bail!("there is nothing to add");
        }
        self.store
            .append_to_feature(&scope.project_name, &slug, text)?;
        self.feature_board(&scope)
    }

    /// Moves the current feature to another column.
    ///
    /// An agent may pick work up, say it is stuck, and offer what it has.
    /// It may not accept its own work: `done` is the human's move, and
    /// letting the worker make it would leave the review column a place
    /// things pass through rather than stop at.
    pub fn move_feature_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        state: FeatureState,
        detail: Option<&str>,
    ) -> anyhow::Result<FeatureBoard> {
        let scope = self.agent_scope(pane_id)?;
        if !state.agent_may_enter() {
            anyhow::bail!(
                "an agent cannot accept its own work — submit it with                  `argus-hook feature submit \"<what you did>\"` and let a human accept it"
            );
        }
        let features = self.store.features(&scope.project_name)?;
        let Some(slug) = self.current_feature(&scope, &features)? else {
            anyhow::bail!("this checkout is not on a feature yet");
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.move_feature(
            &scope.project_name,
            &slug,
            &FeatureMove {
                state,
                detail: detail.map(str::to_string),
                actor: Actor::Agent,
                session: session.map(str::to_string),
                at,
            },
        )?;
        let board = self.feature_board(&scope)?;
        self.broadcast_decisions(
            &scope.project_name,
            self.store.decisions(&scope.project_name)?,
        );
        Ok(board)
    }

    /// Moves a feature from the board view.
    ///
    /// Unlike the agent's move this one names the feature outright: a
    /// human is looking at the whole project's board, and the checkout
    /// they happen to have selected has nothing to do with the card under
    /// the cursor. It is also the only move that may reach `done`.
    pub fn move_feature_for_client(
        &self,
        project: ProjectId,
        slug: &str,
        state: FeatureState,
        detail: Option<String>,
    ) -> anyhow::Result<()> {
        let name = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .find(|p| p.id == project)
                .map(|p| p.name.clone())
                .ok_or_else(|| anyhow::anyhow!("no such project"))?
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.move_feature(
            &name,
            slug,
            &FeatureMove {
                state,
                detail,
                actor: Actor::Human,
                session: None,
                at,
            },
        )?;
        self.broadcast_decisions(&name, self.store.decisions(&name)?);
        Ok(())
    }

    /// Rewrites a feature's brief from the view.
    pub fn set_feature_body_for_client(
        &self,
        project: ProjectId,
        slug: &str,
        body: String,
    ) -> anyhow::Result<()> {
        let name = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .find(|p| p.id == project)
                .map(|p| p.name.clone())
                .ok_or_else(|| anyhow::anyhow!("no such project"))?
        };
        self.store.set_feature_body(&name, slug, &body)?;
        self.broadcast_decisions(&name, self.store.decisions(&name)?);
        Ok(())
    }

    /// The feature the next decision from this pane is filed under.
    pub(super) fn feature_for_agent(&self, scope: &AgentScope) -> anyhow::Result<Option<String>> {
        let features = self.store.features(&scope.project_name)?;
        self.current_feature(scope, &features)
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
