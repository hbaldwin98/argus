//! Key handling, mode by mode.
//!
//! Which handler a key reaches is decided once, at the top of [`App::on_key`],
//! and the modes do not fall through to each other: a prompt swallows the
//! keys a prompt uses, and the navigation bindings are simply not reachable
//! while one is open. That is what keeps a typed character from also being
//! a command.
//!
//! Most modes' keys are a table in `app/mode/keymap`, which names the
//! actions below. The handlers written out here are the ones a table of
//! single keys cannot express: typed text, and the leader chord.

use super::*;
use crate::dropped::Dropped;

/// The most a pasted file may be. Under the frame limit with room for the
/// message around it; a screenshot is a few hundred kilobytes.
const PASTE_FILE_MAX: usize = 32 * 1024 * 1024;

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

    /// Ctrl-V, with or without shift, and Alt-V. Taken by Argus everywhere,
    /// including inside a pane: what the child would have made of it
    /// (quoted-insert in a line editor, visual block in vim) is worth less
    /// than pasting reliably, and the leader chord still reaches the
    /// child's own keys.
    ///
    /// Alt-V because Windows Terminal keeps Ctrl-V and Ctrl-Shift-V for its
    /// own paste and never passes them on — and its paste of an image is
    /// nothing at all. Alt-V is the key Claude Code chose for the same
    /// reason, so it is the one a Windows user already knows.
    fn is_paste_key(key: &KeyEvent) -> bool {
        let v = matches!(key.code, KeyCode::Char('v') | KeyCode::Char('V'));
        let held = key.modifiers;
        v && (held.contains(KeyModifiers::CONTROL) || held.contains(KeyModifiers::ALT))
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        self.handle_key(key);
        self.refresh_board_if_stale();
    }

    fn handle_key(&mut self, key: KeyEvent) {
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
        // The keymap opens over whatever is already up: "what can I press
        // here" is a question about the mode you are in, not a reason to
        // leave it. `?` is a character on a typing surface, so it only
        // means this where nothing is taking text.
        if self.help.is_some() {
            return self.on_key_help(key);
        }
        if key.code == KeyCode::Char('?') && !self.takes_text() {
            self.help = Some(Help::default());
            return;
        }
        match self.mode() {
            Mode::Prompt => self.on_key_prompt(key),
            Mode::DirPicker => self.on_key_dir_picker(key),
            Mode::Picker => self.on_key_picker(key),
            Mode::CheckoutFilter => self.on_key_checkout_filter(key),
            Mode::Overlay(OverlayMode::Review) => self.on_key_review(key),
            Mode::Overlay(OverlayMode::History) => self.on_key_history(key),
            Mode::Overlay(OverlayMode::Brief) => self.on_key_brief(key),
            Mode::Overlay(OverlayMode::Pane) => self.on_key_floating_pane(key),
            Mode::Pane => self.on_key_pane_content(key),
            // The typed line takes every key, digits included, so a title
            // with an `x` in it does not delete the row behind it, and the
            // first escape puts the line away rather than the view.
            Mode::Stage(View::Feature) if self.line.is_some() => self.on_key_line(key),
            Mode::Overlay(OverlayMode::SequenceDiagram | OverlayMode::Settings)
            | Mode::Stage(_)
            | Mode::Rail => {
                self.press(key);
            }
        }
    }

    /// The keymap window's own keys: scrolling, and out.
    fn on_key_help(&mut self, key: KeyEvent) {
        let Some(help) = &mut self.help else { return };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => help.scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => help.scroll = help.scroll.saturating_sub(1),
            KeyCode::Char('d') | KeyCode::PageDown => help.scroll += 10,
            KeyCode::Char('u') | KeyCode::PageUp => help.scroll = help.scroll.saturating_sub(10),
            KeyCode::Char('g') | KeyCode::Home => help.scroll = 0,
            // Anything else closes it. A window opened to be read is one
            // you want out of the way again, and having to find its exit
            // key is the problem it exists to solve.
            _ => self.help = None,
        }
    }

    /// Whether pasted text has somewhere to land — the same routing
    /// `on_paste` walks, asked ahead of time by the paste key so it can
    /// say so instead of reading the clipboard for nothing.
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
    ///
    /// An image on it, with a pane to take one, wins over text: a screenshot
    /// copied from a browser comes with its URL as text, and the picture is
    /// what was meant. Only a pane can take one; the prompts and pickers
    /// are typed text.
    ///
    /// Over SSH there is no desktop clipboard to read, but the terminal's
    /// own paste still arrives as a bracketed paste, so the alert names
    /// that route instead of leaving the user with no way in.
    fn paste_clipboard(&mut self) {
        if let Some(pane) = self.paste_target() {
            if let Some(png) = (self.clipboard_image)() {
                if self.paste_file(pane, "clipboard.png".to_string(), png) {
                    self.report("pasted an image");
                }
                return;
            }
        }
        let Some(text) = (self.clipboard)() else {
            self.alert(
                "no clipboard here: paste text with your terminal's paste key; \
                 for an image, run argus on your desktop with --host",
            );
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

    /// The pane a paste reaches: the one typed into, with nothing above it
    /// — a prompt, a picker — that takes the text first. The same order
    /// `on_paste` walks.
    fn paste_target(&self) -> Option<PaneId> {
        let something_above = self.prompt.is_some() || self.dir_picker.is_some() || self.picker.is_some();
        if something_above {
            return None;
        }
        self.input_pane()
    }

    pub fn on_paste(&mut self, text: String) {
        self.clear_status();
        if let Some(prompt) = &mut self.prompt {
            let input = match prompt {
                Prompt::NewWorktree { input, .. }
                | Prompt::NewRepository { input, .. }
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
        let Some(pane) = self.input_pane() else {
            // Said, never typed: a dropped path's letters would be
            // commands here, and one of them opened a prompt.
            self.report("nothing here takes pasted text");
            return;
        };
        match crate::dropped::file(&text) {
            Some(Dropped::Here(path)) => {
                if self.paste_dropped(pane, &path) {
                    return;
                }
            }
            // Pasted all the same: the path is what was typed, and an
            // agent told it will say it cannot find it. The bar says
            // why first.
            Some(Dropped::Elsewhere(name)) => self.alert(format!(
                "{name} is not on this machine; to paste it, run argus where it is with --host"
            )),
            None => {}
        }
        let _ = self.out.send(ClientMsg::Paste { pane, text });
    }

    /// Carries a file dropped on the terminal to the pane's host. False
    /// when it could not cross, having said why on the bar; the path then
    /// pastes as the text it was, which still names the file to a daemon
    /// on this machine.
    fn paste_dropped(&mut self, pane: PaneId, path: &std::path::Path) -> bool {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.alert(format!("could not read {name}: {e}"));
                return false;
            }
        };
        let crossed = self.paste_file(pane, name.clone(), bytes);
        if crossed {
            self.report(format!("pasted {name}"));
        }
        crossed
    }

    /// Ships a file from this machine to the pane, where the daemon keeps
    /// it and pastes its path there. False, having said why, when the
    /// daemon is from before files crossed, or the file is more than a
    /// frame holds.
    fn paste_file(&mut self, pane: PaneId, name: String, bytes: Vec<u8>) -> bool {
        if !self.pastes_files {
            let (argusd, fix) = self.daemon_and_its_fix();
            self.alert(format!("{argusd} predates pasting images; {fix}"));
            return false;
        }
        if bytes.len() > PASTE_FILE_MAX {
            self.alert(format!("{name} is too large to paste ({} MB)", bytes.len() >> 20));
            return false;
        }
        let _ = self.out.send(ClientMsg::PasteFile { pane, name, bytes });
        true
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
                        let request_id = self.awaited.ask(Then::SelectCheckout);
                        let _ = self.out.send(ClientMsg::CreateWorktree {
                            checkout: base,
                            branch,
                            request_id,
                        });
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
            Prompt::NewRepository {
                project,
                parent,
                input,
            } => match key.code {
                KeyCode::Enter => {
                    let project = *project;
                    // An empty name is not an empty request: the browse
                    // already said where, and this is "that directory
                    // itself" — a folder that exists and only wants a
                    // `git init`.
                    let name = input.trim();
                    let path = if name.is_empty() {
                        parent.clone()
                    } else {
                        crate::dirpicker::join(parent, name)
                    };
                    self.prompt = None;
                    let request_id = self.awaited.ask(Then::SelectRepository);
                    let _ = self.out.send(ClientMsg::InitRepository {
                        project,
                        path,
                        request_id,
                    });
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
                        let request_id = self.awaited.ask(Then::SelectProject);
                        let _ = self.out.send(ClientMsg::AddProject { path, request_id });
                    }
                    DirTarget::Repository(project) => {
                        let request_id = self.awaited.ask(Then::SelectRepository);
                        let _ = self.out.send(ClientMsg::AddRepository {
                            project,
                            path,
                            request_id,
                        });
                    }
                    // Nothing is created yet: the directory just chosen is
                    // where the repository goes, and it still needs a name.
                    DirTarget::NewRepository(project) => {
                        self.prompt = Some(Prompt::NewRepository {
                            project,
                            parent: path,
                            input: String::new(),
                        });
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

    pub(super) fn cycle_selected_setting(&mut self, delta: isize) {
        if let Some(Overlay::Settings { sel }) = self.overlay {
            self.cycle_setting(sel, delta);
        }
    }

    /// A pane floating over the stage is a typing surface like the
    /// workspace terminal, so the same leader gets you out of it.
    fn on_key_floating_pane(&mut self, key: KeyEvent) {
        if self.leader_pending && is_leader(&key) {
            return;
        }
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.close_overlay(),
                KeyCode::Char('?') => self.help = Some(Help::default()),
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

    /// Two keymaps, because a brief is read far more often than it is
    /// written. View mode navigates with single keys, from the keymap; in
    /// insert mode every key is a character, and `Esc` is the way back.
    fn on_key_brief(&mut self, key: KeyEvent) {
        let Some(view) = &mut self.brief else {
            self.close_overlay();
            return;
        };
        if view.mode != BriefMode::Insert {
            self.press(key);
            return;
        }
        match key.code {
            KeyCode::Esc => {
                view.view_mode();
                // Leaving insert is the save point: it is the moment
                // the user stops typing, and it costs no extra key.
                self.save_brief();
            }
            KeyCode::Enter => view.newline(),
            KeyCode::Backspace => view.backspace(),
            KeyCode::Left => view.move_column(-1),
            KeyCode::Right => view.move_column(1),
            KeyCode::Up => view.move_by(-1),
            KeyCode::Down => view.move_by(1),
            KeyCode::Home => view.start_of_line(),
            KeyCode::End => view.end_of_line(),
            KeyCode::Tab => {
                for _ in 0..2 {
                    view.insert_char(' ');
                }
            }
            KeyCode::Char(c) => view.insert_char(c),
            _ => {}
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
                KeyCode::Char('?') => self.help = Some(Help::default()),
                KeyCode::Char(c) if View::from_digit(c).is_some() => {
                    self.open_view(View::from_digit(c).unwrap())
                }
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

    /// Only the table draws every checkout row, so only there does a
    /// filter have anything on screen to narrow.
    pub(super) fn checkouts_filterable(&self) -> bool {
        self.view == View::Checkouts
    }

    pub(super) fn begin_checkout_filter(&mut self) {
        self.checkout_filtering = true;
        self.checkout_filter.clear();
        self.sync_checkout_filter_selection();
    }

    fn on_key_checkout_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.checkout_filter.clear();
                self.checkout_filtering = false;
                self.sync_checkout_filter_selection();
            }
            KeyCode::Enter => {
                self.checkout_filtering = false;
                self.sync_checkout_filter_selection();
            }
            KeyCode::Backspace => {
                self.checkout_filter.pop();
                self.sync_checkout_filter_selection();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.checkout_filter.push(c);
                self.sync_checkout_filter_selection();
            }
            _ => {}
        }
    }

    /// The Panes stage's cursor, stepped through the cards in the order
    /// they are drawn.
    pub(super) fn move_overview_pane(&mut self, delta: i32) {
        let locations = self.overview_pane_locations();
        if locations.is_empty() {
            return;
        }
        let current = self
            .pane_location()
            .and_then(|selected| locations.iter().position(|location| *location == selected))
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, locations.len() as i32 - 1) as usize;
        self.select_pane_location(locations[next]);
    }

    /// A line being typed in the feature view.
    fn on_key_line(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.line = None,
            KeyCode::Enter => self.commit_line(),
            KeyCode::Backspace => self.backspace_line(),
            KeyCode::Char(c) => self.type_into_line(c),
            _ => {}
        }
    }

    pub(super) fn scroll_diagram(&mut self, delta: i32) {
        if let Some(view) = &mut self.diagram {
            let visible = self.layout.overlay.inner.height.max(1) as usize;
            view.scroll_by(delta, visible);
        }
    }

    pub(super) fn pan_diagram(&mut self, delta: i32) {
        if let Some(view) = &mut self.diagram {
            let visible = self.layout.overlay.inner.width.max(1) as usize;
            view.scroll_horizontal_by(delta, visible);
        }
    }

    /// Focus without a diff would trap every keystroke, so a key that finds
    /// nothing loaded to act on is the way out.
    fn on_key_review(&mut self, key: KeyEvent) {
        if !self.press(key) && self.review.is_none() {
            self.focus = Focus::Checkouts;
        }
    }

    /// The same diff again, fresh: the commit it was, or the working tree.
    pub(super) fn refresh_review(&mut self) {
        if let Some(oid) = self
            .review
            .as_ref()
            .and_then(|v| v.review.commit.as_ref().map(|c| c.oid.clone()))
        {
            return self.open_commit_review(oid, None);
        }
        self.open_review();
    }

    pub(super) fn flip_review_base(&mut self) {
        // The side toggle is meaningless on a commit, and flipping it here
        // would silently change which side the next uncommitted review
        // opens on.
        if self
            .review
            .as_ref()
            .is_some_and(|v| v.review.commit.is_some())
        {
            return;
        }
        self.review_base = self.review_base.next();
        self.open_review();
    }

    pub(super) fn open_reviewed_line_in_editor(&mut self) {
        let Some(v) = &self.review else {
            return;
        };
        let checkout = v.review.checkout;
        let Some(a) = v.anchor() else {
            return;
        };
        let line = a.preferred_start();
        let request_id = self.editor_request();
        let _ = self.out.send(ClientMsg::OpenInEditor {
            checkout,
            path: a.path,
            line,
            external: self.editor_mode().is_external(),
            command: self.editor_command(),
            request_id,
        });
        self.close_overlay();
    }

    pub(super) fn comment_on_reviewed_lines(&mut self) {
        let Some(anchor) = self.review.as_ref().and_then(|v| v.anchor()) else {
            return;
        };
        self.prompt = Some(Prompt::Comment {
            anchor,
            input: String::new(),
        });
    }

    /// As [`App::on_key_review`]: with no commit list loaded, a key that
    /// finds nothing to act on is the way out.
    fn on_key_history(&mut self, key: KeyEvent) {
        if !self.press(key) && self.history.is_none() {
            self.focus = Focus::Checkouts;
        }
    }

    /// `h` folds the commit the cursor is in before it closes the overlay,
    /// the way it steps back out of a review.
    pub(super) fn fold_or_close_history(&mut self) {
        let folded = self.history.as_mut().is_some_and(|v| v.collapse());
        if !folded {
            self.close_overlay();
        }
    }
}
