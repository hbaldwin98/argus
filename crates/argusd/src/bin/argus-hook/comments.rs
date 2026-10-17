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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::serve_once;

    #[test]
    fn comments_are_read_from_the_daemon_and_rendered_in_order() {
        let comments = vec![ReviewComment {
            id: 4,
            anchor: argus_protocol::ReviewAnchor {
                base: argus_protocol::ReviewBase::Staged,
                commit: None,
                path: "src/main.rs".to_string(),
                old_path: None,
                old_start: Some(9),
                old_end: Some(9),
                new_start: Some(10),
                new_end: Some(10),
                text: vec!["+changed".to_string()],
            },
            body: "fix this".to_string(),
        }];
        let (address, server) = serve_once(&serde_json::to_string(&comments).unwrap());

        let message = comments_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(message, "#4 [staged] src/main.rs:10 `+changed`: fix this");
        assert!(head.starts_with("POST /pane/4/comments HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
        assert_eq!(
            comments_message(&["extra"], "", ""),
            "could not read comments: comments takes no arguments"
        );
    }
}
