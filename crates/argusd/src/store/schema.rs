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

/// Every note change an agent made, in the order it made them.
///
/// The audit is the other half of allowing the write at all: a human
/// reading a note that has grown three checkboxes needs to be able to ask
/// which agent added them and when. Keyed by the same durable note
/// identity as `note`, and holding the harness session rather than the
/// pane id, since a pane id means nothing after a restart.
///
/// Kept even after the note it describes is gone: what an agent claimed to
/// have done outlives the checkout it did it in, and a deleted note is
/// exactly when the record matters most.
pub(super) const SCHEMA_V4: &str = r#"
CREATE TABLE note_audit (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    at      INTEGER NOT NULL,
    scope   TEXT NOT NULL,
    key     TEXT NOT NULL,
    session TEXT,
    action  TEXT NOT NULL,
    detail  TEXT NOT NULL
);
CREATE INDEX note_audit_note ON note_audit (scope, key, id);
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

/// The decision board, one row per decision, keyed by project name for the
/// same reason notes are: ids do not survive a restart.
///
/// `parent` is a self-reference rather than a foreign key. A board is read
/// whole and reassembled in the client, which already has to survive a
/// parent it cannot see — a decision recorded against a project that was
/// renamed, say — so a constraint here would refuse writes to protect an
/// invariant the reader does not rely on.
///
/// Append-only by design. A decision that turns out to be wrong gets a new
/// row, and the old row's `superseded_by` is what says so; nothing but
/// that column is ever updated.
pub(super) const SCHEMA_V5: &str = r#"
CREATE TABLE decision (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project       TEXT    NOT NULL,
    parent        INTEGER,
    at            INTEGER NOT NULL,
    session       TEXT,
    checkout      TEXT,
    chose         TEXT    NOT NULL,
    over_         TEXT,
    because       TEXT,
    superseded_by INTEGER
);
CREATE INDEX decision_project ON decision (project, id);
"#;
