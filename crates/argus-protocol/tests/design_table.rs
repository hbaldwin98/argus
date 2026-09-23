//! Whether `DESIGN.md`'s responsibility table still names every module.
//!
//! The table is how a reader finds where a question is answered, and it
//! drifted by two dozen rows before anything noticed. It lives here only
//! because the workspace has no root package; it checks all three crates.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CRATES: [&str; 3] = ["argus-protocol", "argusd", "argus-client"];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The backticked names in each table's first column, one set per table in
/// the order the tables appear.
fn listed_modules() -> Vec<BTreeSet<String>> {
    let design = std::fs::read_to_string(workspace().join("DESIGN.md")).unwrap();
    let section = design
        .split("## Where each responsibility lives")
        .nth(1)
        .expect("DESIGN.md has the responsibility section");
    let section = section.split("\n## ").next().unwrap();

    let mut tables: Vec<BTreeSet<String>> = Vec::new();
    for line in section.lines() {
        if line.starts_with("| module |") {
            tables.push(BTreeSet::new());
            continue;
        }
        let (Some(table), Some(first)) = (tables.last_mut(), line.split('|').nth(1)) else {
            continue;
        };
        table.extend(first.split('`').skip(1).step_by(2).map(str::to_owned));
    }
    tables
}

/// Every module under `src`, named the way the table names it: a path
/// without `.rs`, `mod.rs` standing for its directory. Test modules, crate
/// roots and extra binaries are not responsibilities of their own.
fn source_modules(src: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, src: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" && name != "bin" {
                    walk(&path, src, out);
                }
                continue;
            }
            if !name.ends_with(".rs") || name == "tests.rs" || name == "lib.rs" {
                continue;
            }
            let rel = path.strip_prefix(src).unwrap().with_extension("");
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.insert(rel.strip_suffix("/mod").unwrap_or(&rel).to_owned());
        }
    }
    let mut out = BTreeSet::new();
    walk(src, src, &mut out);
    out
}

#[test]
fn every_module_has_a_row_in_the_responsibility_table() {
    let tables = listed_modules();
    assert_eq!(tables.len(), CRATES.len(), "one table per crate");

    for (krate, listed) in CRATES.iter().zip(&tables) {
        let present = source_modules(&workspace().join("crates").join(krate).join("src"));
        let missing: Vec<_> = present.difference(listed).collect();
        let stale: Vec<_> = listed.difference(&present).collect();
        assert!(
            missing.is_empty() && stale.is_empty(),
            "{krate}: missing from DESIGN.md {missing:?}, listed but gone {stale:?}"
        );
    }
}
