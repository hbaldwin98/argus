//! Which release is the newest, and which of its archives this machine
//! runs: read from GitHub's answer for the latest release, and named the
//! way `.github/workflows/release.yml` names them.

use serde::Deserialize;

/// Where the releases are published, from the workspace's `repository`.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A version as releases are numbered, `MAJOR.MINOR.PATCH`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// `0.38.1` or `v0.38.1`; nothing else, so a tag that is not a
    /// release's is not mistaken for one.
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        let mut part = || parts.next()?.parse::<u64>().ok();
        let version = Version(part()?, part()?, part()?);
        parts.next().is_none().then_some(version)
    }

    pub fn this_build() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("the workspace version is MAJOR.MINOR.PATCH")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The API address of the latest release: the newest that is neither a
/// draft nor a prerelease.
pub fn latest_url() -> String {
    let path = REPOSITORY.trim_start_matches("https://github.com/");
    format!("https://api.github.com/repos/{path}/releases/latest")
}

/// The latest release's version, from the API's answer.
pub fn latest_version(answer: &str) -> anyhow::Result<Version> {
    #[derive(Deserialize)]
    struct Latest {
        tag_name: String,
    }
    let latest: Latest = serde_json::from_str(answer)?;
    Version::parse(&latest.tag_name)
        .ok_or_else(|| anyhow::anyhow!("the latest release is tagged {:?}, not as a version", latest.tag_name))
}

/// The release target this build is, among those a release is built for;
/// `None` on any other, which has no archive to update from.
pub fn this_target() -> Option<&'static str> {
    target_of(std::env::consts::OS, std::env::consts::ARCH)
}

fn target_of(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

/// The archive `version` was packaged in for `target`, which is also the
/// folder inside it that holds the executables.
pub fn package(version: Version, target: &str) -> String {
    format!("argus-{version}-{target}")
}

/// The archive's file name: a zip on Windows, where the release packs
/// with `Compress-Archive`, a gzipped tar everywhere else.
pub fn archive_name(version: Version, target: &str) -> String {
    let extension = if target.contains("windows") { "zip" } else { "tar.gz" };
    format!("{}.{extension}", package(version, target))
}

pub fn download_url(version: Version, file: &str) -> String {
    format!("{REPOSITORY}/releases/download/v{version}/{file}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_three_numbers_with_or_without_its_tag_s_v() {
        assert_eq!(Version::parse("v0.38.1"), Some(Version(0, 38, 1)));
        assert_eq!(Version::parse("1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("v1.2"), None);
        assert_eq!(Version::parse("v1.2.3.4"), None);
        assert_eq!(Version::parse("v1.2.3-rc1"), None);
        assert_eq!(Version::parse("nightly"), None);
    }

    #[test]
    fn versions_order_by_number_not_by_text() {
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert!(Version(1, 0, 0) > Version(0, 38, 1));
    }

    #[test]
    fn this_build_has_a_version() {
        assert_eq!(Version::this_build().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_latest_release_is_read_from_its_tag() {
        let answer = r#"{"tag_name":"v0.39.0","name":"Argus 0.39.0","assets":[]}"#;
        assert_eq!(latest_version(answer).unwrap(), Version(0, 39, 0));
        assert!(latest_version(r#"{"tag_name":"nightly"}"#).is_err());
        assert!(latest_version("not json").is_err());
    }

    #[test]
    fn archives_are_named_as_the_release_workflow_names_them() {
        // `.github/workflows/release.yml`: `argus-$version-$target`, a zip on
        // Windows and a .tar.gz elsewhere.
        let version = Version(0, 38, 1);
        assert_eq!(archive_name(version, "x86_64-pc-windows-msvc"), "argus-0.38.1-x86_64-pc-windows-msvc.zip");
        assert_eq!(archive_name(version, "aarch64-apple-darwin"), "argus-0.38.1-aarch64-apple-darwin.tar.gz");
        assert_eq!(
            download_url(version, "argus-0.38.1-aarch64-apple-darwin.tar.gz"),
            "https://github.com/hbaldwin98/argus/releases/download/v0.38.1/argus-0.38.1-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(latest_url(), "https://api.github.com/repos/hbaldwin98/argus/releases/latest");
    }

    #[test]
    fn only_the_five_released_targets_have_an_archive() {
        assert_eq!(target_of("linux", "aarch64"), Some("aarch64-unknown-linux-gnu"));
        assert_eq!(target_of("windows", "x86_64"), Some("x86_64-pc-windows-msvc"));
        assert_eq!(target_of("windows", "aarch64"), None);
        assert_eq!(target_of("freebsd", "x86_64"), None);
    }
}
