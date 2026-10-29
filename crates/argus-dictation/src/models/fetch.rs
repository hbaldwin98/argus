//! Downloading a model's file: resumed from where a stopped one ended, checked against its size and
//! sha256 before it takes its own name, and stopped when asked, keeping what it has.
//!
//! learn-harness's `electron/dictation-models.cjs`, in process over ureq as `argus update` fetches
//! a release: certificates are the system's own, and nothing else is started to do it.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::catalog::Entry;
use super::store;

/// What a server sends for a file from a byte on.
pub enum Opened {
    Body { body: Box<dyn Read + Send>, resumed: bool },
    /// It has nothing past that byte: the part already held is the whole file.
    Whole,
}

/// Where a file's bytes come from: the network, or a test's own.
pub trait Source {
    /// `url` from byte `from` on. `resumed` false is a server that ignored the range and sends the
    /// whole file again.
    fn open(&self, url: &str, from: u64) -> Result<Opened, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetched {
    Done,
    /// Asked to stop; the part already written stays, to resume from.
    Stopped,
}

/// Fetches `entry` into `dir`, telling `progress` the bytes on disk as they grow.
pub fn fetch(entry: &Entry, dir: &Path, source: &dyn Source, stop: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<Fetched, String> {
    if store::on_disk(dir, entry) == store::OnDisk::Ready {
        return Ok(Fetched::Done);
    }
    std::fs::create_dir_all(dir).map_err(|err| format!("could not make {}: {err}", dir.display()))?;
    let partial = store::partial(dir, entry);
    let mut from = std::fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
    if from > entry.size {
        from = 0;
    }
    let mut write = |from: u64, body: &mut dyn Read| -> Result<Fetched, String> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(from > 0)
            .truncate(from == 0)
            .open(&partial)
            .map_err(|err| format!("could not write {}: {err}", partial.display()))?;
        let (mut done, mut chunk) = (from, vec![0; 256 * 1024]);
        progress(done);
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(Fetched::Stopped);
            }
            let read = body.read(&mut chunk).map_err(|err| format!("the download broke off: {err}"))?;
            if read == 0 {
                return Ok(Fetched::Done);
            }
            done += read as u64;
            if done > entry.size {
                return Err(format!("{} is larger than it should be", entry.file));
            }
            file.write_all(&chunk[..read]).map_err(|err| format!("could not write {}: {err}", partial.display()))?;
            progress(done);
        }
    };
    let fetched = match source.open(&entry.url(), from)? {
        Opened::Whole => Fetched::Done,
        Opened::Body { mut body, resumed } => write(if resumed { from } else { 0 }, &mut body)?,
    };
    if fetched == Fetched::Stopped {
        return Ok(Fetched::Stopped);
    }
    let held = std::fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
    if held != entry.size {
        return Err(format!("the download ended at {held} of {} bytes; it resumes from there", entry.size));
    }
    if !matches_hash(&partial, entry.sha256)? {
        // Every byte of it is suspect, so none is kept to resume from.
        let _ = std::fs::remove_file(&partial);
        return Err(format!("{} did not match its sha256; it was deleted, so download it again", entry.file));
    }
    std::fs::rename(&partial, store::path(dir, entry)).map_err(|err| format!("could not keep {}: {err}", entry.file))?;
    Ok(Fetched::Done)
}

fn matches_hash(path: &Path, sha256: &str) -> Result<bool, String> {
    let mut file = std::fs::File::open(path).map_err(|err| format!("could not read {}: {err}", path.display()))?;
    let (mut hash, mut chunk) = (Sha256::new(), vec![0; 1 << 20]);
    loop {
        let read = file.read(&mut chunk).map_err(|err| format!("could not read {}: {err}", path.display()))?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    let actual: String = hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(actual.eq_ignore_ascii_case(sha256))
}

/// The network, through ureq with the system's certificates.
pub struct Https {
    agent: ureq::Agent,
}

impl Default for Https {
    fn default() -> Self {
        use ureq::tls::{RootCerts, TlsConfig};
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .user_agent(format!("argus/{}", env!("CARGO_PKG_VERSION")))
            // learn-harness's: a connection that never comes, or a server that never answers, is
            // given up on; a body is read for as long as it keeps coming.
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .new_agent();
        Https { agent }
    }
}

impl Source for Https {
    fn open(&self, url: &str, from: u64) -> Result<Opened, String> {
        let mut ask = self.agent.get(url);
        if from > 0 {
            ask = ask.header("Range", format!("bytes={from}-"));
        }
        match ask.call() {
            Ok(response) => {
                let resumed = response.status().as_u16() == 206;
                Ok(Opened::Body { body: Box::new(response.into_body().into_reader()), resumed })
            }
            Err(ureq::Error::StatusCode(416)) if from > 0 => Ok(Opened::Whole),
            Err(err) => Err(format!("could not download: {err}")),
        }
    }
}

#[cfg(test)]
mod tests;
