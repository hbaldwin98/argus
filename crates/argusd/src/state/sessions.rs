//! A repository's agent sessions as the daemon answers for them: what its
//! harnesses kept for its checkouts, joined to the agent panes running in
//! them, and what one of them said.
//!
//! What is needed from the tree is taken under the lock and the files are
//! read after it is let go: a listing opens a hundred transcripts, and
//! every keystroke waits on that lock.

use argus_protocol::sessions::{Session, MAX_SESSIONS};
use argus_protocol::transcript::Conversation;

use super::*;
use crate::harness::transcript::{self, Places, Source};

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
        Ok(move || {
            // A repository with no checkout has nowhere a session ran.
            let listed = places.map(|places| transcript::sessions(&harnesses, &places)).unwrap_or_default();
            joined(listed, running)
        })
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
