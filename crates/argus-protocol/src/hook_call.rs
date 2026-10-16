//! What each pane-API endpoint is posted and what it answers with, declared
//! once: `argus-hook` calls through the declaration and the daemon answers
//! through it, so a body or a reply cannot mean one thing on each side.
//!
//! Status, title and session are not here: they are text posted with no
//! reply, and the status names its report in the path, which a `const`
//! endpoint cannot. Their bounds are still in [`Endpoint::max_body`].

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::hook::Endpoint;

/// How a body or a reply is written on the wire.
pub trait Form<T> {
    fn write(value: &T) -> Result<Vec<u8>, String>;
    /// `T`, or why the bytes are not one.
    fn read(bytes: &[u8]) -> Result<T, String>;
}

/// JSON. A failed read is serde's own words, so an agent that sent a block
/// kind the schema lacks is told which.
pub enum Json {}

/// The bytes as text, as a note or a directory is sent; never refused.
pub enum Text {}

/// Nothing: a read takes no body, and a write answers with none.
pub enum Empty {}

impl<T: Serialize + DeserializeOwned> Form<T> for Json {
    fn write(value: &T) -> Result<Vec<u8>, String> {
        serde_json::to_vec(value).map_err(|error| error.to_string())
    }

    fn read(bytes: &[u8]) -> Result<T, String> {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
}

impl Form<String> for Text {
    fn write(value: &String) -> Result<Vec<u8>, String> {
        Ok(value.as_bytes().to_vec())
    }

    fn read(bytes: &[u8]) -> Result<String, String> {
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// The reads post an empty body, and one that posts anything else is still
/// a read: the daemon never looked at it.
impl Form<()> for Empty {
    fn write(_: &()) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }

    fn read(_: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// One endpoint's body and reply.
pub trait Call {
    const ENDPOINT: Endpoint;
    /// What the body is called when one is not it: "task change", read as
    /// "not a task change: <why>".
    const BODY: &'static str;
    type Body;
    type BodyForm: Form<Self::Body>;
    type Reply;
    type ReplyForm: Form<Self::Reply>;
}

/// Moves the pane into the checkout at the directory posted.
pub enum Checkout {}
impl Call for Checkout {
    const ENDPOINT: Endpoint = Endpoint::Checkout;
    const BODY: &'static str = "directory";
    type Body = String;
    type BodyForm = Text;
    type Reply = ();
    type ReplyForm = Empty;
}

pub enum Comments {}
impl Call for Comments {
    const ENDPOINT: Endpoint = Endpoint::Comments;
    const BODY: &'static str = "read";
    type Body = ();
    type BodyForm = Empty;
    type Reply = Vec<crate::ReviewComment>;
    type ReplyForm = Json;
}

pub enum Decisions {}
impl Call for Decisions {
    const ENDPOINT: Endpoint = Endpoint::Decisions;
    const BODY: &'static str = "read";
    type Body = ();
    type BodyForm = Empty;
    type Reply = crate::DecisionBoard;
    type ReplyForm = Json;
}

pub enum Decide {}
impl Call for Decide {
    const ENDPOINT: Endpoint = Endpoint::Decide;
    const BODY: &'static str = "decision";
    type Body = crate::DecisionWrite;
    type BodyForm = Json;
    type Reply = crate::Decision;
    type ReplyForm = Json;
}

pub enum ChangeDecision {}
impl Call for ChangeDecision {
    const ENDPOINT: Endpoint = Endpoint::DecisionChange;
    const BODY: &'static str = "decision change";
    type Body = crate::DecisionChange;
    type BodyForm = Json;
    type Reply = crate::DecisionBoard;
    type ReplyForm = Json;
}

pub enum Features {}
impl Call for Features {
    const ENDPOINT: Endpoint = Endpoint::Features;
    const BODY: &'static str = "read";
    type Body = ();
    type BodyForm = Empty;
    type Reply = crate::FeatureBoard;
    type ReplyForm = Json;
}

/// Answered with the board as it stands afterwards; the daemon's
/// `feature_action_for_agent` says why.
pub enum ChangeFeature {}
impl Call for ChangeFeature {
    const ENDPOINT: Endpoint = Endpoint::Feature;
    const BODY: &'static str = "feature change";
    type Body = crate::FeatureAction;
    type BodyForm = Json;
    type Reply = crate::FeatureBoard;
    type ReplyForm = Json;
}

pub enum Tasks {}
impl Call for Tasks {
    const ENDPOINT: Endpoint = Endpoint::Tasks;
    const BODY: &'static str = "task change";
    type Body = crate::TaskAction;
    type BodyForm = Json;
    type Reply = crate::TaskList;
    type ReplyForm = Json;
}

pub enum Diagrams {}
impl Call for Diagrams {
    const ENDPOINT: Endpoint = Endpoint::Diagrams;
    const BODY: &'static str = "diagram change";
    type Body = crate::DiagramAction;
    type BodyForm = Json;
    type Reply = crate::DiagramList;
    type ReplyForm = Json;
}

/// The map read with an empty body, or an agent's draft of it, which waits
/// for the person.
pub enum TrackerMap {}
impl Call for TrackerMap {
    const ENDPOINT: Endpoint = Endpoint::TrackerMap;
    const BODY: &'static str = "tracker map draft";
    type Body = String;
    type BodyForm = Text;
    type Reply = crate::TrackerMap;
    type ReplyForm = Json;
}

pub enum Reconcile {}
impl Call for Reconcile {
    const ENDPOINT: Endpoint = Endpoint::Reconcile;
    const BODY: &'static str = "reconcile change";
    type Body = crate::ReconcileChange;
    type BodyForm = Json;
    type Reply = crate::Feature;
    type ReplyForm = Json;
}

pub enum Telemetry {}
impl Call for Telemetry {
    const ENDPOINT: Endpoint = Endpoint::Telemetry;
    const BODY: &'static str = "telemetry report";
    type Body = crate::AgentTelemetry;
    type BodyForm = Json;
    type Reply = ();
    type ReplyForm = Empty;
}

pub enum ChangeView {}
impl Call for ChangeView {
    const ENDPOINT: Endpoint = Endpoint::View;
    const BODY: &'static str = "view change";
    type Body = crate::ViewChange;
    type BodyForm = Json;
    type Reply = ();
    type ReplyForm = Empty;
}

/// The reply's shape depends on the change: an entry, a list of them or a
/// record, so it is read as JSON and taken apart per change.
pub enum SavedView {}
impl Call for SavedView {
    const ENDPOINT: Endpoint = Endpoint::SavedView;
    const BODY: &'static str = "saved view change";
    type Body = crate::SavedViewChange;
    type BodyForm = Json;
    type Reply = serde_json::Value;
    type ReplyForm = Json;
}

pub enum Launch {}
impl Call for Launch {
    const ENDPOINT: Endpoint = Endpoint::Agent;
    const BODY: &'static str = "agent launch";
    type Body = crate::AgentLaunch;
    type BodyForm = Json;
    type Reply = crate::AgentLaunched;
    type ReplyForm = Json;
}

#[cfg(test)]
mod tests;
