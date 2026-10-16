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

use super::asker::Asker;
use super::board_address::BoardAddress;
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
        let address = self.address_for_agent(pane_id, filing.scope)?;
        self.feature_board(&address, filing.feature.as_deref())
    }

    pub(in crate::state) fn feature_board(
        &self,
        address: &BoardAddress,
        named: Option<&str>,
    ) -> anyhow::Result<FeatureBoard> {
        let features = self.store.features(&address.key)?;
        let current = self.board_feature(address, &features, named)?;
        let decisions = self.store.decisions(&address.key)?;
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
            project: Some(address.project),
            project_name: address.project_name.clone(),
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
        address: &BoardAddress,
        features: &[Feature],
        named: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        match named {
            Some(slug) if features.iter().any(|f| f.slug == slug) => Ok(Some(slug.to_string())),
            Some(slug) => {
                anyhow::bail!("no feature `{slug}` on this board; `feature list` names them")
            }
            None => self.current_feature(address, features),
        }
    }

    /// The feature this checkout is explicitly assigned within the selected
    /// artifact scope. Origin remains history and never recreates a removed
    /// or transferred assignment.
    fn current_feature(
        &self,
        address: &BoardAddress,
        features: &[Feature],
    ) -> anyhow::Result<Option<String>> {
        let Some(checkout) = &address.checkout_path else {
            return Ok(None);
        };
        if let Some(slug) = self.store.artifact_feature_scope(checkout, &address.key)? {
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
        let address = self.address_for_agent(pane_id, filing.scope)?;
        let by = Asker::Agent {
            pane: pane_id,
            session,
        };
        let about = self.apply_feature(&address, by, filing.feature.as_deref(), action)?;
        self.feature_board(&address, about.as_deref())
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
        let address = self.address_for_client(project, checkout)?;
        self.apply_feature(&address, Asker::Person, None, action).map(drop)
    }

    /// Applies one change to the board `address` names and pushes it to
    /// every client. `named` is the feature an agent's request names, if
    /// it names one. Answers with the feature the change was about when
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
    pub(in crate::state) fn apply_feature(
        &self,
        address: &BoardAddress,
        by: Asker<'_>,
        named: Option<&str>,
        action: FeatureAction,
    ) -> anyhow::Result<Option<String>> {
        let key = address.key.as_str();
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
                    Asker::Person => {
                        self.store.add_feature(key, &write, None, None, at, None)?;
                    }
                    Asker::Agent { session, .. } => {
                        let path = address.checkout()?;
                        let checkout = path.to_string_lossy();
                        let feature = self.store.add_feature(
                            key,
                            &write,
                            Some(&checkout),
                            self.branch_of(path).as_deref(),
                            at,
                            session,
                        )?;
                        self.store.set_artifact_feature_scope(path, key, &feature.slug)?;
                    }
                }
            }
            // Pushed, because a feature row names the checkouts pointed at
            // it: this is the moment a feature somebody wrote down gains a
            // place where work on it can be seen happening.
            FeatureAction::Select { slug } => {
                let Asker::Agent { .. } = by else {
                    anyhow::bail!("a person moves a feature between checkouts with a transfer");
                };
                self.store
                    .set_artifact_feature_scope(address.checkout()?, key, &slug)?;
            }
            // Refused when the checkout is on no feature, rather than
            // opening one: what to call a feature is the decision this
            // whole scope hangs off, and it is not one to make out of a
            // stray note. Pushed, because the brief is drawn beside the
            // decisions, and a paragraph added mid-task belongs where it
            // is being read.
            FeatureAction::Append { text } => {
                let Asker::Agent { .. } = by else {
                    anyhow::bail!("a person rewrites a brief whole");
                };
                let features = self.store.features(key)?;
                let Some(slug) = self.board_feature(address, &features, named)? else {
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
                if let Asker::Agent { .. } = by {
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
                let Asker::Person = by else {
                    anyhow::bail!("an agent takes a feature up with `argus-hook feature use`");
                };
                let (source, destination) = self.transfer_paths(address, to)?;
                self.store
                    .transfer_artifact_feature_scope(key, &slug, &source, &destination)?;
            }
            // A new feature is opened as a person's `Open` is, for the same
            // reason: a note is work not yet started.
            FeatureAction::Capture { into, capture } => {
                let Asker::Person = by else {
                    anyhow::bail!("a note is captured from the feature view");
                };
                let capture = capture
                    .checked(into.is_none())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let slug = self.store.capture_feature(key, into.as_deref(), &capture, at)?;
                // The tasks as well as the board: a client holding the
                // feature's list open is owed the rows a note just added.
                if address.is_pushed() {
                    self.pushes.board(super::BoardNews::Tasks(Box::new(argus_protocol::TaskList {
                        repositories: address.repositories.clone(),
                        project_name: address.project_name.clone(),
                        tasks: self.store.tasks(key, &slug)?,
                        feature: Some(slug),
                    })));
                }
            }
        }
        self.broadcast_decisions(address, self.store.decisions(key)?);
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
        by: Asker<'_>,
        at: i64,
    ) -> anyhow::Result<()> {
        let (actor, session) = match by {
            Asker::Person => (Actor::Human, None),
            Asker::Agent { session, .. } => {
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

    /// Where a transfer moves a feature from and to: the checkout the board
    /// was addressed from, and another checkout of its repository, since a
    /// board is a repository's.
    fn transfer_paths(
        &self,
        address: &BoardAddress,
        destination: CheckoutId,
    ) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
        let source = address.checkout()?;
        let inner = self.inner.lock().unwrap();
        let project = inner
            .projects
            .iter()
            .find(|candidate| candidate.id == address.project)
            .ok_or_else(|| anyhow::anyhow!("no such project"))?;
        let holding_source = |repository: &&Repository| {
            repository.checkouts.iter().any(|checkout| checkout.path == source)
        };
        if !project.repositories.iter().any(|repository| holding_source(&repository)) {
            anyhow::bail!("source checkout is not in this project");
        }
        // A repository listed twice has a checkout at the source path in
        // each listing; the one meant is the listing the destination is in.
        let destination_checkout = project
            .repositories
            .iter()
            .filter(holding_source)
            .find_map(|repository| {
                repository.checkouts.iter().find(|checkout| checkout.id == destination)
            })
            .ok_or_else(|| anyhow::anyhow!("destination checkout is not in the source repository"))?;
        Ok((source.to_path_buf(), destination_checkout.path.clone()))
    }

    /// The feature an agent's request is about: the one it names, or else
    /// the one its checkout is on.
    pub(in crate::state) fn feature_for_agent(
        &self,
        address: &BoardAddress,
        named: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        let features = self.store.features(&address.key)?;
        self.board_feature(address, &features, named)
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

