//! A repository's agent sessions as the daemon answers for them: what its
//! harnesses kept for its checkouts, joined to the agent panes running in
//! them, and what one of them said.
//!
//! What is needed from the tree is taken under the lock and the files are
//! read after it is let go: a listing opens a hundred transcripts, and
//! every keystroke waits on that lock.

use argus_protocol::agents::sessions::{is_plain_session_id, Resume, Session, MAX_SESSIONS};
use argus_protocol::agents::transcript::{rfc3339, Conversation};

use super::*;
use crate::harness::transcript::{self, Places, Source};
use crate::store::{SeenSession, Sighting};

/// An agent pane, as a session listing needs it.
struct Running {
    pane: PaneId,
    checkout: CheckoutId,
    harness: Option<String>,
    session_id: String,
    title: String,
}

impl Daemon {
    /// The work of listing a repository's sessions, to be run off the
    /// message loop.
    pub fn list_sessions(
        &self,
        repository: RepositoryId,
    ) -> anyhow::Result<impl FnOnce() -> Vec<Session> + Send + 'static> {
        let (places, running) = {
            let inner = self.inner.lock().unwrap();
            let repository = inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .find(|r| r.id == repository)
                .ok_or_else(|| anyhow::anyhow!("no such repository"))?;
            let primary = repository
                .checkouts
                .iter()
                .find(|c| c.primary)
                .or_else(|| repository.checkouts.first())
                .map(|c| c.id);
            let places = primary.map(|primary| Places {
                checkouts: repository.checkouts.iter().map(|c| (c.id, c.path.clone())).collect(),
                worktree_root: worktree_context(&inner.projects, primary).map(|context| context.root),
                primary,
            });
            let running: Vec<Running> = repository
                .checkouts
                .iter()
                .flat_map(|c| c.panes.iter().map(move |pane| (c.id, pane)))
                .filter(|(_, pane)| pane.is_running_agent())
                .filter_map(|(checkout, pane)| {
                    Some(Running {
                        pane: pane.id,
                        checkout,
                        harness: pane.harness.clone(),
                        session_id: pane.harness_session_id.clone()?,
                        title: pane.title.clone(),
                    })
                })
                .collect();
            (places, running)
        };
        let harnesses = self.transcript_sources();
        let seen = self.store.seen_sessions().unwrap_or_else(|e| {
            tracing::warn!("could not read the sessions seen: {e}");
            Vec::new()
        });
        Ok(move || {
            // A repository with no checkout has nowhere a session ran.
            let Some(places) = places else {
                return joined(Vec::new(), running);
            };
            let listed = transcript::sessions(&harnesses, &places);
            joined(with_seen(listed, seen, &places), running)
        })
    }

    /// Records the sessions running in `panes` whose harness keeps no
    /// transcript, so each still lists — with its title, times and
    /// checkout — once its pane is gone. One a transcript records is the
    /// transcript's to list.
    pub(super) fn saw_sessions(&self, panes: &[crate::store::SessionPane]) {
        let sightings: Vec<Sighting> = panes
            .iter()
            .filter(|pane| pane.kind == PaneKind::Agent)
            .filter_map(|pane| {
                let harness = pane.harness.clone().unwrap_or_else(|| "generic".to_string());
                let keeps = self.harnesses.iter().any(|h| h.name == harness && h.transcript.is_some());
                Some(Sighting {
                    harness,
                    session_id: pane.harness_session_id.clone().filter(|_| !keeps)?,
                    checkout_path: pane.checkout_path.clone(),
                    title: pane.title.clone(),
                })
            })
            .collect();
        if let Err(e) = self.store.saw_sessions(&sightings, argus_protocol::ago::now()) {
            tracing::warn!("could not record the sessions seen: {e}");
        }
    }

    /// The work of reading one session's transcript, to be run off the
    /// message loop.
    pub fn read_session(
        &self,
        checkout: CheckoutId,
        harness: &str,
        session_id: String,
    ) -> anyhow::Result<impl FnOnce() -> Conversation + Send + 'static> {
        let path = self.checkout_path(checkout)?;
        let source = self
            .harnesses
            .iter()
            .find(|h| h.name == harness)
            .and_then(|h| h.transcript.clone());
        Ok(move || transcript::conversation(source.as_ref(), &session_id, &path))
    }

    /// How each harness that can reopen one exact conversation does it:
    /// the command of the first template that runs it with the harness's
    /// `resume_id` after, `{session_id}` left for the client to fill. The
    /// client names the command before it is run, and `resume_session`
    /// starts the same template, so what it names is what runs.
    pub fn resumes(&self) -> Vec<Resume> {
        let templates = self.templates.lock().unwrap().clone();
        let mut resumes: Vec<Resume> = Vec::new();
        for template in &templates {
            let harness = self.harness_for(template);
            if harness.resume_id.is_empty() || resumes.iter().any(|r| r.harness == harness.name) {
                continue;
            }
            let argv = template.cmd.iter().chain(&harness.resume_id).cloned().collect();
            resumes.push(Resume { harness: harness.name.to_string(), argv });
        }
        resumes
    }

    /// Starts an agent on `checkout` reopening the session `session_id`
    /// that `harness` kept, from the template `resumes` names for it.
    pub fn resume_session(
        self: &Arc<Self>,
        checkout: CheckoutId,
        harness: &str,
        session_id: &str,
    ) -> anyhow::Result<PaneId> {
        if !is_plain_session_id(session_id) {
            anyhow::bail!("{session_id:?} is not a session id Argus will put on a command line");
        }
        let template = {
            let templates = self.templates.lock().unwrap();
            templates
                .iter()
                .find(|t| {
                    let runs = self.harness_for(t);
                    runs.name == harness && !runs.resume_id.is_empty()
                })
                .map(|t| t.name.clone())
        };
        let template = template.ok_or_else(|| anyhow::anyhow!("no agent template can resume a {harness} session"))?;
        self.start_agent(checkout, &template, Start::Picked, Some(session_id.to_string()), None)
    }

    /// Each harness that keeps transcripts, named with where. A wrapper
    /// configured to read another CLI's files (`transcript = "claude"`)
    /// would list every one of them a second time, so a place already
    /// listed is listed under the harness named first.
    fn transcript_sources(&self) -> Vec<(String, Source)> {
        let mut sources: Vec<(String, Source)> = Vec::new();
        for harness in &self.harnesses {
            let Some(source) = &harness.transcript else {
                continue;
            };
            if !sources.iter().any(|(_, listed)| listed == source) {
                sources.push((harness.name.clone(), source.clone()));
            }
        }
        sources
    }
}

/// The listed sessions and those Argus saw in the repository's checkouts
/// that no transcript lists, newest first. A seen session's times are the
/// first and last Argus saw of it. One whose checkout is gone is listed as
/// a transcript from there would be — under the primary checkout, its
/// worktree named as removed — when the directory was where the repository
/// puts worktrees, and not at all otherwise, as nothing then says it was
/// this repository's. A directory still there that is no checkout is not
/// listed either: resumed on the primary, it would run where it never did.
fn with_seen(mut listed: Vec<Session>, seen: Vec<SeenSession>, places: &Places) -> Vec<Session> {
    for seen in seen {
        if listed.iter().any(|s| s.session_id == seen.session_id) {
            continue;
        }
        let at = places.checkouts.iter().find(|(_, path)| same_path(path, &seen.checkout_path));
        let (checkout, removed_worktree) = match at {
            Some(&(checkout, _)) => (checkout, None),
            None if transcript::removed_worktree(&seen.checkout_path, places) => {
                (places.primary, Some(seen.checkout_path.to_string_lossy().into_owned()))
            }
            None => continue,
        };
        listed.push(Session {
            harness: seen.harness,
            session_id: seen.session_id,
            checkout,
            removed_worktree,
            title: Some(seen.title).filter(|title| !title.is_empty()),
            started: Some(rfc3339(seen.started)),
            ended: Some(rfc3339(seen.ended)),
            modified: Some(seen.ended.saturating_mul(1000)),
            turns: None,
            pane: None,
        });
    }
    // Stable, so two sessions written the same millisecond keep their order.
    listed.sort_by_key(|s| std::cmp::Reverse(s.modified));
    listed.truncate(MAX_SESSIONS);
    listed
}

/// The listed sessions with the pane each is running in. An agent running
/// a session no transcript lists — its harness keeps none, or has not
/// written it yet — is listed first, as the newest there is.
fn joined(mut sessions: Vec<Session>, running: Vec<Running>) -> Vec<Session> {
    let mut unlisted = Vec::new();
    for agent in running {
        match sessions.iter_mut().find(|s| s.session_id == agent.session_id) {
            Some(session) => {
                session.pane = Some(agent.pane);
                // The harness it was started under is the one that resumes
                // it, whichever's files it was found in.
                if let Some(harness) = agent.harness {
                    session.harness = harness;
                }
            }
            None => unlisted.push(Session {
                harness: agent.harness.unwrap_or_else(|| "generic".to_string()),
                session_id: agent.session_id,
                checkout: agent.checkout,
                removed_worktree: None,
                title: Some(agent.title).filter(|title| !title.is_empty()),
                started: None,
                ended: None,
                modified: None,
                turns: None,
                pane: Some(agent.pane),
            }),
        }
    }
    unlisted.extend(sessions);
    unlisted.truncate(MAX_SESSIONS);
    unlisted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(id: &str, harness: &str) -> Session {
        Session {
            harness: harness.into(),
            session_id: id.into(),
            checkout: CheckoutId(1),
            removed_worktree: None,
            title: Some(format!("{id} title")),
            started: None,
            ended: None,
            modified: Some(1),
            turns: Some(3),
            pane: None,
        }
    }

    fn running(pane: u64, id: &str, harness: &str) -> Running {
        Running {
            pane: PaneId(pane),
            checkout: CheckoutId(2),
            harness: Some(harness.into()),
            session_id: id.into(),
            title: format!("pane {pane}"),
        }
    }

    #[test]
    fn a_session_running_in_a_pane_says_which() {
        let sessions = joined(
            vec![listed("a", "claude"), listed("b", "claude")],
            vec![running(9, "b", "herdr")],
        );
        assert_eq!(sessions[1].pane, Some(PaneId(9)));
        assert_eq!(sessions[1].harness, "herdr", "resumed by the harness it runs under");
        assert_eq!(sessions[0].pane, None);
    }

    fn seen(id: &str, checkout: &str, ended: u64) -> SeenSession {
        SeenSession {
            harness: "generic".into(),
            session_id: id.into(),
            checkout_path: PathBuf::from(checkout),
            title: format!("{id} seen"),
            started: 1_791_968_400,
            ended,
        }
    }

    #[test]
    fn a_session_whose_harness_keeps_no_transcript_lists_from_what_argus_saw() {
        let places = Places {
            checkouts: vec![(CheckoutId(1), PathBuf::from("/src/app")), (CheckoutId(2), PathBuf::from("/src/app-wt"))],
            worktree_root: None,
            primary: CheckoutId(1),
        };
        let mut transcribed = listed("a", "claude");
        transcribed.modified = Some(1_791_968_500_000);
        let sessions = with_seen(
            vec![transcribed],
            vec![
                seen("g1", "/src/app-wt", 1_791_968_600),
                seen("a", "/src/app", 1_791_968_700),
                seen("gone", "/src/elsewhere", 1_791_968_800),
            ],
            &places,
        );
        let ids: Vec<_> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, ["g1", "a"], "newest first; a transcript's own listing wins; a gone checkout lists nothing");
        let g1 = &sessions[0];
        assert_eq!((g1.harness.as_str(), g1.checkout), ("generic", CheckoutId(2)));
        assert_eq!(g1.title.as_deref(), Some("g1 seen"));
        assert_eq!(g1.started.as_deref(), Some("2026-10-14T09:00:00Z"));
        assert_eq!(g1.ended.as_deref(), Some("2026-10-14T09:03:20Z"));
        assert_eq!(g1.turns, None);
    }

    #[test]
    fn a_seen_session_from_a_removed_worktree_lists_as_removed_and_one_from_elsewhere_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        let root = dir.path().join("worktrees").join("app");
        // A directory still there that is no checkout of the repository.
        let stray = app.join("notes");
        std::fs::create_dir_all(&stray).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let places = Places {
            checkouts: vec![(CheckoutId(1), app.clone())],
            worktree_root: Some(root.clone()),
            primary: CheckoutId(1),
        };
        let text = |p: PathBuf| p.to_string_lossy().into_owned();
        let sessions = with_seen(
            Vec::new(),
            vec![
                seen("rooted", &text(root.join("spike")), 1_791_968_900),
                seen("tooled", &text(app.join(".claude").join("worktrees").join("fix")), 1_791_968_800),
                seen("elsewhere", &text(dir.path().join("other")), 1_791_968_700),
                seen("stray", &text(stray.clone()), 1_791_968_600),
            ],
            &places,
        );
        let listed: Vec<_> = sessions
            .iter()
            .map(|s| (s.session_id.as_str(), s.checkout, s.removed_worktree.is_some()))
            .collect();
        assert_eq!(listed, [("rooted", CheckoutId(1), true), ("tooled", CheckoutId(1), true)]);
        assert_eq!(sessions[0].removed_worktree, Some(text(root.join("spike"))), "named as Argus saw it");
    }

    #[tokio::test]
    async fn a_closed_session_of_a_harness_with_no_transcript_still_lists_and_says_it_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::state::tests::running_claude_config(dir.path());
        // A harness no config names is the generic one, which keeps none.
        config.agents[0].harness = Some("plain".into());
        let d = Daemon::new(config);
        let repository = d.snapshot()[0].repositories[0].id;
        let checkout = crate::state::tests::only_checkout(&d);
        let pane = d.spawn_agent(checkout, "claude").unwrap();
        d.set_pane_session_id(pane, "plain-1");
        d.report_pane_title(pane, None, "Tidy the release notes");
        d.close_pane(pane).unwrap();

        let sessions = d.list_sessions(repository).unwrap()();
        let session = sessions.iter().find(|s| s.session_id == "plain-1").expect("listed after its pane closed");
        assert_eq!(session.harness, "generic");
        assert_eq!(session.checkout, checkout);
        assert_eq!(session.title.as_deref(), Some("Tidy the release notes"));
        assert!(session.started.is_some() && session.ended.is_some() && session.modified.is_some());
        assert_eq!(session.pane, None);

        let read = d.read_session(checkout, &session.harness, session.session_id.clone()).unwrap()();
        assert_eq!(read, Conversation::NoTranscript);
    }

    #[test]
    fn an_agent_whose_harness_keeps_no_transcript_is_still_a_session() {
        let sessions = joined(vec![listed("a", "claude")], vec![running(4, "oc-1", "opencode")]);
        let ids: Vec<_> = sessions.iter().map(|s| (s.session_id.as_str(), s.pane)).collect();
        assert_eq!(ids, [("oc-1", Some(PaneId(4))), ("a", None)]);
        assert_eq!(sessions[0].title.as_deref(), Some("pane 4"));
        assert_eq!(sessions[0].checkout, CheckoutId(2));
        assert_eq!(sessions[0].turns, None);
    }
}
