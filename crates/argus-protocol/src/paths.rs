//! Where Argus keeps its files. Shared so the daemon and the client agree
//! on it — and so `ARGUS_CONFIG_DIR` points both of them at the same place.

use std::path::PathBuf;

/// The named instance this process belongs to, when there is one.
///
/// A name scopes both the wire endpoint (see [`crate::transport`]) and a
/// slice of the config directory, which is what lets a worktree build run
/// beside an installed one: neither touches the other's pipe, projects, or
/// session state.
pub fn instance_name() -> Option<String> {
    parse_instance(std::env::var("ARGUS_INSTANCE").ok())
}

fn parse_instance(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// `ARGUS_CONFIG_DIR` overrides the platform location. Tests need it, so
/// they never read or scribble on the real user's config, and it makes a
/// throwaway instance alongside a real one possible.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARGUS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let base = directories::ProjectDirs::from("", "", "argus")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // An instance gets its own slice rather than sharing the installed
    // install's directory: a dev daemon that could read the real one's
    // projects would eventually write to them too.
    match instance_name() {
        Some(name) => base.join("instances").join(name),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_name_is_trimmed_and_may_not_be_empty() {
        // A script exporting an empty variable should mean "no instance",
        // not an endpoint named after nothing.
        assert_eq!(parse_instance(None), None);
        assert_eq!(parse_instance(Some("   ".to_string())), None);
        assert_eq!(
            parse_instance(Some(" dev ".to_string())),
            Some("dev".to_string())
        );
    }
}
