//! `argus-hook agent`: an agent starting another agent in a pane of its own,
//! here, in a new worktree, or in another repository's checkout.

use std::path::PathBuf;
use std::time::Duration;

use argus_protocol::{AgentLaunch, AgentLaunched, LaunchPlace};

use super::*;

/// Long enough for `git worktree add`, which a launch into a new worktree
/// waits on; the setup commands after it are not waited for.
const LAUNCH_WAIT: Duration = Duration::from_secs(60);

const USAGE: &str = "agent: `agent [--template <name>] [--worktree <branch> [--from <checkout>] | --in <checkout>] (--stdin | <prompt>...)`";

pub(super) fn agent(rest: &[&str]) {
    let stdin = rest.contains(&"--stdin").then(|| {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        text
    });
    println!("{}", agent_message(rest, stdin, &env_url(), &env_token()));
}

fn agent_message(rest: &[&str], stdin: Option<String>, base_url: &str, token: &str) -> String {
    let launch = match launch_from(rest, stdin) {
        Ok(launch) => launch,
        Err(message) => return format!("Not started: {message}"),
    };
    let Ok(body) = serde_json::to_string(&launch) else {
        return "Not started: unencodable".into();
    };
    let url = endpoint_url(base_url, Endpoint::Agent);
    match post_response_within(&url, token, &body, LAUNCH_WAIT) {
        Some((200, answer)) => match serde_json::from_str::<AgentLaunched>(&answer) {
            Ok(started) => format!(
                "Started {} as pane {} in {}.",
                started.template,
                started.pane.0,
                started.checkout.display()
            ),
            Err(_) => "Started.".into(),
        },
        Some((_, refusal)) => format!("Not started: {}", refusal.trim()),
        None => "Not started: the Argus daemon did not answer.".into(),
    }
}

/// The launch `rest` asks for. A prompt is required: an agent started with
/// nothing to do waits at its prompt for a person, and the agent that
/// started it cannot type there.
fn launch_from(rest: &[&str], stdin: Option<String>) -> Result<AgentLaunch, String> {
    let (mut template, mut worktree, mut from, mut inside) = (None, None, None, None);
    let mut words = Vec::new();
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| {
            args.next()
                .map(|v| v.to_string())
                .ok_or_else(|| format!("{flag} wants a value; {USAGE}"))
        };
        match *arg {
            "--template" => template = Some(value("--template")?),
            "--worktree" => worktree = Some(value("--worktree")?),
            "--from" => from = Some(PathBuf::from(value("--from")?)),
            "--in" => inside = Some(PathBuf::from(value("--in")?)),
            "--stdin" => {}
            word => words.push(word),
        }
    }
    let prompt = match stdin {
        Some(_) if !words.is_empty() => {
            return Err("a prompt comes from --stdin or the arguments, not both".into())
        }
        Some(text) => text,
        None => words.join(" "),
    };
    if prompt.trim().is_empty() {
        return Err(format!("say what the agent is to do; {USAGE}"));
    }
    let place = match (worktree, from, inside) {
        (Some(_), _, Some(_)) => return Err("--worktree and --in name different places; give one".into()),
        (None, Some(_), _) => return Err("--from names where a --worktree is made".into()),
        (Some(branch), from, None) => LaunchPlace::Worktree { branch, from },
        (None, None, Some(path)) => LaunchPlace::Checkout { path },
        (None, None, None) => LaunchPlace::Here,
    };
    Ok(AgentLaunch {
        template,
        prompt: Some(prompt.trim_end().to_string()),
        place,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_after_the_flags_are_the_prompt() {
        let launch = launch_from(&["--worktree", "resize", "fix", "the", "resize"], None).unwrap();
        assert_eq!(launch.prompt.as_deref(), Some("fix the resize"));
        assert_eq!(launch.place, LaunchPlace::Worktree { branch: "resize".into(), from: None });
        assert_eq!(launch.template, None);
    }

    #[test]
    fn a_brief_on_stdin_is_the_prompt() {
        let launch = launch_from(&["--in", "/src/other", "--stdin"], Some("one\ntwo\n".into())).unwrap();
        assert_eq!(launch.prompt.as_deref(), Some("one\ntwo"));
        assert_eq!(launch.place, LaunchPlace::Checkout { path: "/src/other".into() });
    }

    #[test]
    fn an_agent_with_nothing_to_do_is_not_started() {
        assert!(launch_from(&["--worktree", "x"], None).is_err());
        assert!(launch_from(&["--stdin"], Some("  \n".into())).is_err());
    }

    #[test]
    fn two_places_or_a_stray_from_are_refused() {
        assert!(launch_from(&["--worktree", "x", "--in", "/y", "go"], None).is_err());
        assert!(launch_from(&["--from", "/y", "go"], None).is_err());
        assert!(launch_from(&["--template"], None).is_err());
    }

    #[test]
    fn a_daemon_that_is_not_there_is_said_aloud() {
        let said = agent_message(&["go"], None, "http://127.0.0.1:1/pane/1", "t");
        assert!(said.starts_with("Not started:"), "{said}");
    }
}
