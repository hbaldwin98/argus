//! Which feature a checkout is working on, and the document that says
//! what that feature is — changed through one path, whoever asks.
//!
//! The decision board used to be one tree per project, which answered
//! "what has this project ever decided" — a question nobody asks. An agent
//! picking up work needs the handful of choices made while building the
//! thing it is about to touch, and everything else on a broad board
//! is noise it has to read past. So decisions are filed under a feature,
//! and the feature an agent is on is resolved here.
//!
//! Resolution is deliberately not a flag. A checkout points at a feature
//! and the pointer is durable, so an agent that never mentions features
//! still files its decisions in the right place; the flag — a request
//! naming a feature, `--feature` to the helper — exists only for the case
//! the checkout cannot answer, which is several features sharing one
//! checkout, where moving its pointer would move every other agent's.

use argus_protocol::{
    Actor, ArtifactScope, Decision, Feature, FeatureAction, FeatureBoard, FeatureMove,
    FeatureState, PaneId, ProjectId,
};

use super::agents::AgentScope;
use super::*;

/// Where an agent's request is filed: on which board, and on it, a feature
/// named outright or else the one the agent's checkout is on. A bare
/// `ArtifactScope` is a filing that names none, which is what every
/// request did before a request could name one.
#[derive(Debug, Clone, Default)]
pub struct Filing {
    pub scope: ArtifactScope,
    pub feature: Option<String>,
}

impl From<ArtifactScope> for Filing {
    fn from(scope: ArtifactScope) -> Self {
        Filing {
            scope,
            feature: None,
        }
    }
}

impl Daemon {
    /// Everything an agent needs to know about where it is: the project's
    /// features, which one this checkout is on, and that feature's
    /// decisions.
    ///
    /// Asked about a feature by name, the board is about that one: its
    /// decisions, and `current` naming it.
    pub fn feature_board_for_agent(
        &self,
        pane_id: PaneId,
        filing: impl Into<Filing>,
    ) -> anyhow::Result<FeatureBoard> {
        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        self.feature_board(&scope, filing.scope, filing.feature.as_deref())
    }

    fn feature_board(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
        named: Option<&str>,
    ) -> anyhow::Result<FeatureBoard> {
        let key = scope.artifact_key(artifact_scope);
        let features = self.store.features(key)?;
        let current = self.board_feature(scope, artifact_scope, &features, named)?;
        let decisions = self.store.decisions(key)?;
        let unfiled = decisions.iter().filter(|d| d.feature.is_none()).count();
        // An agent reads everything but what was withdrawn, which is kept
        // for people reading the history in the view; `decisions_for_agent`
        // says why a question still open is not held back.
        let scoped: Vec<Decision> = match &current {
            Some(slug) => decisions
                .into_iter()
                .filter(|d| d.feature.as_deref() == Some(slug.as_str()) && !d.withdrawn())
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

    /// The feature a request is about: the one it names, which has to be on
    /// this board, or else the one this checkout is on.
    fn board_feature(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
        features: &[Feature],
        named: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        match named {
            Some(slug) if features.iter().any(|f| f.slug == slug) => Ok(Some(slug.to_string())),
            Some(slug) => {
                anyhow::bail!("no feature `{slug}` on this board; `feature list` names them")
            }
            None => self.current_feature(scope, artifact_scope, features),
        }
    }

    /// The feature this checkout is explicitly assigned within the selected
    /// artifact scope. Origin remains history and never recreates a removed
    /// or transferred assignment.
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
        Ok(None)
    }

    /// A change from an agent, answered with the board as it stands
    /// afterwards: every one of these changes what the next `decide` from
    /// this checkout is filed under, so the agent has to be able to see
    /// where it now is.
    pub fn feature_action_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        action: FeatureAction,
        filing: impl Into<Filing>,
    ) -> anyhow::Result<FeatureBoard> {
        let filing = filing.into();
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(filing.scope);
        let by = FeatureActor::Agent {
            scope: &scope,
            session,
            filing: &filing,
        };
        let about = self.apply_feature(&scope.project_name, key, &by, action)?;
        self.feature_board(&scope, filing.scope, about.as_deref())
    }

    /// A change from the feature view, which names the board by the
    /// checkout it is looking at rather than resolving one from a pane:
    /// whichever checkout a person has selected has nothing to do with the
    /// row under their cursor. The answer is the push every client gets.
    pub fn feature_action_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        action: FeatureAction,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        let by = FeatureActor::Person { project, checkout };
        self.apply_feature(&name, &key, &by, action).map(drop)
    }

    /// Applies one change to the board `key` names and pushes the board to
    /// every client. Answers with the feature the change was about when
    /// the asker could not have named it: the one an append resolved.
    ///
    /// Every rule that differs by who asked is an arm of this match, so
    /// the two sides cannot drift by being written twice:
    ///
    /// - an agent's `Open` records where it was cut and points the
    ///   agent's checkout at it; a person's is work not yet started;
    /// - `Select` and `Append` are about the agent's checkout, which a
    ///   person does not have;
    /// - an agent's `Done` and `Reopen` are recorded as the agent's, with
    ///   its session, and say nothing: what was said is the person's;
    /// - an agent drops only a feature with nothing under it;
    /// - `Transfer` and `Capture` are the view's.
    fn apply_feature(
        &self,
        project_name: &str,
        key: &str,
        by: &FeatureActor<'_>,
        action: FeatureAction,
    ) -> anyhow::Result<Option<String>> {
        let at = super::decisions::now();
        let mut about = None;
        match action {
            FeatureAction::Open(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                match by {
                    // Nothing to record as its origin and no checkout to
                    // point at it: whichever agent picks it up says so
                    // with `argus-hook feature use`, and that is when it
                    // gains a home.
                    FeatureActor::Person { .. } => {
                        self.store.add_feature(key, &write, None, None, at, None)?;
                    }
                    FeatureActor::Agent { scope, session, .. } => {
                        let checkout = scope.checkout_path.to_string_lossy();
                        let feature = self.store.add_feature(
                            key,
                            &write,
                            Some(&checkout),
                            self.branch_of(&scope.checkout_path).as_deref(),
                            at,
                            *session,
                        )?;
                        self.store
                            .set_artifact_feature_scope(&scope.checkout_path, key, &feature.slug)?;
                    }
                }
            }
            // Pushed, because a feature row names the checkouts pointed at
            // it: this is the moment a feature somebody wrote down gains a
            // place where work on it can be seen happening.
            FeatureAction::Select { slug } => {
                let FeatureActor::Agent { scope, .. } = by else {
                    anyhow::bail!("a person moves a feature between checkouts with a transfer");
                };
                self.store
                    .set_artifact_feature_scope(&scope.checkout_path, key, &slug)?;
            }
            // Refused when the checkout is on no feature, rather than
            // opening one: what to call a feature is the decision this
            // whole scope hangs off, and it is not one to make out of a
            // stray note. Pushed, because the brief is drawn beside the
            // decisions, and a paragraph added mid-task belongs where it
            // is being read.
            FeatureAction::Append { text } => {
                let FeatureActor::Agent { scope, filing, .. } = by else {
                    anyhow::bail!("a person rewrites a brief whole");
                };
                let features = self.store.features(key)?;
                let named = filing.feature.as_deref();
                let Some(slug) = self.board_feature(scope, filing.scope, &features, named)? else {
                    anyhow::bail!("this checkout is not on a feature yet");
                };
                if text.trim().is_empty() {
                    anyhow::bail!("there is nothing to add");
                }
                self.store.append_to_feature(key, &slug, &text)?;
                about = Some(slug);
            }
            FeatureAction::Done { slug, detail } => {
                self.move_feature(key, &slug, FeatureState::Done, detail, by, at)?
            }
            FeatureAction::Reopen { slug, detail } => {
                self.move_feature(key, &slug, FeatureState::Open, detail, by, at)?
            }
            FeatureAction::Retitle { slug, title } => self.store.rename_feature(key, &slug, &title)?,
            FeatureAction::Rewrite { slug, body } => self.store.set_feature_body(key, &slug, &body)?,
            FeatureAction::Drop { slug } => {
                if let FeatureActor::Agent { .. } = by {
                    self.refuse_dropping_work(key, &slug)?;
                }
                self.store.remove_feature(key, &slug)?;
            }
            FeatureAction::Hold { slug, reason } => {
                self.store.hold_feature(key, &slug, Some(&reason))?
            }
            FeatureAction::Unhold { slug } => self.store.hold_feature(key, &slug, None)?,
            FeatureAction::Wait { slug, on } => self.store.add_feature_wait(key, &slug, &on)?,
            FeatureAction::Unwait { slug, on } => {
                self.store.remove_feature_wait(key, &slug, &on)?
            }
            FeatureAction::Link { slug, link } => {
                let link = link.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                self.store.link_feature(key, &slug, &link)?;
            }
            FeatureAction::Unlink { slug, key: ticket } => {
                self.store.unlink_feature(key, &slug, &ticket)?
            }
            FeatureAction::Transfer { slug, to } => {
                let FeatureActor::Person { project, checkout } = by else {
                    anyhow::bail!("an agent takes a feature up with `argus-hook feature use`");
                };
                let (source, destination) = self.transfer_paths(*project, *checkout, to)?;
                self.store
                    .transfer_artifact_feature_scope(key, &slug, &source, &destination)?;
            }
            // A new feature is opened as a person's `Open` is, for the same
            // reason: a note is work not yet started.
            FeatureAction::Capture { into, capture } => {
                let FeatureActor::Person { .. } = by else {
                    anyhow::bail!("a note is captured from the feature view");
                };
                let capture = capture
                    .checked(into.is_none())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let slug = self.store.capture_feature(key, into.as_deref(), &capture, at)?;
                // The tasks as well as the board: a client holding the
                // feature's list open is owed the rows a note just added.
                let _ = self.tasks_tx.send(argus_protocol::TaskList {
                    repositories: self.repositories_keyed(key),
                    project_name: project_name.to_string(),
                    tasks: self.store.tasks(key, &slug)?,
                    feature: Some(slug),
                });
            }
        }
        self.broadcast_decisions(project_name, key, self.store.decisions(key)?);
        Ok(about)
    }

    /// Accepts or reopens a feature. A person's move is the person's; an
    /// agent's is carried out on a person's word and logged as the
    /// agent's, with its session, so the history says who carried the word
    /// out rather than claiming the person made the move themselves.
    fn move_feature(
        &self,
        key: &str,
        slug: &str,
        state: FeatureState,
        detail: Option<String>,
        by: &FeatureActor<'_>,
        at: i64,
    ) -> anyhow::Result<()> {
        let (actor, session) = match by {
            FeatureActor::Person { .. } => (Actor::Human, None),
            FeatureActor::Agent { session, .. } => {
                if detail.is_some() {
                    anyhow::bail!("what was said about a move is the person's to say");
                }
                (Actor::Agent, session.map(str::to_string))
            }
        };
        let mv = FeatureMove {
            state,
            detail,
            actor,
            session,
            at,
        };
        self.store.move_feature(key, slug, &mv)?;
        Ok(())
    }

    /// Refuses an agent's drop of a feature with anything filed under it.
    ///
    /// Removing a feature takes its tasks and diagrams with it and leaves
    /// its decisions unfiled, and work other agents filed there is not an
    /// agent's to take back in one move: it can empty the feature a task at
    /// a time, each of which it can undo, and a person can remove a whole
    /// one from the feature view.
    fn refuse_dropping_work(&self, key: &str, slug: &str) -> anyhow::Result<()> {
        let tasks = self.store.tasks(key, slug)?.len();
        let diagrams = self.store.sequence_diagrams(key, slug)?.len();
        let decisions = self
            .store
            .decisions(key)?
            .iter()
            .filter(|d| d.feature.as_deref() == Some(slug))
            .count();
        if tasks + diagrams + decisions > 0 {
            anyhow::bail!(
                "{slug} has {tasks} task(s), {decisions} decision(s) and {diagrams} diagram(s) \
                 under it; drop its tasks first, or a person can remove it from the feature view"
            );
        }
        Ok(())
    }

    /// Where a transfer moves a feature from and to: two checkouts of one
    /// repository, since a board is a repository's.
    fn transfer_paths(
        &self,
        project: ProjectId,
        source: CheckoutId,
        destination: CheckoutId,
    ) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
        let inner = self.inner.lock().unwrap();
        let project = inner
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .ok_or_else(|| anyhow::anyhow!("no such project"))?;
        let (repository, source_checkout) = project
            .repositories
            .iter()
            .find_map(|repository| {
                repository
                    .checkouts
                    .iter()
                    .find(|checkout| checkout.id == source)
                    .map(|checkout| (repository, checkout))
            })
            .ok_or_else(|| anyhow::anyhow!("source checkout is not in this project"))?;
        let destination_checkout = repository
            .checkouts
            .iter()
            .find(|checkout| checkout.id == destination)
            .ok_or_else(|| anyhow::anyhow!("destination checkout is not in the source repository"))?;
        Ok((source_checkout.path.clone(), destination_checkout.path.clone()))
    }

    /// The feature the next decision from this pane is filed under.
    pub(super) fn feature_for_agent(
        &self,
        scope: &AgentScope,
        filing: &Filing,
    ) -> anyhow::Result<Option<String>> {
        let features = self.store.features(scope.artifact_key(filing.scope))?;
        self.board_feature(scope, filing.scope, &features, filing.feature.as_deref())
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

/// Who is changing a feature, and where they are looking from.
enum FeatureActor<'a> {
    /// A person at a client, on the board of the checkout they selected.
    Person {
        project: ProjectId,
        checkout: CheckoutId,
    },
    /// An agent: the conversation it is, the checkout its pane is in, and
    /// the feature its request names, if it names one.
    Agent {
        scope: &'a AgentScope,
        session: Option<&'a str>,
        filing: &'a Filing,
    },
}
