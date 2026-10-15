//! Running the configured extensions: each started with its own URL and
//! token, started again with a growing wait when it exits, and stopped
//! when the daemon stops.
//!
//! An extension is admitted before it starts and retired the moment it
//! exits, so a crashed one shows nothing — no tabs, no keys, no dialogs —
//! until the one started in its place publishes again.

use std::process::Stdio;
use std::time::{Duration, Instant};

use argus_protocol::{extension_url, EXTENSION_VAR, TOKEN_VAR, URL_VAR};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::*;
use crate::config::ExtensionConfig;

/// How long the first restart waits, and the longest any waits. One that
/// ran this long before exiting starts from the shortest again: it was
/// working, and what ended it is not the start.
const RESTART_MIN: Duration = Duration::from_secs(1);
const RESTART_MAX: Duration = Duration::from_secs(60);

/// The extensions this daemon runs, for stopping them with it.
pub struct ExtensionHost {
    stop: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
    /// How many times any was started, for a test to see a restart.
    #[cfg(test)]
    starts: Arc<std::sync::atomic::AtomicUsize>,
}

impl ExtensionHost {
    /// Ends every extension and waits until each has gone.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

/// What each extension's task is handed.
struct Running {
    config: ExtensionConfig,
    logs: Option<PathBuf>,
    first_wait: Duration,
    starts: Arc<std::sync::atomic::AtomicUsize>,
}

impl Daemon {
    /// Starts every extension `configs` declares, each in a task of its own
    /// that keeps it running, its output kept in `logs`. After the hook
    /// server, whose port each is handed.
    pub fn start_extensions(self: &Arc<Self>, configs: Vec<ExtensionConfig>, logs: PathBuf) -> ExtensionHost {
        self.start_extensions_waiting(configs, Some(logs), RESTART_MIN)
    }

    fn start_extensions_waiting(
        self: &Arc<Self>,
        configs: Vec<ExtensionConfig>,
        logs: Option<PathBuf>,
        first_wait: Duration,
    ) -> ExtensionHost {
        let (stop, stopped) = watch::channel(false);
        let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let tasks = configs
            .into_iter()
            .map(|config| {
                let running = Running { config, logs: logs.clone(), first_wait, starts: starts.clone() };
                let (daemon, stopped) = (self.clone(), stopped.clone());
                tokio::spawn(async move { daemon.keep_running(running, stopped).await })
            })
            .collect();
        ExtensionHost {
            stop,
            tasks,
            #[cfg(test)]
            starts,
        }
    }

    async fn keep_running(self: Arc<Self>, running: Running, mut stopped: watch::Receiver<bool>) {
        let Running { config, logs, first_wait, starts } = running;
        let mut wait = first_wait;
        loop {
            let token = self.admit_extension(&config.name);
            let started = Instant::now();
            starts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            match self.launch_command(&config, &token, logs.as_deref()).spawn() {
                Ok(mut child) => {
                    tracing::info!("extension {} started", config.name);
                    tokio::select! {
                        status = child.wait() => {
                            tracing::warn!("extension {} exited: {status:?}", config.name);
                        }
                        _ = stopped.changed() => {
                            let _ = child.kill().await;
                            self.retire_extension(&config.name);
                            return;
                        }
                    }
                }
                Err(error) => tracing::warn!("could not start extension {}: {error}", config.name),
            }
            self.retire_extension(&config.name);
            if started.elapsed() >= RESTART_MAX {
                wait = first_wait;
            }
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = stopped.changed() => return,
            }
            wait = (wait * 2).min(RESTART_MAX);
        }
    }

    /// The command an extension runs as: its own program and arguments,
    /// told where the pane API is and its token, its output kept in a log
    /// in `logs`, the previous run's replaced.
    fn launch_command(
        &self,
        config: &ExtensionConfig,
        token: &str,
        logs: Option<&std::path::Path>,
    ) -> tokio::process::Command {
        let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        let dir = config.dir.as_deref().map(config::expand_home).unwrap_or_else(argus_protocol::config_dir);
        let mut command = tokio::process::Command::new(&config.command[0]);
        command
            .args(&config.command[1..])
            .envs(&config.env)
            .env(URL_VAR, extension_url(port, &config.name))
            .env(TOKEN_VAR, token)
            .env(EXTENSION_VAR, &config.name)
            .current_dir(dir)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let log = logs.map(|logs| logs.join(format!("extension-{}.log", config.name)));
        match log.map(|log| std::fs::File::create(log).and_then(|out| Ok((out.try_clone()?, out)))) {
            Some(Ok((out, err))) => {
                command.stdout(out).stderr(err);
            }
            _ => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        // A script run by an interpreter would otherwise open a console
        // window of its own beside a daemon that has none.
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon() -> Arc<Daemon> {
        Daemon::new(crate::config::ConfigFile::default())
    }

    fn extension(name: &str, command: &[&str]) -> ExtensionConfig {
        ExtensionConfig {
            name: name.to_string(),
            command: command.iter().map(|part| part.to_string()).collect(),
            env: Default::default(),
            dir: Some(".".to_string()),
        }
    }

    /// A command that exits at once, on either platform.
    fn exits_at_once(name: &str) -> ExtensionConfig {
        if cfg!(windows) {
            extension(name, &["cmd", "/C", "exit 1"])
        } else {
            extension(name, &["sh", "-c", "exit 1"])
        }
    }

    async fn starts_reach(host: &ExtensionHost, count: usize) -> bool {
        for _ in 0..250 {
            if host.starts.load(std::sync::atomic::Ordering::Relaxed) >= count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    #[tokio::test]
    async fn an_extension_that_exits_is_started_again_and_admits_nothing_once_stopped() {
        let d = daemon();
        let host = d.start_extensions_waiting(vec![exits_at_once("crashy")], None, Duration::from_millis(10));
        assert!(starts_reach(&host, 3).await, "started again after each exit");
        host.stop().await;
        assert!(!d.extension_authorized("crashy", Some("anything")), "nothing left admitted");
    }

    #[tokio::test]
    async fn one_that_cannot_start_is_tried_again() {
        let d = daemon();
        let missing = extension("missing", &["argus-no-such-program-anywhere"]);
        let host = d.start_extensions_waiting(vec![missing], None, Duration::from_millis(10));
        assert!(starts_reach(&host, 2).await);
        host.stop().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stopping_ends_a_running_extension_and_takes_its_views_away() {
        let d = daemon();
        let host = d.start_extensions_waiting(vec![extension("sleeper", &["sleep", "30"])], None, Duration::from_millis(10));
        assert!(starts_reach(&host, 1).await);
        let view: argus_protocol::ExtView = serde_json::from_str(r#"{"name":"v","title":"V"}"#).unwrap();
        for _ in 0..100 {
            if d.publish_views("sleeper", vec![view.clone()]).is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(d.extension_views().len(), 1);
        tokio::time::timeout(Duration::from_secs(5), host.stop()).await.expect("stops promptly");
        assert!(d.extension_views().is_empty());
    }
}
