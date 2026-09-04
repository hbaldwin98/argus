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

/// Features, and which feature each checkout is working on.
///
/// A decision gains a `feature` so a board can be read one feature at a
/// time; rows written before this stay NULL, and are reported as unfiled
/// rather than dragged under a feature nobody chose.
///
/// `feature_scope` is keyed by checkout path, the same durable identity
/// notes use. It is what makes `decide` need no flag: the checkout an
/// agent is in answers which feature it is deciding under, and survives
/// the restart that throws pane ids away.
pub(super) const SCHEMA_V6: &str = r#"
CREATE TABLE feature (
    project         TEXT    NOT NULL,
    slug            TEXT    NOT NULL,
    title           TEXT    NOT NULL,
    body            TEXT    NOT NULL DEFAULT '',
    origin_checkout TEXT,
    origin_branch   TEXT,
    at              INTEGER NOT NULL,
    session         TEXT,
    PRIMARY KEY (project, slug)
) WITHOUT ROWID;

CREATE TABLE feature_scope (
    checkout_path TEXT PRIMARY KEY,
    project       TEXT NOT NULL,
    slug          TEXT NOT NULL
) WITHOUT ROWID;

ALTER TABLE decision ADD COLUMN feature TEXT;
CREATE INDEX decision_feature ON decision (project, feature, id);
"#;

/// The board state a feature is in, and every move between states.
///
/// The state is a column on `feature` rather than a table of its own: a
/// board is read whole on every change, and a column is what that read
/// costs nothing. The moves are their own table for the reason
/// `note_audit` is — a human looking at a column asks who put it there and
/// what they said, and the current state cannot answer that.
///
/// `claimed_by` holds a harness session, not a pane id, so a claim
/// outlives the restart that hands out fresh ids. `evidence` is what an
/// agent offers when it submits; it is deliberately one field and not a
/// history, because the history is `feature_event`.
///
/// No CHECK on `state`: SQLite cannot alter one afterwards, so a constraint
/// here would make adding a sixth column a table rebuild. The states are
/// enforced where they are decided, on the transition.
pub(super) const SCHEMA_V7: &str = r#"
ALTER TABLE feature ADD COLUMN state      TEXT NOT NULL DEFAULT 'proposed';
ALTER TABLE feature ADD COLUMN claimed_by TEXT;
ALTER TABLE feature ADD COLUMN claimed_at INTEGER;
ALTER TABLE feature ADD COLUMN blocker    TEXT;
ALTER TABLE feature ADD COLUMN evidence   TEXT;

CREATE TABLE feature_event (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    at      INTEGER NOT NULL,
    project TEXT    NOT NULL,
    slug    TEXT    NOT NULL,
    state   TEXT    NOT NULL,
    actor   TEXT    NOT NULL,
    session TEXT,
    detail  TEXT
);
CREATE INDEX feature_event_feature ON feature_event (project, slug, id);
"#;
