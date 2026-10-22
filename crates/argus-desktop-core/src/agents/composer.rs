//! What the composer shows and does: which repository, checkout and agent are picked, the line that
//! says where the start will be, and the call a start sends.
//!
//! The repository is the rail's chosen one, so picking another chooses it there. The checkout, the
//! agent and the message are a draft the window keeps until a pane it started is made
//! (COMPOSE-4..10); the picks the draft does not hold fall back to what the rail shows, so a rail
//! update for another pane never changes what is picked.

use crate::connection::page::{Call, Checkout, Rail, Repository};

/// What the person has chosen or typed in the composer and the daemon has not yet answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub checkout: Option<u64>,
    pub template: Option<String>,
    pub text: String,
}

/// An option of a picker: what it stands for and the words it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick<T> {
    pub value: T,
    pub label: String,
}

/// The line between the two rules over the title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    /// The project lists no repository.
    NoRepository,
    /// The repository is folded on the rail, so it lists no checkout to pick.
    Folded { repository: String },
    /// A shell or agent would start here: the repository's own working directory (named by the
    /// repository) or a worktree (named by itself), on a branch when it has one.
    Starting { worktree: bool, name: String, branch: Option<String> },
}

/// The composer as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerPage {
    pub title: String,
    pub place: Where,
    pub repositories: Vec<Pick<u64>>,
    pub repository: Option<u64>,
    pub checkouts: Vec<Pick<u64>>,
    pub checkout: Option<u64>,
    /// The daemon's templates in Rust's order, the last started first.
    pub templates: Vec<String>,
    pub template: Option<String>,
    /// A shell needs a checkout; an agent needs a template as well.
    pub can_shell: bool,
    pub can_start: bool,
}

/// The repository shown: the rail's chosen one, else the first.
pub fn repository(rail: &Rail) -> Option<&Repository> {
    let repositories = rail.project.as_ref().map(|project| project.repositories.as_slice()).unwrap_or_default();
    repositories.iter().find(|repo| Some(repo.id) == rail.repository).or_else(|| repositories.first())
}

/// The draft's checkout while the repository lists it, else the one on screen there, else the
/// first: its own working directory.
pub fn checkout<'a>(rail: &Rail, repository: Option<&'a Repository>, draft: &Draft) -> Option<&'a Checkout> {
    let checkouts = repository.map(|repo| repo.checkouts.as_slice()).unwrap_or_default();
    checkouts
        .iter()
        .find(|checkout| Some(checkout.id) == draft.checkout)
        .or_else(|| {
            let shown = rail.shown.as_ref()?;
            checkouts.iter().find(|checkout| repository.is_some_and(|repo| repo.name == shown.repository) && checkout.name == shown.checkout)
        })
        .or_else(|| checkouts.first())
}

/// The draft's agent while the daemon offers it, else the first offered.
pub fn template<'a>(rail: &'a Rail, draft: &Draft) -> Option<&'a str> {
    rail.templates.iter().find(|name| Some(name.as_str()) == draft.template.as_deref()).or_else(|| rail.templates.first()).map(String::as_str)
}

impl ComposerPage {
    pub fn of(rail: &Rail, draft: &Draft) -> Self {
        let repo = repository(rail);
        let co = checkout(rail, repo, draft);
        let name = template(rail, draft);
        let place = match (repo, co) {
            (None, _) => Where::NoRepository,
            (Some(repo), None) => Where::Folded { repository: repo.name.clone() },
            (Some(repo), Some(co)) => {
                let worktree = co.kind != "main";
                Where::Starting { worktree, name: if worktree { co.name.clone() } else { repo.name.clone() }, branch: (!co.branch.is_empty()).then(|| co.branch.clone()) }
            }
        };
        Self {
            title: repo.map_or_else(|| "No repository to start in".to_owned(), |repo| format!("What should start in {}?", repo.name)),
            place,
            repositories: rail
                .project
                .iter()
                .flat_map(|project| &project.repositories)
                .map(|repo| Pick { value: repo.id, label: repo.name.clone() })
                .collect(),
            repository: repo.map(|repo| repo.id),
            checkouts: repo
                .iter()
                .flat_map(|repo| &repo.checkouts)
                .map(|co| Pick { value: co.id, label: if !co.branch.is_empty() && co.branch != co.name { format!("{} \u{b7} {}", co.name, co.branch) } else { co.name.clone() } })
                .collect(),
            checkout: co.map(|co| co.id),
            templates: rail.templates.clone(),
            template: name.map(str::to_owned),
            can_shell: co.is_some(),
            can_start: co.is_some() && name.is_some(),
        }
    }
}

impl Draft {
    /// Picking a repository clears the checkout draft; the rail follows when it is another than
    /// the rail's own, which is the call to send.
    pub fn pick_repository(&mut self, rail: &Rail, repository: u64) -> Option<Call> {
        self.checkout = None;
        (Some(repository) != rail.repository).then_some(Call::ChooseRepository { repository })
    }

    pub fn pick_checkout(&mut self, checkout: u64) {
        self.checkout = Some(checkout);
    }

    pub fn pick_template(&mut self, template: &str) {
        self.template = Some(template.to_owned());
    }

    /// The start: an agent from the picked template with the message untrimmed when it says
    /// anything, else a shell, which ignores the message. None when there is no checkout (or, for an
    /// agent, no template).
    pub fn start(&self, rail: &Rail, agent: bool) -> Option<Call> {
        let co = checkout(rail, repository(rail), self)?;
        let name = template(rail, self);
        if agent && name.is_none() {
            return None;
        }
        let said = !self.text.trim().is_empty();
        Some(Call::Compose {
            checkout: co.id,
            template: if agent { name.map(str::to_owned) } else { None },
            prompt: if agent && said { Some(self.text.clone()) } else { None },
        })
    }

    /// Hears the rail change: a start the daemon made (the composer's stage gone and a pane newly
    /// shown) ends the draft's message. The picks stay.
    pub fn settle(&mut self, before: &Rail, after: &Rail) {
        let made = after.shown.as_ref().is_some_and(|shown| before.shown.as_ref().map(|was| was.pane) != Some(shown.pane));
        if before.stage == crate::connection::page::Stage::Composer && after.stage == crate::connection::page::Stage::Workspace && made {
            self.text.clear();
        }
    }
}

#[cfg(test)]
mod tests;
