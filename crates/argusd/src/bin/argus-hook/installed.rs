//! The form Argus writes into a harness's own hook config: reading the
//! event the harness pipes in, and answering with only the JSON its
//! runner needs.

use super::*;

pub(super) fn installed_hook(url: &str, rest: &[&str]) {
    let Some(token) = rest.first() else { return };
    let (key, raw, note, title) = installed_input(rest, std::io::stdin());
    let inherited_url = env_url();
    let inherited_token = env_token();
    let (url, token) = routed_hook(url, token, &inherited_url, &inherited_token);
    let session = key.and_then(|key| raw.as_deref().and_then(|raw| json_string(raw, key)));
    let _ = post_as(&url, &token, &note, session.as_deref());
    if rest.contains(&OWNS_SESSION_FLAG) {
        post_session_id(&url, &token, session.as_deref());
    }
    post_title(&url, &token, &title, session.as_deref());

    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "{}",
        hook_reply(
            raw.as_deref(),
            rest.contains(&"--inject-instructions"),
            &env_instructions(),
        )
    );
    let _ = out.flush();
}

/// The JSON a hook runner needs so it does not treat bookkeeping as a
/// denied tool or a blocked prompt. Claude Code keys off `toolCall` and
/// wants `decision`; Cursor keys off `tool_name` and wants `permission`.
pub(super) fn hook_reply(raw: Option<&str>, inject_instructions: bool, instructions: &str) -> String {
    let raw = raw.unwrap_or("");
    if raw.contains("\"toolCall\"") {
        return r#"{"decision":"allow"}"#.to_string();
    }
    if raw.contains("\"tool_name\"") {
        return r#"{"permission":"allow"}"#.to_string();
    }
    if (raw.contains("\"invocationNum\"") || inject_instructions) && !instructions.is_empty() {
        return serde_json::json!({
            "injectSteps": [{ "ephemeralMessage": instructions }]
        })
        .to_string();
    }
    "{}".to_string()
}

pub(super) fn env_instructions() -> String {
    std::env::var(INSTRUCTIONS_VAR).unwrap_or_default()
}

pub(super) fn installed_input<'a>(
    rest: &'a [&str],
    stdin: impl Read,
) -> (Option<&'a str>, Option<String>, String, String) {
    let key = rest
        .iter()
        .position(|arg| *arg == SESSION_KEY_FLAG)
        .and_then(|index| rest.get(index + 1))
        .copied();
    let raw = (rest.contains(&NOTE_FLAG) || rest.contains(&TITLE_FLAG) || key.is_some())
        .then(|| read_hook_input(stdin));
    let note = if rest.contains(&NOTE_FLAG) {
        raw.as_deref().map(note_from).unwrap_or_default()
    } else {
        String::new()
    };
    let title = if rest.contains(&TITLE_FLAG) {
        raw.as_deref().map(title_from).unwrap_or_default()
    } else {
        String::new()
    };
    (key, raw, note, title)
}

/// Records the conversation identity Argus resumes this pane with. Only the
/// event a harness fires when *its own* session starts carries the flag that
/// gets here, so a CLI started from inside the pane cannot claim it.
pub(super) fn post_session_id(url: &str, token: &str, id: Option<&str>) {
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return;
    };
    if let Some(base) = pane_base(url) {
        let _ = post_as(&endpoint_url(&base, Endpoint::Session), token, id, Some(id));
    }
}

pub(super) fn post_title(url: &str, token: &str, title: &str, session: Option<&str>) {
    if title.is_empty() {
        return;
    }
    if let Some(base) = pane_base(url) {
        let _ = post_as(&endpoint_url(&base, Endpoint::Title), token, title, session);
    }
}

/// The message a harness hands its hook on stdin.
///
/// Cursor's runner writes one JSON object and then waits for stdout without
/// closing the pipe. Reading to EOF would deadlock until the hook timeout
/// killed the process — after which the status POST never ran. One complete
/// JSON value is enough; plain text still reads to the end of the stream.
pub(super) fn read_hook_input(mut reader: impl Read) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Ok(s) = std::str::from_utf8(&buf) {
                    if json_value(s).is_some() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

pub(super) fn json_value(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let mut de = serde_json::Deserializer::from_str(trimmed);
    serde::Deserialize::deserialize(&mut de).ok()
}

pub(super) fn json_string(raw: &str, key: &str) -> Option<String> {
    let v = json_value(raw)?;
    if let Some(s) = v
        .get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    // Cursor's sessionStart names the same id `session_id`; every other
    // event puts it on `conversation_id`. Asking for either must find both.
    for alias in ["conversation_id", "session_id"] {
        if alias == key {
            continue;
        }
        if let Some(s) = v
            .get(alias)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Harnesses hand hooks a JSON event where they can. `message` is Claude
/// Code's field for the text of what it is waiting on; a harness that sends
/// plain text instead still gets its first line used.
pub(super) fn note_from(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(tool) = v.get("toolCall") {
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
            if let Some(cmd) = tool
                .get("args")
                .and_then(|a| a.get("CommandLine"))
                .and_then(|v| v.as_str())
            {
                return format!("{name}: {cmd}");
            }
            return name.to_string();
        }
        for key in ["message", "text", "reason", "prompt"] {
            if let Some(s) = v.get(key).and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    return s.trim().to_string();
                }
            }
        }
        // Valid JSON with nothing we recognize is not worth showing raw.
        return String::new();
    }
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// The user's prompt, when a harness event carries one. Tool names and
/// session bookkeeping are not titles — a working pane named "Shell" says
/// less than the template already does.
pub(super) fn title_from(raw: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .to_string();
    };
    for key in ["prompt", "query", "user_message", "userMessage", "text"] {
        if let Some(s) = json_prompt_string(&v, key) {
            return s;
        }
    }
    String::new()
}

pub(super) fn json_prompt_string(v: &serde_json::Value, key: &str) -> Option<String> {
    let field = v.get(key)?;
    if let Some(s) = field.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(s.to_string());
    }
    field
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
