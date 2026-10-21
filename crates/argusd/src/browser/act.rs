//! What an agent's built-in browser commands do to the tab the person is
//! watching: each a few DevTools calls on the daemon's own session, so an
//! agent with no browser tool needs none of its own — and navigates the
//! tab it has rather than opening one per page, as a hand-written client
//! tends to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use argus_protocol::{navigable, BrowserAct, BrowserInput};
use base64::Engine;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use super::cdp::Cdp;
use super::input::commands;

/// Where an answer goes: the command's words for the agent, or why not.
pub(crate) type Reply = oneshot::Sender<Result<String, String>>;

/// How long a page has to finish loading before the command answers anyway:
/// a page that never settles has still been navigated to.
const LOADING: Duration = Duration::from_secs(15);
/// The most of a page's text an answer carries, so a long page does not
/// fill the agent's context; the answer says when it was cut.
const MOST_TEXT: usize = 40_000;

static SHOTS: AtomicU64 = AtomicU64::new(0);

/// Does `act` to the tab `session` is attached to, and answers on `reply`.
/// `New` needs no tab; every other command does.
pub(super) async fn run(cdp: Cdp, session: Option<String>, act: BrowserAct, shots: PathBuf, reply: Reply) {
    let _ = reply.send(answer(&cdp, session.as_deref(), act, &shots).await);
}

async fn answer(cdp: &Cdp, session: Option<&str>, act: BrowserAct, shots: &Path) -> Result<String, String> {
    if let BrowserAct::New { url } = &act {
        let url = url.clone().unwrap_or_else(|| "about:blank".into());
        if !navigable(&url) {
            return Err(format!("{url} is not a web address"));
        }
        cdp.call("Target.createTarget", json!({ "url": url }), None).await.map_err(|e| e.to_string())?;
        return Ok(format!("opened a tab at {url}; it is the one shown, and the one the other commands act on"));
    }
    let session = session.ok_or("the browser has no tab open; `argus-hook browser new <url>` opens one")?;
    match act {
        BrowserAct::Go { url } => {
            if !navigable(&url) {
                return Err(format!("{url} is not a web address"));
            }
            let went = cdp.call("Page.navigate", json!({ "url": url }), Some(session)).await.map_err(|e| e.to_string())?;
            if let Some(error) = went["errorText"].as_str().filter(|e| !e.is_empty()) {
                return Err(format!("{url}: {error}"));
            }
            settled(cdp, session).await
        }
        BrowserAct::Back => {
            evaluate(cdp, session, "history.back()").await?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            settled(cdp, session).await
        }
        BrowserAct::Reload => {
            cdp.call("Page.reload", json!({}), Some(session)).await.map_err(|e| e.to_string())?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            settled(cdp, session).await
        }
        BrowserAct::Text => {
            let text = evaluate(cdp, session, "document.body ? document.body.innerText : ''").await?;
            Ok(clipped(text.as_str().unwrap_or_default()))
        }
        BrowserAct::Click { selector } => {
            let at = evaluate(cdp, session, &centre_of(&selector)).await?;
            let (Some(x), Some(y)) = (at["x"].as_f64(), at["y"].as_f64()) else {
                return Err(format!("nothing on the page matches {selector}"));
            };
            for (method, params) in commands(&BrowserInput::Click { x, y }) {
                cdp.call(method, params, Some(session)).await.map_err(|e| e.to_string())?;
            }
            Ok(format!("clicked {selector}"))
        }
        BrowserAct::Type { selector, text } => {
            let focused = evaluate(cdp, session, &focus(&selector)).await?;
            if focused != Value::Bool(true) {
                return Err(format!("nothing on the page matches {selector}"));
            }
            cdp.call("Input.insertText", json!({ "text": text }), Some(session)).await.map_err(|e| e.to_string())?;
            Ok(format!("typed into {selector}"))
        }
        BrowserAct::Eval { expression } => {
            let value = evaluate(cdp, session, &expression).await?;
            Ok(if value.is_null() { "undefined".into() } else { value.to_string() })
        }
        BrowserAct::Shot => {
            let shot = cdp.call("Page.captureScreenshot", json!({ "format": "png" }), Some(session)).await.map_err(|e| e.to_string())?;
            let png = base64::engine::general_purpose::STANDARD
                .decode(shot["data"].as_str().unwrap_or_default())
                .map_err(|e| e.to_string())?;
            std::fs::create_dir_all(shots).map_err(|e| e.to_string())?;
            let path = shots.join(format!("shot-{}.png", SHOTS.fetch_add(1, Ordering::Relaxed) + 1));
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        BrowserAct::New { .. } | BrowserAct::Tabs => unreachable!("answered before a tab is needed"),
    }
}

/// A script's value, or the exception it threw, in its own words.
async fn evaluate(cdp: &Cdp, session: &str, expression: &str) -> Result<Value, String> {
    let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true });
    let answer = cdp.call("Runtime.evaluate", params, Some(session)).await.map_err(|e| e.to_string())?;
    if let Some(thrown) = answer.get("exceptionDetails") {
        let words = thrown["exception"]["description"].as_str().or(thrown["text"].as_str()).unwrap_or("the script threw");
        return Err(words.to_string());
    }
    Ok(answer["result"]["value"].clone())
}

/// Waits for the page to finish loading, or for `LOADING` to pass, and says
/// where it is now.
async fn settled(cdp: &Cdp, session: &str) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + LOADING;
    while tokio::time::Instant::now() < deadline {
        if evaluate(cdp, session, "document.readyState").await.ok().as_ref().and_then(Value::as_str) == Some("complete") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let at = evaluate(cdp, session, "({ url: location.href, title: document.title })").await?;
    let (url, title) = (at["url"].as_str().unwrap_or_default(), at["title"].as_str().unwrap_or_default());
    Ok(if title.is_empty() { url.to_string() } else { format!("{url} — {title}") })
}

/// The script that brings the first match of `selector` into view and
/// gives the middle of it, in the page's CSS pixels, or null.
fn centre_of(selector: &str) -> String {
    format!(
        "(() => {{ const e = document.querySelector({}); if (!e) return null; e.scrollIntoView({{ block: 'center', inline: 'center' }}); \
         const r = e.getBoundingClientRect(); return {{ x: r.left + r.width / 2, y: r.top + r.height / 2 }}; }})()",
        json!(selector)
    )
}

fn focus(selector: &str) -> String {
    format!("(() => {{ const e = document.querySelector({}); if (!e) return false; e.focus(); return true; }})()", json!(selector))
}

fn clipped(text: &str) -> String {
    match text.char_indices().nth(MOST_TEXT) {
        Some((at, _)) => format!("{}\n[the page goes on; cut at {MOST_TEXT} characters]", &text[..at]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selector_is_put_in_the_script_as_a_string_not_as_code() {
        let script = centre_of("a[href=\"x\"]'); alert(1); ('");
        assert!(script.contains(r#"document.querySelector("a[href=\"x\"]'); alert(1); ('")"#), "{script}");
    }

    #[test]
    fn a_long_page_is_cut_and_says_so() {
        let long = "é".repeat(MOST_TEXT + 5);
        let cut = clipped(&long);
        assert!(cut.starts_with(&"é".repeat(MOST_TEXT)) && cut.ends_with("characters]"));
        assert_eq!(clipped("short"), "short");
    }
}
