//! Everything the client, the daemon, and `argus-hook` have to agree on.
//!
//! Three binaries share no types unless they live here, so anything
//! written on both sides of a boundary belongs in this crate: the two
//! message enums and their framing, the tree a client renders, the pane
//! API's URL grammar and environment, and the shapes review and board data
//! travel in. Written twice they drift in silence; written once they
//! cannot.

pub mod adr;
pub mod agent_launch;
pub mod agent_view;
pub mod view_check;
pub mod view_links;
pub mod view_patch;
pub mod view_press;
pub mod view_requests;
pub mod view_tree;
pub mod ago;
pub mod artifacts;
pub mod awaited;
pub mod board_follower;
pub mod capture;
pub mod checkout_path;
pub mod code;
pub mod cell;
pub mod column;
pub mod column_verbs;
pub mod damage;
pub mod decisions;
pub mod diagrams;
pub mod extension_guide;
pub mod extension_keys;
pub mod ext_console;
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
pub mod reconcile;
pub mod review;
pub mod review_session;
pub mod review_view;
pub mod saved_view;
pub mod sessions;
pub mod ssh_bridge;
pub mod ssh_hosts;
pub mod standing;
pub mod tasks;
pub mod tracker_links;
pub mod transcript;
pub mod transport;
pub mod tree;

pub use ago::ago;
pub use agent_launch::{AgentLaunch, AgentLaunched, LaunchPlace, MAX_LAUNCH_BYTES};
pub use agent_view::{
    AgentView, Block, BoardCard, BoardColumn, ChartShape, CheckItem, Chip, Column, Field, FormField, FormInput,
    GraphDirection, GraphEdge, GraphNode, GridMin, Item, LogLevel, LogLine, MatrixCell, RowAlign, RowItem,
    RowSelect, Series, ShownView, Stat, Tab, TimelineEvent, Tone, TreeNode, Value, ViewChange,
};
pub use checkout_path::is_checkout_relative;
pub use view_check::{
    is_web_url, MAX_BLOCK_DEPTH, MAX_CHART_POINTS, MAX_CHART_SERIES, MAX_FORM_FIELDS, MAX_GRAPH_EDGES,
    MAX_GRAPH_NODES, MAX_LOG_LINES, MAX_TABLE_ROWS, MAX_TREE_DEPTH, MAX_TREE_NODES, MAX_TREND_POINTS,
    MAX_VIEWS_PER_PANE, MAX_VIEW_ACTIONS, MAX_VIEW_BYTES, MAX_VIEW_ID_BYTES, MAX_VIEW_NUMBER,
    MAX_VIEW_TITLE_BYTES, MIN_REFRESH_SECS,
};
pub use view_press::{Answer, AnswerValue, NotOffered, ViewAction, ViewPress, MAX_ANSWER_BYTES};
pub use saved_view::{
    FeatureRef, SavedView, SavedViewChange, SavedViewEntry, SavedViewRecord, SavedViewWrite, ViewScope,
    ViewSource, WriteScope, MAX_ATTACHED_FEATURES, MAX_EMBED_DEPTH, MAX_RECIPE_BYTES, MAX_SAVED_VIEWS,
    MAX_SAVED_VIEW_WRITE_BYTES, MAX_SOURCE_BYTES, MAX_VIEW_SOURCES,
};
pub use keys::KeyboardMode;
pub use view_tree::Slot;
pub use view_links::linked_views;
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
pub use ext_console::{ConsoleEnd, ExtConsole, MAX_CONSOLE_ARGS, MAX_CONSOLE_ARG_BYTES, MAX_CONSOLE_ENV};
pub use extensions::{
    CommandScope, ExtAbout, ExtAction, ExtAnswer, ExtPlace, ExtCommand, ExtDescription, ExtDialog, ExtDialogKind,
    ExtDialogRaised, ExtEvent, ExtField, ExtItem, ExtKey, ExtPane, ExtPaneOpened, ExtRow, ExtScope, ExtView,
    Refusals, Refused,
};
pub use focus::{Focus, FocusCheckout, FocusPane, FocusProject, FocusRepository, View};
pub use framing::{read_known_msg, read_msg, write_frame, write_msg, FramingError};
pub use held_tree::{transitions, HeldTree, Lineage, Transition};
pub use hello::{
    ADD_PROJECT, AGENT_VIEWS, VIEW_ACTIONS, Greeting, Hello, Refusal, CAPABILITIES, CELL_RUNS, FOCUS, GREETING_WAIT, PASTE_FILE,
    PANE_TELEMETRY, PROTOCOL, SESSIONS, CONSOLES, EXTENSIONS, EXTENSION_DIALOGS, DESCRIBE_EXTENSIONS,
    RENAME_PANE, SAVED_VIEWS, CODE,
};
pub use code::{
    CodeChange, CodeFile, CodeLine, CodeMatch, CodeMatches, CodeSymbol, CodeTree, CodeVersion, Removed, SearchOptions,
    MAX_CODE_MATCHES,
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
    LineKind, Review, ReviewAnchor, ReviewBase, ReviewComment, ReviewContext, MAX_HISTORY_COMMITS,
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
    MAX_LINK_KEY_BYTES, MAX_LINK_ROLE_BYTES, MAX_LINK_TRACKER_BYTES, MAX_LINK_URL_BYTES,
    MAX_TRACKER_MAP_BYTES,
};
pub use reconcile::{
    Changes, Failed, Noticed, ProposalWrite, ProposedLine, Reconcile, ReconcileAction, ReconcileChange,
    Reconciled, Seen, Ticked,
};
pub use column::CheckoutRow;
pub use column_verbs::Verb;
pub use tree::{
    AgentTelemetry, CheckoutInfo, ChildAgentInfo, GitStatus, PaneInfo, PaneKind, PaneState, PaneStatus,
    ProjectInfo, RepositoryInfo, WorkspaceInfo,
};
