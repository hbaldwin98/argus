//! Fetching a release's files over HTTPS, in this process, and holding an
//! archive to the checksum published beside it.
//!
//! In process rather than through `curl` or PowerShell: a program that
//! starts a downloader to fetch executables is what endpoint security
//! watches for. Certificates are checked against the operating system's
//! own store, so a machine behind a proxy that re-signs TLS trusts what
//! its administrator installed.

use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::tls::{RootCerts, TlsConfig};

/// Far past any archive a release has had, which are under 20 MB, so a
/// server that never stops sending cannot fill the memory.
const ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;
/// An API answer or a checksum file.
const TEXT_LIMIT: u64 = 1024 * 1024;

pub struct Fetcher {
    agent: ureq::Agent,
}

impl Fetcher {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            // GitHub's API refuses a request that names no agent.
            .user_agent(format!("argus/{}", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .new_agent();
        Fetcher { agent }
    }

    pub fn text(&self, url: &str) -> anyhow::Result<String> {
        let bytes = self.bytes(url, TEXT_LIMIT)?;
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("{url} did not answer with text"))
    }

    /// The archive at `url`, telling `progress` the bytes read so far and
    /// the total when the server said it.
    pub fn archive(&self, url: &str, progress: &mut dyn FnMut(u64, Option<u64>)) -> anyhow::Result<Vec<u8>> {
        use std::io::Read;

        let mut response =
            self.agent.get(url).call().map_err(|error| anyhow::anyhow!("could not fetch {url}: {error}"))?;
        let total = response.body().content_length();
        let mut reader = response.body_mut().with_config().limit(ARCHIVE_LIMIT).reader();
        let mut archive = Vec::with_capacity(total.unwrap_or(0).min(ARCHIVE_LIMIT) as usize);
        let mut chunk = vec![0; 64 * 1024];
        progress(0, total);
        loop {
            let read = reader.read(&mut chunk).map_err(|error| anyhow::anyhow!("could not read {url}: {error}"))?;
            if read == 0 {
                break;
            }
            archive.extend_from_slice(&chunk[..read]);
            progress(archive.len() as u64, total);
        }
        Ok(archive)
    }

    fn bytes(&self, url: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| anyhow::anyhow!("could not fetch {url}: {error}"))?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|error| anyhow::anyhow!("could not read {url}: {error}"))
    }
}

/// Checks `archive` against its `.sha256` file, `HASH  NAME` as the
/// release workflow writes it, and that the file is about this archive.
pub fn verify(archive: &[u8], name: &str, checksum_file: &str) -> anyhow::Result<()> {
    let mut fields = checksum_file.split_whitespace();
    let (Some(expected), Some(named)) = (fields.next(), fields.next()) else {
        anyhow::bail!("the checksum published for {name} is not `HASH  NAME`");
    };
    if named.trim_start_matches('*') != name {
        anyhow::bail!("the checksum published for {name} is for {named}");
    }
    let actual: String = Sha256::digest(archive).iter().map(|byte| format!("{byte:02x}")).collect();
    if !actual.eq_ignore_ascii_case(expected) {
        anyhow::bail!("{name} does not match its published checksum: downloaded {actual}, published {expected}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The SHA-256 of "abc".
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn an_archive_matching_its_published_checksum_passes() {
        verify(b"abc", "a.zip", &format!("{ABC}  a.zip\n")).unwrap();
        // Windows writes it with Set-Content, upper case would also do.
        verify(b"abc", "a.zip", &format!("{}  a.zip\r\n", ABC.to_uppercase())).unwrap();
    }

    #[test]
    fn an_archive_that_differs_from_its_checksum_is_refused() {
        let refused = verify(b"abd", "a.zip", &format!("{ABC}  a.zip")).unwrap_err();
        assert!(refused.to_string().contains("does not match"), "{refused}");
    }

    #[test]
    fn a_checksum_for_another_file_or_in_another_shape_is_refused() {
        assert!(verify(b"abc", "a.zip", &format!("{ABC}  b.zip")).is_err());
        assert!(verify(b"abc", "a.zip", ABC).is_err());
        assert!(verify(b"abc", "a.zip", "").is_err());
    }
}
