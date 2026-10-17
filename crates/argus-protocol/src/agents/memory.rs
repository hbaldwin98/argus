//! The durable working memory an agent receives for one request.
//!
//! A work context is a topic, not an assignment. Artifacts record the useful
//! results of work under that topic, and a packet selects only the material a
//! caller explicitly asked for. Keeping selection here makes the boundary
//! testable before storage, hooks, and harnesses start imposing their own
//! assumptions on it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// A short title should remain a label rather than becoming a second brief.
pub const MAX_WORK_CONTEXT_TITLE_BYTES: usize = 200;
/// A context brief is useful background, but it is not a design document.
pub const MAX_WORK_CONTEXT_BRIEF_BYTES: usize = 8192;
/// Individual artifacts should be concise enough to survive repeated reads.
pub const MAX_ARTIFACT_TEXT_BYTES: usize = 8192;
pub const DEFAULT_PACKET_MAX_ITEMS: usize = 64;
pub const DEFAULT_PACKET_MAX_BYTES: usize = 32 * 1024;

/// Where a durable memory item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemorySource {
    Human,
    Agent,
    External,
    Legacy,
}

/// Attribution retained with every durable context item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: MemorySource,
    pub session: Option<String>,
    pub checkout: Option<String>,
    /// Unix timestamp in seconds, matching the existing durable records.
    pub at: i64,
}

/// A durable topic around which agents accumulate understanding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkContext {
    /// Stable and human-readable. Legacy feature slugs can be imported here
    /// without changing the identifiers agents already know.
    pub id: String,
    pub title: String,
    pub brief: String,
    #[serde(default)]
    pub external: Vec<String>,
    pub provenance: Provenance,
}

/// The kinds of durable result an agent may leave for a later agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Decision,
    Task,
    Finding,
    Assumption,
    Question,
    Summary,
}

/// The typed payload of one artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactContent {
    Decision {
        chose: String,
        over: Option<String>,
        because: Option<String>,
    },
    Task {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        external: Option<String>,
    },
    Finding {
        text: String,
    },
    Assumption {
        text: String,
    },
    Question {
        text: String,
    },
    Summary {
        text: String,
    },
}

impl ArtifactContent {
    pub fn kind(&self) -> ArtifactKind {
        match self {
            ArtifactContent::Decision { .. } => ArtifactKind::Decision,
            ArtifactContent::Task { .. } => ArtifactKind::Task,
            ArtifactContent::Finding { .. } => ArtifactKind::Finding,
            ArtifactContent::Assumption { .. } => ArtifactKind::Assumption,
            ArtifactContent::Question { .. } => ArtifactKind::Question,
            ArtifactContent::Summary { .. } => ArtifactKind::Summary,
        }
    }
}

/// The correction state of an artifact.
///
/// Corrections retain the provenance of the actor that made them. The old
/// payload remains available when a caller explicitly asks for history, but
/// it cannot silently guide a normal packet after it is corrected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactState {
    #[default]
    Active,
    Completed {
        corrected_by: Provenance,
    },
    Superseded {
        replacement: i64,
        corrected_by: Provenance,
    },
    Withdrawn {
        corrected_by: Provenance,
        reason: String,
    },
}

impl ArtifactState {
    pub fn is_active(&self) -> bool {
        matches!(self, ArtifactState::Active)
    }
}

/// One typed, attributed memory item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: i64,
    pub context: String,
    pub content: ArtifactContent,
    pub provenance: Provenance,
    #[serde(default)]
    pub state: ArtifactState,
}

/// A context entry explicitly forwarded by a person or agent.
///
/// Forwarding is durable input to a later packet, unlike the old operation
/// that only pasted text into a currently live terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardedMaterial {
    pub id: i64,
    pub context: Option<String>,
    pub label: String,
    pub body: String,
    pub provenance: Provenance,
}

/// A store-independent view of memory used by the selector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Monotonically increases whenever relevant memory changes. The store
    /// owns this value; the selector deliberately does not hash user content.
    pub revision: u64,
    pub contexts: Vec<WorkContext>,
    pub artifacts: Vec<Artifact>,
    pub forwards: Vec<ForwardedMaterial>,
}

/// Limits applied to the packet contents, before transport framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketLimits {
    pub max_items: usize,
    pub max_bytes: usize,
}

impl Default for PacketLimits {
    fn default() -> Self {
        PacketLimits {
            max_items: DEFAULT_PACKET_MAX_ITEMS,
            max_bytes: DEFAULT_PACKET_MAX_BYTES,
        }
    }
}

/// What an agent explicitly wants considered for one request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextQuery {
    /// No context means no artifact is inferred from the checkout. Explicit
    /// forwarded material may still be included.
    pub work_context: Option<String>,
    pub checkout: Option<String>,
    pub request: Option<String>,
    /// Forward ids are ordered by the caller because that order can carry
    /// meaning in a human's handoff.
    #[serde(default)]
    pub forwarded: Vec<i64>,
    #[serde(default)]
    pub include_history: bool,
    #[serde(default)]
    pub limits: PacketLimits,
}

/// Why one item was admitted to a packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InclusionReason {
    SelectedContext,
    ExplicitForward,
    CurrentContext,
    HistoricalContext,
}

/// The three kinds of material a packet can carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacketItem {
    WorkContext(WorkContext),
    Artifact(Artifact),
    Forwarded(ForwardedMaterial),
}

/// One packet item with an explanation suitable for an agent or inspector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketEntry {
    pub item: PacketItem,
    pub reason: InclusionReason,
}

impl PacketEntry {
    /// The binary protocol encoding gives the selector a deterministic budget
    /// without making the selection depend on a JSON formatter.
    pub fn encoded_len(&self) -> usize {
        rmp_serde::to_vec(self).map_or(usize::MAX, |encoded| encoded.len())
    }
}

/// The bounded material an agent should use for one request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPacket {
    pub revision: u64,
    pub work_context: Option<String>,
    pub checkout: Option<String>,
    pub request: Option<String>,
    pub entries: Vec<PacketEntry>,
    pub used_bytes: usize,
    pub omitted_items: usize,
}

/// Select a bounded packet without inferring relevance from checkout state.
///
/// The selected context brief comes first, followed by explicitly forwarded
/// material in caller order, then active artifacts newest first. Historical
/// artifacts are admitted only when requested. Items that do not fit are
/// skipped rather than allowing one long entry to consume the whole packet.
pub fn select_context_packet(snapshot: &MemorySnapshot, query: &ContextQuery) -> ContextPacket {
    let mut packet = ContextPacket {
        revision: snapshot.revision,
        work_context: query.work_context.clone(),
        checkout: query.checkout.clone(),
        request: query.request.clone(),
        ..ContextPacket::default()
    };

    let mut context_entry = None;
    let mut artifact_entries = Vec::new();
    if let Some(context_id) = query.work_context.as_deref() {
        if let Some(context) = snapshot.contexts.iter().find(|c| c.id == context_id) {
            context_entry = Some(PacketEntry {
                item: PacketItem::WorkContext(context.clone()),
                reason: InclusionReason::SelectedContext,
            });
        }

        let mut artifacts: Vec<&Artifact> = snapshot
            .artifacts
            .iter()
            .filter(|artifact| artifact.context == context_id)
            .filter(|artifact| query.include_history || artifact.state.is_active())
            .collect();
        artifacts.sort_by(|left, right| {
            right
                .provenance
                .at
                .cmp(&left.provenance.at)
                .then_with(|| right.id.cmp(&left.id))
        });
        artifact_entries.extend(artifacts.into_iter().map(|artifact| PacketEntry {
            item: PacketItem::Artifact(artifact.clone()),
            reason: if artifact.state.is_active() {
                InclusionReason::CurrentContext
            } else {
                InclusionReason::HistoricalContext
            },
        }));
    }

    let mut seen_forwards = HashSet::new();
    let mut forwards = Vec::new();
    for id in &query.forwarded {
        if !seen_forwards.insert(*id) {
            continue;
        }
        if let Some(forward) = snapshot.forwards.iter().find(|forward| forward.id == *id) {
            forwards.push(PacketEntry {
                item: PacketItem::Forwarded(forward.clone()),
                reason: InclusionReason::ExplicitForward,
            });
        }
    }
    let mut candidates = Vec::new();
    if let Some(context) = context_entry {
        candidates.push(context);
    }
    candidates.extend(forwards);
    candidates.extend(artifact_entries);

    for entry in candidates {
        let entry_bytes = entry.encoded_len();
        let fits_items = packet.entries.len() < query.limits.max_items;
        let fits_bytes = entry_bytes <= query.limits.max_bytes.saturating_sub(packet.used_bytes);
        if !fits_items || !fits_bytes {
            packet.omitted_items += 1;
            continue;
        }
        packet.used_bytes += entry_bytes;
        packet.entries.push(entry);
    }
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance(source: MemorySource, at: i64) -> Provenance {
        Provenance {
            source,
            session: Some("session-1".into()),
            checkout: Some("/repo/worktree".into()),
            at,
        }
    }

    fn context(id: &str) -> WorkContext {
        WorkContext {
            id: id.into(),
            title: id.into(),
            brief: "brief".into(),
            external: Vec::new(),
            provenance: provenance(MemorySource::Human, 1),
        }
    }

    fn finding(id: i64, context: &str, at: i64, text: &str) -> Artifact {
        Artifact {
            id,
            context: context.into(),
            content: ArtifactContent::Finding { text: text.into() },
            provenance: provenance(MemorySource::Agent, at),
            state: ArtifactState::Active,
        }
    }

    fn forward(id: i64, label: &str) -> ForwardedMaterial {
        ForwardedMaterial {
            id,
            context: None,
            label: label.into(),
            body: format!("body-{label}"),
            provenance: provenance(MemorySource::Human, id),
        }
    }

    fn packet(query: ContextQuery, snapshot: MemorySnapshot) -> ContextPacket {
        select_context_packet(&snapshot, &query)
    }

    #[test]
    fn only_an_explicit_context_supplies_artifacts() {
        let snapshot = MemorySnapshot {
            revision: 7,
            contexts: vec![context("one"), context("two")],
            artifacts: vec![
                finding(1, "one", 1, "selected"),
                finding(2, "two", 2, "other"),
            ],
            forwards: Vec::new(),
        };
        let packet = packet(
            ContextQuery {
                work_context: Some("one".into()),
                ..ContextQuery::default()
            },
            snapshot,
        );

        assert_eq!(packet.revision, 7);
        assert_eq!(packet.entries.len(), 2);
        assert!(matches!(packet.entries[0].item, PacketItem::WorkContext(_)));
        assert!(matches!(
            packet.entries[1].item,
            PacketItem::Artifact(Artifact { id: 1, .. })
        ));
    }

    #[test]
    fn no_context_does_not_use_checkout_or_sole_context_as_an_assignment() {
        let snapshot = MemorySnapshot {
            contexts: vec![context("only")],
            artifacts: vec![finding(1, "only", 1, "not implicit")],
            forwards: vec![forward(9, "handoff")],
            ..MemorySnapshot::default()
        };
        let packet = packet(
            ContextQuery {
                checkout: Some("/repo/worktree".into()),
                forwarded: vec![9],
                ..ContextQuery::default()
            },
            snapshot,
        );

        assert_eq!(packet.entries.len(), 1);
        assert!(matches!(packet.entries[0].item, PacketItem::Forwarded(_)));
    }

    #[test]
    fn explicit_forwards_keep_their_order_and_are_not_duplicated() {
        let snapshot = MemorySnapshot {
            forwards: vec![forward(1, "first"), forward(2, "second")],
            ..MemorySnapshot::default()
        };
        let packet = packet(
            ContextQuery {
                forwarded: vec![2, 1, 2],
                ..ContextQuery::default()
            },
            snapshot,
        );

        let labels = packet
            .entries
            .iter()
            .map(|entry| match &entry.item {
                PacketItem::Forwarded(material) => material.label.as_str(),
                _ => "unexpected",
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, ["second", "first"]);
        assert!(packet
            .entries
            .iter()
            .all(|entry| entry.reason == InclusionReason::ExplicitForward));
    }

    #[test]
    fn active_artifacts_are_newest_first_and_history_is_opt_in() {
        let mut old = finding(1, "one", 1, "old");
        old.state = ArtifactState::Completed {
            corrected_by: provenance(MemorySource::Human, 3),
        };
        let snapshot = MemorySnapshot {
            contexts: vec![context("one")],
            artifacts: vec![old, finding(2, "one", 2, "new")],
            ..MemorySnapshot::default()
        };

        let current = packet(
            ContextQuery {
                work_context: Some("one".into()),
                ..ContextQuery::default()
            },
            snapshot.clone(),
        );
        assert!(matches!(
            current.entries[1].item,
            PacketItem::Artifact(Artifact { id: 2, .. })
        ));
        assert_eq!(current.entries.len(), 2);

        let history = packet(
            ContextQuery {
                work_context: Some("one".into()),
                include_history: true,
                ..ContextQuery::default()
            },
            snapshot,
        );
        assert_eq!(history.entries.len(), 3);
        assert_eq!(
            history.entries[2].reason,
            InclusionReason::HistoricalContext
        );
    }

    #[test]
    fn item_and_byte_limits_omit_material_without_reordering_the_rest() {
        let snapshot = MemorySnapshot {
            contexts: vec![context("one")],
            artifacts: vec![finding(1, "one", 1, "finding")],
            forwards: vec![forward(9, "handoff")],
            ..MemorySnapshot::default()
        };
        let item_limited = packet(
            ContextQuery {
                work_context: Some("one".into()),
                forwarded: vec![9],
                limits: PacketLimits {
                    max_items: 1,
                    max_bytes: DEFAULT_PACKET_MAX_BYTES,
                },
                ..ContextQuery::default()
            },
            snapshot.clone(),
        );
        assert_eq!(item_limited.entries.len(), 1);
        assert_eq!(item_limited.omitted_items, 2);

        let byte_limited = packet(
            ContextQuery {
                work_context: Some("one".into()),
                forwarded: vec![9],
                limits: PacketLimits {
                    max_items: DEFAULT_PACKET_MAX_ITEMS,
                    max_bytes: 1,
                },
                ..ContextQuery::default()
            },
            snapshot,
        );
        assert!(byte_limited.entries.is_empty());
        assert_eq!(byte_limited.used_bytes, 0);
        assert_eq!(byte_limited.omitted_items, 3);
    }

    #[test]
    fn correction_provenance_survives_historical_reads() {
        let mut finding = finding(1, "one", 1, "withdrawn");
        finding.state = ArtifactState::Withdrawn {
            corrected_by: provenance(MemorySource::Human, 2),
            reason: "the observation was false".into(),
        };
        let packet = packet(
            ContextQuery {
                work_context: Some("one".into()),
                include_history: true,
                ..ContextQuery::default()
            },
            MemorySnapshot {
                contexts: vec![context("one")],
                artifacts: vec![finding],
                ..MemorySnapshot::default()
            },
        );

        let PacketItem::Artifact(artifact) = &packet.entries[1].item else {
            panic!("expected the historical artifact")
        };
        let ArtifactState::Withdrawn { corrected_by, .. } = &artifact.state else {
            panic!("expected withdrawal state")
        };
        assert_eq!(corrected_by.source, MemorySource::Human);
    }

    #[test]
    fn a_packet_survives_protocol_encoding() {
        let packet = packet(
            ContextQuery {
                work_context: Some("one".into()),
                request: Some("understand the parser boundary".into()),
                ..ContextQuery::default()
            },
            MemorySnapshot {
                revision: 11,
                contexts: vec![context("one")],
                artifacts: vec![finding(1, "one", 1, "the parser owns framing")],
                ..MemorySnapshot::default()
            },
        );
        let encoded = rmp_serde::to_vec(&packet).unwrap();
        let decoded: ContextPacket = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(decoded, packet);
    }
}
