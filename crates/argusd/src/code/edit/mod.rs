//! A working-tree file held open for one client to edit: the text last
//! read or written and the version it is, a client's change written only
//! while the disk still holds that text, and any other write — an agent's
//! above all — read back and sent as a change of its own as it lands.

pub mod text;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argus_protocol::{is_checkout_relative, CheckoutId, EditChanged, EditOpened, ServerMsg, TextChange};
use tokio::sync::mpsc;

use crate::code::{BINARY_NOTE, BINARY_SNIFF, MAX_FILE_BYTES, TOO_LARGE_NOTE};

/// How long apart two reads of a write that just landed are made. A write
/// that truncates first can be read empty halfway, and sent so, the editor
/// would see its whole text deleted and put back: a write is taken once two
/// reads this far apart agree.
const SETTLE: Duration = Duration::from_millis(8);
/// How many reads may disagree before the file is taken as it last read.
const SETTLE_READS: usize = 5;

/// Why a file could not be read: for now, or for good.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// Held by a writer, or between a delete and its replacement.
    Passing(String),
    /// Gone, too large, binary, or not text.
    Lasting(String),
}

fn read(file: &Path, path: &str) -> Result<String, Failure> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Failure::Passing(format!("{path} was deleted"))),
        Err(e) => return Err(Failure::Passing(format!("could not read {path}: {e}"))),
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Failure::Lasting(format!("{path} is {TOO_LARGE_NOTE}")));
    }
    if bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0) {
        return Err(Failure::Lasting(format!("{path} is a {BINARY_NOTE}")));
    }
    String::from_utf8(bytes).map_err(|_| Failure::Lasting(format!("{path} is not UTF-8 text")))
}

/// Waits out a write still landing: returns once two reads `SETTLE` apart
/// agree, or after `SETTLE_READS`. What it read is not used — the reader
/// reads again under the lock — only that the file has stopped moving.
fn settle(file: &Path, path: &str) {
    let mut last = read(file, path);
    for _ in 0..SETTLE_READS {
        std::thread::sleep(SETTLE);
        let again = read(file, path);
        if again == last {
            return;
        }
        last = again;
    }
}

/// One file as last read or written for the client.
#[derive(Debug)]
pub struct Held {
    checkout: CheckoutId,
    path: String,
    file: PathBuf,
    /// With each CRLF read as LF; `crlf` puts them back on a write.
    text: String,
    crlf: bool,
    version: u64,
}

impl Held {
    /// `path` in the checkout at `root`, refused when it is not a text file
    /// inside it. A link that leads out of the checkout is read by the Code
    /// stage but never written through.
    pub fn open(checkout: CheckoutId, root: &Path, path: &str) -> Result<Held, String> {
        if !is_checkout_relative(path) {
            return Err(format!("{path} is not a path inside the checkout"));
        }
        let file = root.join(path);
        let real = file.canonicalize().map_err(|e| format!("could not open {path}: {e}"))?;
        let inside = root.canonicalize().is_ok_and(|root| real.starts_with(root));
        if !inside {
            return Err(format!("{path} leads outside the checkout"));
        }
        if !real.is_file() {
            return Err(format!("{path} is not a file"));
        }
        let raw = read(&file, path).map_err(|(Failure::Passing(why) | Failure::Lasting(why))| why)?;
        let (text, crlf) = text::lf(raw);
        Ok(Held { checkout, path: path.to_owned(), file, text, crlf, version: 0 })
    }

    pub fn opened(&self) -> ServerMsg {
        ServerMsg::EditOpened(EditOpened {
            checkout: self.checkout,
            path: self.path.clone(),
            version: self.version,
            text: self.text.clone(),
        })
    }

    pub fn closed(&self, reason: String) -> ServerMsg {
        ServerMsg::EditClosed { checkout: self.checkout, path: self.path.clone(), reason }
    }

    fn refused(&self, base: u64, reason: Option<String>) -> ServerMsg {
        ServerMsg::EditRefused { checkout: self.checkout, path: self.path.clone(), base, reason }
    }

    /// What the disk holds now, as the change from what is held, which it
    /// becomes; `None` when nothing changed or the file could not be read
    /// just now. `Err` says why the file can be held no longer.
    pub fn reread(&mut self) -> Result<Option<ServerMsg>, String> {
        let raw = match read(&self.file, &self.path) {
            Ok(raw) => raw,
            Err(Failure::Passing(_)) => return Ok(None),
            Err(Failure::Lasting(why)) => return Err(why),
        };
        let (text, crlf) = text::lf(raw);
        self.crlf = crlf;
        if text == self.text {
            return Ok(None);
        }
        let changes = text::diff(&self.text, &text);
        let from = self.version;
        self.version += 1;
        self.text = text;
        Ok(Some(ServerMsg::EditChanged(EditChanged {
            checkout: self.checkout,
            path: self.path.clone(),
            from,
            version: self.version,
            changes,
        })))
    }

    /// Writes the client's `changes`, made on `base`, when the disk still
    /// holds what `base` was: what to tell the client, in order. A write by
    /// anything else that landed first is told before the refusal, so the
    /// client can make its change again on it.
    pub fn write(&mut self, base: u64, changes: &[TextChange]) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        match self.reread() {
            Ok(Some(changed)) => out.push(changed),
            Ok(None) => {}
            Err(why) => {
                out.push(self.closed(why));
                return out;
            }
        }
        if base != self.version {
            out.push(self.refused(base, None));
            return out;
        }
        let text = match text::apply(&self.text, changes) {
            Ok(text) => text,
            Err(why) => {
                out.push(self.refused(base, Some(why)));
                return out;
            }
        };
        let bytes = text::on_disk(&text, self.crlf);
        if bytes.len() > MAX_FILE_BYTES {
            out.push(self.refused(base, Some(format!("{} would be {TOO_LARGE_NOTE}", self.path))));
            return out;
        }
        if text != self.text {
            if let Err(e) = std::fs::write(&self.file, bytes.as_bytes()) {
                out.push(self.refused(base, Some(format!("could not write {}: {e}", self.path))));
                return out;
            }
            self.text = text;
            self.version += 1;
        }
        out.push(ServerMsg::EditSaved {
            checkout: self.checkout,
            path: self.path.clone(),
            base,
            version: self.version,
        });
        out
    }
}

/// A file open for one connection: what is held of it, and the task that
/// watches the disk for it, which ends when this is dropped — the client
/// closed it, opened it again, or went.
pub struct Open {
    held: Arc<Mutex<Option<Held>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Open {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Open {
    /// Reads `path` and tells `out` it is open, or why it is not, then
    /// sends each write to it by anything else as it lands.
    pub fn start(checkout: CheckoutId, root: PathBuf, path: String, out: mpsc::UnboundedSender<ServerMsg>) -> Open {
        let held = Arc::new(Mutex::new(None::<Held>));
        let slot = held.clone();
        let task = tokio::spawn(async move {
            let opened = {
                let (slot, out) = (slot.clone(), out.clone());
                tokio::task::spawn_blocking(move || match Held::open(checkout, &root, &path) {
                    Ok(held) => {
                        let file = held.file.clone();
                        let mut slot = slot.lock().unwrap();
                        let _ = out.send(held.opened());
                        *slot = Some(held);
                        Some((file, path))
                    }
                    Err(reason) => {
                        let _ = out.send(ServerMsg::EditClosed { checkout, path, reason });
                        None
                    }
                })
                .await
            };
            let Ok(Some((file, path))) = opened else {
                return;
            };
            let (wake, mut woken) = mpsc::channel::<()>(1);
            // Held as long as the task runs.
            let _watch = crate::system::file_watch::watch(&file, move || {
                let _ = wake.try_send(());
            });
            while woken.recv().await.is_some() {
                let (slot, out, file, path) = (slot.clone(), out.clone(), file.clone(), path.clone());
                let ended = tokio::task::spawn_blocking(move || {
                    // Outside the lock: the client's own writes must not
                    // wait the settling out.
                    settle(&file, &path);
                    let mut slot = slot.lock().unwrap();
                    let Some(held) = slot.as_mut() else {
                        return true;
                    };
                    match held.reread() {
                        Ok(Some(changed)) => {
                            let _ = out.send(changed);
                            false
                        }
                        Ok(None) => false,
                        Err(why) => {
                            let _ = out.send(held.closed(why));
                            *slot = None;
                            true
                        }
                    }
                })
                .await
                .unwrap_or(true);
                if ended {
                    break;
                }
            }
        });
        Open { held, task }
    }

    /// Writes the client's change off the message loop, telling `out` how
    /// it went.
    pub fn edit(&self, checkout: CheckoutId, path: String, base: u64, changes: Vec<TextChange>, out: mpsc::UnboundedSender<ServerMsg>) {
        let held = self.held.clone();
        tokio::task::spawn_blocking(move || {
            let mut slot = held.lock().unwrap();
            let Some(file) = slot.as_mut() else {
                let reason = Some(format!("{path} is not open"));
                let _ = out.send(ServerMsg::EditRefused { checkout, path, base, reason });
                return;
            };
            let said = file.write(base, &changes);
            let closed = said.iter().any(|msg| matches!(msg, ServerMsg::EditClosed { .. }));
            for msg in said {
                let _ = out.send(msg);
            }
            if closed {
                *slot = None;
            }
        });
    }
}

#[cfg(test)]
mod tests;
