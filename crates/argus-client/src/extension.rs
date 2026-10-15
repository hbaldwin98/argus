//! `argus extension`: the guide to writing an extension, and what the
//! running extensions are — listed, or one shown whole — as the daemon
//! describes them.
//!
//! Written for an agent as much as a person, so `show` prints Markdown: the
//! extension's own guide, then every command and view with what each key,
//! scope and action means. Like `argus focus` it never starts a daemon;
//! none running means none of its extensions is either.

use std::fmt::Write as _;
use std::time::Duration;

use anyhow::Context;
use argus_protocol::{
    transport, ClientMsg, CommandScope, ExtDescription, ExtPlace, ExtScope, ServerMsg, DESCRIBE_EXTENSIONS,
    GREETING_WAIT,
};

use crate::launch::{greet, Connected, Greeted};

/// How long the daemon has to answer. It answers from memory, so this only
/// runs out on a connection that has gone quiet, an ssh one most likely.
const ANSWER_WAIT: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq)]
pub enum Ask {
    Guide,
    List,
    Show(String),
}

/// What `argus extension …` asks, from what followed `extension`.
pub fn parse(args: &[String]) -> Option<Ask> {
    match args {
        [guide] if guide == "guide" => Some(Ask::Guide),
        [list] if list == "list" => Some(Ask::List),
        [show, name] if show == "show" && !name.is_empty() => Some(Ask::Show(name.clone())),
        _ => None,
    }
}

pub async fn run(ask: Ask, host: Option<String>) -> anyhow::Result<()> {
    let name = match ask {
        Ask::Guide => {
            print!("{}", argus_protocol::extension_guide::guide());
            return Ok(());
        }
        Ask::List => None,
        Ask::Show(name) => Some(name),
    };
    let described = describe(host.as_deref()).await?;
    match name {
        None => print!("{}", listing(&described)),
        Some(name) => {
            let one = described.iter().find(|e| e.name == name).ok_or_else(|| not_running(&name, &described))?;
            print!("{}", shown(one));
        }
    }
    Ok(())
}

/// Every running extension, from the daemon here or on `host`.
async fn describe(host: Option<&str>) -> anyhow::Result<Vec<ExtDescription>> {
    let Connected { channels: (out, mut incoming), daemon, .. } = match host {
        Some(host) => crate::remote::connect(host).await?,
        None => connect_local().await?,
    };
    if !daemon.is_some_and(|d| d.can(DESCRIBE_EXTENSIONS)) {
        anyhow::bail!("argusd predates `argus extension`; `argus server restart` to update it");
    }
    out.send(ClientMsg::DescribeExtensions).context("argusd closed the connection")?;
    tokio::time::timeout(ANSWER_WAIT, described(&mut incoming))
        .await
        .context("argusd did not answer")?
        .context("argusd closed the connection before answering")
}

async fn connect_local() -> anyhow::Result<Connected> {
    if !transport::is_daemon_listening() {
        anyhow::bail!("argusd is not running, so no extension is");
    }
    let stream = transport::connect().await.context("could not connect to argusd")?;
    match greet(crate::wire::connection_channels(stream), GREETING_WAIT).await {
        Greeted::Up(connected) => Ok(connected),
        _ => anyhow::bail!("argusd closed the connection before saying anything"),
    }
}

/// The description, past the tree and anything else the daemon sends a
/// client that has just greeted.
async fn described(incoming: &mut tokio::sync::mpsc::Receiver<ServerMsg>) -> Option<Vec<ExtDescription>> {
    while let Some(msg) = incoming.recv().await {
        if let ServerMsg::ExtensionsDescribed(described) = msg {
            return Some(described);
        }
    }
    None
}

fn not_running(name: &str, described: &[ExtDescription]) -> anyhow::Error {
    let running: Vec<&str> = described.iter().map(|e| e.name.as_str()).collect();
    let running = if running.is_empty() { "none is".to_string() } else { format!("running: {}", running.join(", ")) };
    anyhow::anyhow!(
        "no extension {name} is running ({running}); a configured one that keeps exiting says why in \
         extension-{name}.log, in the daemon's config directory"
    )
}

fn plural(count: usize, one: &str) -> String {
    if count == 1 { format!("1 {one}") } else { format!("{count} {one}s") }
}

/// One line per running extension: its name, what it publishes, and what
/// it says it is.
pub fn listing(described: &[ExtDescription]) -> String {
    if described.is_empty() {
        return "no extension is running; `argus extension guide` says how to write and declare one\n".into();
    }
    let counts: Vec<String> = described
        .iter()
        .map(|e| format!("{}, {}", plural(e.views.len(), "view"), plural(e.commands.len(), "command")))
        .collect();
    let width = described.iter().map(|e| e.name.len()).max().unwrap_or(0);
    let counted = counts.iter().map(String::len).max().unwrap_or(0);
    let mut out = String::new();
    for (extension, counts) in described.iter().zip(&counts) {
        let summary = extension.about.as_ref().map_or("(says nothing of itself)", |a| a.summary.as_str());
        let _ = writeln!(out, "{:width$}  {counts:counted$}  {summary}", extension.name);
    }
    out
}

/// One extension whole, as Markdown: what it says it is, then each command
/// and view with what its keys, scope and actions mean.
pub fn shown(extension: &ExtDescription) -> String {
    let mut out = format!("# {}\n\n", extension.name);
    match &extension.about {
        Some(about) => {
            let _ = writeln!(out, "{}\n", about.summary);
            if let Some(guide) = about.guide.as_deref().filter(|g| !g.trim().is_empty()) {
                let _ = writeln!(out, "{}\n", guide.trim_end());
            }
        }
        None => out.push_str("It has not said what it is (`PUT about`).\n\n"),
    }

    out.push_str("## Commands\n\n");
    if extension.commands.is_empty() {
        out.push_str("None.\n");
    }
    for command in &extension.commands {
        let key = command.key.as_ref().map_or("no key".to_string(), |k| format!("key {}", k.spoken()));
        let scope = match &command.scope {
            CommandScope::Anywhere => "heard anywhere, and through the leader from a pane".to_string(),
            CommandScope::View(view) => format!("heard in its view `{view}`"),
            CommandScope::Checkout => "heard on a checkout row".to_string(),
            CommandScope::Pane => "heard on a pane row, and through the leader from a pane".to_string(),
        };
        let _ = writeln!(out, "- `{}`: {} ({key}; {scope})", command.name, command.title);
        if let Some(help) = &command.help {
            let _ = writeln!(out, "  {help}");
        }
    }

    out.push_str("\n## Views\n");
    if extension.views.is_empty() {
        out.push_str("\nNone.\n");
    }
    for view in &extension.views {
        let scope = match view.scope {
            ExtScope::Global => "shown everywhere".to_string(),
            ExtScope::Repository(id) => format!("shown in repository {}", id.0),
            ExtScope::Checkout(id) => format!("shown at checkout {}", id.0),
        };
        let place = match view.place {
            ExtPlace::Panel | ExtPlace::Right => "opens docked on the right",
            ExtPlace::Left => "opens docked on the left",
            ExtPlace::Bottom => "opens docked at the bottom",
            ExtPlace::Stage => "opens as a page",
        };
        let icon = view.icon.as_deref().map(|icon| format!("; icon `{icon}`")).unwrap_or_default();
        let _ = writeln!(out, "\n### `{}`: {}\n\n{scope}; {place}{icon}\n", view.name, view.title);
        let columns: Vec<String> = view
            .columns
            .iter()
            .map(|c| match (c.numeric, c.optional) {
                (true, true) => format!("{} (numeric, optional)", c.title),
                (true, false) => format!("{} (numeric)", c.title),
                (false, true) => format!("{} (optional)", c.title),
                (false, false) => c.title.clone(),
            })
            .collect();
        let _ = writeln!(out, "Columns: {}", columns.join(", "));
        if view.actions.is_empty() {
            continue;
        }
        out.push_str("\nActions:\n\n");
        for action in &view.actions {
            let key = action.key.as_deref().map_or("no key".to_string(), |k| format!("key {k}"));
            let on = if action.view_wide { "on the view" } else { "on the selected row" };
            let on = if view.default.as_ref() == Some(&action.id) { format!("{on}; the default") } else { on.to_string() };
            let confirm = action.confirm.as_deref().map(|q| format!("; asks {q:?} first")).unwrap_or_default();
            let _ = writeln!(out, "- `{}`: {} ({key}; {on}{confirm})", action.id, action.label);
            if let Some(help) = &action.help {
                let _ = writeln!(out, "  {help}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{Column, ExtAbout, ExtAction, ExtCommand, ExtKey, ExtView, RepositoryId};

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn docker() -> ExtDescription {
        ExtDescription {
            name: "docker".into(),
            about: Some(ExtAbout { summary: "Docker containers".into(), guide: Some("Press r to restart.".into()) }),
            commands: vec![ExtCommand {
                extension: "docker".into(),
                name: "up".into(),
                title: "Start a compose service".into(),
                key: Some(ExtKey { alt: true, ..ExtKey::plain("s") }),
                scope: CommandScope::Checkout,
                help: Some("Asks which service, then runs docker compose up -d.".into()),
            }],
            views: vec![ExtView {
                extension: "docker".into(),
                name: "containers".into(),
                title: "Containers".into(),
                scope: ExtScope::Repository(RepositoryId(4)),
                place: ExtPlace::Stage,
                icon: Some("box".into()),
                columns: vec![
                    Column { title: "Name".into(), numeric: false, optional: false },
                    Column { title: "CPU".into(), numeric: true, optional: true },
                ],
                rows: Vec::new(),
                actions: vec![ExtAction {
                    id: "restart".into(),
                    label: "Restart".into(),
                    key: Some("r".into()),
                    view_wide: false,
                    confirm: Some("Restart it?".into()),
                    help: Some("docker restart, on the selected container.".into()),
                }],
                default: Some("restart".into()),
            }],
        }
    }

    #[test]
    fn extension_reads_guide_list_and_show() {
        assert_eq!(parse(&args(&["guide"])), Some(Ask::Guide));
        assert_eq!(parse(&args(&["list"])), Some(Ask::List));
        assert_eq!(parse(&args(&["show", "docker"])), Some(Ask::Show("docker".into())));
        assert_eq!(parse(&args(&["show"])), None);
        assert_eq!(parse(&args(&["list", "x"])), None);
        assert_eq!(parse(&args(&[])), None);
    }

    #[test]
    fn a_listing_says_what_each_publishes_and_is() {
        let quiet = ExtDescription { name: "k8s".into(), about: None, commands: Vec::new(), views: Vec::new() };
        let text = listing(&[docker(), quiet]);
        assert_eq!(
            text,
            "docker  1 view, 1 command    Docker containers\n\
             k8s     0 views, 0 commands  (says nothing of itself)\n"
        );
        assert!(listing(&[]).starts_with("no extension is running"));
    }

    #[test]
    fn showing_one_prints_its_guide_commands_and_views() {
        let text = shown(&docker());
        assert!(text.starts_with("# docker\n\nDocker containers\n\nPress r to restart.\n"), "{text}");
        assert!(text.contains("- `up`: Start a compose service (key Alt S; heard on a checkout row)\n  Asks which"), "{text}");
        assert!(text.contains("### `containers`: Containers\n\nshown in repository 4; opens as a page; icon `box`"), "{text}");
        assert!(text.contains("Columns: Name, CPU (numeric, optional)"), "{text}");
        assert!(
            text.contains("- `restart`: Restart (key r; on the selected row; the default; asks \"Restart it?\" first)\n  docker restart"),
            "{text}"
        );
    }

    #[test]
    fn one_not_running_is_refused_with_those_that_are() {
        let error = not_running("k8s", &[docker()]).to_string();
        assert!(error.starts_with("no extension k8s is running (running: docker)"), "{error}");
        assert!(error.contains("extension-k8s.log"), "{error}");
    }

    #[tokio::test]
    async fn the_description_is_read_past_what_the_daemon_sends_first() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        tx.send(ServerMsg::Tree(Vec::new())).await.unwrap();
        tx.send(ServerMsg::ExtensionsDescribed(vec![docker()])).await.unwrap();
        assert_eq!(described(&mut rx).await, Some(vec![docker()]));
        drop(tx);
        assert_eq!(described(&mut rx).await, None, "a daemon gone answers nothing");
    }
}
