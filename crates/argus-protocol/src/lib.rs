//! Everything the client, the daemon, and `argus-hook` have to agree on.
//!
//! Three binaries share no types unless they live here, so anything
//! written on both sides of a boundary belongs in this crate: the two
//! message enums and their framing, the tree a client renders, the pane
//! API's URL grammar and environment, and the shapes review and board data
//! travel in. Written twice they drift in silence; written once they
//! cannot.

pub mod artifacts;
pub mod cell;
pub mod decisions;
pub mod features;
pub mod framing;
pub mod hook;
pub mod ids;
pub mod memory;
pub mod message;
pub mod paths;
pub mod review;
pub mod tasks;
pub mod transport;
pub mod tree;

pub use artifacts::ArtifactScope;
pub use cell::{
    diff_grid, Cell, CellSpan, Color, Cursor, CursorShape, MouseEncoding, MouseMode, MouseTracking,
    BLANK,
};
pub use compact_str::{CompactString, ToCompactString};
pub use decisions::{Decision, DecisionBoard, DecisionTreeRow, DecisionWrite, MAX_DECISION_BYTES};
pub use features::{
    slugify, Actor, Feature, FeatureAction, FeatureBoard, FeatureEvent, FeatureMove, FeatureState,
    FeatureWrite, MAX_FEATURE_BODY_BYTES, MAX_FEATURE_TITLE_BYTES,
};
pub use framing::{read_msg, write_frame, write_msg, FramingError};
pub use hook::{
    pane_path, parse_pane_path, Endpoint, Report, HELPER_VAR, INSTRUCTIONS_COMMAND,
    INSTRUCTIONS_VAR, NOTE_FLAG, OWNS_SESSION_FLAG, PANE_VAR, SESSION_HEADER, SESSION_KEY_FLAG,
    TITLE_FLAG, TOKEN_VAR, URL_VAR,
};
pub use ids::{CheckoutId, IdGen, PaneId, ProjectId, RepositoryId, WorkspaceId};
pub use memory::{
    select_context_packet, Artifact, ArtifactContent, ArtifactKind, ArtifactState, ContextPacket,
    ContextQuery, ForwardedMaterial, InclusionReason, MemorySnapshot, MemorySource, PacketEntry,
    PacketItem, PacketLimits, Provenance, WorkContext, DEFAULT_PACKET_MAX_BYTES,
    DEFAULT_PACKET_MAX_ITEMS, MAX_ARTIFACT_TEXT_BYTES, MAX_WORK_CONTEXT_BRIEF_BYTES,
    MAX_WORK_CONTEXT_TITLE_BYTES,
};
pub use message::{ClientMsg, DirEntry, DirListing, ServerMsg};
pub use paths::{config_dir, instance_name};
pub use review::{
    ChangeKind, CommitFile, CommitInfo, DiffLine, FileDiff, HighlightKind, HighlightSpan, Hunk,
    LineKind, Review, ReviewAnchor, ReviewBase, ReviewComment, MAX_HISTORY_COMMITS,
    MAX_REVIEW_COMMENTS, MAX_REVIEW_COMMENT_BYTES,
};
pub use tasks::{
    checked_task_body, Task, TaskAction, TaskCounts, TaskList, TaskState, TaskWrite,
    MAX_TASK_BODY_BYTES, MAX_TASK_TITLE_BYTES,
};
pub use tree::{
    CheckoutInfo, ChildAgentInfo, GitStatus, PaneInfo, PaneKind, PaneStatus, ProjectInfo,
    RepositoryInfo, WorkspaceInfo,
};
