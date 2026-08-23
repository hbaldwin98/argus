use crossterm::event::{KeyCode, KeyEvent};
use orion_protocol::{CheckoutInfo, ClientMsg, PaneId, PaneInfo, ProjectInfo, ServerMsg};
use tokio::sync::mpsc::UnboundedSender;

use crate::grid::Grid;
use crate::keys::{encode_key, is_leader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Checkouts,
    Panes,
    PaneContent,
}

pub struct App {
    pub tree: Vec<ProjectInfo>,
    pub focus: Focus,
    pub sel_project: usize,
    pub sel_checkout: usize,
    pub sel_pane: usize,
    pub subscribed: Option<PaneId>,
    pub grid: Option<Grid>,
    pub leader_pending: bool,
    pub should_quit: bool,
    pub status: String,
    out: UnboundedSender<ClientMsg>,
}

impl App {
    pub fn new(out: UnboundedSender<ClientMsg>) -> Self {
        App {
            tree: Vec::new(),
            focus: Focus::Projects,
            sel_project: 0,
            sel_checkout: 0,
            sel_pane: 0,
            subscribed: None,
            grid: None,
            leader_pending: false,
            should_quit: false,
            status: "j/k move  l/enter open  h/esc back  s: shell  x: kill  q: detach".to_string(),
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
    }

    pub fn on_server_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Tree(t) => {
                self.tree = t;
                self.clamp();
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
        if self.focus == Focus::PaneContent {
            self.on_key_pane_content(key);
        } else {
            self.on_key_nav(key);
        }
    }

    fn on_key_pane_content(&mut self, key: KeyEvent) {
        if self.leader_pending {
            self.leader_pending = false;
            if key.code == KeyCode::Esc {
                self.ascend();
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
            KeyCode::Char('x') => self.kill_selected(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let sel = match self.focus {
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
                if let Some(pane) = self.current_pane() {
                    let id = pane.id;
                    self.subscribed = Some(id);
                    self.grid = None;
                    let _ = self.out.send(ClientMsg::Subscribe { pane: id });
                    self.focus = Focus::PaneContent;
                }
            }
            Focus::PaneContent => {}
        }
    }

    fn ascend(&mut self) {
        match self.focus {
            Focus::PaneContent => {
                if let Some(pane) = self.subscribed.take() {
                    let _ = self.out.send(ClientMsg::Unsubscribe { pane });
                }
                self.grid = None;
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
        }
    }

    fn kill_selected(&mut self) {
        if self.focus == Focus::Panes {
            if let Some(pane) = self.current_pane() {
                let _ = self.out.send(ClientMsg::Kill { pane: pane.id });
            }
        }
    }

    pub fn resize_pane(&mut self, rows: u16, cols: u16) {
        if let Some(pane) = self.subscribed {
            let _ = self.out.send(ClientMsg::Resize { pane, rows, cols });
        }
    }
}
