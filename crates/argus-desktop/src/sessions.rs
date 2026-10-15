//! What the window has asked of a repository's agent sessions — the list,
//! what one of them said, and how each harness resumes one — and what it
//! holds of the answers. Only the latest of each is wanted: an answer to a
//! request since replaced is dropped. Asked only of a daemon that listed
//! `SESSIONS`, since an older one hangs up on a message it does not know.

use argus_protocol::sessions::{is_plain_session_id, Resume, Session};
use argus_protocol::transcript::Conversation;
use argus_protocol::{ClientMsg, RepositoryId, ServerMsg, SESSIONS};

use crate::status::StatusBar;

#[derive(Debug, Default)]
pub struct Sessions {
    listing: Option<u64>,
    listed: Option<(RepositoryId, Vec<Session>)>,
    /// The read asked for, and the session it is of.
    reading: Option<(u64, String)>,
    read: Option<(String, Conversation)>,
    resuming: Option<u64>,
    /// Each harness that resumes a session by id, with the argv it would
    /// run, `{session_id}` unfilled; `None` until the daemon has said.
    resumes: Option<Vec<Resume>>,
}

impl Sessions {
    /// The request for `repository`'s sessions, or `None` when the daemon
    /// cannot take it.
    pub fn list(&mut self, status: &StatusBar, repository: RepositoryId, request_id: u64) -> Option<ClientMsg> {
        status.daemon_can(SESSIONS).then_some(())?;
        self.listing = Some(request_id);
        Some(ClientMsg::ListSessions { request_id, repository })
    }

    /// The request for what a listed session said, or `None` when the
    /// daemon cannot take it.
    pub fn read(&mut self, status: &StatusBar, session: &Session, request_id: u64) -> Option<ClientMsg> {
        status.daemon_can(SESSIONS).then_some(())?;
        self.reading = Some((request_id, session.session_id.clone()));
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
                if self.listing == Some(request_id) {
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
}

#[cfg(test)]
mod tests {
    use argus_protocol::transcript::{Entry, Transcript};
    use argus_protocol::{read_msg, write_msg, CheckoutId, Hello, PaneId};

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
}
