//! The mouse, and the geometry it needs.
//!
//! A click has to be resolved against the same layout the last frame drew:
//! what the operator clicked is whatever they were looking at. The rail and
//! each stage answer which of their rows a point is on (`ui`); this says
//! what landing there does.

use super::*;

impl App {
    /// Whether a mouse event can be ignored outright.
    ///
    /// Nothing in the client follows the pointer — there is no hover state,
    /// and `encode_mouse` has no VT sequence for a move with no button
    /// held — so a pointer crossing the terminal changes nothing on screen.
    /// It was still costing a full frame each, which is a few hundred
    /// milliseconds of drawing per second of moving the mouse.
    pub fn mouse_is_idle(&self, ev: &MouseEvent) -> bool {
        matches!(ev.kind, MouseEventKind::Moved)
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        self.handle_mouse(ev);
        // A click on the rail can move the checkout the feature view reads.
        self.refresh_board_if_stale();
    }

    fn handle_mouse(&mut self, ev: MouseEvent) {
        if self.picker.is_some() || self.prompt.is_some() || self.dir_picker.is_some() {
            return;
        }
        if self.update_selection(&ev) {
            return;
        }
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            self.selection = None;
        }
        // The keymap window is the same kind of modal as an overlay: a
        // click anywhere puts it away, and none of it reaches what is
        // underneath. Scrolling reads it.
        if self.help.is_some() {
            match ev.kind {
                MouseEventKind::ScrollDown => self.scroll_help(1),
                MouseEventKind::ScrollUp => self.scroll_help(-1),
                MouseEventKind::Down(_) => self.help = None,
                _ => {}
            }
            return;
        }
        // The tab strip is above every other surface, overlays included:
        // it is the one row on screen that is not about whatever is open.
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            if let Some(view) =
                crate::ui::tab_at(self.layout.views, ev.column, ev.row)
            {
                self.open_view_from_tab(view);
                return;
            }
        }
        // Same acknowledgement as a keypress, but only for a deliberate one:
        // a mouse crossing the terminal is not the user reading anything.
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            self.clear_status();
        }
        // A floating window is modal: clicks inside it are its own, and a
        // click outside dismisses it. Without this a click would fall
        // through to the columns underneath, moving focus while the keys
        // still went to the overlay — no way in and no way out.
        if self.overlay.is_some() {
            let inside = in_rect(self.layout.overlay.outer, ev.column, ev.row);
            if !inside {
                if matches!(ev.kind, MouseEventKind::Down(_)) {
                    self.close_overlay();
                }
                return;
            }
            if let Some(pane) = self.overlay_pane() {
                if self.start_selection(pane, &ev, self.layout.overlay.inner) {
                    return;
                }
                self.forward_mouse(pane, &ev, self.layout.overlay.inner);
            }
            return;
        }
        if crate::ui::command_center_sidebar_contains(self, ev.column, ev.row) {
            match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    // An agent in the AGENTS list goes straight to it: its
                    // repository opens in the rail and its pane takes the stage.
                    if let Some(location) =
                        crate::ui::command_center_agent_at(self, ev.column, ev.row)
                    {
                        self.select_pane_location(location);
                        self.open_view(View::Workspace);
                        self.focus = Focus::Panes;
                        self.clamp();
                        return;
                    }
                    if crate::ui::command_center_project_header_at(self, ev.column, ev.row) {
                        self.open_project_picker();
                        return;
                    }
                    if let Some(target) =
                        crate::ui::command_center_rail_target_at(self, ev.column, ev.row)
                    {
                        match target {
                            crate::ui::CommandCenterRailTarget::Repository(repository) => {
                                self.sel_repository = repository;
                                self.sel_checkout = self.home_checkout_row();
                                self.sel_pane = 0;
                                self.focus = Focus::Repositories;
                                if let Some(id) = self
                                    .current_project()
                                    .and_then(|project| project.repositories.get(repository))
                                    .map(|repository| repository.id)
                                {
                                    self.expanded_repositories.clear();
                                    self.expanded_repositories.insert(id);
                                }
                            }
                            crate::ui::CommandCenterRailTarget::Checkout(repository, checkout) => {
                                self.sel_repository = repository;
                                self.sel_checkout = checkout;
                                self.sel_pane = 0;
                                self.focus = Focus::Checkouts;
                            }
                            // The rail selects; the terminal itself takes
                            // typing. Keeping the keys here is what lets a
                            // bare `x` close the pane just clicked.
                            crate::ui::CommandCenterRailTarget::Pane(location) => {
                                self.select_pane_location(location);
                                self.open_view(View::Workspace);
                                self.focus = Focus::Panes;
                            }
                        }
                        self.clamp();
                    }
                }
                MouseEventKind::ScrollUp => self.adjust_selection(Focus::Repositories, -1),
                MouseEventKind::ScrollDown => self.adjust_selection(Focus::Repositories, 1),
                _ => {}
            }
            return;
        }
        // A view that is not the workspace owns the stage outright, and
        // the pane whose column used to be there is not on screen. Without
        // this, a click on the board reads as a click on that pane: focus
        // lands in it and every later keypress goes to a child nobody can
        // see.
        if self.view != View::Workspace {
            if self.view == View::Panes {
                if matches!(ev.kind, MouseEventKind::Down(MouseButton::Left)) {
                    if let Some(location) =
                        crate::ui::command_center_pane_at(self, ev.column, ev.row)
                    {
                        self.select_pane_location(location);
                        self.open_view(View::Workspace);
                        self.focus = Focus::PaneContent;
                    }
                }
                return;
            }
            if self.view == View::Checkouts {
                if matches!(ev.kind, MouseEventKind::Down(MouseButton::Left)) {
                    if let Some(checkout) =
                        crate::ui::command_center_checkout_at(self, ev.column, ev.row)
                    {
                        self.sel_checkout = checkout;
                        self.sel_pane = 0;
                        self.focus = Focus::Checkouts;
                        self.clamp();
                    }
                }
                return;
            }
            if self.view != View::Feature {
                return;
            }
            match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.focus = Focus::View;
                    let panels = [
                        (self.layout.features, FeaturePanel::Features),
                        (self.layout.feature_tasks, FeaturePanel::Tasks),
                        (self.layout.feature_diagrams, FeaturePanel::Diagrams),
                        (self.layout.feature_decisions, FeaturePanel::Decisions),
                    ];
                    let hit = panels
                        .into_iter()
                        .find(|(panel, _)| in_rect(panel.outer, ev.column, ev.row));
                    let Some((panel, which)) = hit else { return };
                    let row =
                        crate::ui::command_center_feature_row_at(self, which, ev.column, ev.row);
                    // A click in a panel's empty space still moves the
                    // keys there: the gesture said which panel to be in
                    // even when it landed past the last row.
                    match (which, row) {
                        (FeaturePanel::Features, Some(row)) => {
                            self.select_feature_row(row + panel.first)
                        }
                        (FeaturePanel::Tasks, Some(row)) => self.select_task(row + panel.first),
                        (FeaturePanel::Diagrams, Some(row)) => {
                            self.select_diagram(row + panel.first)
                        }
                        (FeaturePanel::Decisions, Some(row)) => {
                            self.select_decision_row(row + panel.first)
                        }
                        (which, None) => self.go_to_panel(which),
                    }
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    let delta = if matches!(ev.kind, MouseEventKind::ScrollUp) {
                        -1
                    } else {
                        1
                    };
                    if in_rect(self.layout.feature_brief.outer, ev.column, ev.row) {
                        self.feature_brief_scroll =
                            self.feature_brief_scroll.saturating_add_signed(delta as i16);
                        return;
                    }
                    // The wheel scrolls the section under it, not whichever
                    // one last had the keys.
                    if let Some((_, which)) = [
                        (self.layout.features, FeaturePanel::Features),
                        (self.layout.feature_tasks, FeaturePanel::Tasks),
                        (self.layout.feature_diagrams, FeaturePanel::Diagrams),
                        (self.layout.feature_decisions, FeaturePanel::Decisions),
                    ]
                    .into_iter()
                    .find(|(panel, _)| in_rect(panel.outer, ev.column, ev.row))
                    {
                        self.go_to_panel(which);
                    }
                    self.move_in_feature(delta);
                }
                _ => {}
            }
            return;
        }
        // A click landing on the workspace terminal both forwards to the
        // child and (for presses) switches into typing mode, regardless of
        // what was focused before.
        //
        // The hit test is separate from the encoding: an event over the
        // terminal belongs to it even when the child wants no mouse reports
        // and nothing is sent.
        if in_rect(self.layout.terminal.inner, ev.column, ev.row) {
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::PaneContent;
            }
            if let Some(pane) = self.column_pane() {
                if self.start_selection(pane, &ev, self.layout.terminal.inner) {
                    return;
                }
                self.forward_mouse(pane, &ev, self.layout.terminal.inner);
            }
            return;
        }
        // The terminal's frame has no rows to hit — clicking it just puts
        // keyboard focus back on whatever it is showing. With nothing
        // running there, focus would be a mode with no keys and no way out
        // but the leader.
        if matches!(ev.kind, MouseEventKind::Down(MouseButton::Left))
            && in_rect(self.layout.terminal.outer, ev.column, ev.row)
        {
            if self.review.is_some() {
                self.focus = Focus::Review;
            } else if self.current_pane().is_some() {
                self.focus = Focus::PaneContent;
            }
        }
    }

    /// What mouse reporting the child in `pane` has asked for. An unknown
    /// pane defaults to none, so nothing is forwarded until a snapshot has
    /// actually said otherwise.
    fn pane_mouse(&self, pane: PaneId) -> argus_protocol::MouseTracking {
        self.grids.get(&pane).map(|g| g.mouse).unwrap_or_default()
    }

    /// A pane without mouse reporting leaves left-drag to Argus. Shift is
    /// the explicit escape hatch when a mouse-aware child owns plain drag.
    fn start_selection(&mut self, pane: PaneId, ev: &MouseEvent, area: Rect) -> bool {
        if !matches!(ev.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }
        let local = !self.pane_mouse(pane).enabled() || ev.modifiers.contains(KeyModifiers::SHIFT);
        if local {
            self.selection = Some(crate::selection::TerminalSelection::start(
                pane, ev.column, ev.row, area,
            ));
        }
        local
    }

    /// Continue a local drag even beyond the pane border, like a native
    /// terminal selection. The selection owns events until left release.
    fn update_selection(&mut self, ev: &MouseEvent) -> bool {
        let relevant = matches!(
            ev.kind,
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
        );
        if !relevant || self.selection.is_none() {
            return false;
        }
        let selection = self.selection.as_mut().expect("checked above");
        selection.update(ev.column, ev.row);
        if matches!(ev.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.finish_selection();
        }
        true
    }

    fn finish_selection(&mut self) {
        let selection = self.selection.as_ref().expect("selection owns the release");
        let text = self
            .grids
            .get(&selection.pane)
            .map(|grid| selection.text(grid.view()))
            .unwrap_or_default();
        if !selection.moved() || text.is_empty() {
            self.selection = None;
        } else if (self.clipboard_write)(&text) {
            self.report("copied selection");
        } else {
            self.alert("could not write to the clipboard");
        }
    }

    /// Encode a mouse event for the child, or turn a wheel into a cursor
    /// key when the child is on the alternate screen without mouse
    /// reporting. That last path is xterm's alternate-scroll (DECSET 1007)
    /// and what Claude, Codex, and Cursor Agent actually listen for.
    fn forward_mouse(&mut self, pane: PaneId, ev: &MouseEvent, area: Rect) {
        if let Some(bytes) = encode_mouse(ev, area, self.pane_mouse(pane)) {
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
            return;
        }
        if !self.grids.get(&pane).is_some_and(|g| g.alternate_screen) {
            // The normal screen is the one with history behind it, so a
            // wheel there moves this client's view rather than reaching the
            // child at all. A shell prints and scrolls away; this is the
            // only way back to what it said.
            match ev.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_pane(pane, super::scroll::wheel_lines(true))
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_pane(pane, super::scroll::wheel_lines(false))
                }
                _ => {}
            }
            return;
        }
        let code = match ev.kind {
            MouseEventKind::ScrollUp => KeyCode::Up,
            MouseEventKind::ScrollDown => KeyCode::Down,
            _ => return,
        };
        let bytes = encode_key(&KeyEvent::new(code, KeyModifiers::NONE));
        if !bytes.is_empty() {
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
        }
    }

    pub fn resize_pane(&mut self, pane: PaneId, rows: u16, cols: u16) {
        let _ = self.out.send(ClientMsg::Resize { pane, rows, cols });
    }

    /// Every pane on screen with the area it is drawn in. Each pty is sized
    /// from its own, so a floating editor and the column behind it do not
    /// have to agree on a width.
    pub fn live_panes(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        if let Some(id) = self.column_pane() {
            out.push((id, self.layout.terminal.inner));
        }
        if let Some(id) = self.overlay_pane() {
            out.push((id, self.layout.overlay.inner));
        }
        out
    }
}
