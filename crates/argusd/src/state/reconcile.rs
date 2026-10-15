//! A feature's reconcile with its tickets: who may ask, propose, tick and
//! report, and what the agent is told at each turn.
//!
//! The person asks an agent and ticks lines; the agent reads, proposes and
//! reports. Argus talks to no tracker: it holds the proposal, tells the
//! agent which lines it may carry out and in what words, and when the agent
//! reports, keeps how the feature's tasks and decisions stood so the next
//! look can say what changed since.

use argus_protocol::reconcile::{checked_ask, checked_text, seen, Stage, MAX_FAILURE_BYTES};
use argus_protocol::{Feature, Reconcile, ReconcileAction, ReconcileChange, Reconciled};

use super::asker::{pane_label, Asker};
use super::board_address::BoardAddress;
use super::*;

/// One turn of a reconcile, in whichever vocabulary it was asked in: an
/// agent's read, proposal or report, or the person's ask, ticked lines or
/// turning down.
enum Turn {
    Agent(ReconcileChange),
    Person(ReconcileAction),
}

impl Daemon {
    /// An agent's read, proposal or report, answered with the feature as it
    /// then stands.
    pub fn reconcile_for_agent(
        &self,
        pane_id: PaneId,
        change: ReconcileChange,
        filing: impl Into<super::features::Filing>,
    ) -> anyhow::Result<Feature> {
        let filing = filing.into();
        let address = self.address_for_agent(pane_id, filing.scope)?;
        let Some(slug) = self.feature_for_agent(&address, &filing)? else {
            anyhow::bail!("this checkout is on no feature; name one with --feature <slug>");
        };
        let by = Asker::Agent {
            pane: pane_id,
            session: None,
        };
        self.apply_reconcile(&address, by, &slug, Turn::Agent(change))
    }

    /// The person's ask, ticked lines or turning down, from the view.
    pub fn reconcile_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        slug: &str,
        action: ReconcileAction,
    ) -> anyhow::Result<()> {
        let address = self.address_for_client(project, checkout)?;
        self.apply_reconcile(&address, Asker::Person, slug, Turn::Person(action))
            .map(drop)
    }

    /// Takes one turn of `slug`'s reconcile and pushes the board, answering
    /// with the feature as it then stands.
    ///
    /// Who may take which turn is this match, side by side: an agent
    /// reads, proposes and reports; the person asks an agent, ticks the
    /// lines it may carry out and turns a proposal down.
    fn apply_reconcile(
        &self,
        address: &BoardAddress,
        by: Asker<'_>,
        slug: &str,
        turn: Turn,
    ) -> anyhow::Result<Feature> {
        let key = address.key.as_str();
        let feature = self.reconciled_feature(key, slug)?;
        let (under_way, _) = self.store.reconcile(key, slug)?;
        let carrying = under_way.as_ref().is_some_and(|r| r.stage() == Stage::Carrying);
        match (by, turn) {
            (Asker::Agent { .. }, Turn::Agent(ReconcileChange::Read)) => {}
            (Asker::Agent { pane, .. }, Turn::Agent(ReconcileChange::Propose { proposal })) => {
                if carrying {
                    anyhow::bail!(
                        "the person's ticked lines are being carried out; report on them before proposing again"
                    );
                }
                let proposal = proposal.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                // An agent proposing unasked keeps the ask it was given in
                // chat, which it says itself.
                let asked = match (&under_way, proposal.asked.is_empty()) {
                    (Some(r), true) => r.asked.clone(),
                    _ => proposal.asked,
                };
                let reconcile = Reconcile {
                    asked,
                    pane,
                    by: pane_label(pane),
                    at: super::decisions::now(),
                    proposed: true,
                    lines: proposal.lines,
                    noticed: proposal.noticed,
                    carrying: Vec::new(),
                };
                self.store.set_reconcile(key, slug, Some(&reconcile))?;
            }
            (Asker::Agent { pane, .. }, Turn::Agent(ReconcileChange::Report { failed })) => {
                let Some(mut left) = under_way.filter(|r| r.stage() == Stage::Carrying) else {
                    anyhow::bail!("nothing was ticked for you to carry out on {slug}");
                };
                let mut lines = Vec::new();
                for failure in failed {
                    let Some(ticked) = left.carrying.iter().find(|t| t.line == failure.line) else {
                        anyhow::bail!("line {} was not ticked", failure.line);
                    };
                    let Some(line) = left.lines.get(failure.line) else {
                        anyhow::bail!("there is no line {}", failure.line);
                    };
                    let mut error = failure.error.trim().to_string();
                    let mut cut = error.len().min(MAX_FAILURE_BYTES);
                    while !error.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    error.truncate(cut);
                    lines.push(argus_protocol::ProposedLine {
                        text: ticked.text.clone(),
                        failed: Some(if error.is_empty() { "failed".to_string() } else { error }),
                        ..line.clone()
                    });
                }
                // A failed line comes back unticked with what the tracker
                // said; the lines that went through are done with.
                left.carrying.clear();
                left.lines = lines;
                left.at = super::decisions::now();
                let left = (!left.lines.is_empty()).then_some(left);
                let decisions = self.store.decisions(key)?;
                let filed: Vec<_> = decisions.iter().filter(|d| d.feature.as_deref() == Some(slug)).collect();
                let reconciled = Reconciled {
                    at: super::decisions::now(),
                    by: pane_label(pane),
                    seen: seen(&self.store.tasks(key, slug)?, &filed),
                };
                self.store.finish_reconcile(key, slug, left.as_ref(), &reconciled)?;
            }
            (Asker::Person, Turn::Person(ReconcileAction::Ask { pane, ask })) => {
                if carrying {
                    anyhow::bail!("the ticked lines are still being carried out");
                }
                let ask = checked_ask(&ask).map_err(|e| anyhow::anyhow!("{e}"))?;
                let agent = self
                    .address_for_agent(pane, argus_protocol::ArtifactScope::RepositoryBranch)
                    .map_err(|_| anyhow::anyhow!("{} is not a live agent", pane_label(pane)))?;
                if agent.key != key {
                    anyhow::bail!("{} works in another repository", pane_label(pane));
                }
                if !self.notify(pane, &ask_message(&feature, &ask)) {
                    anyhow::bail!("{} could not be told", pane_label(pane));
                }
                let reconcile = Reconcile {
                    asked: ask,
                    pane,
                    by: pane_label(pane),
                    at: super::decisions::now(),
                    proposed: false,
                    lines: Vec::new(),
                    noticed: Vec::new(),
                    carrying: Vec::new(),
                };
                self.store.set_reconcile(key, slug, Some(&reconcile))?;
            }
            (Asker::Person, Turn::Person(ReconcileAction::CarryOut { ticked })) => {
                let Some(mut reconcile) = under_way.filter(|r| r.stage() == Stage::Proposed) else {
                    anyhow::bail!("no proposal waits on {slug}");
                };
                if ticked.is_empty() {
                    anyhow::bail!("tick the lines to carry out first");
                }
                let mut checked = Vec::with_capacity(ticked.len());
                for mut line in ticked {
                    if line.line >= reconcile.lines.len() || checked.iter().any(|t: &argus_protocol::Ticked| t.line == line.line) {
                        anyhow::bail!("the proposal has changed; read it again");
                    }
                    line.text = checked_text(&line.text).map_err(|e| anyhow::anyhow!("{e}"))?;
                    checked.push(line);
                }
                reconcile.carrying = checked;
                if !self.notify(reconcile.pane, &carry_out_message(&feature, &reconcile)) {
                    anyhow::bail!("{} is gone; turn this proposal down and ask another agent", reconcile.by);
                }
                self.store.set_reconcile(key, slug, Some(&reconcile))?;
            }
            (Asker::Person, Turn::Person(ReconcileAction::Discard)) => {
                if under_way.is_none() {
                    anyhow::bail!("no reconcile is under way on {slug}");
                }
                self.store.set_reconcile(key, slug, None)?;
            }
            (Asker::Person, Turn::Agent(_)) => {
                anyhow::bail!("an agent reads, proposes and reports on a reconcile")
            }
            (Asker::Agent { .. }, Turn::Person(_)) => {
                anyhow::bail!("the person asks for a reconcile and ticks its lines")
            }
        }
        self.broadcast_decisions(address, self.store.decisions(key)?);
        self.reconciled_feature(key, slug)
    }

    fn reconciled_feature(&self, key: &str, slug: &str) -> anyhow::Result<Feature> {
        self.store
            .features(key)?
            .into_iter()
            .find(|f| f.slug == slug)
            .ok_or_else(|| anyhow::anyhow!("there is no feature {slug}"))
    }
}

/// What an agent asked to reconcile is told, on one line: it may be typed
/// into its terminal, where a newline would send it early.
fn ask_message(feature: &Feature, ask: &str) -> String {
    format!(
        "Reconcile feature #{number} \"{title}\" (`{slug}`) with its tickets. The person asks: {ask} \
         Read the tracker map (`argus-hook tracker-map`), the feature (`argus-hook feature {slug}`) and \
         the tickets with your own tools, and write nothing, in Argus or in a tracker. Then propose \
         lines within the ask with `argus-hook reconcile propose --feature {slug} --stdin` and wait: \
         the person ticks the ones you may carry out.",
        number = feature.number,
        title = one_line(&feature.title),
        slug = feature.slug,
    )
}

fn carry_out_message(feature: &Feature, reconcile: &Reconcile) -> String {
    let n = reconcile.carrying.len();
    format!(
        "The person ticked {n} line{s} of your reconcile proposal for #{number} (`{slug}`). Read them \
         with `argus-hook reconcile --feature {slug}` and carry out exactly those, in the words given \
         there, and nothing else; then `argus-hook reconcile report --feature {slug}`, adding \
         `--failed <line> \"<what the tracker said>\"` for each that failed.",
        s = if n == 1 { "" } else { "s" },
        number = feature.number,
        slug = feature.slug,
    )
}

fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}
