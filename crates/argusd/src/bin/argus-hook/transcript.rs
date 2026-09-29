//! `argus-hook transcript`: entries a harness pushes of its conversation
//! when its transcript is not a file Argus can read.
//!
//! A harness with shell hooks and nothing more says what happened in
//! flags; a plugin that can build the whole shape hands a push as JSON on
//! stdin. Either way the daemon holds the entries in memory and shows them
//! to whoever watches the pane.

use argus_protocol::{Body, Entry, Push, Update};

use super::*;

pub(super) fn transcript(rest: &[&str]) {
    let push = if rest.iter().any(|arg| arg.starts_with("--") && *arg != "--fresh") {
        from_flags(rest)
    } else {
        let mut raw = String::new();
        let _ = std::io::stdin().read_to_string(&mut raw);
        match serde_json::from_str::<Push>(&raw) {
            Ok(mut push) => {
                push.fresh |= rest.contains(&"--fresh");
                push
            }
            Err(error) => {
                println!("argus-hook transcript wants a push as JSON on stdin, or flags: {error}");
                return;
            }
        }
    };
    if push.updates.is_empty() && !push.fresh {
        return;
    }
    let Ok(body) = serde_json::to_string(&push) else {
        return;
    };
    let _ = post(&endpoint_url(&env_url(), Endpoint::Transcript), &env_token(), &body);
}

/// A push from flags: `--prompt`, `--reply`, `--notice` and `--turn-end`,
/// each one entry in order; `--id` names the next, so saying it again
/// replaces it; `--fresh` starts the conversation over.
pub(super) fn from_flags(rest: &[&str]) -> Push {
    let mut push = Push {
        fresh: false,
        updates: Vec::new(),
    };
    let mut named: Option<String> = None;
    let mut args = rest.iter();
    while let Some(flag) = args.next() {
        let mut text = || args.next().map(|v| v.to_string()).unwrap_or_default();
        let body = match *flag {
            "--prompt" => Body::Prompt { text: text() },
            "--reply" => Body::Reply { text: text() },
            "--notice" => Body::Notice { text: text() },
            "--turn-end" => Body::TurnEnd { millis: None },
            "--id" => {
                named = args.next().map(|v| v.to_string());
                continue;
            }
            "--fresh" => {
                push.fresh = true;
                continue;
            }
            _ => continue,
        };
        let id = named.take().unwrap_or_else(|| fresh_id(push.updates.len()));
        push.updates.push(Update::Upsert(Entry { id, at: None, body }));
    }
    push
}

/// An id no earlier entry has: the time, and the entry's place in this push.
fn fresh_id(place: usize) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("hook-{nanos}-{place}")
}
