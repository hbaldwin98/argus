//! Everything the client, the daemon, and `argus-hook` have to agree on.
//!
//! Three binaries share no types unless they live here, so anything
//! written on both sides of a boundary belongs in this crate: the two
//! message enums and their framing, the tree a client renders, the pane
//! API's URL grammar and environment, and the shapes review and board data
//! travel in. Written twice they drift in silence; written once they
//! cannot.

pub mod adr;
pub mod agent_view;
pub mod ago;
pub mod artifacts;
pub mod awaited;
pub mod board_follower;
pub mod capture;
pub mod cell;
pub mod column;
pub mod column_verbs;
pub mod damage;
pub mod decisions;
pub mod diagrams;
pub mod extension_keys;
pub mod extensions;
pub mod features;
pub mod flow;
pub mod focus;
pub mod framing;
pub mod fuzzy;
pub mod held_tree;
pub mod history_view;
pub mod hello;
pub mod hook;
pub mod ids;
pub mod inbox;
pub mod keys;
pub mod memory;
pub mod message;
pub mod mouse;
pub mod outline;
pub mod pane_view;
pub mod paste;
pub mod paths;
pub mod review;
pub mod review_session;
pub mod review_view;
pub mod sessions;
pub mod standing;
pub mod tasks;
pub mod tracker_links;
pub mod transcript;
pub mod transport;
pub mod tree;

pub use ago::ago;
pub use agent_view::{
    AgentView, Block, Chip, Column, Field, Item, ShownView, Stat, Tab, Tone, Value, ViewChange, MAX_BLOCK_DEPTH,
    MAX_TABLE_ROWS, MAX_VIEWS_PER_PANE, MAX_VIEW_BYTES, MAX_VIEW_ID_BYTES, MAX_VIEW_TITLE_BYTES,
};
pub use artifacts::ArtifactScope;
pub use board_follower::{BoardFollower, BoardScope, Received};
pub use cell::{
    Cell, CellSpan, Color, Cursor, CursorShape, MouseEncoding, MouseMode, MouseTracking,
    BLANK,
};
pub use compact_str::{CompactString, ToCompactString};
pub use damage::{damage, grid_from_runs, grid_runs, CellRun, Scroll, Style};
pub use capture::{Capture, CapturedDecision, CapturedTask, MAX_CAPTURE_ROWS};
pub use decisions::{
    outline_number, Consequence, Decision, DecisionBoard, DecisionChange, DecisionState,
    DecisionText, DecisionTreeRow, DecisionWrite, MAX_DECISION_BYTES, MAX_DECISION_CONSEQUENCES,
    MAX_DECISION_OPTIONS,
};
pub use diagrams::{
    DiagramAction, DiagramList, DiagramWrite, SequenceDiagram, MAX_DIAGRAM_BODY_BYTES,
    MAX_DIAGRAM_TITLE_BYTES,
};
pub use features::{
    is_slug, slugify, Actor, Feature, FeatureAction, FeatureBoard, FeatureEvent, FeatureMove, FeatureState,
    FeatureWrite, MAX_FEATURE_BODY_BYTES, MAX_FEATURE_TITLE_BYTES, MAX_HOLD_BYTES,
};
pub use extensions::{
    CommandScope, ExtAction, ExtAnswer, ExtCommand, ExtDialog, ExtDialogKind, ExtEvent, ExtField, ExtItem,
    ExtKey, ExtPane, ExtRow, ExtScope, ExtView, Refusals, Refused,
};
pub use focus::{Focus, FocusCheckout, FocusPane, FocusProject, FocusRepository, View};
pub use framing::{read_known_msg, read_msg, write_frame, write_msg, FramingError};
pub use held_tree::{transitions, HeldTree, Transition};
pub use hello::{
    ADD_PROJECT, AGENT_VIEWS, Greeting, Hello, Refusal, CAPABILITIES, CELL_RUNS, FOCUS, GREETING_WAIT, PASTE_FILE,
    PANE_TELEMETRY, PROTOCOL, SESSIONS, CONSOLES, EXTENSIONS, EXTENSION_DIALOGS,
};
pub use hook::{
    endpoint_url, extension_path, extension_prefix, extension_url, parse_extension_path, feature_url, pane_path, pane_url, parse_pane_path, parse_pane_url,
    parse_request_target, requested_feature, requested_scope, Endpoint, ExtEndpoint, PaneUrl, EXTENSION_VAR, Report, ARTIFACT_SCOPE_VAR, CONTEXT_COMMAND, HELPER_VAR,
    BACKGROUND_HEADER, DIRECTORY_HEADER, INSTRUCTIONS_COMMAND, INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, PANE_VAR, SESSION_HEADER,
    SESSION_KEY_FLAG, TITLE_FLAG, TOKEN_VAR, URL_VAR,
};
pub use ids::{CheckoutId, IdGen, PaneId, ProjectId, RepositoryId, WorkspaceId};
pub use inbox::InboxItem;
pub use memory::{
    select_context_packet, Artifact, ArtifactContent, ArtifactKind, ArtifactState, ContextPacket,
    ContextQuery, ForwardedMaterial, InclusionReason, MemorySnapshot, MemorySource, PacketEntry,
    PacketItem, PacketLimits, Provenance, WorkContext, DEFAULT_PACKET_MAX_BYTES,
    DEFAULT_PACKET_MAX_ITEMS, MAX_ARTIFACT_TEXT_BYTES, MAX_WORK_CONTEXT_BRIEF_BYTES,
    MAX_WORK_CONTEXT_TITLE_BYTES,
};
pub use message::{ClientMsg, Created, DirEntry, DirListing, ProjectPreview, ServerMsg};
pub use paths::{config_dir, instance_name};
pub use review::{
    ChangeKind, CommitFile, CommitInfo, DiffLine, FileDiff, HighlightKind, HighlightSpan, Hunk,
    LineKind, Review, ReviewAnchor, ReviewBase, ReviewComment, MAX_HISTORY_COMMITS,
    MAX_REVIEW_COMMENTS, MAX_REVIEW_COMMENT_BYTES,
};
pub use tasks::{
    checked_task_body, checked_task_line, Criterion, Task, TaskAction, TaskCounts, TaskList,
    TaskNote, TaskPlace, TaskState, TaskTreeRow, TaskWrite, MAX_CRITERION_BYTES,
    MAX_TASK_BODY_BYTES, MAX_TASK_CRITERIA, MAX_TASK_NOTES, MAX_TASK_NOTE_BYTES,
    MAX_TASK_TITLE_BYTES,
};
pub use tracker_links::{
    checked_map, LinkWrite, MapChange, MapDraft, TrackerLink, TrackerMap, MAX_FEATURE_LINKS,
    MAX_LINK_KEY_BYTES, MAX_LINK_TRACKER_BYTES, MAX_LINK_URL_BYTES, MAX_TRACKER_MAP_BYTES,
};
pub use column::CheckoutRow;
pub use column_verbs::Verb;
pub use tree::{
    AgentTelemetry, CheckoutInfo, ChildAgentInfo, GitStatus, PaneInfo, PaneKind, PaneState, PaneStatus,
    ProjectInfo, RepositoryInfo, WorkspaceInfo,
};
