//! `argus-hook diagram`: the current feature's sequence diagrams, read and
//! written as Mermaid source.

use std::io::Read;

use crate::*;

pub(crate) fn diagram(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", diagram_message(rest, &env_url(), &env_token()));
}

/// `diagram`, `diagram add <title> --stdin`, `diagram rewrite <id> <title>
/// --stdin`, `diagram drop <id>`.
fn diagram_message(rest: &[&str], base_url: &str, token: &str) -> String {
    match rest.first().copied() {
        None | Some("list") => write_diagram(DiagramAction::List, base_url, token),
        Some("add") => {
            let write = match parse_diagram_add_args(&rest[1..]) {
                Ok(write) => write,
                Err(message) => return format!("could not change diagram: {message}"),
            };
            write_diagram(DiagramAction::Add(write), base_url, token)
        }
        Some("rewrite") => {
            let Some(id) = rest.get(1).and_then(|id| id.parse::<i64>().ok()) else {
                return "could not change diagram: rewrite wants the number `diagram` prints".to_string();
            };
            let write = match parse_diagram_add_args(&rest[2..]) {
                Ok(write) => write,
                Err(message) => return format!("could not change diagram: {message}"),
            };
            write_diagram(DiagramAction::Rewrite { id, write }, base_url, token)
        }
        Some("drop") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_diagram(DiagramAction::Remove { id }, base_url, token),
            None => "could not change diagram: drop wants the number `diagram` prints".to_string(),
        },
        Some(other) => format!(
            "could not change diagram: `{other}` is not one of add, rewrite, drop"
        ),
    }
}

/// Title is positional; `--stdin` reads Mermaid source from standard input.
fn parse_diagram_add_args(args: &[&str]) -> Result<DiagramWrite, String> {
    let mut title: Vec<&str> = Vec::new();
    let mut from_stdin = false;
    for arg in args {
        match *arg {
            "--stdin" => from_stdin = true,
            other if other.starts_with("--") => {
                return Err(format!("{other} is not one of --stdin"))
            }
            word => title.push(word),
        }
    }
    if title.is_empty() {
        return Err("add wants a title".into());
    }
    let body = if from_stdin {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("could not read Mermaid source: {e}"))?;
        source
    } else {
        return Err("add wants --stdin with the Mermaid source (sequenceDiagram …)".into());
    };
    DiagramWrite {
        title: title.join(" "),
        body,
    }
    .checked()
    .map_err(str::to_string)
}

fn write_diagram(action: DiagramAction, base_url: &str, token: &str) -> String {
    match call::<hook::call::Diagrams>(base_url, token, &action) {
        Ok(list) => format_diagrams(&list),
        Err(Refusal::Unreadable) => "the diagram list changed".to_string(),
        Err(refusal) => refusal.said("could not change diagram"),
    }
}

fn format_diagrams(list: &DiagramList) -> String {
    let Some(feature) = &list.feature else {
        return "this checkout is not on a feature, so it has no sequence diagrams. \
                `argus-hook feature` says where it is."
            .to_string();
    };
    if list.diagrams.is_empty() {
        return format!(
            "No sequence diagrams under {feature} yet. Add one with \
             `argus-hook diagram add \"<title>\" --stdin` and paste Mermaid source."
        );
    }
    let mut lines = vec![format!("Sequence diagrams under {feature}:")];
    for diagram in &list.diagrams {
        lines.push(format!("#{:<3} {}", diagram.id, diagram.title));
        for line in diagram.body.lines() {
            lines.push(format!("      {line}"));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagram_rewrite_names_the_diagram_before_anything_is_sent() {
        let said = diagram_message(&["rewrite", "flow", "--stdin"], "http://127.0.0.1:1", "t");
        assert!(said.contains("rewrite wants the number"), "{said}");
        let said = diagram_message(&["rename", "3"], "http://127.0.0.1:1", "t");
        assert!(said.contains("not one of add, rewrite, drop"), "{said}");
    }

    #[test]
    fn diagram_add_requires_stdin_for_source() {
        let error = parse_diagram_add_args(&["open", "overlay"]).unwrap_err();
        assert!(error.contains("--stdin"), "{error}");
    }
}
