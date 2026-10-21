//! Everything the client, the daemon, and `argus-hook` have to agree on.
//!
//! Three binaries share no types unless they live here, so anything
//! written on both sides of a boundary belongs in this crate: the two
//! message enums and their framing, the tree a client renders, the pane
//! API's URL grammar and environment, and the shapes review and board data
//! travel in. Written twice they drift in silence; written once they
//! cannot.

pub mod agents;
pub mod ago;
pub mod board;
pub mod browser;
pub mod checkout_path;
pub mod code;
pub mod connection;
pub mod extensions;
pub mod files;
pub mod fuzzy;
pub mod hook;
pub mod panes;
pub mod paths;
pub mod review;
pub mod tree;
pub mod views;

pub use ago::ago;
pub use browser::{navigable, BrowserFrame, BrowserInput, BrowserKey, BrowserPage, BrowserRequest, BrowserTab, BROWSER_VAR};
pub use agents::launch::{AgentLaunch, AgentLaunched, LaunchPlace, MAX_LAUNCH_BYTES};
pub use agents::roster::{roster, AgentListing};
pub use views::agent_view::{
    AgentView, Block, BoardCard, BoardColumn, ChartShape, CheckItem, Chip, Column, Field, FormField, FormInput,
    GraphDirection, GraphEdge, GraphNode, GridMin, Item, LogLevel, LogLine, MatrixCell, RowAlign, RowItem,
    RowSelect, Series, ShownView, Stat, Tab, TimelineEvent, Tone, TreeNode, Value, ViewChange,
};
pub use checkout_path::is_checkout_relative;
pub use views::check::{
    is_web_url, MAX_BLOCK_DEPTH, MAX_CHART_POINTS, MAX_CHART_SERIES, MAX_FORM_FIELDS, MAX_GRAPH_EDGES,
    MAX_GRAPH_NODES, MAX_LOG_LINES, MAX_TABLE_ROWS, MAX_TREE_DEPTH, MAX_TREE_NODES, MAX_TREND_POINTS,
    MAX_VIEWS_PER_PANE, MAX_VIEW_ACTIONS, MAX_VIEW_BYTES, MAX_VIEW_ID_BYTES, MAX_VIEW_NUMBER,
    MAX_VIEW_TITLE_BYTES, MIN_REFRESH_SECS,
};
pub use views::press::{Answer, AnswerValue, NotOffered, ViewAction, ViewPress, MAX_ANSWER_BYTES};
pub use views::saved::{
    FeatureRef, SavedView, SavedViewChange, SavedViewEntry, SavedViewRecord, SavedViewWrite, ViewScope,
    ViewSource, WriteScope, MAX_ATTACHED_FEATURES, MAX_EMBED_DEPTH, MAX_RECIPE_BYTES, MAX_SAVED_VIEWS,
    MAX_SAVED_VIEW_WRITE_BYTES, MAX_SOURCE_BYTES, MAX_VIEW_SOURCES,
};
pub use panes::keys::KeyboardMode;
pub use views::tree::Slot;
pub use views::links::linked_views;
pub use board::artifacts::ArtifactScope;
pub use board::follower::{BoardFollower, BoardScope, FeatureListing, Folding, Received};
pub use panes::cell::{
    Cell, CellSpan, Color, Cursor, CursorShape, MouseEncoding, MouseMode, MouseTracking,
    BLANK,
};
pub use compact_str::{CompactString, ToCompactString};
pub use panes::damage::{damage, grid_from_runs, grid_runs, CellRun, Scroll, Style};
pub use board::capture::{Capture, CapturedDecision, CapturedTask, MAX_CAPTURE_ROWS};
pub use board::decisions::{
    outline_number, Consequence, Decision, DecisionBoard, DecisionChange, DecisionState,
    DecisionText, DecisionTreeRow, DecisionWrite, MAX_DECISION_BYTES, MAX_DECISION_CONSEQUENCES,
    MAX_DECISION_OPTIONS,
};
pub use board::diagrams::{
    DiagramAction, DiagramList, DiagramWrite, SequenceDiagram, MAX_DIAGRAM_BODY_BYTES,
    MAX_DIAGRAM_TITLE_BYTES,
};
pub use board::features::{
    is_slug, numbered_slug, slugify, Actor, Feature, FeatureAction, FeatureBoard, FeatureEvent, FeatureMove, FeatureState,
    FeatureWrite, MAX_FEATURE_BODY_BYTES, MAX_FEATURE_TITLE_BYTES, MAX_HOLD_BYTES,
};
pub use extensions::console::{ConsoleEnd, ExtConsole, MAX_CONSOLE_ARGS, MAX_CONSOLE_ARG_BYTES, MAX_CONSOLE_ENV};
pub use extensions::{
    CommandScope, ExtAbout, ExtAction, ExtAnswer, ExtPlace, ExtCommand, ExtDescription, ExtDialog, ExtDialogKind,
    ExtDialogRaised, ExtEvent, ExtField, ExtItem, ExtKey, ExtPane, ExtPaneOpened, ExtRow, ExtScope, ExtTaken, ExtView,
    Refusals, Refused,
};
pub use tree::focus::{Focus, FocusCheckout, FocusPane, FocusProject, FocusRepository, View};
pub use connection::framing::{read_known_msg, read_msg, write_frame, write_msg, FramingError};
pub use tree::held::{transitions, HeldTree, Lineage, Transition};
pub use connection::hello::{
    ADD_PROJECT, AGENT_VIEWS, BROWSERS, VIEW_ACTIONS, Greeting, Hello, Refusal, CAPABILITIES, CELL_RUNS, FOCUS, GREETING_WAIT, PASTE_FILE,
    PANE_TELEMETRY, PROTOCOL, SESSIONS, CONSOLES, EXTENSIONS, EXTENSION_DIALOGS, DESCRIBE_EXTENSIONS,
    RENAME_PANE, SAVED_VIEWS, CODE, CODE_EDIT, FILES, REQUEST_REFUSALS,
};
pub use connection::request_refusal::RequestRefusal;
pub use code::{
    CodeChange, CodeFile, CodeLine, CodeMatch, CodeMatches, CodeSymbol, CodeTree, CodeVersion, Removed, SearchOptions,
    MAX_CODE_MATCHES,
};
pub use code::edit::{EditChanged, EditOpened, TextChange};
pub use files::{CheckoutDir, CheckoutEntry, EntryGit, MAX_DIR_ENTRIES};
pub use hook::{
    endpoint_url, extension_path, extension_prefix, extension_url, parse_extension_path, feature_url, pane_path, pane_url, parse_pane_path, parse_pane_url,
    parse_request_target, requested_feature, requested_scope, Endpoint, ExtEndpoint, PaneUrl, EXTENSION_VAR, MAX_HOOK_BODY, Report, ARTIFACT_SCOPE_VAR, CONTEXT_COMMAND, HELPER_VAR,
    BACKGROUND_HEADER, DIRECTORY_HEADER, INSTRUCTIONS_COMMAND, INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, PANE_VAR, SESSION_HEADER,
    SESSION_KEY_FLAG, TITLE_FLAG, TOKEN_VAR, URL_VAR,
};
pub use tree::ids::{CheckoutId, IdGen, PaneId, ProjectId, RepositoryId, WorkspaceId};
pub use hook::inbox::InboxItem;
pub use agents::memory::{
    select_context_packet, Artifact, ArtifactContent, ArtifactKind, ArtifactState, ContextPacket,
    ContextQuery, ForwardedMaterial, InclusionReason, MemorySnapshot, MemorySource, PacketEntry,
    PacketItem, PacketLimits, Provenance, WorkContext, DEFAULT_PACKET_MAX_BYTES,
    DEFAULT_PACKET_MAX_ITEMS, MAX_ARTIFACT_TEXT_BYTES, MAX_WORK_CONTEXT_BRIEF_BYTES,
    MAX_WORK_CONTEXT_TITLE_BYTES,
};
pub use connection::message::{ClientMsg, Created, DirEntry, DirListing, ProjectPreview, ServerMsg};
pub use paths::{config_dir, instance_name};
pub use review::{
    ChangeKind, CommitFile, CommitInfo, DiffLine, FileDiff, HighlightKind, HighlightSpan, Hunk,
    LineKind, Review, ReviewAnchor, ReviewBase, ReviewComment, ReviewContext, MAX_HISTORY_COMMITS,
    MAX_REVIEW_COMMENTS, MAX_REVIEW_COMMENT_BYTES,
};
pub use board::tasks::{
    checked_task_body, checked_task_line, Criterion, Task, TaskAction, TaskCounts, TaskList,
    TaskNote, TaskPlace, TaskState, TaskTreeRow, TaskWrite, MAX_CRITERION_BYTES,
    MAX_TASK_BODY_BYTES, MAX_TASK_CRITERIA, MAX_TASK_NOTES, MAX_TASK_NOTE_BYTES,
    MAX_TASK_TITLE_BYTES,
};
pub use board::tracker_links::{
    checked_map, LinkWrite, MapChange, MapDraft, TrackerLink, TrackerMap, MAX_FEATURE_LINKS,
    MAX_LINK_KEY_BYTES, MAX_LINK_ROLE_BYTES, MAX_LINK_TRACKER_BYTES, MAX_LINK_URL_BYTES,
    MAX_TRACKER_MAP_BYTES,
};
pub use board::reconcile::{
    Changes, Failed, Noticed, ProposalWrite, ProposedLine, Reconcile, ReconcileAction, ReconcileChange,
    Reconciled, Seen, Ticked,
};
pub use tree::column::CheckoutRow;
pub use tree::column::verbs::Verb;
pub use tree::{
    AgentTelemetry, CheckoutInfo, ChildAgentInfo, GitStatus, PaneInfo, PaneKind, PaneState, PaneStatus,
    ProjectInfo, RepositoryInfo, WorkspaceInfo,
};
