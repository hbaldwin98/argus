//! An agent starting another agent: in a pane of its own, in the checkout,
//! worktree or other repository it names, where it outlives the turn that
//! asked for it and the person sees it like any agent they started.

use std::path::Path;

use argus_protocol::{AgentLaunch, AgentLaunched, LaunchPlace};

use super::*;

impl Daemon {
    pub(crate) async fn launch_agent_for_agent(
        self: &Arc<Self>,
        pane: PaneId,
        launch: AgentLaunch,
    ) -> anyhow::Result<AgentLaunched> {
        let (here, own_template) = self.launching_pane(pane)?;
        let template = match launch.template.filter(|t| !t.trim().is_empty()) {
            Some(template) => template,
            None => own_template.ok_or_else(|| {
                anyhow::anyhow!("this pane was not started from a template; name one with --template")
            })?,
        };
        let checkout = match launch.place {
            LaunchPlace::Here => here,
            LaunchPlace::Checkout { path } => self.shown_checkout_at(&path)?,
            LaunchPlace::Worktree { branch, from } => {
                let base = match from {
                    Some(path) => self.shown_checkout_at(&path)?,
                    None => here,
                };
                let made = self.create_worktree(base, branch).await?;
                // Not awaited, as the window's worktree is not: the agent
                // starts beside its setup, and an install can take minutes.
                let daemon = self.clone();
                tokio::spawn(async move {
                    if let Err(error) = daemon.setup_worktree(made).await {
                        tracing::warn!("setting up a launched agent's worktree: {error}");
                    }
                });
                made
            }
        };
        let daemon = self.clone();
        let (prompt, started) = (launch.prompt, template.clone());
        let new_pane = tokio::task::spawn_blocking(move || {
            daemon.spawn_agent_asking(checkout, &started, prompt.as_deref())
        })
        .await??;
        let path = {
            let inner = self.inner.lock().unwrap();
            find_checkout_ref(&inner.projects, checkout).map(|c| c.path.clone())
        };
        Ok(AgentLaunched {
            pane: new_pane,
            template,
            checkout: path.ok_or_else(|| anyhow::anyhow!("the checkout went away"))?,
        })
    }

    /// The checkout the asking agent is in, and the template it was started
    /// from. Only a live agent may start another: a shell's hook
    /// environment is a person's, and they have the window for this.
    fn launching_pane(&self, pane: PaneId) -> anyhow::Result<(CheckoutId, Option<String>)> {
        let inner = self.inner.lock().unwrap();
        let found = checkouts(&inner.projects)
            .find_map(|c| {
                let p = c.panes.iter().find(|p| p.id == pane)?;
                Some((c.id, p))
            })
            .filter(|(_, p)| p.is_running_agent())
            .map(|(checkout, p)| (checkout, p.template.clone()))
            .ok_or_else(|| anyhow::anyhow!("only a live agent pane can start another agent"));
        found
    }

    fn shown_checkout_at(&self, path: &Path) -> anyhow::Result<CheckoutId> {
        self.checkout_at(path).ok_or_else(|| {
            anyhow::anyhow!(
                "Argus shows no checkout at {}; add its repository from the window first",
                path.display()
            )
        })
    }
}
