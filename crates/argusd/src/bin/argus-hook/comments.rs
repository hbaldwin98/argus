//! `argus-hook comments`: the pane's review comments, one a line, for the
//! agent they are addressed to.

use super::*;

pub(super) fn comments(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", comments_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(super) fn comments_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let comments: Vec<ReviewComment> = match read::<hook::call::Comments>("comments", rest, base_url, token) {
        Ok(comments) => comments,
        Err(message) => return message,
    };
    if comments.is_empty() {
        return "no review comments".to_string();
    }
    comments
        .iter()
        .map(|comment| {
            format!(
                "#{} [{}] {}",
                comment.id,
                comment.anchor.base.label(),
                comment.anchor.notification(&comment.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
