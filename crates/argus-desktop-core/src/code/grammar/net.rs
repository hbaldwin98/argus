//! How a grammar's bytes come from the network: HTTPS to the three hosts a
//! grammar is ever fetched from and no others, and an answer that is not a
//! plain 200 is said as such. A trait, so the install flow is tested against
//! a table of answers instead of npm.

use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig};

/// Where the page's content security policy lets the editor talk
/// (EDITOR-CSP-1): the registry that searches, and the two jsDelivr hosts
/// that list and serve a package. A url anywhere else is refused before a
/// request is made, so a malformed listing cannot point the install elsewhere.
const HOSTS: [&str; 3] = ["registry.npmjs.org", "data.jsdelivr.com", "cdn.jsdelivr.net"];

/// A parser is a few megabytes at the largest; this stops a server that
/// never stops sending from filling the memory.
pub const FILE_LIMIT: u64 = 32 * 1024 * 1024;
/// A search answer or a file listing.
pub const TEXT_LIMIT: u64 = 8 * 1024 * 1024;

pub trait Http {
    /// The body at `url`, or the sentence to show when there is none:
    /// `<url> answered <status>` for a refusal.
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}

/// Whether `url` is an `https` address on one of the three hosts.
pub fn allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some_and(|h| HOSTS.contains(&h)))
}

pub struct Net {
    agent: ureq::Agent,
}

impl Net {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .user_agent(format!("argus/{}", env!("CARGO_PKG_VERSION")))
            // The status is the answer's to say, not an error of the call's;
            // and a redirect is not followed, which could leave the three hosts.
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .new_agent();
        Net { agent }
    }
}

impl Default for Net {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for Net {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        if !allowed(url) {
            return Err(format!("{url} is not an address Argus fetches grammars from"));
        }
        let mut response = self.agent.get(url).call().map_err(|error| format!("could not fetch {url}: {error}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("{url} answered {status}"));
        }
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|error| format!("could not read {url}: {error}"))
    }
}

/// A table of answers by url, for the flows above this to be tested without
/// a network; a url not in it is a 404, and every request is kept.
#[cfg(test)]
#[derive(Default)]
pub struct Table {
    pub answers: std::collections::HashMap<String, Vec<u8>>,
    pub asked: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
impl Table {
    pub fn with(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
        self.answers.insert(url.to_owned(), body.into());
        self
    }
}

#[cfg(test)]
impl Http for Table {
    fn get(&self, url: &str, _limit: u64) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        self.answers.get(url).cloned().ok_or_else(|| format!("{url} answered 404"))
    }
}

#[cfg(test)]
mod tests;
