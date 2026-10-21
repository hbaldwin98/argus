//! What the window has asked of a repository's agent sessions — the list,
//! what one of them said, and how each harness resumes one — and what it
//! holds of the answers, or why the daemon refused one. Only the latest of
//! each is wanted: an answer to a request since replaced is dropped. Asked
//! only of a daemon that listed `SESSIONS`, since an older one hangs up on
//! a message it does not know.
//!
//! It also holds what each pane this window resumed a session in was
//! resumed from. The tree does not carry it, and need not: the pane is the
//! one `Created` named in answer to this window's own resume, and the
//! session's time and turns are the daemon's listing of it, so nothing
//! here is a guess. Another client, or this one restarted, does not know.

use std::collections::HashMap;

use argus_protocol::agents::sessions::{is_plain_session_id, Resume, Session};
use argus_protocol::agents::transcript::{rfc3339, Conversation};
use argus_protocol::{ClientMsg, PaneId, RepositoryId, ServerMsg, SESSIONS};

use crate::connection::page::Resumed;
use crate::chrome::status::StatusBar;

#[derive(Debug, Default)]
pub struct Sessions {
    /// The listing asked for, and the repository it is of.
    listing: Option<(u64, RepositoryId)>,
    listed: Option<(RepositoryId, Vec<Session>)>,
    /// Why the daemon would not list a repository's sessions. Held as that
    /// listing's answer, so the dialog says it rather than waiting on it.
    refused_list: Option<(RepositoryId, String)>,
    /// The read asked for, and the session it is of.
    reading: Option<(u64, String)>,
    read: Option<(String, Conversation)>,
    /// Why the daemon would not read a session, by its id. Held as the
    /// read's answer: a read wanted while nothing is held would otherwise
    /// be asked again, and refused again, on every turn.
    refused_read: Option<(String, String)>,
    resuming: Option<u64>,
    /// Each harness that resumes a session by id, with the argv it would
    /// run, `{session_id}` unfilled; `None` until the daemon has said.
    resumes: Option<Vec<Resume>>,
    /// The panes a session was resumed in, by this window, while each
    /// lasts.
    resumed: HashMap<PaneId, Resumed>,
}

impl Sessions {
    /// The request for `repository`'s sessions, or `None` when the daemon
    /// cannot take it.
    pub fn list(&mut self, status: &StatusBar, repository: RepositoryId, request_id: u64) -> Option<ClientMsg> {
        status.daemon_can(SESSIONS).then_some(())?;
        self.listing = Some((request_id, repository));
        self.refused_list.take_if(|(refused, _)| *refused == repository);
        Some(ClientMsg::ListSessions { request_id, repository })
    }

    /// The request for what a listed session said, or `None` when the
    /// daemon cannot take it.
    pub fn read(&mut self, status: &StatusBar, session: &Session, request_id: u64) -> Option<ClientMsg> {
        status.daemon_can(SESSIONS).then_some(())?;
        self.reading = Some((request_id, session.session_id.clone()));
        self.refused_read.take_if(|(refused, _)| *refused == session.session_id);
        Some(ClientMsg::ReadSession {
            request_id,
            checkout: session.checkout,
            harness: session.harness.clone(),
            session_id: session.session_id.clone(),
        })
    }

    /// The request for how each harness resumes a session, or `None` when
    /// the daemon cannot take it.
    pub fn ask_resumes(&mut self, status: &StatusBar, request_id: u64) -> Option<ClientMsg> {
        status.daemon_can(SESSIONS).then_some(())?;
        self.resuming = Some(request_id);
        Some(ClientMsg::ListResumes { request_id })
    }

    /// Takes an answer to a request still wanted; any other message is
    /// handed back.
    pub fn receive(&mut self, msg: ServerMsg) -> Option<ServerMsg> {
        match msg {
            ServerMsg::Sessions { request_id, repository, sessions } => {
                if self.listing.is_some_and(|(asked, _)| asked == request_id) {
                    self.listed = Some((repository, sessions));
                }
                None
            }
            ServerMsg::SessionTranscript { request_id, session_id, conversation } => {
                if self.reading.as_ref().is_some_and(|(asked, _)| *asked == request_id) {
                    self.reading = None;
                    self.read = Some((session_id, *conversation));
                }
                None
            }
            ServerMsg::Resumes { request_id, resumes } => {
                if self.resuming == Some(request_id) {
                    self.resumes = Some(resumes);
                }
                None
            }
            other => Some(other),
        }
    }

    /// Takes the refusal of the listing or the read still wanted, holding
    /// why in its place; false for any other request's.
    pub fn failed(&mut self, request_id: u64, reason: &str) -> bool {
        if let Some((_, repository)) = self.listing.take_if(|(asked, _)| *asked == request_id) {
            self.refused_list = Some((repository, reason.to_owned()));
            return true;
        }
        if let Some((_, session_id)) = self.reading.take_if(|(asked, _)| *asked == request_id) {
            self.refused_read = Some((session_id, reason.to_owned()));
            return true;
        }
        false
    }

    /// Why the daemon would not list `repository`'s sessions.
    pub fn list_refusal(&self, repository: RepositoryId) -> Option<&str> {
        self.refused_list.as_ref().filter(|(r, _)| *r == repository).map(|(_, why)| why.as_str())
    }

    /// Why the daemon would not read what `session_id` said.
    pub fn read_refusal(&self, session_id: &str) -> Option<&str> {
        self.refused_read.as_ref().filter(|(id, _)| id == session_id).map(|(_, why)| why.as_str())
    }

    pub fn listed(&self, repository: RepositoryId) -> Option<&[Session]> {
        self.listed.as_ref().filter(|(r, _)| *r == repository).map(|(_, s)| s.as_slice())
    }

    pub fn transcript(&self, session_id: &str) -> Option<&Conversation> {
        self.read.as_ref().filter(|(id, _)| id == session_id).map(|(_, c)| c)
    }

    /// Whether what `session_id` said has been asked for and not answered.
    pub fn reading(&self, session_id: &str) -> bool {
        self.reading.as_ref().is_some_and(|(_, id)| id == session_id)
    }

    /// The argv that resumes `session_id` under `harness`, or `None` when
    /// the daemon named no way to, has not said yet, or would refuse the id.
    pub fn resume_command(&self, harness: &str, session_id: &str) -> Option<Vec<String>> {
        is_plain_session_id(session_id).then_some(())?;
        let resume = self.resumes.as_ref()?.iter().find(|r| r.harness == harness)?;
        Some(resume.argv.iter().map(|arg| arg.replace("{session_id}", session_id)).collect())
    }

    /// `pane` is the one the daemon made resuming `from`, and the tree
    /// holds it.
    pub fn resumed_in(&mut self, pane: PaneId, from: Resumed) {
        self.resumed.insert(pane, from);
    }

    pub fn resumed_from(&self, pane: PaneId) -> Option<&Resumed> {
        self.resumed.get(&pane)
    }

    /// Forgets the panes `holds` says are gone.
    pub fn forget_resumed(&mut self, holds: impl Fn(PaneId) -> bool) {
        self.resumed.retain(|&pane, _| holds(pane));
    }
}

/// What a session resumed now would be resumed from: its last time and
/// its turns, as the daemon listed it.
pub fn resumed_from(session: &Session) -> Resumed {
    Resumed {
        at: session.ended.clone().or_else(|| session.modified.map(|ms| rfc3339(ms / 1000))),
        turns: session.turns,
    }
}

#[cfg(test)]
mod tests {
    use argus_protocol::agents::transcript::{Entry, Transcript};
    use argus_protocol::{read_msg, write_msg, CheckoutId, Hello};

    use super::*;

    fn daemon(capabilities: &[&str]) -> StatusBar {
        let mut status = StatusBar::new(false);
        let hello = Hello {
            capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
            ..Hello::this_build()
        };
        status.connected(Some(hello), None);
        status
    }

    /// A message as it crosses the socket, not as it was built.
    async fn over_the_wire(msg: &ServerMsg) -> ServerMsg {
        let mut bytes = Vec::new();
        write_msg(&mut bytes, msg).await.unwrap();
        read_msg(&mut bytes.as_slice()).await.unwrap()
    }

    fn session(id: &str) -> Session {
        Session {
            harness: "claude".into(),
            session_id: id.into(),
            checkout: CheckoutId(3),
            removed_worktree: None,
            title: Some("Add a stop command".into()),
            started: Some("2026-10-14T09:00:00Z".into()),
            ended: None,
            modified: Some(1_800_000_000_000),
            turns: Some(4),
            pane: Some(PaneId(7)),
        }
    }

    #[test]
    fn a_daemon_that_did_not_list_sessions_is_not_asked() {
        let mut sessions = Sessions::default();
        assert!(sessions.list(&StatusBar::new(false), RepositoryId(1), 1).is_none());
        assert!(sessions.list(&daemon(&[]), RepositoryId(1), 1).is_none());
        assert!(sessions.read(&daemon(&[]), &session("s1"), 2).is_none());
        let asked = sessions.list(&daemon(&[SESSIONS]), RepositoryId(1), 3);
        assert!(matches!(asked, Some(ClientMsg::ListSessions { request_id: 3, repository: RepositoryId(1) })));
    }

    #[tokio::test]
    async fn only_the_latest_listing_is_kept() {
        let hello = daemon(&[SESSIONS]);
        let mut sessions = Sessions::default();
        sessions.list(&hello, RepositoryId(1), 1);
        sessions.list(&hello, RepositoryId(1), 2);
        let answer = |request_id, id: &str| ServerMsg::Sessions {
            request_id,
            repository: RepositoryId(1),
            sessions: vec![session(id)],
        };
        assert!(sessions.receive(over_the_wire(&answer(1, "stale")).await).is_none());
        assert!(sessions.listed(RepositoryId(1)).is_none());
        sessions.receive(over_the_wire(&answer(2, "s1")).await);
        assert_eq!(sessions.listed(RepositoryId(1)), Some(&[session("s1")][..]));
        assert!(sessions.listed(RepositoryId(2)).is_none());
    }

    #[tokio::test]
    async fn a_read_session_arrives_as_its_conversation() {
        let hello = daemon(&[SESSIONS]);
        let mut sessions = Sessions::default();
        let asked = sessions.read(&hello, &session("s1"), 5).unwrap();
        assert!(matches!(
            asked,
            ClientMsg::ReadSession { request_id: 5, checkout: CheckoutId(3), ref harness, ref session_id }
                if harness == "claude" && session_id == "s1"
        ));
        let conversation = Conversation::Read(Transcript {
            entries: vec![Entry::User { text: "Add a stop command".into(), at: None }],
            ..Transcript::default()
        });
        let answer = ServerMsg::SessionTranscript {
            request_id: 5,
            session_id: "s1".into(),
            conversation: Box::new(conversation.clone()),
        };
        assert!(sessions.reading("s1"));
        sessions.receive(over_the_wire(&answer).await);
        assert_eq!(sessions.transcript("s1"), Some(&conversation));
        assert!(!sessions.reading("s1"), "answered");
        assert!(sessions.receive(ServerMsg::Restarting).is_some(), "not its to take");
    }

    #[test]
    fn a_refusal_is_held_for_the_ask_it_answers_and_another_is_not() {
        let hello = daemon(&[SESSIONS]);
        let mut sessions = Sessions::default();
        sessions.list(&hello, RepositoryId(1), 1);
        sessions.read(&hello, &session("s1"), 2);
        assert!(!sessions.failed(9, "not ours"), "neither asked it");
        assert!(sessions.list_refusal(RepositoryId(1)).is_none() && sessions.read_refusal("s1").is_none());
        assert!(sessions.reading("s1"), "and the read still waits");

        assert!(sessions.failed(2, "no such checkout: 3"));
        assert_eq!(sessions.read_refusal("s1"), Some("no such checkout: 3"));
        assert!(!sessions.reading("s1"));
        assert!(sessions.failed(1, "gone"));
        assert_eq!(sessions.list_refusal(RepositoryId(1)), Some("gone"));
        assert!(sessions.list_refusal(RepositoryId(2)).is_none());
        assert!(!sessions.failed(1, "again"), "answered once");

        // Asked again, the refusal is no longer the answer.
        sessions.list(&hello, RepositoryId(1), 3);
        sessions.read(&hello, &session("s1"), 4);
        assert!(sessions.list_refusal(RepositoryId(1)).is_none() && sessions.read_refusal("s1").is_none());
    }

    #[tokio::test]
    async fn a_resume_command_fills_the_session_id_into_its_harness_argv() {
        let hello = daemon(&[SESSIONS]);
        let mut sessions = Sessions::default();
        assert!(sessions.ask_resumes(&daemon(&[]), 1).is_none());
        let asked = sessions.ask_resumes(&hello, 2);
        assert!(matches!(asked, Some(ClientMsg::ListResumes { request_id: 2 })));
        assert_eq!(sessions.resume_command("claude", "s1"), None, "not said yet");
        let argv = vec!["claude".to_string(), "--resume".to_string(), "{session_id}".to_string()];
        let answer = ServerMsg::Resumes { request_id: 2, resumes: vec![Resume { harness: "claude".into(), argv }] };
        sessions.receive(over_the_wire(&answer).await);
        assert_eq!(sessions.resume_command("claude", "s1").unwrap(), ["claude", "--resume", "s1"]);
        assert_eq!(sessions.resume_command("generic", "s1"), None);
        assert_eq!(sessions.resume_command("claude", "s1 & calc"), None, "never offered to the daemon");
    }

    #[test]
    fn a_resumed_session_is_said_by_its_last_time_and_its_turns() {
        let listed = session("s1");
        assert_eq!(
            resumed_from(&Session { ended: Some("2026-10-14T13:52:00Z".into()), ..listed.clone() }),
            Resumed { at: Some("2026-10-14T13:52:00Z".into()), turns: Some(4) }
        );
        // No time read from the transcript: when its file was last written.
        assert_eq!(resumed_from(&listed).at.as_deref(), Some("2027-01-15T08:00:00Z"));
        let bare = Session { modified: None, turns: None, ..listed };
        assert_eq!(resumed_from(&bare), Resumed { at: None, turns: None });
    }

    #[test]
    fn what_a_pane_was_resumed_from_is_kept_while_the_pane_lasts() {
        let mut sessions = Sessions::default();
        let from = Resumed { at: None, turns: Some(4) };
        sessions.resumed_in(PaneId(9), from.clone());
        sessions.resumed_in(PaneId(10), from.clone());
        assert_eq!(sessions.resumed_from(PaneId(9)), Some(&from));
        sessions.forget_resumed(|pane| pane == PaneId(10));
        assert_eq!(sessions.resumed_from(PaneId(9)), None);
        assert_eq!(sessions.resumed_from(PaneId(10)), Some(&from));
    }
}
