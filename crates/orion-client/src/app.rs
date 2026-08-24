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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use orion_protocol::{Cell, CellSpan, CheckoutId, GitStatus, PaneKind, PaneStatus, ProjectId};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn pane(id: u64, title: &str) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind: PaneKind::Shell,
            title: title.to_string(),
            status: PaneStatus::Idle,
        }
    }

    fn checkout(id: u64, name: &str, primary: bool, panes: Vec<PaneInfo>) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(id),
            name: name.to_string(),
            path: format!("/repo/{name}"),
            primary,
            git: None,
            panes,
        }
    }

    /// Two projects; the first has a primary checkout with two panes and a
    /// linked worktree with none, the second has a single empty checkout.
    fn tree() -> Vec<ProjectInfo> {
        vec![
            ProjectInfo {
                id: ProjectId(1),
                name: "orion".to_string(),
                checkouts: vec![
                    checkout(10, "master", true, vec![pane(100, "shell"), pane(101, "claude")]),
                    checkout(11, "feat", false, vec![]),
                ],
            },
            ProjectInfo {
                id: ProjectId(2),
                name: "other".to_string(),
                checkouts: vec![checkout(20, "main", true, vec![])],
            },
        ]
    }

    struct Harness {
        app: App,
        rx: UnboundedReceiver<ClientMsg>,
    }

    impl Harness {
        /// An app that has already received the fixture tree, with the
        /// resulting Subscribe traffic drained so tests assert only on what
        /// they themselves trigger.
        fn new() -> Self {
            let (tx, rx) = unbounded_channel();
            let mut app = App::new(tx);
            app.on_server_msg(ServerMsg::Tree(tree()));
            app.templates = vec!["claude".to_string(), "codex".to_string()];
            let mut h = Harness { app, rx };
            h.sent();
            h
        }

        fn key(&mut self, code: KeyCode) {
            self.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }

        fn leader(&mut self) {
            self.key(KeyCode::Null);
        }

        fn keys(&mut self, s: &str) {
            for c in s.chars() {
                self.key(KeyCode::Char(c));
            }
        }

        fn sent(&mut self) -> Vec<ClientMsg> {
            let mut out = Vec::new();
            while let Ok(msg) = self.rx.try_recv() {
                out.push(msg);
            }
            out
        }
    }

    // --- Miller-column navigation -----------------------------------------

    #[test]
    fn starts_focused_on_projects() {
        let h = Harness::new();
        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.current_project().unwrap().name, "orion");
    }

    #[test]
    fn l_descends_and_h_ascends_through_every_column() {
        let mut h = Harness::new();
        for expected in [Focus::Checkouts, Focus::Panes, Focus::PaneContent] {
            h.key(KeyCode::Char('l'));
            assert_eq!(h.app.focus, expected);
        }
        // Leaving the innermost column needs the leader chord: a bare `h`
        // there is a character typed at the child, not a navigation key.
        h.leader();
        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        for expected in [Focus::Checkouts, Focus::Projects] {
            h.key(KeyCode::Char('h'));
            assert_eq!(h.app.focus, expected);
        }
    }

    #[test]
    fn ascending_past_projects_is_a_no_op() {
        let mut h = Harness::new();
        h.keys("hhh");
        assert_eq!(h.app.focus, Focus::Projects, "must not fall off the left");
    }

    #[test]
    fn cannot_descend_into_a_checkout_with_no_panes() {
        let mut h = Harness::new();
        h.keys("lj"); // checkouts column, select the linked worktree
        assert_eq!(h.app.current_checkout().unwrap().name, "feat");
        h.keys("ll");
        assert_eq!(h.app.focus, Focus::Panes, "no pane to descend into");
    }

    #[test]
    fn j_and_k_move_within_the_focused_column_only() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.sel_project, 1);
        assert_eq!(h.app.sel_checkout, 0, "other columns untouched");
        h.key(KeyCode::Char('k'));
        assert_eq!(h.app.sel_project, 0);
    }

    #[test]
    fn selection_does_not_run_off_either_end() {
        let mut h = Harness::new();
        h.keys("kkk");
        assert_eq!(h.app.sel_project, 0);
        h.keys("jjjjj");
        assert_eq!(h.app.sel_project, 1, "clamped to the last project");
    }

    #[test]
    fn descending_resets_the_child_columns_selection() {
        let mut h = Harness::new();
        h.keys("llj"); // into panes, select the second pane
        assert_eq!(h.app.sel_pane, 1);
        h.keys("hh"); // back to projects
        h.keys("ll"); // descend again
        assert_eq!(h.app.sel_pane, 0, "re-entering a column starts at the top");
    }

    #[test]
    fn moving_to_a_project_with_fewer_checkouts_clamps_the_selection() {
        let mut h = Harness::new();
        h.keys("lj"); // checkouts, index 1
        assert_eq!(h.app.sel_checkout, 1);
        h.app.sel_project = 1; // "other" has only one checkout
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.sel_checkout, 0, "clamped into range");
    }

    // --- live-view subscription -------------------------------------------

    #[test]
    fn the_live_view_subscribes_to_the_selected_pane_without_descending() {
        // The rightmost column always shows a pane; it never has to take
        // over the screen for content to be visible.
        let mut h = Harness::new();
        assert_eq!(h.app.subscribed, Some(PaneId(100)), "first pane, from Projects focus");
        assert!(h.sent().is_empty());
    }

    #[test]
    fn changing_pane_selection_unsubscribes_the_old_and_subscribes_the_new() {
        let mut h = Harness::new();
        h.keys("llj");
        let msgs = h.sent();
        assert!(
            matches!(msgs[0], ClientMsg::Unsubscribe { pane: PaneId(100) }),
            "{msgs:?}"
        );
        assert!(matches!(msgs[1], ClientMsg::Subscribe { pane: PaneId(101) }), "{msgs:?}");
        assert_eq!(h.app.subscribed, Some(PaneId(101)));
    }

    #[test]
    fn selecting_a_paneless_checkout_unsubscribes_and_clears_the_grid() {
        let mut h = Harness::new();
        h.app.grid = Some(crate::grid::Grid::new(vec![]));
        h.keys("lj");
        assert_eq!(h.app.subscribed, None);
        assert!(h.app.grid.is_none(), "stale content must not linger");
        assert!(matches!(h.sent()[0], ClientMsg::Unsubscribe { .. }));
    }

    #[test]
    fn ascending_out_of_a_pane_keeps_it_subscribed() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.leader();
        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        assert_eq!(h.app.subscribed, Some(PaneId(100)), "live view keeps showing it");
        assert!(h.sent().is_empty(), "no resubscribe churn");
    }

    #[test]
    fn damage_for_an_unsubscribed_pane_is_ignored() {
        let mut h = Harness::new();
        h.app.grid = Some(crate::grid::Grid::new(vec![vec![Cell::default()]]));
        h.app.on_server_msg(ServerMsg::Damage {
            pane: PaneId(999),
            spans: vec![CellSpan {
                row: 0,
                col: 0,
                cells: vec![Cell {
                    ch: "X".to_string(),
                    ..Default::default()
                }],
            }],
        });
        assert_eq!(h.app.grid.as_ref().unwrap().cells[0][0].ch, " ");
    }

    #[test]
    fn a_snapshot_for_the_subscribed_pane_installs_the_grid() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneSnapshot {
            pane: PaneId(100),
            rows: 1,
            cols: 1,
            cells: vec![vec![Cell::default()]],
        });
        assert!(h.app.grid.is_some());
    }

    // --- typing into a pane ------------------------------------------------

    #[test]
    fn keys_reach_the_child_when_inside_a_pane() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.keys("echo");
        h.key(KeyCode::Enter);

        let bytes: Vec<u8> = h
            .sent()
            .into_iter()
            .flat_map(|m| match m {
                ClientMsg::Input { pane, bytes } => {
                    assert_eq!(pane, PaneId(100));
                    bytes
                }
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(bytes, b"echo\r");
    }

    #[test]
    fn navigation_keys_are_typed_not_interpreted_inside_a_pane() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.keys("hjkq");
        assert_eq!(h.app.focus, Focus::PaneContent, "still typing");
        assert!(!h.app.should_quit, "q must not detach from inside a pane");
        assert_eq!(h.sent().len(), 4, "all four went to the child");
    }

    #[test]
    fn leader_then_esc_leaves_the_pane_without_typing_anything() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.leader();
        assert!(h.app.leader_pending);
        assert!(h.sent().is_empty(), "the leader itself is never forwarded");

        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.leader_pending);
        assert!(h.sent().is_empty());
    }

    #[test]
    fn leader_then_x_closes_the_pane() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.leader();
        h.key(KeyCode::Char('x'));
        assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
        assert_eq!(h.app.focus, Focus::Panes, "land back in the list, not on another pane");
    }

    #[test]
    fn an_unbound_leader_chord_is_swallowed_not_typed() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.leader();
        h.key(KeyCode::Char('Q'));
        assert!(h.sent().is_empty());
        assert!(!h.app.leader_pending, "chord consumed");
    }

    // --- spawning ----------------------------------------------------------

    #[test]
    fn s_spawns_a_shell_in_the_selected_checkout_and_focuses_it() {
        let mut h = Harness::new();
        h.keys("lj"); // the linked worktree, which has no panes
        h.sent();
        h.key(KeyCode::Char('s'));
        assert!(
            matches!(h.sent()[0], ClientMsg::SpawnShell { checkout: CheckoutId(11) }),
            "spawns into the selected checkout"
        );

        // The daemon's next tree carries the new pane.
        let mut t = tree();
        t[0].checkouts[1].panes.push(pane(102, "shell"));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.sel_pane, 0);
        assert_eq!(h.app.focus, Focus::PaneContent, "drops you straight into it");
        assert_eq!(h.app.subscribed, Some(PaneId(102)));
    }

    #[test]
    fn a_spawn_focuses_the_newest_pane_not_the_first() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('s'));
        h.sent();
        let mut t = tree();
        t[0].checkouts[0].panes.push(pane(102, "shell"));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.subscribed, Some(PaneId(102)));
    }

    #[test]
    fn a_picks_an_agent_template_and_spawns_it() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        assert!(h.app.picker.is_some());
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 1);
        h.key(KeyCode::Enter);
        assert!(h.app.picker.is_none());
        match &h.sent()[0] {
            ClientMsg::SpawnAgent { checkout, template } => {
                assert_eq!(*checkout, CheckoutId(10));
                assert_eq!(template, "codex");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn esc_cancels_the_agent_picker_without_spawning() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.key(KeyCode::Esc);
        assert!(h.app.picker.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn the_picker_selection_does_not_run_past_the_ends() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.keys("jjj");
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 1, "two templates");
        h.keys("kkk");
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 0);
    }

    #[test]
    fn the_picker_swallows_navigation_keys() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.key(KeyCode::Char('l'));
        assert_eq!(h.app.focus, Focus::Projects, "column focus must not move behind the modal");
    }

    // --- prompts -----------------------------------------------------------

    #[test]
    fn n_in_the_projects_column_prompts_for_a_directory() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        assert!(matches!(h.app.prompt, Some(Prompt::AddProject { .. })));

        h.keys("/some/dir");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::AddProject { path } => assert_eq!(path, "/some/dir"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn a_new_project_becomes_the_selected_one() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("/d");
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t.push(ProjectInfo {
            id: ProjectId(3),
            name: "new".to_string(),
            checkouts: vec![checkout(30, "new", true, vec![])],
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_project().unwrap().name, "new");
    }

    #[test]
    fn n_in_the_checkouts_column_prompts_for_a_branch() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.sent();
        h.key(KeyCode::Char('n'));
        assert!(matches!(h.app.prompt, Some(Prompt::NewWorktree { .. })));

        h.keys("feat/x");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::CreateWorktree { checkout, branch } => {
                assert_eq!(*checkout, CheckoutId(10), "branched off the selected checkout");
                assert_eq!(branch, "feat/x");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_new_worktree_becomes_the_selected_checkout() {
        let mut h = Harness::new();
        h.keys("ln");
        h.keys("x");
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t[0].checkouts.push(checkout(12, "x", false, vec![]));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_checkout().unwrap().name, "x");
    }

    #[test]
    fn n_does_nothing_in_the_pane_columns() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('n'));
        assert!(h.app.prompt.is_none(), "no checkout context to branch from");
    }

    #[test]
    fn an_empty_prompt_sends_nothing() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("   ");
        h.key(KeyCode::Enter);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty(), "whitespace is not a path");
    }

    #[test]
    fn esc_cancels_a_prompt_and_backspace_edits_it() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("abc");
        h.key(KeyCode::Backspace);
        match &h.app.prompt {
            Some(Prompt::AddProject { input }) => assert_eq!(input, "ab"),
            _ => panic!("expected the add-project prompt to still be open"),
        }
        h.key(KeyCode::Esc);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn a_prompt_swallows_navigation_keys() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("jl");
        assert_eq!(h.app.sel_project, 0, "j typed into the prompt, not a move");
        assert_eq!(h.app.focus, Focus::Projects);
    }

    // --- removing a checkout ------------------------------------------------

    #[test]
    fn the_primary_checkout_cannot_be_removed() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.sent();
        h.key(KeyCode::Char('D'));
        assert!(h.app.prompt.is_none(), "no confirmation is even offered");
        assert!(h.sent().is_empty(), "and nothing is sent to the daemon");
        assert!(h.app.status.contains("primary"));
    }

    #[test]
    fn removing_a_linked_worktree_asks_first_then_sends() {
        let mut h = Harness::new();
        h.keys("lj");
        h.sent();
        h.key(KeyCode::Char('D'));
        match &h.app.prompt {
            Some(Prompt::ConfirmRemoveCheckout { checkout, label }) => {
                assert_eq!(*checkout, CheckoutId(11));
                assert_eq!(label, "feat", "the confirmation names what it will delete");
            }
            _ => panic!("expected a confirmation prompt"),
        }
        assert!(h.sent().is_empty(), "nothing sent before confirming");

        h.key(KeyCode::Char('y'));
        assert!(matches!(
            h.sent()[0],
            ClientMsg::RemoveCheckout {
                checkout: CheckoutId(11)
            }
        ));
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn declining_the_removal_sends_nothing() {
        for decline in ['n', 'q'] {
            let mut h = Harness::new();
            h.keys("lj");
            h.sent();
            h.key(KeyCode::Char('D'));
            h.key(KeyCode::Char(decline));
            if decline == 'n' {
                assert!(h.app.prompt.is_none(), "n declines");
            }
            assert!(h.sent().is_empty(), "{decline} must not delete anything");
        }
    }

    #[test]
    fn d_only_applies_to_the_checkouts_column() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('D'));
        assert!(h.app.prompt.is_none(), "not from the projects column");
    }

    // --- closing panes ------------------------------------------------------

    #[test]
    fn x_closes_the_selected_pane_from_the_panes_column() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('x'));
        assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
    }

    #[test]
    fn x_does_nothing_from_the_other_columns() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('x'));
        h.key(KeyCode::Char('l'));
        h.sent();
        h.key(KeyCode::Char('x'));
        assert!(h.sent().is_empty(), "x is not a global delete");
    }

    // --- tree updates -------------------------------------------------------

    #[test]
    fn a_shrinking_tree_clamps_the_selection_instead_of_dangling() {
        let mut h = Harness::new();
        h.keys("llj"); // second pane of the first checkout
        h.sent();
        let mut t = tree();
        t[0].checkouts[0].panes.pop();
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.sel_pane, 0);
        assert_eq!(h.app.subscribed, Some(PaneId(100)));
    }

    #[test]
    fn an_empty_tree_leaves_nothing_selected_and_nothing_subscribed() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(Vec::new()));
        assert!(h.app.current_project().is_none());
        assert_eq!(h.app.sel_project, 0);
        assert_eq!(h.app.subscribed, None);
    }

    #[test]
    fn templates_arrive_out_of_band() {
        let (tx, _rx) = unbounded_channel();
        let mut app = App::new(tx);
        assert!(app.templates.is_empty());
        app.on_server_msg(ServerMsg::Templates(vec!["claude".to_string()]));
        assert_eq!(app.templates, vec!["claude"]);
    }

    #[test]
    fn a_picker_will_not_open_with_no_templates() {
        let (tx, _rx) = unbounded_channel();
        let mut app = App::new(tx);
        app.on_server_msg(ServerMsg::Tree(tree()));
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(app.picker.is_none());
    }

    #[test]
    fn a_pane_exit_is_reported_in_the_status_line() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(1),
        });
        assert!(h.app.status.contains("exited"), "{}", h.app.status);
    }

    #[test]
    fn a_daemon_error_is_surfaced_not_swallowed() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        assert!(h.app.status.contains("git worktree add failed"));
    }

    #[test]
    fn git_status_rides_along_on_checkout_rows() {
        let mut h = Harness::new();
        let mut t = tree();
        t[0].checkouts[0].git = Some(GitStatus {
            branch: Some("master".to_string()),
            dirty: true,
            changed_files: 2,
            ahead: 1,
            behind: 0,
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
        let g = h.app.current_checkout().unwrap().git.as_ref().unwrap();
        assert_eq!(g.branch.as_deref(), Some("master"));
        assert_eq!(g.changed_files, 2);
    }

    #[test]
    fn q_detaches_from_the_nav_columns() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('q'));
        assert!(h.app.should_quit);
    }

    // --- mouse --------------------------------------------------------------

    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn laid_out(h: &mut Harness) {
        h.app.layout = Layout {
            projects: Rect::new(0, 0, 10, 5),
            checkouts: Rect::new(10, 0, 10, 5),
            panes: Rect::new(20, 0, 10, 5),
            content: Rect::new(30, 0, 20, 5),
        };
    }

    #[test]
    fn clicking_a_row_selects_it_and_focuses_that_column() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(12, 1));
        assert_eq!(h.app.focus, Focus::Checkouts);
        assert_eq!(h.app.sel_checkout, 1);
    }

    #[test]
    fn clicking_an_already_selected_row_descends() {
        let mut h = Harness::new();
        laid_out(&mut h);
        // Row 1 isn't the current selection, so the first click only selects.
        h.app.on_mouse(click(2, 1));
        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.sel_project, 1);
        // Clicking the now-selected row again opens it.
        h.app.on_mouse(click(2, 1));
        assert_eq!(h.app.focus, Focus::Checkouts, "second click opens it");
    }

    #[test]
    fn clicking_past_the_last_row_changes_nothing() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(2, 4));
        assert_eq!(h.app.sel_project, 0);
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn scrolling_a_background_column_does_not_steal_focus() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.keys("lll"); // typing into a pane
        h.sent();
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(h.app.sel_project, 1, "the scroll still moved the list");
        assert_eq!(h.app.focus, Focus::PaneContent, "but focus stayed put");
    }

    #[test]
    fn clicking_the_live_view_switches_to_typing_and_forwards_the_click() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(35, 2));
        assert_eq!(h.app.focus, Focus::PaneContent);
        assert!(
            h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })),
            "the child gets the click too"
        );
    }

    #[test]
    fn the_mouse_is_ignored_while_a_modal_is_open() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.key(KeyCode::Char('n'));
        h.app.on_mouse(click(12, 1));
        assert_eq!(h.app.sel_checkout, 0, "click must not navigate behind the prompt");
        assert!(h.app.prompt.is_some());
    }

    #[test]
    fn resize_is_forwarded_for_the_subscribed_pane() {
        let mut h = Harness::new();
        h.app.resize_pane(30, 100);
        match &h.sent()[0] {
            ClientMsg::Resize { pane, rows, cols } => {
                assert_eq!(*pane, PaneId(100));
                assert_eq!((*rows, *cols), (30, 100));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn resize_with_nothing_subscribed_sends_nothing() {
        let (tx, mut rx) = unbounded_channel();
        let mut app = App::new(tx);
        app.resize_pane(30, 100);
        assert!(rx.try_recv().is_err());
    }
}
