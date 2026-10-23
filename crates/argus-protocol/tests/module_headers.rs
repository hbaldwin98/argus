//! Whether every module opens with a `//!` header naming its responsibility.
//!
//! The header is how a reader finds where a question is answered, so a module
//! without one is a question nobody can find. It replaced a table of every
//! module in a design document, which said the same thing a second time and
//! drifted. It lives here only because the workspace has no root package; it
//! checks every crate in `CRATES`.

use std::path::{Path, PathBuf};

const CRATES: [&str; 6] = ["argus-protocol", "argusd", "argus-client", "argus-desktop-core", "argus-desktop", "argus-gpui"];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every module under `src` but tests, which answer to the module they test.
fn modules(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" {
                modules(&path, out);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" {
            out.push(path);
        }
    }
}

#[test]
fn every_module_opens_with_its_responsibility() {
    for krate in CRATES {
        let mut found = Vec::new();
        modules(&workspace().join("crates").join(krate).join("src"), &mut found);
        assert!(!found.is_empty(), "{krate} has modules");
        let missing: Vec<_> = found
            .iter()
            .filter(|path| !std::fs::read_to_string(path).unwrap().starts_with("//!"))
            .map(|path| path.strip_prefix(workspace()).unwrap_or(path).display().to_string())
            .collect();
        assert!(missing.is_empty(), "{krate}: modules without a //! header {missing:?}");
    }
}
