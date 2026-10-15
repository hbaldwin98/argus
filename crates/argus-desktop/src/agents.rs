//! Which agents this window offers to start: the daemon's templates, in
//! its order, with the one this window last started first while the daemon
//! still offers it — the one A and a checkout's + start — and whether that
//! one changed since it was saved.

pub struct Agents {
    /// The agent templates the daemon offers, in its order.
    templates: Vec<String>,
    /// The template of the agent this window last started, once the tree
    /// showed it running.
    last: Option<String>,
    last_changed: bool,
}

impl Agents {
    /// `last` is the one remembered from the window's last run.
    pub fn new(last: Option<String>) -> Agents {
        Agents { templates: Vec::new(), last, last_changed: false }
    }

    /// The daemon's templates, as it sent them.
    pub fn receive(&mut self, templates: Vec<String>) {
        self.templates = templates;
    }

    /// Whether the daemon offers `template`.
    pub fn offers(&self, template: &str) -> bool {
        self.templates.iter().any(|t| t == template)
    }

    /// The daemon's templates with the one last started first, while the
    /// daemon still offers it.
    pub fn offered(&self) -> Vec<String> {
        let mut agents = self.templates.clone();
        if let Some(at) = self.last.as_ref().and_then(|last| agents.iter().position(|t| t == last)) {
            let last = agents.remove(at);
            agents.insert(0, last);
        }
        agents
    }

    /// What A starts: the first offered.
    pub fn first(&self) -> Option<String> {
        self.offered().into_iter().next()
    }

    /// An agent from `template` is running, read off the tree rather than
    /// the request, so a refused or mistaken spawn never becomes the one
    /// offered first.
    pub fn started(&mut self, template: &str) {
        if self.last.as_deref() == Some(template) {
            return;
        }
        self.last = Some(template.to_owned());
        self.last_changed = true;
    }

    pub fn last(&self) -> Option<&String> {
        self.last.as_ref()
    }

    /// Whether the last agent changed since last asked, so it is saved.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.last_changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(last: Option<&str>) -> Agents {
        let mut agents = Agents::new(last.map(str::to_owned));
        agents.receive(vec!["claude".into(), "codex".into()]);
        agents
    }

    #[test]
    fn the_agent_last_started_is_offered_first_and_saved_once() {
        let mut agents = offering(None);
        assert_eq!(agents.offered(), ["claude", "codex"]);
        agents.started("codex");
        assert_eq!(agents.offered(), ["codex", "claude"]);
        assert_eq!(agents.first().as_deref(), Some("codex"));
        assert!(agents.take_changed());
        assert!(!agents.take_changed(), "saved once");
        agents.started("codex");
        assert!(!agents.take_changed(), "the same one again is no change");
    }

    #[test]
    fn a_remembered_agent_leads_only_while_the_daemon_offers_it() {
        assert_eq!(offering(Some("codex")).offered(), ["codex", "claude"]);
        assert_eq!(offering(Some("aider")).offered(), ["claude", "codex"], "gone from the config");
        assert!(!offering(Some("aider")).offers("aider"));
    }
}
