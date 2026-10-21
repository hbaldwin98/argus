//! A person moving a pane to another checkout of its project, by dragging
//! its row. The pane's process goes with its row, so the row never claims
//! a directory the process is not working in: an agent is told where it
//! works now, and a shell waiting at its prompt is sent a `cd` there. A
//! shell running something is refused, since what is typed would reach
//! that program instead.

use std::path::Path;

use argus_protocol::agents::typed::plain;
use argus_protocol::CheckoutId;

use crate::pty::ShellState;
use crate::state::*;

impl Daemon {
    pub(crate) fn move_to_checkout(&self, pane: PaneId, checkout: CheckoutId) -> anyhow::Result<()> {
        let (kind, from, name, destination) = {
            let mut inner = self.inner.lock().unwrap();
            let (kind, from) = find_pane_with_checkout(&mut inner.projects, pane)
                .map(|(p, from)| (p.kind, from))
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            let to = find_checkout_ref(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            (kind, from, to.name.clone(), to.path.clone())
        };
        if from == checkout {
            return Ok(());
        }
        match kind {
            PaneKind::Agent => {
                self.move_agent_to_checkout(pane, &destination)?;
                // Its row follows where its turns end, so an agent that
                // never goes there is not dragged back by its next turn:
                // only a directory that changes moves the row.
                self.notify(pane, &moved_message(&name, &destination));
                Ok(())
            }
            PaneKind::Shell => self.move_shell(pane, checkout, &destination),
            PaneKind::Editor | PaneKind::Browser => {
                anyhow::bail!("only an agent or a shell moves to another checkout")
            }
        }
    }

    /// Types the `cd` before the row moves, so a shell that cannot be
    /// typed into keeps its row where its directory is.
    fn move_shell(&self, pane: PaneId, checkout: CheckoutId, destination: &Path) -> anyhow::Result<()> {
        let (state, keyboard) = {
            let inner = self.inner.lock().unwrap();
            let p = find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            if matches!(p.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("an exited shell cannot move to another checkout");
            }
            if project_holding(&inner.projects, |c| c.panes.iter().any(|p| p.id == pane))
                != project_holding(&inner.projects, |c| c.id == checkout)
            {
                anyhow::bail!("a shell moves only to a checkout of its own project");
            }
            (p.runtime.shell(), p.runtime.input())
        };
        let shell = match state {
            Some(ShellState::AtPrompt(shell)) => shell,
            Some(ShellState::Running) => anyhow::bail!(
                "the shell is running something: move it once it is back at its prompt"
            ),
            Some(ShellState::Unknown(name)) => {
                anyhow::bail!("Argus does not know how to change directory in {name}")
            }
            None => anyhow::bail!("this pane runs no shell"),
        };
        let keys = shell
            .cd(destination)
            .ok_or_else(|| anyhow::anyhow!("{} cannot be typed into this shell", destination.display()))?;
        keyboard.write(&keys)?;
        {
            let mut inner = self.inner.lock().unwrap();
            // Looked for before the pane is taken out, so a checkout removed
            // since cannot leave the pane in none.
            if find_checkout_ref(&inner.projects, checkout).is_none() {
                anyhow::bail!("no such checkout");
            }
            let (moving, _) = remove_pane_with_checkout(&mut inner.projects, pane)
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            find_checkout(&mut inner.projects, checkout)
                .expect("looked for under the same lock")
                .panes
                .push(moving);
        }
        self.broadcast_tree();
        Ok(())
    }
}

/// The index of the project with a checkout that `matches`.
fn project_holding(projects: &[Project], matches: impl Fn(&Checkout) -> bool) -> Option<usize> {
    projects
        .iter()
        .position(|project| project.repositories.iter().flat_map(|r| &r.checkouts).any(&matches))
}

/// What an agent is told when a person moves it to another checkout.
fn moved_message(name: &str, destination: &Path) -> String {
    format!(
        "The person moved you to the checkout {name} at {path}. Work there from now on: change into \
         that directory, then run `\"$ARGUS_HOOK\" checkout` from it.",
        name = plain(name),
        path = plain(&destination.display().to_string()),
    )
}
