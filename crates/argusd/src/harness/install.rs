//! Writing Argus's hooks into a checkout, and taking them out again.
//!
//! Everything here edits a file the user also owns, so each write is
//! marked as ours and each removal touches only what carries the mark.

use super::*;

impl Harness {
    /// Puts whatever this harness needs into the checkout: a managed block
    /// in its settings file, its plugin module, a skill, or a rules file.
    ///
    /// A harness that needs nothing is the normal case, not a failure.
    pub fn install(
        &self,
        checkout: &Path,
        pane: PaneId,
        port: u16,
        token: &str,
    ) -> anyhow::Result<()> {
        let skill = self.install_skill(checkout);
        let settings = self.install_settings(checkout, pane, port, token);
        let plugin = self.install_plugin(checkout);
        let rule = self.install_rule(checkout);
        skill.and(settings).and(plugin).and(rule)
    }

    fn install_rule(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(rule_path) = &self.rule_file else {
            return Ok(());
        };
        let path = checkout.join(rule_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Cursor rules use `alwaysApply`; AGY and similar use `always_on`.
        let always = if path.extension().and_then(|e| e.to_str()) == Some("mdc") {
            "alwaysApply: true"
        } else {
            "always_on: true"
        };
        let content = format!(
            "---\ndescription: Argus pair-programming environment integration\n{always}\n---\n\n{}",
            self.instructions(checkout)
        );
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn install_plugin(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(plugin) = &self.plugin else {
            return Ok(());
        };
        let path = checkout.join(&plugin.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, plugin.source)?;
        Ok(())
    }

    fn install_settings(
        &self,
        checkout: &Path,
        pane: PaneId,
        port: u16,
        token: &str,
    ) -> anyhow::Result<()> {
        let Some(path) = self.settings_path(checkout) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut root = read_settings(&path);
        let root_obj = root.as_object_mut().expect("just normalized to an object");
        if let Some(version) = self.settings_version {
            root_obj.entry("version").or_insert_with(|| json!(version));
        }

        let hooks = root_obj
            .entry(self.hooks_key.clone())
            .or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        let hooks_obj = hooks.as_object_mut().expect("just normalized to an object");

        let command = helper_path();
        for event in &self.events {
            let entry = status_entry(
                &command,
                pane,
                port,
                token,
                event,
                self.command_string,
                self.bake_command,
            );
            match hooks_obj.get_mut(&event.name) {
                Some(existing) => {
                    remove_managed(existing);
                    self.shape.append(existing, entry, event.matcher.as_deref());
                }
                None => {
                    hooks_obj.insert(
                        event.name.clone(),
                        self.shape.wrap(entry, event.matcher.as_deref()),
                    );
                }
            }
        }
        if let Some(name) = &self.context_event {
            let entry = if self.command_string && !self.bake_command {
                // The helper reads the message from its inherited environment;
                // neither changing paths nor prose invalidates Codex hook trust.
                json!({
                    "type": "command",
                    "command": format!("\"$ARGUS_HOOK\" {INSTRUCTIONS_COMMAND}"),
                    "commandWindows": format!("\"%ARGUS_HOOK%\" {INSTRUCTIONS_COMMAND}"),
                    "timeout": 5
                })
            } else {
                say_entry(&command, &self.instructions(checkout))
            };
            match hooks_obj.get_mut(name) {
                Some(existing) => {
                    if !self.events.iter().any(|event| &event.name == name) {
                        remove_managed(existing);
                    }
                    self.shape.append(existing, entry, None)
                }
                None => {
                    hooks_obj.insert(name.clone(), self.shape.wrap(entry, None));
                }
            }
        }

        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
        Ok(())
    }

    /// Removes everything [`install`] put in the checkout, leaving anything
    /// the user put there untouched.
    pub fn uninstall(&self, checkout: &Path) -> anyhow::Result<()> {
        let settings = self.uninstall_settings(checkout);
        let plugin = self.uninstall_plugin(checkout);
        let rule = self.uninstall_rule(checkout);
        let skill = self.uninstall_skill(checkout);
        settings.and(plugin).and(rule).and(skill)
    }

    fn uninstall_rule(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(rule_path) = &self.rule_file else {
            return Ok(());
        };
        let path = checkout.join(rule_path);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            prune_empty_dirs(checkout, &path);
        }
        Ok(())
    }

    /// Deletes the plugin module, and any directory Argus made only to hold
    /// it. Only ever removes a file that still carries our marker: a user
    /// who replaced it with one of their own keeps it.
    fn uninstall_plugin(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(plugin) = &self.plugin else {
            return Ok(());
        };
        let path = checkout.join(&plugin.path);
        match std::fs::read_to_string(&path) {
            Ok(body) if body.contains(PLUGIN_MARKER) => std::fs::remove_file(&path)?,
            // Missing, unreadable, or someone else's. Nothing to do either
            // way, and none of it is worth failing startup over.
            _ => return Ok(()),
        }
        prune_empty_dirs(checkout, &path);
        Ok(())
    }

    /// Removes Argus's managed hook block, leaving anything the user put in
    /// the same file untouched. Cleans up after itself as it goes: an
    /// emptied hooks key is dropped, and a settings file left with nothing
    /// in it at all is deleted rather than left behind as `{}`.
    fn uninstall_settings(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(path) = self.settings_path(checkout) else {
            return Ok(());
        };
        if !path.exists() {
            return Ok(());
        }

        let mut root = read_settings(&path);
        let root_obj = root.as_object_mut().expect("just normalized to an object");

        let mut removed = false;
        if let Some(hooks) = root_obj
            .get_mut(&self.hooks_key)
            .and_then(Value::as_object_mut)
        {
            for event in self.managed_events() {
                // Only drop an entry we recognize as ours. A user who wrote
                // their own Stop hook keeps it.
                if let Some(value) = hooks.get_mut(event) {
                    removed |= remove_managed(value);
                    if value.as_array().is_some_and(Vec::is_empty) {
                        hooks.remove(event);
                    }
                }
            }
            if hooks.is_empty() {
                root_obj.remove(&self.hooks_key);
            }
        }
        if !removed {
            return Ok(());
        }

        // Cursor's schema leaves `version` behind after hooks are gone; a
        // file that is only that key is litter from us, not a user setting.
        let only_schema_version = self.settings_version.is_some()
            && root_obj.len() == 1
            && root_obj.contains_key("version");
        if root_obj.is_empty() || only_schema_version {
            std::fs::remove_file(&path)?;
            prune_empty_dirs(checkout, &path);
        } else {
            std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
        }
        Ok(())
    }
}

/// Removes the directories a just-deleted file was the last thing in,
/// walking up until something stops it or the checkout itself is reached.
///
/// `remove_dir` refuses a directory with anything in it, which is the whole
/// safety argument: a `.claude/` the user keeps their own settings in, or an
/// `.opencode/` holding their agents, fails on the first step and stays.
pub(super) fn prune_empty_dirs(checkout: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == checkout || std::fs::remove_dir(d).is_err() {
            return;
        }
        dir = d.parent();
    }
}

impl Shape {
    fn wrap(self, entry: Value, matcher: Option<&str>) -> Value {
        match self {
            Shape::Matcher => {
                let mut group = json!({ "hooks": [entry] });
                if let Some(matcher) = matcher {
                    group["matcher"] = Value::String(matcher.to_string());
                }
                json!([group])
            }
            Shape::Flat => json!([entry]),
        }
    }

    fn append(self, existing: &mut Value, entry: Value, matcher: Option<&str>) {
        let Value::Array(mut addition) = self.wrap(entry, matcher) else {
            unreachable!()
        };
        if let Some(items) = existing.as_array_mut() {
            items.append(&mut addition);
        } else {
            *existing = Value::Array(addition);
        }
    }
}

pub(super) fn status_entry(
    command: &str,
    pane: PaneId,
    port: u16,
    token: &str,
    event: &Event,
    command_string: bool,
    bake_command: bool,
) -> Value {
    let mut args = vec![event_target_url(pane, port, event), token.to_string()];
    push_event_flags(&mut args, event);
    if command_string {
        let line = if bake_command {
            baked_command_line(command, pane, port, token, event)
        } else {
            // Codex: env form keeps the on-disk trust hash stable.
            env_command_line(event, false)
        };
        let mut entry = json!({
            "type": "command",
            "command": line,
            "timeout": 5
        });
        if !bake_command {
            entry["commandWindows"] = json!(env_command_line(event, true));
        }
        return entry;
    }
    json!({
        "type": "command",
        "command": command,
        "args": args,
        "timeout": 5
    })
}

/// What a harness's plugin module is called in the checkout. Prefixed so
/// it sorts and reads as ours in a directory the user also keeps their own
/// plugins in.
pub(super) const PLUGIN_FILE: &str = "argus-status.js";

/// The string that identifies a module as one Argus wrote, on its first
/// line. Only a file still carrying it is ours to delete, so a user who
/// replaces it with their own keeps it through an uninstall.
pub(super) const PLUGIN_MARKER: &str = "argus:managed-plugin";

/// A hook that only prints. `say` needs no daemon and no network, so the
/// context an agent starts with doesn't depend on the status port being up.
pub(super) fn say_entry(command: &str, text: &str) -> Value {
    json!({
        "type": "command",
        "command": command,
        "args": ["say", text],
        "timeout": 5
    })
}

/// Parses a settings file into an object, normalizing anything unexpected
/// (missing, corrupt, or not a JSON object) to `{}` — this file is the
/// user's, so a broken one must not stop an agent from spawning.
pub(super) fn read_settings(path: &Path) -> Value {
    let mut root: Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if !root.is_object() {
        root = json!({});
    }
    root
}

pub(super) fn is_managed_item(item: &Value) -> bool {
    match item.get("hooks") {
        Some(Value::Array(inner)) => !inner.is_empty() && inner.iter().all(names_helper),
        _ => names_helper(item),
    }
}

pub(super) fn remove_managed(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let before = items.len();
    items.retain(|item| !is_managed_item(item));
    items.len() != before
}

pub(super) fn names_helper(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| {
            is_hook_helper(command)
                || command.contains("argus-hook")
                || command.contains("orion-hook")
                || command.contains("ARGUS_HOOK")
        })
}

/// Matches our helper by file name, so a block written by a daemon that
/// lived somewhere else on disk — an older build, a different target dir —
/// is still recognized as ours and cleaned up.
pub(super) fn is_hook_helper(command: &str) -> bool {
    let stem = crate::editor::program_stem(command);
    // `orion-hook` is the pre-rename name. A block naming it is still ours,
    // and still fires on every turn until something removes it.
    stem == "argus-hook" || stem == "orion-hook"
}
