//! A feature's reconcile with its tickets: the person asks and ticks, the
//! agent proposes and reports, and each is told what the other did.

use argus_protocol::{
    ArtifactScope, Failed, Feature, InboxItem, ProposalWrite, ProposedLine, ReconcileAction,
    ReconcileChange, Ticked,
};

use super::*;

struct Desk {
    d: Arc<Daemon>,
    project: ProjectId,
    checkout: CheckoutId,
    agent: PaneId,
    slug: String,
}

fn desk(dir: &std::path::Path) -> Desk {
    let d = daemon_with_running_claude(dir);
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    d.set_pane_session_id(agent, "s1");
    let slug = open_feature(&d, agent, "Trim context injection");
    Desk { d, project, checkout, agent, slug }
}

impl Desk {
    fn person(&self, action: ReconcileAction) -> anyhow::Result<()> {
        self.d.reconcile_for_client(self.project, self.checkout, &self.slug, action)
    }

    fn agent(&self, change: ReconcileChange) -> anyhow::Result<Feature> {
        self.d.reconcile_for_agent(self.agent, change, ArtifactScope::default())
    }

    fn propose(&self) -> anyhow::Result<Feature> {
        let line = |to: &str, key: &str, does: &str, text: &str| ProposedLine {
            to: to.into(),
            key: key.into(),
            does: does.into(),
            text: text.into(),
            call: format!("{to} {does}"),
            ..ProposedLine::default()
        };
        self.agent(ReconcileChange::Propose {
            proposal: ProposalWrite {
                asked: String::new(),
                lines: vec![
                    line("Argus", "ENG-94", "add as task 2.4", "Refuse an empty brief"),
                    line("Jira", "ARG-142", "comment with decision D2", "Decision D2: the daemon"),
                ],
                noticed: vec![argus_protocol::Noticed { key: "ENG-93".into(), text: "title differs".into() }],
            },
        })
    }
}

#[tokio::test]
async fn an_ask_is_proposed_ticked_carried_out_and_what_failed_comes_back_unticked() {
    let dir = tempfile::tempdir().unwrap();
    let desk = desk(dir.path());
    let (_, mut inbox) = super::inbox::open_inbox(&desk.d, desk.agent, "s1").await;

    desk.person(ReconcileAction::Ask { pane: desk.agent, ask: "put D2 on the Jira ticket".into() }).unwrap();
    let InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("put D2 on the Jira ticket") && text.contains("write nothing"), "{text}");
    assert!(text.contains(&format!("reconcile propose --feature {} --stdin", desk.slug)), "{text}");
    assert!(!text.contains('\n'), "it may be typed into a terminal");

    let proposed = desk.propose().unwrap();
    let reconcile = proposed.reconcile.unwrap();
    assert_eq!(reconcile.asked, "put D2 on the Jira ticket", "the ask is kept with the proposal");
    assert!(reconcile.proposed && reconcile.carrying.is_empty());

    let edited = Ticked { line: 1, text: "Decision D2 (accepted): the daemon".into() };
    desk.person(ReconcileAction::CarryOut { ticked: vec![Ticked { line: 0, text: "Refuse an empty brief".into() }, edited.clone()] })
        .unwrap();
    let InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("ticked 2 lines") && text.contains("reconcile report"), "{text}");

    let read = desk.agent(ReconcileChange::Read).unwrap().reconcile.unwrap();
    assert_eq!(read.carrying[1], edited, "the agent reads the words the person ticked");

    let after = desk.agent(ReconcileChange::Report { failed: vec![Failed { line: 1, error: "403 from Jira".into() }] }).unwrap();
    let reconciled = after.reconciled.unwrap();
    assert_eq!(reconciled.by, format!("pane #{}", desk.agent.0));
    let left = after.reconcile.unwrap();
    assert_eq!(left.lines.len(), 1, "only the failed line is left");
    assert_eq!((left.lines[0].failed.as_deref(), left.lines[0].text.as_str()), (Some("403 from Jira"), edited.text.as_str()));
    assert!(left.carrying.is_empty(), "and it comes back unticked");
    let _ = desk.d.close_pane(desk.agent);
}

#[tokio::test]
async fn nothing_is_carried_out_that_was_not_proposed_and_ticked() {
    let dir = tempfile::tempdir().unwrap();
    let desk = desk(dir.path());
    let tick = |line| ReconcileAction::CarryOut { ticked: vec![Ticked { line, text: String::new() }] };
    assert!(desk.person(tick(0)).is_err(), "no proposal waits");
    assert!(desk.agent(ReconcileChange::Report { failed: Vec::new() }).is_err(), "nothing was ticked");

    desk.propose().unwrap();
    assert!(desk.person(ReconcileAction::CarryOut { ticked: Vec::new() }).is_err(), "nothing ticked");
    assert!(desk.person(tick(5)).is_err(), "a line the proposal does not have");
    desk.person(tick(0)).unwrap();
    assert!(desk.propose().is_err(), "a proposal waits while the ticked lines are carried out");
    assert!(desk.person(tick(0)).is_err(), "and nothing more is ticked");

    let done = desk.agent(ReconcileChange::Report { failed: Vec::new() }).unwrap();
    assert!(done.reconcile.is_none() && done.reconciled.is_some(), "all of it went through");
    let _ = desk.d.close_pane(desk.agent);
}

#[tokio::test]
async fn an_ask_goes_only_to_a_live_agent_and_a_turned_down_proposal_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let desk = desk(dir.path());
    let ask = |pane| ReconcileAction::Ask { pane, ask: "pull ENG-88".into() };
    assert!(desk.person(ask(PaneId(999))).is_err());
    assert!(desk.person(ReconcileAction::Ask { pane: desk.agent, ask: "  ".into() }).is_err(), "an ask says what");

    let mut rx = desk.d.subscribe_decisions();
    desk.propose().unwrap();
    assert!(rx.try_recv().unwrap().features[0].reconcile.is_some(), "every client is pushed the proposal");
    desk.person(ReconcileAction::Discard).unwrap();
    assert!(rx.try_recv().unwrap().features[0].reconcile.is_none());
    assert!(desk.person(ReconcileAction::Discard).is_err(), "nothing left to turn down");
    let _ = desk.d.close_pane(desk.agent);
}
