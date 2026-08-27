//! Pickers, overlays, prompts, and settings — the transient surfaces that
//! sit in front of the columns.
//!
//! Each opens with the rows it will offer already resolved, rather than
//! looking them up as it draws: the tree can change underneath an open
//! picker, and a list that reshuffles between the keypress that moved the
//! cursor and the one that confirms is how you act on the wrong row.

use super::*;

impl App {
    /// Opens the directory browser at wherever the daemon thinks a browse
    /// should start. The picker goes up straight away, empty: waiting for
    /// the first listing before showing anything would make `n` look like
    /// it had missed the keystroke.
    fn browse_for(&mut self, target: DirTarget) {
        let request = self.next_browse_request;
        self.next_browse_request += 1;
        self.dir_picker = Some(DirPicker::new(target, request));
        let _ = self.out.send(ClientMsg::ListDirectories {
            request_id: request,
            path: String::new(),
        });
    }

    pub(super) fn move_picker(&mut self, delta: isize) {
        let Some(p) = &mut self.picker else { return };
        let last = p.len().saturating_sub(1);
        p.sel = (p.sel as isize + delta).clamp(0, last as isize) as usize;
    }

    /// Puts a review up directly, for tests that must not disturb the
    /// column selection by navigating to it.
    #[cfg(test)]
    pub fn review_for_test(&mut self, checkout: CheckoutId) {
        self.review_wanted = Some((checkout, 1));
    }

    pub fn open_settings(&mut self) {
        self.overlay = Some(Overlay::Settings { sel: 0 });
        self.focus = Focus::Overlay;
    }

    /// Folds the projects column away to a thin strip, or brings it back.
    /// The other four columns absorb the reclaimed width, so collapsing is a
    /// way to give the tree and live view more room rather than a way to
    /// lose the project you are in — the breadcrumb still names it. Stored
    /// on `settings` so the choice survives a restart.
    pub fn toggle_projects_collapsed(&mut self) {
        self.projects_collapsed = !self.projects_collapsed;
        self.settings.projects_collapsed = self.projects_collapsed;
        if self.persist_settings {
            crate::settings::save(&self.settings);
        }
        if self.projects_collapsed && self.focus == Focus::Projects {
            // The collapsed strip is not a focus target, so the cursor has
            // to land somewhere the keys still mean something.
            self.focus = Focus::Repositories;
            self.clamp();
        }
        if self.projects_collapsed {
            self.report("collapsed the projects column — p to expand");
        } else {
            self.report("expanded the projects column");
        }
    }

    pub(super) fn move_setting(&mut self, delta: isize) {
        if let Some(Overlay::Settings { sel }) = &mut self.overlay {
            let last = Setting::ALL.len() as isize - 1;
            *sel = (*sel as isize + delta).clamp(0, last) as usize;
        }
    }

    /// Changing a setting applies it at once and writes it out — there is
    /// no separate save, so there is nothing to forget to press.
    pub(super) fn cycle_setting(&mut self, sel: usize, delta: isize) {
        match Setting::ALL.get(sel) {
            Some(Setting::EditorCmd) => {
                self.prompt = Some(Prompt::EditorCommand {
                    input: self.settings.editor_cmd.clone(),
                });
                return;
            }
            Some(Setting::Editor) => {
                self.settings.editor = if delta > 0 {
                    self.settings.editor.next()
                } else {
                    self.settings.editor.prev()
                };
            }
            Some(Setting::Theme) => {
                let themes = crate::theme::THEMES;
                let here = themes
                    .iter()
                    .position(|t| *t == self.settings.theme)
                    .unwrap_or(0) as isize;
                let n = themes.len() as isize;
                let next = ((here + delta) % n + n) % n;
                self.settings.theme = themes[next as usize].to_string();
                self.theme = Theme::by_name(&self.settings.theme);
            }
            Some(Setting::Notifications) => {
                self.settings.notifications = self.settings.notifications.step(delta);
            }
            None => return,
        }
        if self.persist_settings {
            crate::settings::save(&self.settings);
        }
    }

    /// Opens `pane` in a floating window alongside whatever the columns
    /// are showing. `ephemeral` panes are killed when the window closes.
    pub fn open_overlay_pane(&mut self, pane: PaneId, title: String, ephemeral: bool) {
        self.pane_fullscreen = false;
        self.overlay = Some(Overlay::Pane {
            pane,
            title,
            ephemeral,
        });
        self.focus = Focus::Overlay;
        self.sync_subscription();
    }

    pub fn close_overlay(&mut self) {
        // An editor is its window. Nothing lists it once the window is
        // gone, so leaving it running would strand the process.
        if let Some(Overlay::Pane {
            pane,
            ephemeral: true,
            ..
        }) = self.overlay
        {
            let _ = self.out.send(ClientMsg::Kill { pane });
        }
        self.review = None;
        self.review_wanted = None;
        self.history = None;
        self.history_wanted = None;
        self.pending_history_file = None;
        self.overlay = None;
        self.leader_pending = false;
        self.pane_fullscreen = false;
        self.focus = match self.focus {
            Focus::Overlay => Focus::Panes,
            // A diff was opened from a checkout, so that is where closing
            // it puts you back.
            Focus::Review => Focus::Checkouts,
            other => other,
        };
        self.sync_subscription();
    }

    pub(super) fn open_theme_picker(&mut self) {
        let here = self.theme.name();
        self.picker = Some(Picker::new(
            PickerKind::Theme,
            "theme",
            crate::theme::THEMES.iter().map(|t| t.to_string()).collect(),
            crate::theme::THEMES
                .iter()
                .position(|t| *t == here)
                .unwrap_or(0),
        ));
    }

    /// `n` is contextual on which column has focus: a new project (any
    /// directory, not just preconfigured ones) from the projects column, a
    /// repository added to the selected project from the repositories
    /// column, or a new worktree branched off the selected checkout from
    /// the checkouts column. No-op elsewhere — there's no "current
    /// checkout" to branch from once you're inside the panes/content
    /// columns.
    pub(super) fn new_prompt(&mut self) {
        match self.focus {
            Focus::Projects => self.browse_for(DirTarget::Project),
            Focus::Repositories => {
                if let Some(p) = self.current_project() {
                    self.browse_for(DirTarget::Repository(p.id));
                }
            }
            Focus::Checkouts => {
                if let Some(c) = self.current_checkout() {
                    self.prompt = Some(Prompt::NewWorktree {
                        base: c.id,
                        input: String::new(),
                    });
                } else if let Some(branch) = self.current_branch_row().map(str::to_string) {
                    // The name is not the question here — this branch is —
                    // so there is nothing to prompt for.
                    let Some(base) = self.primary_checkout().map(|c| c.id) else {
                        return;
                    };
                    let _ = self.out.send(ClientMsg::CreateWorktree {
                        checkout: base,
                        branch,
                    });
                }
            }
            _ => {}
        }
    }

    /// Opens a confirmation to remove whatever the focused column selects,
    /// the way `n` adds to whatever it is focused on: a project, one of its
    /// repositories, or a checkout. No-op in the pane columns — a pane is
    /// killed with `x`, not removed.
    ///
    /// Removing a checkout is refused client-side for the primary one — the
    /// repo the user already had, not Argus's to delete — so there's no
    /// round-trip just to be told no (the daemon refuses it too, as defense
    /// in depth).
    pub(super) fn remove_prompt(&mut self) {
        let (target, label) = match self.focus {
            Focus::Projects => {
                let Some(p) = self.current_project() else {
                    return;
                };
                (RemoveTarget::Project(p.id), p.name.clone())
            }
            Focus::Repositories => {
                let Some(r) = self.current_repository() else {
                    return;
                };
                (RemoveTarget::Repository(r.id), r.name.clone())
            }
            // Deleting a remote branch is a push, which is not what `D`
            // does anywhere else in this column.
            Focus::Checkouts if self.current_remote_row().is_some() => {
                self.report("that branch is on the remote; nothing here deletes it");
                return;
            }
            // A branch row has no directory to remove, so `D` there is
            // about the branch itself.
            Focus::Checkouts if self.current_branch_row().is_some() => {
                let branch = self.current_branch_row().unwrap().to_string();
                let Some(checkout) = self.primary_checkout().map(|c| c.id) else {
                    self.report("no checkout to delete the branch from");
                    return;
                };
                (
                    RemoveTarget::Branch {
                        checkout,
                        branch: branch.clone(),
                    },
                    branch,
                )
            }
            Focus::Checkouts => {
                let Some(c) = self.current_checkout() else {
                    return;
                };
                if c.primary {
                    self.report("can't remove the primary checkout");
                    return;
                }
                (RemoveTarget::Checkout(c.id), c.name.clone())
            }
            _ => return,
        };
        self.prompt = Some(Prompt::ConfirmRemove { target, label });
    }

    pub(super) fn open_picker(&mut self) {
        if self.templates.is_empty() || self.current_checkout().is_none() {
            return;
        }
        self.picker = Some(Picker::new(
            PickerKind::Agent,
            "spawn agent",
            self.templates.clone(),
            0,
        ));
    }

    /// `w` switches workspace — the scope of the whole project column, and
    /// daemon-global, so every attached client follows. Opens on the one
    /// that's already open rather than the top of the list, since "look at
    /// where I am, then move" is the usual reason to press it.
    ///
    /// It opens on a single workspace too, because typing a name here is
    /// how a second one comes to exist: the alternative was hand-editing
    /// `projects.toml`, which meant the zero-config install stayed at one
    /// workspace forever.
    pub(super) fn open_workspace_picker(&mut self) {
        if self.workspaces.is_empty() {
            // Only before the daemon's first message; there is always at
            // least `default`.
            self.report("no workspaces yet");
            return;
        }
        let sel = self.workspaces.iter().position(|w| w.open).unwrap_or(0);
        let items = self
            .workspaces
            .iter()
            .map(|w| {
                let panes = if w.panes > 0 {
                    format!("  {}▣", w.panes)
                } else {
                    String::new()
                };
                format!("{}  {}⑂{}", w.name, w.projects, panes)
            })
            .collect();
        self.picker = Some(Picker::new(
            PickerKind::Workspace {
                ids: self.workspaces.iter().map(|w| w.id).collect(),
                names: self.workspaces.iter().map(|w| w.name.clone()).collect(),
            },
            "open workspace",
            items,
            sel,
        ));
    }

    /// `b` asks the daemon for this checkout's branches; the picker opens
    /// when they arrive, so it never shows a stale list.
    pub(super) fn open_branch_picker(&mut self) {
        let Some(id) = self.current_checkout().map(|c| c.id) else {
            self.report("no checkout selected");
            return;
        };
        self.list_wanted = Some(id);
        let _ = self.out.send(ClientMsg::ListBranches { checkout: id });
    }

    pub(super) fn open_file_picker(&mut self) {
        let Some(id) = self.current_checkout().map(|c| c.id) else {
            self.report("no checkout selected");
            return;
        };
        self.list_wanted = Some(id);
        let _ = self.out.send(ClientMsg::ListFiles { checkout: id });
    }

    /// The changed files of the review that is already open — no round
    /// trip, since the diff is in hand.
    pub(super) fn open_change_picker(&mut self) {
        let Some(view) = &self.review else { return };
        let items: Vec<String> = view
            .review
            .files
            .iter()
            .map(|f| format!("{} {}", f.kind.marker(), f.path))
            .collect();
        if items.is_empty() {
            return;
        }
        self.picker = Some(Picker::new(PickerKind::Change, "jump to change", items, 0));
    }

    pub(super) fn confirm_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        // A create row carries the typed name rather than a list entry, so
        // it is handled before anything reads the selection.
        if let (Some(name), true) = (picker.create.clone(), picker.on_create_row()) {
            match &picker.kind {
                PickerKind::Branch { checkout } => {
                    let _ = self.out.send(ClientMsg::CreateBranch {
                        checkout: *checkout,
                        branch: name,
                    });
                }
                PickerKind::Workspace { .. } => {
                    let _ = self.out.send(ClientMsg::CreateWorkspace { name });
                    // The daemon opens what it creates, and it arrives
                    // empty, so the columns must not keep pointing into
                    // the workspace that was open a moment ago.
                    self.reset_navigation();
                }
                _ => {}
            }
            return;
        }
        match &picker.kind {
            PickerKind::Branch { checkout } => {
                let Some(branch) = picker.selected() else {
                    return;
                };
                let _ = self.out.send(ClientMsg::SwitchBranch {
                    checkout: *checkout,
                    branch: branch.to_string(),
                });
            }
            PickerKind::File { checkout } => {
                let Some(path) = picker.selected() else {
                    return;
                };
                let _ = self.out.send(ClientMsg::OpenInEditor {
                    checkout: *checkout,
                    path: path.to_string(),
                    line: None,
                    external: self.settings.editor.is_external(),
                    command: self.editor_command(),
                });
                self.want_editor();
            }
            PickerKind::Change => {
                let Some(idx) = picker.shown.get(picker.sel).copied() else {
                    return;
                };
                if let Some(view) = &mut self.review {
                    view.jump_to_file(idx);
                }
            }
            PickerKind::ReviewRecipient {
                panes,
                checkout,
                anchor,
                body,
            } => {
                let Some(pane) = picker.shown.get(picker.sel).and_then(|i| panes.get(*i)) else {
                    return;
                };
                if self.is_live_agent(*pane) {
                    self.send_review_comment(*checkout, *pane, anchor.clone(), body.clone());
                } else {
                    self.report("that agent is no longer running");
                }
            }
            PickerKind::Agent => {
                let Some(name) = picker.selected() else {
                    return;
                };
                if let Some(checkout) = self.current_checkout() {
                    let _ = self.out.send(ClientMsg::SpawnAgent {
                        checkout: checkout.id,
                        template: name.to_string(),
                    });
                    self.pending_focus_new = true;
                }
            }
            PickerKind::Workspace { ids, .. } => {
                let Some(id) = picker.shown.get(picker.sel).and_then(|i| ids.get(*i)) else {
                    return;
                };
                let _ = self.out.send(ClientMsg::OpenWorkspace { workspace: *id });
                // The incoming tree is a different set of projects, so
                // start at the top rather than keeping an index that meant
                // something else.
                self.reset_navigation();
            }
            PickerKind::Theme => {
                let Some(name) = picker.selected() else {
                    return;
                };
                self.theme = crate::theme::Theme::by_name(name);
                self.report(format!("theme: {name}"));
            }
        }
    }
}
