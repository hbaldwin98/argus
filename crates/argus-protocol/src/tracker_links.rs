//! How a repository's work reaches the team's trackers: a feature's links
//! to tickets, how a ticket's tracker and key are read out of its URL, and
//! the repository's map of what in Argus answers to what in each tracker.
//!
//! Argus talks to no tracker. A link is what a person needs to get from a
//! feature to its ticket, and what an agent needs to know which ticket to
//! read with its own tools: a tracker's name, the key people say aloud, and
//! the URL a browser opens. Nothing from the ticket itself is kept, so
//! nothing shown beside the link can go stale.

use serde::{Deserialize, Serialize};

/// Longer than any tracker's ticket URL; past it the text is not a link.
pub const MAX_LINK_URL_BYTES: usize = 2048;
pub const MAX_LINK_KEY_BYTES: usize = 100;
pub const MAX_LINK_TRACKER_BYTES: usize = 40;
/// A feature is one piece of work; past this many tickets it is a project.
pub const MAX_FEATURE_LINKS: usize = 20;

/// A map is a page of plain words, not a config: long enough for a few
/// trackers' worth of rules, short enough that an agent reads it whole.
pub const MAX_TRACKER_MAP_BYTES: usize = 16 * 1024;

/// One ticket a feature is linked to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackerLink {
    /// What a person calls the tracker: `Linear`, `Jira`, `GitHub`, or the
    /// host for one Argus does not recognise.
    pub tracker: String,
    /// What a person calls the ticket: `ENG-88`, `owner/repo#12`. Unique on
    /// a feature, so it is what an unlink names.
    pub key: String,
    /// What the browser opens. Always `http` or `https`: the client hands it
    /// to the system's opener, which would as happily run a `file:` URL.
    pub url: String,
}

/// A link as it is asked for: a URL, and the tracker or key when the person
/// knows better than the URL does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkWrite {
    pub url: String,
    pub tracker: Option<String>,
    pub key: Option<String>,
}

impl LinkWrite {
    /// The link this asks for, its tracker and key read out of the URL
    /// unless given.
    pub fn checked(self) -> Result<TrackerLink, &'static str> {
        let url = self.url.trim().to_string();
        if url.is_empty() {
            return Err("a link needs the ticket's URL");
        }
        if url.len() > MAX_LINK_URL_BYTES {
            return Err("that is too long to be a ticket's URL");
        }
        if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("a URL has no spaces in it");
        }
        let Some(parts) = UrlParts::parse(&url) else {
            return Err("a link has to be an http or https URL");
        };

        let read = parts.ticket();
        let tracker = given(self.tracker).or_else(|| read.as_ref().map(|r| r.0.clone()));
        let key = given(self.key).or_else(|| read.map(|r| r.1));
        let Some(key) = key else {
            return Err("there is no ticket key in that URL; give one with --key");
        };
        let tracker = tracker.unwrap_or_else(|| parts.host.clone());

        if key.len() > MAX_LINK_KEY_BYTES {
            return Err("a ticket key is a short name like ENG-88");
        }
        if tracker.len() > MAX_LINK_TRACKER_BYTES {
            return Err("a tracker's name is a word or two");
        }
        Ok(TrackerLink { tracker, key, url })
    }
}

/// A repository's tracker map: in plain words, what a feature, a task and a
/// decision answer to in each tracker, and what never leaves Argus. Every
/// agent that links or reconciles reads it first. Argus stores it and never
/// parses it, so a tracker Argus has never heard of works the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerMap {
    /// The map as the person last approved or wrote it. Empty until then.
    pub body: String,
    /// A map an agent has proposed, waiting for the person. An agent can
    /// write only this; it becomes the map when the person approves it.
    pub draft: Option<MapDraft>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapDraft {
    pub body: String,
    /// Who proposed it, as the view names a pane: `pane #32`.
    pub by: String,
    /// Unix seconds.
    pub at: i64,
}

/// A change the person makes to a repository's map. An agent has none of
/// these: it proposes with a draft and waits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapChange {
    /// The map rewritten by hand. A waiting draft stays waiting.
    Write(String),
    /// The draft approved as this text: what the person read, perhaps
    /// edited, rather than whatever the draft holds by the time this
    /// arrives, since an agent can redraft in between.
    Approve(String),
    /// The draft turned down. The map stays as it was.
    Discard,
}

/// The map text as it will be kept, or why it will not.
pub fn checked_map(body: &str) -> Result<String, &'static str> {
    let body = body.trim_end();
    if body.len() > MAX_TRACKER_MAP_BYTES {
        return Err("a tracker map is a page, not a manual; this one is too long");
    }
    if body.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err("a tracker map is plain text");
    }
    Ok(body.to_string())
}

fn given(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// The two pieces of a URL a ticket is found in. Hand-split rather than a
/// URL crate's: Argus needs the host and the path segments, and refuses
/// everything else.
struct UrlParts {
    host: String,
    segments: Vec<String>,
}

impl UrlParts {
    fn parse(url: &str) -> Option<UrlParts> {
        let (scheme, rest) = url.split_once("://")?;
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }

        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority.rsplit('@').next().unwrap_or_default();
        let host = host.split(':').next().unwrap_or_default().to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        if host.is_empty() {
            return None;
        }

        let segments = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Some(UrlParts { host, segments })
    }

    /// The tracker and key the URL names: by its known shape for the
    /// trackers Argus recognises, else the host and the last path segment.
    fn ticket(&self) -> Option<(String, String)> {
        let seg = |i: usize| self.segments.get(i).map(String::as_str);

        if self.host == "linear.app" && seg(1) == Some("issue") {
            return seg(2).map(|key| ("Linear".to_string(), key.to_ascii_uppercase()));
        }
        if self.host.ends_with(".atlassian.net") && seg(0) == Some("browse") {
            return seg(1).map(|key| ("Jira".to_string(), key.to_ascii_uppercase()));
        }
        if self.host == "github.com" && matches!(seg(2), Some("issues" | "pull")) {
            if let (Some(owner), Some(repo), Some(number)) = (seg(0), seg(1), seg(3)) {
                return Some(("GitHub".to_string(), format!("{owner}/{repo}#{number}")));
            }
        }

        let last = self.segments.last()?;
        Some((self.host.clone(), last.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(url: &str) -> Result<TrackerLink, &'static str> {
        LinkWrite { url: url.into(), ..LinkWrite::default() }.checked()
    }

    #[test]
    fn a_linear_url_names_its_issue() {
        let l = link("https://linear.app/acme/issue/ENG-88/trim-argus-context-injection").unwrap();
        assert_eq!((l.tracker.as_str(), l.key.as_str()), ("Linear", "ENG-88"));
    }

    #[test]
    fn a_jira_url_names_its_issue() {
        let l = link("https://acme.atlassian.net/browse/ARG-142?focusedCommentId=3").unwrap();
        assert_eq!((l.tracker.as_str(), l.key.as_str()), ("Jira", "ARG-142"));
    }

    #[test]
    fn a_github_url_names_its_repository_and_number() {
        let l = link("https://github.com/hbaldwin98/argus/issues/12#issuecomment-1").unwrap();
        assert_eq!((l.tracker.as_str(), l.key.as_str()), ("GitHub", "hbaldwin98/argus#12"));
        let pr = link("https://www.github.com/hbaldwin98/argus/pull/40").unwrap();
        assert_eq!(pr.key, "hbaldwin98/argus#40");
    }

    #[test]
    fn an_unknown_tracker_is_its_host_and_the_last_segment() {
        let l = link("https://youtrack.example.com:8443/issue/YT-7/").unwrap();
        assert_eq!((l.tracker.as_str(), l.key.as_str()), ("youtrack.example.com", "YT-7"));
    }

    #[test]
    fn what_the_person_says_beats_what_the_url_says() {
        let l = LinkWrite {
            url: "https://tracker.example/t/1234".into(),
            tracker: Some(" Shortcut ".into()),
            key: Some("SC-1234".into()),
        }
        .checked()
        .unwrap();
        assert_eq!((l.tracker.as_str(), l.key.as_str()), ("Shortcut", "SC-1234"));
    }

    #[test]
    fn the_url_is_kept_as_given() {
        let url = "https://linear.app/acme/issue/eng-88/x";
        let l = link(url).unwrap();
        assert_eq!(l.url, url);
        assert_eq!(l.key, "ENG-88");
    }

    #[test]
    fn only_a_web_url_is_a_link() {
        assert!(link("file:///etc/passwd").is_err());
        assert!(link("javascript:alert(1)").is_err());
        assert!(link("ENG-88").is_err());
        assert!(link("https://").is_err());
        assert!(link("https://linear.app/a b").is_err());
    }

    #[test]
    fn a_map_is_plain_text_of_bounded_length() {
        assert_eq!(checked_map("Linear: a feature is an issue\n\n").unwrap(), "Linear: a feature is an issue");
        assert!(checked_map("a\u{1b}[2Jb").is_err());
        assert!(checked_map(&"x".repeat(MAX_TRACKER_MAP_BYTES + 1)).is_err());
    }

    #[test]
    fn a_url_with_no_path_needs_a_key() {
        assert!(link("https://tracker.example").is_err());
        let keyed = LinkWrite {
            url: "https://tracker.example".into(),
            key: Some("T-1".into()),
            ..LinkWrite::default()
        };
        assert_eq!(keyed.checked().unwrap().tracker, "tracker.example");
    }
}
