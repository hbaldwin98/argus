//! A whole daemon inside the test process, for tests that speak to it as a
//! client does: its store a file in a directory of its own, its accept loop
//! on an endpoint named after that directory, and its panes' processes and
//! clock scripted, so that what it answers depends only on what it was
//! asked. A fixture to stand on: the transcripts here, and generated
//! requests and injected faults after them.

pub mod render;
pub mod requests;
pub mod scrub;
pub mod transcript;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use argus_protocol::connection::transport::{self, Address, Listener};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast;

use crate::config::{ConfigFile, ProjectConfig};
use crate::connection::{accept, ShutdownReason};
use crate::state::scripted::ScriptedDaemon;

/// Either end of a connection, whichever the platform's transport hands
/// back.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Stream for T {}

pub struct Host {
    pub daemon: ScriptedDaemon,
    address: Address,
    shutdown: broadcast::Sender<ShutdownReason>,
    serving: tokio::task::JoinHandle<()>,
    /// Last, so the directory outlives everything holding a path in it.
    dir: tempfile::TempDir,
}

impl Host {
    /// A daemon with one project, `proj`, over the repository
    /// [`repository`] makes at `repo` in the host's directory.
    pub async fn start() -> Host {
        let dir = tempfile::tempdir().expect("a temp dir");
        let repo = dir.path().join("repo");
        repository(&repo);
        let config = ConfigFile {
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![repo.to_string_lossy().into_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };
        Self::start_in(dir, config).await
    }

    /// A daemon over `config`, keeping its store and its endpoint in `dir`.
    pub async fn start_in(dir: tempfile::TempDir, config: ConfigFile) -> Host {
        let store = crate::store::Store::open_at(&dir.path().join("runtime.db"))
            .expect("a store in a fresh directory should open");
        let daemon = ScriptedDaemon::with_store(config, store);
        let address = Address::in_dir(dir.path());
        let mut listener = Listener::bind_at(&address)
            .await
            .expect("a private endpoint should bind");
        let (shutdown, _) = broadcast::channel(1);
        let serving = tokio::spawn({
            let daemon = Arc::clone(&daemon);
            let (shutdown, shutdown_rx) = (shutdown.clone(), shutdown.subscribe());
            async move { accept::serve(&mut listener, &daemon, shutdown, shutdown_rx).await }
        });
        Host { daemon, address, shutdown, serving, dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn repo(&self) -> PathBuf {
        self.root().join("repo")
    }

    /// The first project, as the tree has it now.
    pub fn project(&self) -> argus_protocol::ProjectId {
        self.daemon.snapshot()[0].id
    }

    /// The first project's primary checkout.
    pub fn checkout(&self) -> argus_protocol::CheckoutId {
        self.daemon.snapshot()[0].repositories[0].checkouts[0].id
    }

    /// A new client connection, through the transport every client uses.
    pub async fn connect(&self) -> Box<dyn Stream> {
        Box::new(
            transport::connect_to(&self.address)
                .await
                .expect("the host's endpoint should accept"),
        )
    }

    /// Stops the daemon the way `argus server stop` does, and waits for
    /// every connection to be let go.
    pub async fn stop(self) {
        let _ = self.shutdown.send(ShutdownReason::Stop);
        tokio::time::timeout(std::time::Duration::from_secs(10), self.serving)
            .await
            .expect("the daemon should stop serving once asked")
            .expect("the accept loop should not panic");
    }
}

/// The time every fixture commit is made at: any fixed second will do, as
/// long as it is the same on every machine, so that commit ids are too.
const FIXTURE_TIME: i64 = 1_700_000_000;

/// Makes the repository a host starts with at `path`: two commits on
/// `main`, with the same bytes and the same ids on every machine. The
/// signatures and times are fixed, the initial branch is named rather than
/// read from the user's Git, and line endings are kept as written.
pub fn repository(path: &Path) {
    let mut init = git2::RepositoryInitOptions::new();
    init.initial_head("main");
    let repo = git2::Repository::init_opts(path, &init).expect("a repository should init");
    repo.config()
        .and_then(|mut config| config.set_bool("core.autocrlf", false))
        .expect("the repository's own config should take a value");
    let sig = git2::Signature::new("Fixture", "fixture@example.com", &git2::Time::new(FIXTURE_TIME, 0))
        .expect("a fixed signature");
    let commits: [(&str, &[(&str, &str)]); 2] = [
        ("Start the fixture", &[("README.md", "# fixture\n")]),
        ("Add the answer", &[("src/lib.rs", "pub fn answer() -> u32 {\n    42\n}\n")]),
    ];
    for (message, files) in commits {
        for (name, text) in files {
            let file = path.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, text).unwrap();
        }
        let mut index = repo.index().unwrap();
        for (name, _) in files {
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }
}
