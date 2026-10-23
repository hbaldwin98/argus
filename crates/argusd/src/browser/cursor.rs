//! What the pointer looks like over the tab shown: the CSS cursor under the
//! person's pointer, read after it moves and reported when it changes.
//!
//! DevTools says nothing of the cursor, so the page is asked what is under
//! the point. One question is in flight at a time and only the newest point
//! is asked about, so a fast pointer costs a question per answer, not one
//! per move.

use argus_protocol::BrowserCursor;
use serde_json::json;
use tokio::sync::{mpsc, watch};

use super::cdp::Cdp;
use super::Report;

/// Where the pointer is: the shown tab's session and a point in its CSS
/// pixels.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct At {
    pub(super) session: String,
    pub(super) x: f64,
    pub(super) y: f64,
}

/// Starts reading the cursor under each point sent, until the sender is
/// dropped.
pub(super) fn follow(cdp: Cdp, reports: mpsc::UnboundedSender<Report>) -> watch::Sender<Option<At>> {
    let (points, mut asked) = watch::channel(None::<At>);
    tokio::spawn(async move {
        let mut told = BrowserCursor::Default;
        while asked.changed().await.is_ok() {
            let Some(at) = asked.borrow_and_update().clone() else { continue };
            let params = json!({ "expression": under(at.x, at.y), "returnByValue": true });
            // A page between documents has nothing to ask; the next move asks again.
            let Ok(answer) = cdp.call("Runtime.evaluate", params, Some(&at.session)).await else { continue };
            let cursor = BrowserCursor::from_css(answer["result"]["value"].as_str().unwrap_or_default());
            if cursor != told {
                told = cursor;
                if reports.send(Report::Cursor(cursor)).is_err() {
                    return;
                }
            }
        }
    });
    points
}

/// The script that names the cursor at a point as a browser would draw it.
/// `auto` is resolved as Chrome does: an I-beam over editable or plain text,
/// the arrow elsewhere. A stated cursor with an image falls back to its last
/// keyword, which is what draws when the image cannot.
fn under(x: f64, y: f64) -> String {
    format!(
        "(() => {{ const x = {x}, y = {y}; let e = document.elementFromPoint(x, y); \
         while (e && e.shadowRoot) {{ const inner = e.shadowRoot.elementFromPoint(x, y); if (!inner || inner === e) break; e = inner; }} \
         if (!e) return 'default'; \
         const stated = getComputedStyle(e).cursor.split(',').pop().trim(); \
         if (stated !== 'auto') return stated; \
         if (e.isContentEditable || e.matches('textarea, input:not([type=button],[type=submit],[type=reset],[type=checkbox],[type=radio],[type=range],[type=color],[type=file],[type=image])')) return 'text'; \
         const at = document.caretRangeFromPoint(x, y); \
         if (at && at.startContainer.nodeType === 3) {{ const run = document.createRange(); run.selectNodeContents(at.startContainer); \
           for (const b of run.getClientRects()) if (x >= b.left && x <= b.right && y >= b.top && y <= b.bottom) return 'text'; }} \
         return 'default'; }})()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_point_is_put_in_the_script_as_numbers() {
        let script = under(12.5, 40.0);
        assert!(script.contains("const x = 12.5, y = 40;"), "{script}");
    }
}
