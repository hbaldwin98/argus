//! Turning what the config declares into the tree the daemon runs on.
//!
//! Everything a daemon needs before it can answer a client happens here,
//! once: the declared projects folded together with the panel edits the
//! store remembers, each root scanned so the first client sees a complete
//! tree rather than one that fills in a tick later, and any hooks a
//! previous daemon left behind swept out — they name a port and a token
//! that died with it.

use super::*;

impl Daemon {
    /// A daemon that remembers nothing past this process, for tests.
    ///
    /// Persistence is a store you hand a daemon, not a flag it can be
    /// talked into setting, and this is what keeps the several hundred
    /// tests that build one off the real user's disk without any of them
    /// having to remember to ask.
    #[cfg(test)]
    pub fn new(config: ConfigFile) -> Arc<Self> {
        let store = crate::store::Store::in_memory()
            .expect("an in-memory runtime store needs nothing that can fail");
        Self::with_store(config, store)
    }

    pub fn with_store(config: ConfigFile, store: crate::store::Store) -> Arc<Self> {
        let mut ids = IdGen::default();

        // What the user declared, plus what they did to the panel while it
        // was running. A store that cannot be read costs the overlays, not
        // the config: showing the declared projects beats showing nothing.
        let overlays = store.overlays().unwrap_or_else(|e| {
            tracing::warn!("could not read runtime state: {e}");
            Default::default()
        });
        let config = config::with_overlays(config, &overlays);

        // Workspaces come from three places, in this order: the built-in
        // default (always present, so a config that predates workspaces
        // keeps working), any `[[workspace]]` blocks, and any name a
        // project refers to without declaring. Declaring is therefore
        // optional — `workspace = "x"` on a project is enough to create it.
        let mut workspaces: Vec<Workspace> = Vec::new();
        let intern = intern_workspace;
        let default_ws = intern(&mut workspaces, &mut ids, config::DEFAULT_WORKSPACE);
        for w in &config.workspaces {
            intern(&mut workspaces, &mut ids, &w.name);
        }

        let excluded = overlays.excluded.clone();
        let projects = config
            .projects
            .into_iter()
            .map(|p| {
                let root = p.root.as_deref().map(config::expand_home);
                // An excluded path is out whether the scan turned it up or
                // the config named it outright: "remove this row" means the
                // same thing either way.
                let mut repositories: Vec<Repository> = p
                    .repos
                    .iter()
                    .map(|repo| config::expand_home(repo))
                    .filter(|path| !is_excluded(&excluded, path))
                    .map(|path| new_repository(&mut ids, path, false))
                    .collect();
                // A root is scanned once here so the tree is complete the
                // moment the first client attaches, rather than filling in
                // a tick later. Reconciliation keeps it current after that.
                let scan = crate::git::Scan {
                    exclude: p.exclude.clone(),
                    include: p.include.clone(),
                };
                if let Some(root) = &root {
                    let found = retain_included(
                        &excluded,
                        crate::git::discover_repositories_within(root, &scan),
                    );
                    install_discovered(&mut ids, &mut repositories, &found);
                }
                Project {
                    id: ProjectId(ids.alloc()),
                    workspace: match p.workspace.as_deref() {
                        Some(name) => intern(&mut workspaces, &mut ids, name),
                        None => default_ws,
                    },
                    name: p.name,
                    root,
                    repositories,
                    worktree_root: p.worktree_root.as_deref().map(config::expand_home),
                    setup: p.setup,
                    exclusive: p.exclusive,
                    scan: crate::git::Scan {
                        exclude: p.exclude,
                        include: p.include,
                    },
                }
            })
            .collect();

        let templates = if config.agents.is_empty() {
            config::default_agents()
        } else {
            config.agents
        };
        let harnesses = config::harnesses(config.harnesses);

        // Reopen whatever was open last time, if that workspace still
        // exists — a name can disappear from the config between runs.
        let open = overlays
            .open_workspace
            .as_deref()
            .and_then(|name| workspaces.iter().find(|w| w.name == name).map(|w| w.id))
            .unwrap_or(default_ws);

        let (tree_tx, _) = broadcast::channel(32);
        let (workspaces_tx, _) = broadcast::channel(32);
        let daemon = Arc::new(Daemon {
            inner: StdMutex::new(Inner {
                workspaces,
                projects,
                ids,
                open,
                excluded,
            }),
            starting_agents: StdMutex::new(HashMap::new()),
            workspaces_tx,
            tree_tx,
            templates: StdMutex::new(templates),
            harnesses,
            hook_port: std::sync::atomic::AtomicU16::new(0),
            hook_token: gen_token(),
            restoring: std::sync::atomic::AtomicBool::new(false),
            store,
            restart_attempts: StdMutex::new(HashMap::new()),
            viewers: StdMutex::new(Viewers::default()),
            next_viewer: std::sync::atomic::AtomicU64::new(0),
        });
        // Checkout rows are named after the branch occupying them, and that
        // name now comes from the cache. Reading HEAD is enough for a name;
        // a full `git::status` walks every workdir, which on a cold disk
        // across a project root of many repositories is seconds in front of
        // the first client. The first poll tick fills dirty counts.
        daemon.refresh_git_status_with(crate::git::head);
        daemon
    }

    /// Clears any managed agent hooks left in a configured checkout by a
    /// previous daemon. They name that daemon's ephemeral port and per-boot
    /// token, so every one of them is stale by definition the moment this
    /// process starts — and a stale block fires on every turn of any agent
    /// the user later runs in that directory by hand. Best-effort per
    /// checkout: an unreadable or read-only one must not stop startup.
    pub fn sweep_stale_hooks(&self) {
        for path in self.checkout_paths() {
            for h in &self.harnesses {
                if let Err(e) = h.uninstall(&path) {
                    tracing::warn!(
                        "failed to clear stale {} hooks in {}: {e}",
                        h.name,
                        path.display()
                    );
                }
            }
        }
    }

    pub(super) fn checkout_paths(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap();
        checkouts(&inner.projects).map(|c| c.path.clone()).collect()
    }

    /// The harness an agent template speaks. A template that names none
    /// falls back to one matching its own name, so `name = "claude"` needs
    /// no extra key; anything unrecognized gets [`Harness::generic`], which
    /// installs nothing but still hands the pane the environment.
    pub(super) fn harness_for(&self, template: &AgentConfig) -> crate::harness::Harness {
        let wanted = template.harness.as_deref().unwrap_or(&template.name);
        self.harnesses
            .iter()
            .find(|h| h.name == wanted)
            .cloned()
            .unwrap_or_else(crate::harness::Harness::generic)
    }
}
