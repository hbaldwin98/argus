//! The tables `runtime.db` holds, one constant per migration step.
//!
//! A step is applied once and never edited afterwards: the version number
//! in the database says which have run, so editing an old constant would
//! give two installs different schemas under the same version.

pub(super) const SCHEMA_V1: &str = r#"
CREATE TABLE pane (
    seq                INTEGER PRIMARY KEY,
    checkout_path      TEXT NOT NULL,
    kind               TEXT NOT NULL,
    title              TEXT NOT NULL,
    template           TEXT,
    status             TEXT NOT NULL,
    note               TEXT,
    harness            TEXT,
    harness_session_id TEXT
);

-- Keyed on the root rather than the name: what the user added is a
-- directory, and two directories that happen to share a basename are two
-- projects. The name is what they are called, not what they are.
CREATE TABLE project_overlay (
    root      TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    workspace TEXT NOT NULL
);

CREATE TABLE project_hidden (
    name TEXT PRIMARY KEY
);

CREATE TABLE repo_overlay (
    project TEXT NOT NULL,
    path    TEXT NOT NULL,
    PRIMARY KEY (project, path)
);

CREATE TABLE repo_excluded (
    path TEXT PRIMARY KEY
);

CREATE TABLE workspace_overlay (
    name TEXT PRIMARY KEY
);

CREATE TABLE ui_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// Notes are keyed by durable identity, not by id: ids are handed out
/// fresh every start, and a note has to survive being written today and
/// read next week. A project is its name and a checkout is its path, the
/// same keys the pane and overlay tables already use.
pub(super) const SCHEMA_V3: &str = r#"
CREATE TABLE note (
    scope TEXT NOT NULL,
    key   TEXT NOT NULL,
    body  TEXT NOT NULL,
    PRIMARY KEY (scope, key)
) WITHOUT ROWID;
"#;

pub(super) const SCHEMA_V2: &str = r#"
CREATE TABLE review_comment (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    checkout_path TEXT NOT NULL,
    anchor        TEXT NOT NULL,
    body          TEXT NOT NULL
);
CREATE INDEX review_comment_checkout ON review_comment (checkout_path, id);
"#;
