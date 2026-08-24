use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use orion_protocol::{CheckoutId, CheckoutInfo, ClientMsg, PaneId, PaneInfo, ProjectInfo, ServerMsg};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::grid::Grid;
use crate::keys::{encode_key, is_leader};
use crate::mouse::encode_mouse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Checkouts,
    Panes,
    PaneContent,
}

/// Screen regions from the most recent render, so mouse clicks can be mapped
/// back onto tree rows / pane cells without duplicating layout math.
/// `panes` is the pane-tab strip atop the live view; `content` is the live
/// view itself. Both live in the always-visible rightmost column.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub projects: Rect,
    pub checkouts: Rect,
    pub panes: Rect,
    pub content: Rect,
}

impl Default for Layout {
    fn default() -> Self {
        let zero = Rect::new(0, 0, 0, 0);
        Layout {
            projects: zero,
            checkouts: zero,
            panes: zero,
            content: zero,
        }
    }
}

pub struct Picker {
    pub items: Vec<String>,
    pub sel: usize,
}

/// A modal text/confirm prompt, mutually exclusive with `Picker`. Both new
/// worktree (free text) and remove-checkout (yes/no) go through this so
/// there's one input path and one place `on_mouse` has to know to ignore.
pub enum Prompt {
    NewWorktree { base: CheckoutId, input: String },
    ConfirmRemoveCheckout { checkout: CheckoutId, label: String },
    AddProject { input: String },
}

pub struct App {
    pub tree: Vec<ProjectInfo>,
    pub templates: Vec<String>,
    pub focus: Focus,
    pub sel_project: usize,
    pub sel_checkout: usize,
    pub sel_pane: usize,
    pub subscribed: Option<PaneId>,
    pub grid: Option<Grid>,
    pub leader_pending: bool,
    pub should_quit: bool,
    pub status: String,
    pub layout: Layout,
    pub picker: Option<Picker>,
    pub prompt: Option<Prompt>,
    pending_focus_new: bool,
    pending_focus_new_checkout: bool,
    pending_focus_new_project: bool,
    out: UnboundedSender<ClientMsg>,
}

impl App {
    pub fn new(out: UnboundedSender<ClientMsg>) -> Self {
        App {
            tree: Vec::new(),
            templates: Vec::new(),
            focus: Focus::Projects,
            sel_project: 0,
            sel_checkout: 0,
            sel_pane: 0,
            subscribed: None,
            grid: None,
            leader_pending: false,
            should_quit: false,
            status: "j/k move  l/enter open  h/esc back  s: shell  a: agent  n: new  D: rm-checkout  x: close  q: detach"
                .to_string(),
            layout: Layout::default(),
            picker: None,
            prompt: None,
            pending_focus_new: false,
            pending_focus_new_checkout: false,
            pending_focus_new_project: false,
            out,
        }
    }

    pub fn current_project(&self) -> Option<&ProjectInfo> {
        self.tree.get(self.sel_project)
    }

    pub fn current_checkout(&self) -> Option<&CheckoutInfo> {
        self.current_project()
            .and_then(|p| p.checkouts.get(self.sel_checkout))
    }

    pub fn current_pane(&self) -> Option<&PaneInfo> {
        self.current_checkout().and_then(|c| c.panes.get(self.sel_pane))
    }

    fn clamp(&mut self) {
        let nproj = self.tree.len();
        if nproj == 0 {
            self.sel_project = 0;
        } else if self.sel_project >= nproj {
            self.sel_project = nproj - 1;
        }
        let ncheck = self.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
        if ncheck == 0 {
            self.sel_checkout = 0;
        } else if self.sel_checkout >= ncheck {
            self.sel_checkout = ncheck - 1;
        }
        let npane = self.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
        if npane == 0 {
            self.sel_pane = 0;
        } else if self.sel_pane >= npane {
            self.sel_pane = npane - 1;
        }
        self.sync_subscription();
    }

    /// Keeps the live view subscribed to whatever pane current navigation
    /// state implies, independent of `focus` — the rightmost column always
    /// shows this pane's content alongside the project/checkout columns,
    /// it never takes over the whole screen.
    fn sync_subscription(&mut self) {
        let want = self.current_pane().map(|p| p.id);
        if want == self.subscribed {
            return;
        }
        if let Some(old) = self.subscribed.take() {
            let _ = self.out.send(ClientMsg::Unsubscribe { pane: old });
        }
        self.grid = None;
        if let Some(id) = want {
            self.subscribed = Some(id);
            let _ = self.out.send(ClientMsg::Subscribe { pane: id });
        }
    }

    pub fn on_server_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Tree(t) => {
                self.tree = t;
                self.clamp();
                if self.pending_focus_new_project {
                    self.pending_focus_new_project = false;
                    let n = self.tree.len();
                    if n > 0 {
                        self.sel_project = n - 1;
                        self.clamp();
                    }
                }
                if self.pending_focus_new_checkout {
                    self.pending_focus_new_checkout = false;
                    let n = self.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
                    if n > 0 {
                        self.sel_checkout = n - 1;
                        self.clamp();
                    }
                }
                if self.pending_focus_new {
                    self.pending_focus_new = false;
                    let n = self.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
                    if n > 0 {
                        self.sel_pane = n - 1;
                        self.sync_subscription();
                        self.focus = Focus::PaneContent;
                    }
                }
            }
            ServerMsg::Templates(names) => {
                self.templates = names;
            }
            ServerMsg::PaneSnapshot { pane, cells, .. } => {
                if self.subscribed == Some(pane) {
                    self.grid = Some(Grid::new(cells));
                }
            }
            ServerMsg::Damage { pane, spans } => {
                if self.subscribed == Some(pane) {
                    if let Some(grid) = &mut self.grid {
                        grid.apply(&spans);
                    }
                }
            }
            ServerMsg::PaneClosed { pane, code } => {
                if self.subscribed == Some(pane) {
                    self.status = format!("pane exited ({code:?})");
                }
            }
            ServerMsg::Error { message } => {
                self.status = format!("error: {message}");
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.prompt.is_some() {
            self.on_key_prompt(key);
        } else if self.picker.is_some() {
            self.on_key_picker(key);
        } else if self.focus == Focus::PaneContent {
            self.on_key_pane_content(key);
        } else {
            self.on_key_nav(key);
        }
    }

    fn on_key_prompt(&mut self, key: KeyEvent) {
        let Some(prompt) = &mut self.prompt else { return };
        match prompt {
            Prompt::NewWorktree { base, input } => match key.code {
                KeyCode::Enter => {
                    let branch = input.trim().to_string();
                    let base = *base;
                    self.prompt = None;
                    if !branch.is_empty() {
                        let _ = self.out.send(ClientMsg::CreateWorktree { checkout: base, branch });
                        self.pending_focus_new_checkout = true;
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::ConfirmRemoveCheckout { checkout, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let _ = self.out.send(ClientMsg::RemoveCheckout { checkout: *checkout });
                    self.prompt = None;
                }
                KeyCode::Esc | KeyCode::Char('n') => self.prompt = None,
                _ => {}
            },
            Prompt::AddProject { input } => match key.code {
                KeyCode::Enter => {
                    let path = input.trim().to_string();
                    self.prompt = None;
                    if !path.is_empty() {
                        let _ = self.out.send(ClientMsg::AddProject { path });
                        self.pending_focus_new_project = true;
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

    fn on_key_picker(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(p) = &mut self.picker {
                    if p.sel + 1 < p.items.len() {
                        p.sel += 1;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(p) = &mut self.picker {
                    p.sel = p.sel.saturating_sub(1);
                }
            }
            KeyCode::Enter => self.confirm_picker(),
            KeyCode::Esc | KeyCode::Char('q') => self.picker = None,
            _ => {}
        }
    }

    fn on_key_pane_content(&mut self, key: KeyEvent) {
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.ascend(),
                KeyCode::Char('x') => self.close_current(),
                _ => {}
            }
            return;
        }
        if is_leader(&key) {
            self.leader_pending = true;
            return;
        }
        let Some(pane) = self.subscribed else { return };
        let bytes = encode_key(&key);
        if !bytes.is_empty() {
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
            KeyCode::Char('D') => self.remove_checkout_prompt(),
            KeyCode::Char('x') => self.kill_selected(),
            _ => {}
        }
    }

    /// `n` is contextual on which column has focus: a new project (any
    /// directory, not just preconfigured ones) from the projects column, or
    /// a new worktree branched off the selected checkout from the
    /// checkouts column. No-op elsewhere — there's no "current checkout"
    /// to branch from once you're inside the panes/content columns.
    fn new_prompt(&mut self) {
        match self.focus {
            Focus::Projects => {
                self.prompt = Some(Prompt::AddProject { input: String::new() });
            }
            Focus::Checkouts => {
                if let Some(c) = self.current_checkout() {
                    self.prompt = Some(Prompt::NewWorktree {
                        base: c.id,
                        input: String::new(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Opens a confirmation to remove the selected checkout. Refused
    /// client-side for the primary checkout — the repo the user already
    /// had, not Orion's to delete — so there's no round-trip just to be
    /// told no (the daemon refuses it too, as defense in depth).
    fn remove_checkout_prompt(&mut self) {
        if self.focus != Focus::Checkouts {
            return;
        }
        let Some(c) = self.current_checkout() else { return };
        if c.primary {
            self.status = "can't remove the primary checkout".to_string();
            return;
        }
        self.prompt = Some(Prompt::ConfirmRemoveCheckout {
            checkout: c.id,
            label: c.name.clone(),
        });
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.picker.is_some() || self.prompt.is_some() {
            return;
        }
        // The live view is always visible in the rightmost column, so a
        // click landing on it both forwards to the child and (for presses)
        // switches into typing mode, regardless of what was focused before.
        if let Some(bytes) = encode_mouse(&ev, self.layout.content) {
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::PaneContent;
            }
            if let Some(pane) = self.subscribed {
                let _ = self.out.send(ClientMsg::Input { pane, bytes });
            }
            return;
        }
        // Anything outside the live view always navigates, even while
        // "inside" a pane for typing — a click on another column should
        // switch to it, not get swallowed by the pane that currently has
        // keyboard focus.
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click_nav(ev.column, ev.row),
            MouseEventKind::ScrollUp => self.scroll_at(ev.column, ev.row, -1),
            MouseEventKind::ScrollDown => self.scroll_at(ev.column, ev.row, 1),
            _ => {}
        }
    }

    /// Scroll-wheel selection change for whichever list column the cursor is
    /// over, independent of `focus` — so scrolling a background column
    /// doesn't steal focus away from a pane you're typing into.
    fn scroll_at(&mut self, x: u16, y: u16, delta: i32) {
        let target = if in_rect(self.layout.projects, x, y) {
            Focus::Projects
        } else if in_rect(self.layout.checkouts, x, y) {
            Focus::Checkouts
        } else if in_rect(self.layout.panes, x, y) {
            Focus::Panes
        } else {
            return;
        };
        self.adjust_selection(target, delta);
    }

    fn click_nav(&mut self, x: u16, y: u16) {
        if let Some(idx) = row_in(self.layout.projects, x, y) {
            if idx < self.tree.len() {
                let already = self.focus == Focus::Projects && self.sel_project == idx;
                self.sel_project = idx;
                self.focus = Focus::Projects;
                self.clamp();
                if already {
                    self.descend();
                }
            }
            return;
        }
        if let Some(idx) = row_in(self.layout.checkouts, x, y) {
            let n = self.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
            if idx < n {
                let already = self.focus == Focus::Checkouts && self.sel_checkout == idx;
                self.sel_checkout = idx;
                self.focus = Focus::Checkouts;
                self.clamp();
                if already {
                    self.descend();
                }
            }
            return;
        }
        if let Some(idx) = row_in(self.layout.panes, x, y) {
            let n = self.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
            if idx < n {
                let already = self.focus == Focus::Panes && self.sel_pane == idx;
                self.sel_pane = idx;
                self.focus = Focus::Panes;
                self.clamp();
                if already {
                    self.descend();
                }
            }
        }
    }

    fn move_selection(&mut self, delta: i32) {
        self.adjust_selection(self.focus, delta);
    }

    fn adjust_selection(&mut self, target: Focus, delta: i32) {
        let sel = match target {
            Focus::Projects => &mut self.sel_project,
            Focus::Checkouts => &mut self.sel_checkout,
            Focus::Panes => &mut self.sel_pane,
            Focus::PaneContent => return,
        };
        let new = *sel as i32 + delta;
        if new >= 0 {
            *sel = new as usize;
        }
        self.clamp();
    }

    fn descend(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.current_project().is_some() {
                    self.sel_checkout = 0;
                    self.focus = Focus::Checkouts;
                }
            }
            Focus::Checkouts => {
                if self.current_checkout().is_some() {
                    self.sel_pane = 0;
                    self.focus = Focus::Panes;
                }
            }
            Focus::Panes => {
                if self.current_pane().is_some() {
                    self.focus = Focus::PaneContent;
                }
            }
            Focus::PaneContent => {}
        }
    }

    fn ascend(&mut self) {
        match self.focus {
            Focus::PaneContent => {
                // Deliberately does not unsubscribe: the live view keeps
                // showing this pane in the rightmost column while browsing.
                self.leader_pending = false;
                self.focus = Focus::Panes;
            }
            Focus::Panes => self.focus = Focus::Checkouts,
            Focus::Checkouts => self.focus = Focus::Projects,
            Focus::Projects => {}
        }
    }

    fn spawn_shell(&mut self) {
        if let Some(checkout) = self.current_checkout() {
            let _ = self.out.send(ClientMsg::SpawnShell { checkout: checkout.id });
            self.pending_focus_new = true;
        }
    }

    fn open_picker(&mut self) {
        if self.templates.is_empty() || self.current_checkout().is_none() {
            return;
        }
        self.picker = Some(Picker {
            items: self.templates.clone(),
            sel: 0,
        });
    }

    fn confirm_picker(&mut self) {
        let Some(picker) = self.picker.take() else { return };
        let Some(name) = picker.items.get(picker.sel) else { return };
        if let Some(checkout) = self.current_checkout() {
            let _ = self.out.send(ClientMsg::SpawnAgent {
                checkout: checkout.id,
                template: name.clone(),
            });
            self.pending_focus_new = true;
        }
    }

    fn kill_selected(&mut self) {
        if self.focus == Focus::Panes {
            self.close_current();
        }
    }

    /// Closes whatever pane is currently shown in the live view — reachable
    /// both from the open-agents list (`x`) and, via the leader chord, from
    /// inside the pane itself (`<leader>x`), since a bare `x` there is just
    /// a character typed at the child.
    fn close_current(&mut self) {
        if let Some(pane) = self.current_pane() {
            let _ = self.out.send(ClientMsg::Kill { pane: pane.id });
            // Land back in the open-agents list rather than staying "in"
            // PaneContent — the pane at this index may now be a different
            // one once the removal lands, and typing should never go to a
            // pane the user didn't choose.
            self.focus = Focus::Panes;
        }
    }

    pub fn resize_pane(&mut self, rows: u16, cols: u16) {
        if let Some(pane) = self.subscribed {
            let _ = self.out.send(ClientMsg::Resize { pane, rows, cols });
        }
    }
}

fn row_in(area: Rect, x: u16, y: u16) -> Option<usize> {
    if !in_rect(area, x, y) {
        return None;
    }
    Some((y - area.y) as usize)
}

fn in_rect(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}
