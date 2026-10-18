//! What in a transcript differs between runs and machines while the
//! daemon's behavior does not, written in words that stay put: the host's
//! directory, the build's version and the wall clock. Order is the
//! transcript's to settle, since only it knows where a request ends.

use std::path::Path;

use serde_json::Value;

/// How far a stamp may sit outside the run and still be the run's own: a
/// second for rounding either way, and a minute for a slow machine.
const CLOCK_SLACK: i64 = 60;

pub struct Scrub {
    /// Every spelling of the host's directory, longest first, so that one
    /// spelling never leaves part of a longer one behind.
    roots: Vec<String>,
    version: &'static str,
    started: i64,
}

impl Scrub {
    pub fn new(root: &Path) -> Scrub {
        let mut roots = Vec::new();
        let mut spell = |path: &Path| {
            let text = path.to_string_lossy().into_owned();
            // A canonical Windows path carries the verbatim prefix, and
            // either kind may be written with forward slashes.
            let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
            roots.push(text.replace('\\', "/"));
            roots.push(text);
        };
        spell(root);
        if let Ok(canonical) = root.canonicalize() {
            spell(&canonical);
        }
        roots.sort_by_key(|root| std::cmp::Reverse(root.len()));
        roots.dedup();
        Scrub {
            roots,
            version: env!("CARGO_PKG_VERSION"),
            started: now(),
        }
    }

    pub fn value(&self, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text)),
            Value::Number(number) => match number.as_i64().and_then(|n| self.stamp(n)) {
                Some(stamp) => Value::String(stamp.to_owned()),
                None => Value::Number(number),
            },
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.value(v)).collect()),
            Value::Object(fields) => Value::Object(
                fields.into_iter().map(|(k, v)| (k, self.value(v))).collect(),
            ),
            other => other,
        }
    }

    /// A path under the host's directory reads the same on every platform:
    /// `[root]` and then forward slashes.
    fn text(&self, text: String) -> String {
        if text == self.version {
            return "[version]".to_owned();
        }
        let mut text = text;
        for root in &self.roots {
            text = text.replace(root.as_str(), "[root]");
        }
        if text.contains("[root]") {
            text = text.replace('\\', "/");
        }
        text
    }

    /// A number that could only be a stamp taken during the run, in
    /// seconds or in milliseconds.
    fn stamp(&self, n: i64) -> Option<&'static str> {
        let during = |secs: i64| (self.started - CLOCK_SLACK..=now() + CLOCK_SLACK).contains(&secs);
        if during(n) {
            Some("[now]")
        } else if during(n / 1000) {
            Some("[now ms]")
        } else {
            None
        }
    }
}

fn now() -> i64 {
    argus_protocol::ago::now() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_spelling_of_the_root_reads_as_one() {
        let dir = tempfile::tempdir().unwrap();
        let scrub = Scrub::new(dir.path());
        let native = dir.path().join("repo").to_string_lossy().into_owned();
        let forward = native.replace('\\', "/");
        let scrubbed = scrub.value(json!([native, forward, "unrelated/path"]));
        assert_eq!(scrubbed, json!(["[root]/repo", "[root]/repo", "unrelated/path"]));
    }

    #[test]
    fn a_stamp_from_the_run_is_scrubbed_and_a_fixed_one_is_not() {
        let scrub = Scrub::new(Path::new("/nowhere"));
        let now = now();
        let scrubbed = scrub.value(json!({ "at": now, "ms": now * 1000, "fixed": 1_700_000_000, "small": 3 }));
        assert_eq!(
            scrubbed,
            json!({ "at": "[now]", "ms": "[now ms]", "fixed": 1_700_000_000, "small": 3 })
        );
    }

    #[test]
    fn the_builds_version_is_scrubbed_only_where_it_is_the_whole_string() {
        let scrub = Scrub::new(Path::new("/nowhere"));
        let version = env!("CARGO_PKG_VERSION");
        let scrubbed = scrub.value(json!([version, format!("v{version}")]));
        assert_eq!(scrubbed, json!(["[version]", format!("v{version}")]));
    }
}
