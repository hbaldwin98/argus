//! Client<->daemon transport: a Unix domain socket on Unix, a named pipe on
//! Windows. Both sides talk length-prefixed msgpack (see [`crate::framing`])
//! over whatever stream type this module hands back.

use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = directories::BaseDirs::new().and_then(|d| {
        d.runtime_dir()
            .map(|p| p.to_path_buf())
            .or_else(|| Some(d.cache_dir().to_path_buf()))
    }) {
        dir
    } else {
        std::env::temp_dir()
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::io;
    use tokio::net::{UnixListener, UnixStream};

    pub fn socket_path() -> PathBuf {
        // A named instance gets its own file, so two daemons — an
        // installed one and a worktree's — can listen side by side.
        let file = match crate::instance_name() {
            Some(name) => format!("argus-{name}.sock"),
            None => "argus.sock".to_string(),
        };
        runtime_dir().join(file)
    }

    pub struct Listener(UnixListener);

    impl Listener {
        pub async fn bind() -> io::Result<Self> {
            let path = socket_path();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::remove_file(&path);
            Ok(Listener(UnixListener::bind(&path)?))
        }

        pub async fn accept(&mut self) -> io::Result<UnixStream> {
            let (stream, _addr) = self.0.accept().await?;
            Ok(stream)
        }
    }

    pub async fn connect() -> io::Result<UnixStream> {
        UnixStream::connect(socket_path()).await
    }

    pub fn is_daemon_listening() -> bool {
        std::os::unix::net::UnixStream::connect(socket_path()).is_ok()
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
    use tokio::time::{sleep, Duration};

    pub fn pipe_name() -> String {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
        // A named instance gets its own pipe, so two daemons — an
        // installed one and a worktree's — can listen side by side.
        match crate::instance_name() {
            Some(name) => format!(r"\\.\pipe\argus-{user}-{name}"),
            None => format!(r"\\.\pipe\argus-{user}"),
        }
    }

    pub struct Listener {
        name: String,
        current: NamedPipeServer,
    }

    impl Listener {
        pub async fn bind() -> io::Result<Self> {
            let name = pipe_name();
            let current = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&name)?;
            Ok(Listener { name, current })
        }

        pub async fn accept(&mut self) -> io::Result<NamedPipeServer> {
            self.current.connect().await?;
            let next = ServerOptions::new().create(&self.name)?;
            Ok(std::mem::replace(&mut self.current, next))
        }
    }

    pub async fn connect() -> io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
        let name = pipe_name();
        let mut last_err = None;
        for attempt in 0..20u64 {
            match ClientOptions::new().open(&name) {
                Ok(client) => return Ok(client),
                Err(e) => {
                    last_err = Some(e);
                    sleep(Duration::from_millis(25 * (attempt + 1))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "could not open argus pipe")))
    }

    pub fn is_daemon_listening() -> bool {
        ClientOptions::new().open(pipe_name()).is_ok()
    }
}

#[cfg(unix)]
pub use imp::{connect, is_daemon_listening, socket_path, Listener};

#[cfg(windows)]
pub use imp::{connect, is_daemon_listening, pipe_name, Listener};
