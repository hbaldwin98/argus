//! Structured transcript events: prompt, thinking, tools, output, result.

use argus_protocol::{AgentTranscriptEvent, Endpoint, TranscriptKind};

use super::*;

/// `argus-hook event prompt "what the user asked"`
///
/// `argus-hook event tool --name shell [--text detail]`
pub(super) fn event(rest: &[&str]) {
    let Some((kind, tail)) = rest.split_first() else {
        return;
    };
    let Some(kind) = TranscriptKind::parse(kind) else {
        return;
    };
    let mut text = None;
    let mut tool = None;
    let mut args = tail.iter();
    while let Some(flag) = args.next() {
        match *flag {
            "--text" => text = args.next().map(|s| s.to_string()),
            "--tool" | "--name" => tool = args.next().map(|s| s.to_string()),
            other if text.is_none() && !other.starts_with('-') => {
                text = Some(other.to_string());
            }
            _ => {}
        }
    }
    let body = AgentTranscriptEvent { kind, text, tool };
    if let (Some(base), Ok(json)) = (pane_base(&env_url()), serde_json::to_string(&body)) {
        let _ = post_as(
            &endpoint_url(&base, Endpoint::Event),
            &env_token(),
            &json,
            None,
        );
    }
}
