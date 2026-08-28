//! Key handling, mode by mode.
//!
//! Which handler a key reaches is decided once, at the top of [`App::on_key`],
//! and the modes do not fall through to each other: a prompt swallows the
//! keys a prompt uses, and the navigation bindings are simply not reachable
//! while one is open. That is what keeps a typed character from also being
//! a command.

use super::*;

impl App {
    /// Shuts any floating window, from anywhere, whatever has focus.
    ///
    /// The leader is the *nice* way out, but it depends on the terminal
    /// delivering Ctrl-Space, and a floating pane consumes every other key
    /// on purpose. When that combination fails there is nothing left to
    /// press, so this one is checked before any handler runs and is never
    /// forwarded to a child. F-keys are reliably delivered and no terminal
    /// editor binds F12 by default.
    fn is_panic_key(key: &KeyEvent) -> bool {
        key.code == KeyCode::F(12)
    }

    /// Ctrl-V, with or without shift. Taken by Argus everywhere, including
    /// inside a pane: what the child would have made of it (quoted-insert
    /// in a line editor, visual block in vim) is worth less than pasting
    /// reliably, and the leader chord still reaches the child's own keys.
    fn is_paste_key(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('v') | KeyCode::Char('V'))
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // The left of the bar is the breadcrumb's seat and a message only
        // borrows it. Pressing anything is the acknowledgement that hands it
        // back; without that, the last error or exit hides where you are for
        // the rest of the session. Cleared before dispatch, so a handler is
        // still free to set its own.
        self.clear_status();
        if Self::is_paste_key(&key) {
            self.paste_clipboard();
            return;
        }
        if Self::is_panic_key(&key) {
            if self.overlay.is_some() {
                self.close_overlay();
                self.report("closed the floating window");
            }
            return;
        }
        if self.prompt.is_some() {
            self.on_key_prompt(key);
        } else if self.dir_picker.is_some() {
            self.on_key_dir_picker(key);
        } else if self.picker.is_some() {
            self.on_key_picker(key);
        } else if self.overlay.is_some() {
            self.on_key_overlay(key);
        } else if self.focus == Focus::Review {
            self.on_key_review(key);
        } else if self.focus == Focus::PaneContent {
            self.on_key_pane_content(key);
        } else {
            self.on_key_nav(key);
        }
    }

    /// Whether pasted text has somewhere to land — the same routing
    /// `on_paste` walks, asked ahead of time so a coalesced burst with no
    /// target is replayed as keystrokes instead of vanishing.
    pub fn accepts_paste(&self) -> bool {
        if let Some(prompt) = &self.prompt {
            return !matches!(prompt, Prompt::ConfirmRemove { .. });
        }
        if self.dir_picker.is_some() {
            return true;
        }
        if let Some(picker) = &self.picker {
            return picker.kind.is_fuzzy();
        }
        self.input_pane().is_some()
    }

    /// The pane typed text goes to: the floating window if one is up,
    /// otherwise the focused column's pane.
    pub fn input_pane(&self) -> Option<PaneId> {
        self.overlay.as_ref().and_then(Overlay::pane).or_else(|| {
            (self.focus == Focus::PaneContent)
                .then(|| self.column_pane())
                .flatten()
        })
    }

    /// Pastes what is actually on the clipboard, rather than what the
    /// timing of a run of keystrokes suggested was one.
    fn paste_clipboard(&mut self) {
        let Some(text) = (self.clipboard)() else {
            self.alert("could not read the clipboard");
            return;
        };
        if text.is_empty() {
            self.report("the clipboard is empty");
            return;
        }
        if !self.accepts_paste() {
            self.report("nothing here takes pasted text");
            return;
        }
        let lines = text.lines().count();
        self.on_paste(crate::clipboard::normalize(&text));
        self.report(format!(
            "pasted {lines} line{}",
            if lines == 1 { "" } else { "s" }
        ));
    }

    pub fn on_paste(&mut self, text: String) {
        self.clear_status();
        if let Some(prompt) = &mut self.prompt {
            let input = match prompt {
                Prompt::NewWorktree { input, .. }
                | Prompt::Comment { input, .. }
                | Prompt::EditorCommand { input } => Some(input),
                Prompt::ConfirmRemove { .. } => None,
            };
            if let Some(input) = input {
                input.extend(text.chars().filter(|c| !c.is_control()));
            }
            return;
        }
        if let Some(picker) = &mut self.dir_picker {
            picker.paste(&text);
            return;
        }
        if let Some(picker) = &mut self.picker {
            if picker.kind.is_fuzzy() {
                picker
                    .query
                    .extend(text.chars().filter(|c| !c.is_control()));
                picker.refilter();
            }
            return;
        }
        if let Some(pane) = self.input_pane() {
            let _ = self.out.send(ClientMsg::Paste { pane, text });
        }
    }

    fn on_key_prompt(&mut self, key: KeyEvent) {
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        match prompt {
            Prompt::NewWorktree { base, input } => match key.code {
                KeyCode::Enter => {
                    let branch = input.trim().to_string();
                    let base = *base;
                    self.prompt = None;
                    if !branch.is_empty() {
                        let _ = self.out.send(ClientMsg::CreateWorktree {
                            checkout: base,
                            branch,
                        });
                        self.pending_focus_new_checkout = self.current_repository().map(|r| r.id);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::ConfirmRemove { target, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let _ = self.out.send(target.message());
                    self.prompt = None;
                }
                KeyCode::Esc | KeyCode::Char('n') => self.prompt = None,
                _ => {}
            },
            Prompt::Comment { anchor, input } => match key.code {
                KeyCode::Enter => {
                    let anchor = anchor.clone();
                    let body = input.trim().to_string();
                    let empty = input.trim().is_empty();
                    self.prompt = None;
                    if !empty {
                        self.send_to_agent(anchor, body);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::EditorCommand { input } => match key.code {
                KeyCode::Enter => {
                    let cmd = input.trim().to_string();
                    self.prompt = None;
                    self.settings.editor_cmd = cmd;
                    if self.persist_settings {
                        crate::settings::save(&self.settings);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
        }
    }

    fn on_key_dir_picker(&mut self, key: KeyEvent) {
        let Some(picker) = &mut self.dir_picker else {
            return;
        };
        match picker.on_key(key) {
            DirAction::None => {}
            DirAction::Close => self.dir_picker = None,
            DirAction::Browse(path) => {
                let request = self.next_browse_request;
                self.next_browse_request += 1;
                picker.pending = Some(request);
                let _ = self.out.send(ClientMsg::ListDirectories {
                    request_id: request,
                    path,
                });
            }
            DirAction::Choose(path) => {
                let target = picker.target;
                self.dir_picker = None;
                match target {
                    DirTarget::Project => {
                        let _ = self.out.send(ClientMsg::AddProject { path });
                        self.pending_focus_new_project = true;
                    }
                    DirTarget::Repository(project) => {
                        let _ = self.out.send(ClientMsg::AddRepository { project, path });
                        self.pending_focus_new_repository = Some(project);
                    }
                }
            }
        }
    }

    fn on_key_picker(&mut self, key: KeyEvent) {
        let fuzzy = self.picker.as_ref().is_some_and(|p| p.kind.is_fuzzy());
        // On a fuzzy picker every printable key is query text, so movement
        // moves to the arrows and ctrl-n/p. On a plain one j/k still work.
        match key.code {
            KeyCode::Down => self.move_picker(1),
            KeyCode::Up => self.move_picker(-1),
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_picker(1)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_picker(-1)
            }
            KeyCode::Char('j') if !fuzzy => self.move_picker(1),
            KeyCode::Char('k') if !fuzzy => self.move_picker(-1),
            KeyCode::Enter => self.confirm_picker(),
            KeyCode::Esc => self.picker = None,
            KeyCode::Char('q') if !fuzzy => self.picker = None,
            KeyCode::Backspace if fuzzy => {
                if let Some(p) = &mut self.picker {
                    p.query.pop();
                    p.refilter();
                }
            }
            KeyCode::Char(c) if fuzzy => {
                if let Some(p) = &mut self.picker {
                    p.query.push(c);
                    p.refilter();
                }
            }
            _ => {}
        }
    }

    /// An overlay holding a pane is a typing surface like the content
    /// column, so the same leader gets you out of it.
    fn on_key_overlay(&mut self, key: KeyEvent) {
        if matches!(self.overlay, Some(Overlay::Review)) {
            self.on_key_review(key);
            return;
        }
        if matches!(self.overlay, Some(Overlay::History)) {
            self.on_key_history(key);
            return;
        }
        if let Some(Overlay::Settings { sel }) = &mut self.overlay {
            let sel = *sel;
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.move_setting(1),
                KeyCode::Char('k') | KeyCode::Up => self.move_setting(-1),
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.cycle_setting(sel, 1),
                KeyCode::Char('h') | KeyCode::Left => self.cycle_setting(sel, -1),
                KeyCode::Esc | KeyCode::Char('q') => self.close_overlay(),
                _ => {}
            }
            return;
        }
        if self.leader_pending && is_leader(&key) {
            return;
        }
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.close_overlay(),
                KeyCode::Char('x') => {
                    if let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) {
                        let _ = self.out.send(ClientMsg::Kill { pane });
                    }
                    self.close_overlay();
                }
                _ => {}
            }
            return;
        }
        if is_leader(&key) {
            self.leader_pending = true;
            return;
        }
        let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) else {
            return;
        };
        let bytes = encode_key(&key);
        if !bytes.is_empty() {
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
        }
    }

    fn on_key_pane_content(&mut self, key: KeyEvent) {
        if self.leader_pending && is_leader(&key) {
            return;
        }
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.ascend(),
                KeyCode::Tab => self.open_review(),
                KeyCode::Char('H') => self.open_history(),
                KeyCode::Char('f') => self.pane_fullscreen = !self.pane_fullscreen,
                KeyCode::Char('x') => self.close_current(),
                KeyCode::Char('N') => self.jump_to_next_attention(),
                _ => {}
            }
            return;
        }
        if is_leader(&key) {
            self.leader_pending = true;
            return;
        }
        let Some(pane) = self.column_pane() else {
            return;
        };
        // Shift-PageUp/Down is the terminal convention for scrollback, and
        // taking only the shifted pair leaves the child its own paging keys
        // — a pager or an editor inside the pane still gets them unshifted.
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::PageUp => return self.page_pane(pane, -1),
                KeyCode::PageDown => return self.page_pane(pane, 1),
                _ => {}
            }
        }
        let bytes = encode_key(&key);
        if !bytes.is_empty() {
            // Typing is a statement that the present is what matters; every
            // terminal snaps to the bottom on it, and the child's echo would
            // otherwise land somewhere the operator cannot see.
            self.scroll_to_live(pane);
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
        }
    }

    fn on_key_nav(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('l') | KeyCode::Enter | KeyCode::Right => self.descend(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => self.ascend(),
            KeyCode::Char('s') => self.spawn_shell(),
            KeyCode::Char('a') => self.open_picker(),
            KeyCode::Char('n') => self.new_prompt(),
            KeyCode::Char('D') => self.remove_prompt(),
            KeyCode::Char('w') => self.open_workspace_picker(),
            KeyCode::Char('t') => self.open_theme_picker(),
            KeyCode::Char('S') => self.open_settings(),
            KeyCode::Char('b') => self.open_branch_picker(),
            KeyCode::Char('B') => self.toggle_branches(),
            KeyCode::Char('F') => self.fetch(),
            KeyCode::Char('P') => self.pull(),
            KeyCode::Char('f') => self.open_file_picker(),
            KeyCode::Char('R') | KeyCode::Tab => self.open_review(),
            KeyCode::Char('H') => self.open_history(),
            KeyCode::Char('x') => self.kill_selected(),
            KeyCode::Char('p') => self.toggle_projects_collapsed(),
            KeyCode::Char('N') => self.jump_to_next_attention(),
            _ => {}
        }
    }

    fn on_key_review(&mut self, key: KeyEvent) {
        // Taken first so they don't sit inside the view borrow.
        match key.code {
            KeyCode::Char('R') | KeyCode::Char('r') => {
                if let Some(oid) = self
                    .review
                    .as_ref()
                    .and_then(|v| v.review.commit.as_ref().map(|c| c.oid.clone()))
                {
                    return self.open_commit_review(oid, None);
                }
                return self.open_review();
            }
            KeyCode::Char('H') => return self.open_history(),
            KeyCode::Char('b') => {
                // The side toggle is meaningless on a commit, and flipping
                // it here would silently change which side the next
                // uncommitted review opens on.
                if self
                    .review
                    .as_ref()
                    .is_some_and(|v| v.review.commit.is_some())
                {
                    return;
                }
                self.review_base = self.review_base.next();
                return self.open_review();
            }
            KeyCode::Char('f') => return self.open_change_picker(),
            KeyCode::Char('h') | KeyCode::Left => return self.close_review(),
            KeyCode::Esc | KeyCode::Char('q') => return self.close_overlay(),
            _ => {}
        }
        let Some(v) = &mut self.review else {
            // Focus without a view would trap every keystroke; the only
            // honest thing is to leave.
            self.focus = Focus::Checkouts;
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => v.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => v.move_by(-1),
            KeyCode::Char('d') | KeyCode::PageDown => v.move_by(10),
            KeyCode::Char('u') | KeyCode::PageUp => v.move_by(-10),
            KeyCode::Char(']') => v.jump_file(true),
            KeyCode::Char('[') => v.jump_file(false),
            KeyCode::Char('g') | KeyCode::Home => v.top_of_diff(),
            KeyCode::Char('G') | KeyCode::End => v.bottom_of_diff(),
            KeyCode::Char('V') | KeyCode::Char('v') => v.toggle_mark(),
            KeyCode::Char('e') => {
                let checkout = v.review.checkout;
                if let Some(a) = v.anchor() {
                    let line = a.preferred_start();
                    let _ = self.out.send(ClientMsg::OpenInEditor {
                        checkout,
                        path: a.path,
                        line,
                        external: self.settings.editor.is_external(),
                        command: self.editor_command(),
                    });
                    self.want_editor();
                    self.close_overlay();
                }
            }
            KeyCode::Char('c') => {
                let anchor = v.anchor();
                if let Some(anchor) = anchor {
                    self.prompt = Some(Prompt::Comment {
                        anchor,
                        input: String::new(),
                    });
                }
            }
            // The tree has likely moved on under an agent still editing it.
            _ => {}
        }
    }

    fn on_key_history(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('R') => return self.open_review(),
            KeyCode::Char('r') | KeyCode::Char('H') => return self.open_history(),
            // `h`/Left folds the commit the cursor is in before it closes
            // the overlay, the way it steps back out of a review.
            KeyCode::Char('h') | KeyCode::Left => {
                let folded = self.history.as_mut().is_some_and(|v| v.collapse());
                if !folded {
                    self.close_overlay();
                }
                return;
            }
            KeyCode::Esc | KeyCode::Char('q') => return self.close_overlay(),
            KeyCode::Char('l') | KeyCode::Enter | KeyCode::Right => {
                return self.drill_into_history()
            }
            _ => {}
        }
        let Some(v) = &mut self.history else {
            self.focus = Focus::Checkouts;
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => v.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => v.move_by(-1),
            KeyCode::Char('d') | KeyCode::PageDown => v.move_by(10),
            KeyCode::Char('u') | KeyCode::PageUp => v.move_by(-10),
            KeyCode::Char(']') => v.jump_commit(true),
            KeyCode::Char('[') => v.jump_commit(false),
            KeyCode::Char('g') | KeyCode::Home => v.top_of_list(),
            KeyCode::Char('G') | KeyCode::End => v.bottom_of_list(),
            _ => {}
        }
    }
}
