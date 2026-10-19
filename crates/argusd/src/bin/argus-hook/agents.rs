//! `argus-hook agents`: the other agents Argus is running, in every
//! project, one a line with the pane id a view links them by.

use super::*;

pub(super) fn agents(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", agents_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

/// The line that says how to use an id, since the id is why an agent asks.
const HOW: &str = r#"link one in a view as {"kind": "pane", "pane": <id>, "label": "<title>"}"#;

pub(super) fn agents_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let agents = match read::<hook::call::Agents>("agents", rest, base_url, token) {
        Ok(agents) => agents,
        Err(message) => return message,
    };
    if agents.is_empty() {
        return "no other agents".to_string();
    }
    let mut lines: Vec<String> = agents
        .iter()
        .map(|agent| {
            let place = format!("{}/{}", agent.repository, agent.checkout);
            let place = if agent.here { place } else { format!("{place} in project {}", agent.project) };
            let note = agent.note.as_deref().map(|note| format!(": {note}")).unwrap_or_default();
            format!("#{} {} [{}] {place}{note}", agent.pane.0, agent.title, agent.status.word())
        })
        .collect();
    lines.push(HOW.to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::serve_once;
    use argus_protocol::{AgentListing, PaneId, PaneStatus};

    fn listing(pane: u64, title: &str, project: &str, here: bool) -> AgentListing {
        AgentListing {
            pane: PaneId(pane),
            title: title.into(),
            template: Some("claude".into()),
            status: PaneStatus::Working,
            note: None,
            project: project.into(),
            repository: "orion".into(),
            checkout: "main".into(),
            here,
        }
    }

    // A view that named an agent of another project could only print its
    // name: nothing told the agent writing it that agent's pane.
    #[test]
    fn agents_are_listed_with_their_pane_and_where_they_are() {
        let mut waiting = listing(9, "argus-s2-ba", "argus", false);
        waiting.status = PaneStatus::Waiting;
        waiting.note = Some("needs the token".into());
        let agents = vec![listing(5, "reliab-adapter", "reliab", true), waiting];
        let (address, server) = serve_once(&serde_json::to_string(&agents).unwrap());

        let message = agents_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(
            message,
            format!(
                "#5 reliab-adapter [working] orion/main\n\
                 #9 argus-s2-ba [waiting] orion/main in project argus: needs the token\n{HOW}"
            )
        );
        assert!(head.starts_with("POST /pane/4/agents HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
    }

    #[test]
    fn no_other_agents_says_so_and_arguments_are_refused() {
        let (address, server) = serve_once("[]");
        let message = agents_message(&[], &format!("http://{address}/pane/4"), "secret");
        server.join().unwrap();
        assert_eq!(message, "no other agents");
        assert_eq!(agents_message(&["all"], "", ""), "could not read agents: agents takes no arguments");
    }
}
