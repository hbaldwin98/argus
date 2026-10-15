//! What the running extensions have published — their views, commands and
//! open dialogs — and what the person does with them on its way back.
//!
//! The daemon owns extensions and the clients only draw them: under
//! `argus --host` only the daemon there is beside what an extension
//! watches, and every client draws the same view. A view changes only when
//! its extension publishes it again, so an action a client sends is
//! forwarded and nothing else; the client never predicts what it did.
//!
//! Only an extension the daemon admitted may publish, with the token it was
//! handed for its own name, so one extension cannot speak as another. While
//! one is down its tabs, keys and open dialogs are gone.

use std::collections::{BTreeMap, HashSet, VecDeque};

use argus_protocol::extension_keys::{action_key_refusal, command_key_refusal};
use argus_protocol::extensions::{MAX_COMMANDS, MAX_ITEMS, MAX_VIEWS};
use argus_protocol::{
    CommandScope, ExtAnswer, ExtCommand, ExtDialog, ExtDialogKind, ExtEvent, ExtPane, ExtView, Refusals,
    Refused,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::*;

/// The longest notice an extension may put on the status bar, which has one
/// line for it.
const MAX_NOTICE: usize = 300;

/// How many commands an extension is remembered to have been sent, for a
/// dialog answering one to find the client that ran it.
const REMEMBERED_RUNS: usize = 16;

#[derive(Default)]
pub(super) struct Registry {
    extensions: BTreeMap<String, Extension>,
    /// Names dialogs, commands sent and event streams, daemon-wide.
    next_id: u64,
    /// The clients that can draw a dialog.
    askers: HashSet<ViewerId>,
}

struct Extension {
    token: String,
    views: Vec<ExtView>,
    commands: Vec<ExtCommand>,
    /// The stream its events go down, by the generation of the connection
    /// holding it: the newest replaces an older one, as a pane's inbox does.
    events: Option<(u64, UnboundedSender<ExtEvent>)>,
    dialogs: Vec<OpenDialog>,
    /// The commands it was sent lately, and the client each was run in.
    runs: VecDeque<(u64, ViewerId)>,
}

struct OpenDialog {
    id: u64,
    viewer: ViewerId,
    kind: ExtDialogKind,
}

/// What a connection is told when an extension changes something. Views
/// and commands are read whole when told; a dialog names the one client it
/// is for, and every other connection drops it.
#[derive(Debug, Clone)]
pub enum ExtensionNews {
    Views,
    Commands,
    Notice { extension: String, text: String },
    Dialog { viewer: ViewerId, msg: ServerMsg },
    Withdrawn { viewer: ViewerId, id: u64 },
}

impl Registry {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn get(&mut self, name: &str) -> anyhow::Result<&mut Extension> {
        self.extensions.get_mut(name).ok_or_else(|| anyhow::anyhow!("no extension {name} is running"))
    }
}

impl Daemon {
    pub fn subscribe_extensions(&self) -> broadcast::Receiver<ExtensionNews> {
        self.extensions_tx.subscribe()
    }

    fn tell_extensions(&self, news: ExtensionNews) {
        let _ = self.extensions_tx.send(news);
    }

    /// Lets `name` publish, with a token of its own, which it is handed
    /// when started. Admitting a name again — a restart — starts it from
    /// nothing, as a new process holds nothing of the old one's.
    pub fn admit_extension(&self, name: &str) -> String {
        self.retire_extension(name);
        let token = gen_token();
        let extension = Extension {
            token: token.clone(),
            views: Vec::new(),
            commands: Vec::new(),
            events: None,
            dialogs: Vec::new(),
            runs: VecDeque::new(),
        };
        self.extensions.lock().unwrap().extensions.insert(name.to_string(), extension);
        token
    }

    /// Takes everything `name` published off every client, its open
    /// dialogs with it: an extension that is down shows nothing.
    pub fn retire_extension(&self, name: &str) {
        let Some(gone) = self.extensions.lock().unwrap().extensions.remove(name) else {
            return;
        };
        for dialog in gone.dialogs {
            self.tell_extensions(ExtensionNews::Withdrawn { viewer: dialog.viewer, id: dialog.id });
        }
        if !gone.views.is_empty() {
            self.tell_extensions(ExtensionNews::Views);
        }
        if !gone.commands.is_empty() {
            self.tell_extensions(ExtensionNews::Commands);
        }
    }

    /// Whether `bearer` is the token `name` was admitted with.
    pub(super) fn extension_authorized(&self, name: &str, bearer: Option<&str>) -> bool {
        let registry = self.extensions.lock().unwrap();
        match (registry.extensions.get(name), bearer) {
            (Some(extension), Some(bearer)) => extension.token == bearer,
            _ => false,
        }
    }

    /// Every view every extension publishes, in the order of their names.
    pub fn extension_views(&self) -> Vec<ExtView> {
        let registry = self.extensions.lock().unwrap();
        registry.extensions.values().flat_map(|e| e.views.iter().cloned()).collect()
    }

    pub fn extension_commands(&self) -> Vec<ExtCommand> {
        let registry = self.extensions.lock().unwrap();
        registry.extensions.values().flat_map(|e| e.commands.iter().cloned()).collect()
    }

    /// Replaces every view `name` publishes, or none of them when one breaks
    /// a limit, which is refused by its path (`views[1].rows[3][0]`). An
    /// action key the view keeps, or one already taken in the view, is
    /// dropped from the action and said in the answer; the action is kept.
    pub fn publish_views(&self, name: &str, views: Vec<ExtView>) -> anyhow::Result<Refusals> {
        if views.len() > MAX_VIEWS {
            anyhow::bail!("views: at most {MAX_VIEWS}");
        }
        let mut checked: Vec<ExtView> = Vec::with_capacity(views.len());
        for (i, view) in views.into_iter().enumerate() {
            let view = view.checked().map_err(|e| anyhow::anyhow!("views[{i}].{e}"))?;
            if checked.iter().any(|v| v.name == view.name) {
                anyhow::bail!("views[{i}].name: {:?} is another view's", view.name);
            }
            checked.push(view);
        }
        let mut views = checked;
        let mut refusals = Refusals::default();
        let mut registry = self.extensions.lock().unwrap();
        let extension = registry.get(name)?;
        for view in &mut views {
            view.extension = name.to_string();
            let mut taken: Vec<String> = Vec::new();
            for action in &mut view.actions {
                let Some(key) = action.key.clone() else { continue };
                let commanded = extension.commands.iter().find(|c| {
                    c.scope == CommandScope::View(view.name.clone())
                        && c.key.as_ref().is_some_and(|k| !k.ctrl && !k.alt && k.key == key)
                });
                let reason = action_key_refusal(&key)
                    .or_else(|| taken.contains(&key).then(|| format!("{key} is another action's in {}", view.name)))
                    .or_else(|| commanded.map(|c| format!("{key} is the command {}'s", c.title)));
                match reason {
                    Some(reason) => {
                        action.key = None;
                        refusals.refused.push(Refused { name: format!("{}/{}", view.name, action.id), reason });
                    }
                    None => taken.push(key),
                }
            }
        }
        extension.views = views;
        drop(registry);
        self.tell_refusals(name, &refusals);
        self.tell_extensions(ExtensionNews::Views);
        Ok(refusals)
    }

    /// Replaces every command `name` adds. A key Argus keeps, another
    /// extension's, or one of its own view's actions' is refused with the
    /// reason, and the command is kept without it.
    pub fn publish_commands(&self, name: &str, mut commands: Vec<ExtCommand>) -> anyhow::Result<Refusals> {
        if commands.len() > MAX_COMMANDS {
            anyhow::bail!("at most {MAX_COMMANDS} commands");
        }
        for (i, command) in commands.iter().enumerate() {
            if command.name.is_empty() || commands[..i].iter().any(|c| c.name == command.name) {
                anyhow::bail!("each command needs a name of its own, not {:?}", command.name);
            }
        }
        let mut refusals = Refusals::default();
        let mut registry = self.extensions.lock().unwrap();
        let others: Vec<ExtCommand> = registry
            .extensions
            .iter()
            .filter(|(other, _)| other.as_str() != name)
            .flat_map(|(_, e)| e.commands.iter().cloned())
            .collect();
        let extension = registry.get(name)?;
        for i in 0..commands.len() {
            commands[i].extension = name.to_string();
            let Some(key) = commands[i].key.clone() else { continue };
            let earlier = others.iter().chain(&commands[..i]).find(|c| c.key.as_ref().is_some_and(|k| k.same_press(&key)));
            let action = match &commands[i].scope {
                CommandScope::View(view) if !key.ctrl && !key.alt => extension
                    .views
                    .iter()
                    .filter(|v| &v.name == view)
                    .flat_map(|v| &v.actions)
                    .find(|a| a.key.as_deref() == Some(key.key.as_str())),
                _ => None,
            };
            let reason = command_key_refusal(&key)
                .or_else(|| earlier.map(|c| format!("{} is {}'s {}", key.spoken(), c.extension, c.title)))
                .or_else(|| action.map(|a| format!("{} is the action {}'s", key.spoken(), a.label)));
            if let Some(reason) = reason {
                commands[i].key = None;
                refusals.refused.push(Refused { name: commands[i].name.clone(), reason });
            }
        }
        extension.commands = commands;
        drop(registry);
        self.tell_refusals(name, &refusals);
        self.tell_extensions(ExtensionNews::Commands);
        Ok(refusals)
    }

    /// A refused key is said on every status bar as well as in the answer:
    /// the person is the one left wondering why the key does nothing.
    fn tell_refusals(&self, name: &str, refusals: &Refusals) {
        for refused in &refusals.refused {
            self.tell_extensions(ExtensionNews::Notice {
                extension: name.to_string(),
                text: format!("no key for {}: {}", refused.name, refused.reason),
            });
        }
    }

    pub fn extension_notice(&self, name: &str, text: &str) -> anyhow::Result<()> {
        self.extensions.lock().unwrap().get(name)?;
        let line = text.lines().next().unwrap_or("").trim();
        let text: String = line.chars().take(MAX_NOTICE).collect();
        self.tell_extensions(ExtensionNews::Notice { extension: name.to_string(), text });
        Ok(())
    }

    /// Opens `name`'s event stream: the generation naming this connection,
    /// and where its events arrive.
    pub(super) fn open_extension_events(&self, name: &str) -> Option<(u64, UnboundedReceiver<ExtEvent>)> {
        let mut registry = self.extensions.lock().unwrap();
        let generation = registry.id();
        let extension = registry.extensions.get_mut(name)?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        extension.events = Some((generation, tx));
        Some((generation, rx))
    }

    /// Whether `name` holds its event stream open, for a test to wait on.
    #[cfg(test)]
    pub(super) fn extension_listening(&self, name: &str) -> bool {
        let registry = self.extensions.lock().unwrap();
        registry.extensions.get(name).is_some_and(|e| e.events.is_some())
    }

    /// Closes the stream a connection opened, unless a newer one replaced
    /// it.
    pub(super) fn close_extension_events(&self, name: &str, generation: u64) {
        let mut registry = self.extensions.lock().unwrap();
        if let Some(extension) = registry.extensions.get_mut(name) {
            if extension.events.as_ref().is_some_and(|(g, _)| *g == generation) {
                extension.events = None;
            }
        }
    }

    /// Hands `event` to the extension, or says it is not listening, which
    /// is the person's to hear rather than an action lost in silence.
    fn send_extension(extension: &Extension, name: &str, event: ExtEvent) -> anyhow::Result<()> {
        let sent = extension.events.as_ref().is_some_and(|(_, tx)| tx.send(event).is_ok());
        anyhow::ensure!(sent, "{name} is not listening");
        Ok(())
    }

    /// An action taken in a view, forwarded as it stands. One the view does
    /// not offer, or a row it does not hold, is refused: a client a publish
    /// behind cannot act on what the extension has since taken away.
    pub fn extension_action(&self, name: &str, view: &str, row: Option<String>, action: &str) -> anyhow::Result<()> {
        let mut registry = self.extensions.lock().unwrap();
        let extension = registry.get(name)?;
        let shown = extension
            .views
            .iter()
            .find(|v| v.name == view)
            .ok_or_else(|| anyhow::anyhow!("{name} shows no view {view}"))?;
        let offered = shown
            .actions
            .iter()
            .find(|a| a.id == action)
            .ok_or_else(|| anyhow::anyhow!("{view} offers no {action}"))?;
        let row = if offered.view_wide {
            None
        } else {
            let row = row.ok_or_else(|| anyhow::anyhow!("{} needs a row", offered.label))?;
            anyhow::ensure!(shown.rows.iter().any(|r| r.key == row), "{view} has no row {row}");
            Some(row)
        };
        let event = ExtEvent::Action { view: view.to_string(), action: action.to_string(), row };
        Self::send_extension(extension, name, event)
    }

    /// A command run in `viewer`, with what it was showing read against the
    /// tree. The id it is sent with is what a dialog answering it names.
    pub fn extension_command(&self, viewer: ViewerId, name: &str, command: &str, context: &View) -> anyhow::Result<()> {
        let context = self.read_view(context);
        let mut registry = self.extensions.lock().unwrap();
        let id = registry.id();
        let extension = registry.get(name)?;
        anyhow::ensure!(extension.commands.iter().any(|c| c.name == command), "{name} has no command {command}");
        let event = ExtEvent::Command { command: command.to_string(), id, context: Some(Box::new(context)) };
        Self::send_extension(extension, name, event)?;
        extension.runs.push_back((id, viewer));
        if extension.runs.len() > REMEMBERED_RUNS {
            extension.runs.pop_front();
        }
        Ok(())
    }

    /// Whether the client `viewer` can draw a dialog, said once it greets.
    pub fn extension_asker(&self, viewer: ViewerId, asks: bool) {
        let mut registry = self.extensions.lock().unwrap();
        if asks {
            registry.askers.insert(viewer);
        } else {
            registry.askers.remove(&viewer);
        }
    }

    /// A client went: its dialogs are withdrawn, and each extension that
    /// asked one is told it was not answered.
    pub fn forget_extension_viewer(&self, viewer: ViewerId) {
        let mut registry = self.extensions.lock().unwrap();
        registry.askers.remove(&viewer);
        for (name, extension) in &mut registry.extensions {
            let (gone, kept): (Vec<_>, Vec<_>) = extension.dialogs.drain(..).partition(|d| d.viewer == viewer);
            extension.dialogs = kept;
            for dialog in gone {
                let _ = Self::send_extension(extension, name, ExtEvent::Cancel { dialog: dialog.id });
            }
        }
    }

    /// Puts `dialog` to one client: the one that ran the command it
    /// answers, else the one the person was last at that can draw it.
    /// Refused when no client can.
    pub fn raise_dialog(&self, name: &str, dialog: ExtDialog) -> anyhow::Result<u64> {
        let items = match &dialog.kind {
            ExtDialogKind::Pick { items, .. } => items.len(),
            ExtDialogKind::Form { fields } => fields.len(),
            _ => 0,
        };
        anyhow::ensure!(items <= MAX_ITEMS, "at most {MAX_ITEMS} items");
        let recent = self.viewers_by_recency();
        let mut registry = self.extensions.lock().unwrap();
        let id = registry.id();
        let askers = registry.askers.clone();
        let extension = registry.get(name)?;
        let ran = dialog
            .answers
            .and_then(|command| extension.runs.iter().find(|(id, _)| *id == command))
            .map(|(_, viewer)| *viewer)
            .filter(|viewer| askers.contains(viewer));
        let focused = recent.into_iter().find(|viewer| askers.contains(viewer));
        let any = askers.iter().min_by_key(|viewer| viewer.0).copied();
        let viewer = ran.or(focused).or(any).ok_or_else(|| anyhow::anyhow!("no window is open to ask"))?;
        extension.dialogs.push(OpenDialog { id, viewer, kind: dialog.kind.clone() });
        drop(registry);
        let msg = ServerMsg::ExtensionDialog { extension: name.to_string(), id, dialog, prompted: ran.is_some() };
        self.tell_extensions(ExtensionNews::Dialog { viewer, msg });
        Ok(id)
    }

    /// The answer `viewer` gave, or `None` for a dialog put away. Only the
    /// client it was put to may answer it, and only in its own kind.
    pub fn answer_dialog(&self, viewer: ViewerId, dialog: u64, answer: Option<ExtAnswer>) -> anyhow::Result<()> {
        let mut registry = self.extensions.lock().unwrap();
        let (name, extension) = registry
            .extensions
            .iter_mut()
            .find(|(_, e)| e.dialogs.iter().any(|d| d.id == dialog && d.viewer == viewer))
            .ok_or_else(|| anyhow::anyhow!("that question is no longer asked"))?;
        let at = extension.dialogs.iter().position(|d| d.id == dialog).expect("found above");
        if let Some(answer) = &answer {
            anyhow::ensure!(answer.fits(&extension.dialogs[at].kind), "that does not answer the question asked");
        }
        extension.dialogs.remove(at);
        let event = match answer {
            Some(answer) => ExtEvent::Answer { dialog, answer },
            None => ExtEvent::Cancel { dialog },
        };
        Self::send_extension(extension, name, event)
    }

    /// A pane an extension asked for: a shell on the checkout it names, or
    /// a console, with `run` typed in as though by the person — the spawn
    /// any client asks for, so what it starts is a pane like any other.
    pub fn open_extension_pane(self: &Arc<Self>, name: &str, pane: ExtPane) -> anyhow::Result<PaneId> {
        self.extensions.lock().unwrap().get(name)?;
        let id = match pane.checkout {
            Some(checkout) => self.spawn_shell(checkout)?,
            None => self.spawn_console()?,
        };
        if let Some(run) = pane.run.filter(|run| !run.trim().is_empty()) {
            self.write_pane(id, format!("{}\r", run.trim()).as_bytes())?;
        }
        Ok(id)
    }
}
