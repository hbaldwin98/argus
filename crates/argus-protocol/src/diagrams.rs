//! Sequence diagrams filed under a feature: Mermaid source stored durably,
//! rendered in the client.

use serde::{Deserialize, Serialize};

pub const MAX_DIAGRAM_TITLE_BYTES: usize = 200;
/// Room for a non-trivial interaction without turning the store into a
/// design repo.
pub const MAX_DIAGRAM_BODY_BYTES: usize = 16_384;

/// One stored sequence diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceDiagram {
    pub id: i64,
    pub feature: String,
    pub title: String,
    pub body: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagramWrite {
    pub title: String,
    pub body: String,
}

impl DiagramWrite {
    pub fn checked(self) -> Result<DiagramWrite, &'static str> {
        if self.title.trim().is_empty() {
            return Err("a diagram needs a title");
        }
        if self.title.len() > MAX_DIAGRAM_TITLE_BYTES {
            return Err("a diagram title is too long");
        }
        if self.body.trim().is_empty() {
            return Err("a diagram needs Mermaid source");
        }
        if self.body.len() > MAX_DIAGRAM_BODY_BYTES {
            return Err("a diagram source is too large");
        }
        Ok(self)
    }
}

/// Read or change the selected feature's diagrams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagramAction {
    List,
    Add(DiagramWrite),
    Remove { id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagramList {
    pub project_name: String,
    pub feature: Option<String>,
    pub diagrams: Vec<SequenceDiagram>,
}
