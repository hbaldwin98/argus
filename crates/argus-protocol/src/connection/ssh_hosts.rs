//! The hosts the user's ssh config names, for the host picker to offer.
//!
//! Only what `Host` lines name outright: a pattern with a wildcard or a
//! negation names no host anyone could pick, and `Match` blocks name none.
//! `Include`d files are read too, relative to `~/.ssh` as ssh reads them.

use std::path::{Path, PathBuf};

/// How deep `Include` is followed, which is also what stops a file that
/// includes itself.
const MAX_INCLUDE_DEPTH: usize = 8;

/// Every plain host `~/.ssh/config` names, in the order it names them.
pub fn hosts() -> Vec<String> {
    let Some(home) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    let mut found = Vec::new();
    read(&ssh.join("config"), &ssh, &home, &mut found, 0);
    found
}

fn read(path: &Path, ssh: &Path, home: &Path, found: &mut Vec<String>, depth: usize) {
    if depth > MAX_INCLUDE_DEPTH {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let Some((keyword, values)) = split(line) else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("host") {
            for name in words(values) {
                let plain = !name.contains(['*', '?', '!']);
                if plain && !found.contains(&name) {
                    found.push(name);
                }
            }
        } else if keyword.eq_ignore_ascii_case("include") {
            for pattern in words(values) {
                for file in expand(&pattern, ssh, home) {
                    read(&file, ssh, home, found, depth + 1);
                }
            }
        }
    }
}

/// A config line's keyword and what follows it, which ssh lets be joined by
/// whitespace, an `=`, or both.
fn split(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let (keyword, rest) = line.split_at(end);
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    Some((keyword, rest))
}

/// Space-separated values, a double-quoted one kept whole.
fn words(values: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = values.trim();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            words.push(quoted[..end].to_string());
            rest = quoted.get(end + 1..).unwrap_or("").trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            words.push(rest[..end].to_string());
            rest = rest[end..].trim_start();
        }
    }
    words
}

/// The files an `Include` names: `~/` from home, anything else relative to
/// `~/.ssh`, and a `*` or `?` in the file name matched against its
/// directory, in name order.
fn expand(pattern: &str, ssh: &Path, home: &Path) -> Vec<PathBuf> {
    let path = match pattern.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => ssh.join(pattern),
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|candidate| wildcard(name, candidate))
        })
        .collect();
    files.sort();
    files
}

/// Whether `text` matches `pattern`, where `*` is any run and `?` any one
/// character.
fn wildcard(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home directory of its own with an `.ssh` in it.
    struct Home(PathBuf);

    impl Home {
        fn new(name: &str) -> Home {
            let dir = std::env::temp_dir().join(format!("argus-ssh-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join(".ssh/config.d")).unwrap();
            Home(dir)
        }

        fn write(&self, relative: &str, text: &str) {
            std::fs::write(self.0.join(relative), text).unwrap();
        }

        fn hosts(&self) -> Vec<String> {
            let ssh = self.0.join(".ssh");
            let mut found = Vec::new();
            read(&ssh.join("config"), &ssh, &self.0, &mut found, 0);
            found
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn plain_hosts_are_offered_and_patterns_are_not() {
        let home = Home::new("plain");
        home.write(
            ".ssh/config",
            "# work\nHost devbox buildbox\n  User me\nhost=gpu\nHost *.corp !bastion web?\nMatch host x\nHost devbox\n",
        );
        assert_eq!(home.hosts(), ["devbox", "buildbox", "gpu"]);
    }

    #[test]
    fn included_files_are_read_where_ssh_would_find_them() {
        let home = Home::new("include");
        home.write(".ssh/config", "Include config.d/*.conf ~/extra\nHost main\n");
        home.write(".ssh/config.d/b.conf", "Host bravo\n");
        home.write(".ssh/config.d/a.conf", "Host alpha\n");
        home.write(".ssh/config.d/skip.txt", "Host skipped\n");
        home.write("extra", "Host \"quoted one\"\n");
        assert_eq!(home.hosts(), ["alpha", "bravo", "quoted one", "main"]);
    }

    #[test]
    fn a_config_that_includes_itself_stops() {
        let home = Home::new("loop");
        home.write(".ssh/config", "Include config\nHost looped\n");
        assert_eq!(home.hosts(), ["looped"]);
    }

    #[test]
    fn wildcards_match_the_way_ssh_globs_do() {
        assert!(wildcard("*.conf", "a.conf"));
        assert!(wildcard("h?st*", "host-1"));
        assert!(!wildcard("*.conf", "a.conf.bak"));
        assert!(wildcard("*", ""));
    }
}
