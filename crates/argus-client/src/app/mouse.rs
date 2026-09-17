//! The mouse, and the geometry it needs.
//!
//! A click has to be resolved against the same layout the last frame drew,
//! so the column arithmetic lives here beside the handlers rather than in
//! the renderer: what the operator clicked is whatever they were looking at.

use super::*;

impl App {
    /// Whether a mouse event can be ignored outright.
    ///
    /// Nothing in the client follows the pointer — there is no hover state,
    /// and `encode_mouse` has no VT sequence for a move with no button
    /// held — so a pointer crossing the terminal changes nothing on screen.
    /// It was still costing a full frame each, which is a few hundred
    /// milliseconds of drawing per second of moving the mouse. A drag in
    /// progress is a real change and is not idle.
    pub fn mouse_is_idle(&self, ev: &MouseEvent) -> bool {
        matches!(ev.kind, MouseEventKind::Moved)
            && self.resizing_gutter.is_none()
            && self.resizing_feature_gutter.is_none()
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
            if let Some(view) = crate::ui::tab_at(self.layout.views, ev.column, ev.row) {
                self.open_view(view);
                return;
            }
        }
        // Clicking the folded-away tab is the mouse equivalent of `p`: it
        // expands the column again. Handled before the column hit-test,
        // which would otherwise park focus on a handle with no rows.
        if matches!(ev.kind, MouseEventKind::Down(_)) && self.on_fold_tabs(ev.column, ev.row) {
            self.unfold_one();
            return;
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
        if matches!(ev.kind, MouseEventKind::Up(MouseButton::Left))
            && self.resizing_feature_gutter.take().is_some()
        {
            self.settings.feature_panel_heights = self.feature_panel_heights.clone();
            if self.persist_settings {
                crate::settings::save(&self.settings);
            }
            return;
        }
        if matches!(ev.kind, MouseEventKind::Up(MouseButton::Left))
            && self.resizing_gutter.take().is_some()
        {
            self.settings.column_widths = self.column_widths.clone();
            if self.persist_settings {
                crate::settings::save(&self.settings);
            }
            return;
        }
        if let MouseEventKind::Drag(MouseButton::Left) = ev.kind {
            if let Some(gutter) = self.resizing_feature_gutter {
                self.resize_feature_at(gutter, ev.row);
                return;
            }
            if let Some(gutter) = self.resizing_gutter {
                self.resize_columns_at(gutter, ev.column);
                return;
            }
        }
        if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
            if self.view == View::Feature {
                if let Some(gutter) = self.feature_gutter_at(ev.column, ev.row) {
                    self.feature_panel_heights = Some(self.rendered_feature_panel_heights());
                    self.resizing_feature_gutter = Some(gutter);
                    return;
                }
            }
            if !self.command_center {
                if let Some(gutter) = self.gutter_at(ev.column, ev.row) {
                    self.column_widths = Some(self.rendered_column_widths());
                    self.resizing_gutter = Some(gutter);
                    return;
                }
            }
        }
        if self.command_center
            && crate::ui::command_center_sidebar_contains(self, ev.column, ev.row)
        {
            match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    // An agent in the AGENTS list goes straight to it: its
                    // repository opens in the rail and its pane takes the stage.
                    if let Some(location) =
                        crate::ui::command_center_agent_at(self, ev.column, ev.row)
                    {
                        self.select_pane_location(location);
                        self.open_view(View::Spine);
                        self.focus = Focus::Panes;
                        self.clamp();
                        return;
                    }
                    if let Some(target) =
                        crate::ui::command_center_rail_target_at(self, ev.column, ev.row)
                    {
                        match target {
                            crate::ui::CommandCenterRailTarget::Repository(repository) => {
                                self.sel_repository = repository;
                                self.sel_checkout = 0;
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
                                self.open_view(View::Spine);
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
        // A view that is not the spine owns the content area outright, and
        // the pane whose column used to be there is not on screen. Without
        // this, a click on the board reads as a click on that pane: focus
        // lands in it and every later keypress goes to a child nobody can
        // see.
        if self.view != View::Spine {
            if self.view == View::Panes {
                if matches!(ev.kind, MouseEventKind::Down(MouseButton::Left)) {
                    if let Some(location) =
                        crate::ui::command_center_pane_at(self, ev.column, ev.row)
                    {
                        self.select_pane_location(location);
                        self.open_view(View::Spine);
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
                        (self.layout.feature_decisions, FeaturePanel::Decisions),
                    ];
                    let hit = panels
                        .into_iter()
                        .find(|(panel, _)| in_rect(panel.outer, ev.column, ev.row));
                    let Some((panel, which)) = hit else { return };
                    // The command center draws features and tasks on one
                    // line each; only decisions keep their reason line.
                    let height = match (self.command_center, which) {
                        (true, FeaturePanel::Features | FeaturePanel::Tasks) => 1,
                        _ => crate::ui::ROW_HEIGHT,
                    };
                    let row = row_in(panel.inner, height, ev.column, ev.row);
                    // A click in a panel's empty space still moves the
                    // keys there: the gesture said which panel to be in
                    // even when it landed past the last row.
                    match (which, row) {
                        (FeaturePanel::Features, Some(row)) => {
                            self.select_feature_row(row + panel.first)
                        }
                        (FeaturePanel::Tasks, Some(row)) => self.select_task(row + panel.first),
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
                    if self.command_center {
                        if in_rect(self.layout.feature_brief.outer, ev.column, ev.row) {
                            self.feature_brief_scroll = self
                                .feature_brief_scroll
                                .saturating_add_signed(delta as i16);
                            return;
                        }
                        // The wheel scrolls the section under it, not
                        // whichever one last had the keys.
                        if let Some((_, which)) = [
                            (self.layout.features, FeaturePanel::Features),
                            (self.layout.feature_tasks, FeaturePanel::Tasks),
                            (self.layout.feature_decisions, FeaturePanel::Decisions),
                        ]
                        .into_iter()
                        .find(|(panel, _)| in_rect(panel.outer, ev.column, ev.row))
                        {
                            self.go_to_panel(which);
                        }
                    }
                    self.move_in_feature(delta);
                }
                _ => {}
            }
            return;
        }
        // The live view is always visible in the rightmost column, so a
        // click landing on it both forwards to the child and (for presses)
        // switches into typing mode, regardless of what was focused before.
        //
        // The hit test is separate from the encoding: an event over the live
        // view belongs to the live view even when the child wants no mouse
        // reports and nothing is sent. Falling through to the nav handlers
        // would scroll the pane list under a wheel turn aimed at the pane.
        if in_rect(self.layout.content.inner, ev.column, ev.row) {
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::PaneContent;
            }
            if let Some(pane) = self.column_pane() {
                if self.start_selection(pane, &ev, self.layout.content.inner) {
                    return;
                }
                self.forward_mouse(pane, &ev, self.layout.content.inner);
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

    fn panels(&self) -> [Panel; 5] {
        [
            self.layout.projects,
            self.layout.repositories,
            self.layout.checkouts,
            self.layout.panes,
            self.layout.content,
        ]
    }

    fn rendered_column_widths(&self) -> Vec<u16> {
        let mut widths: Vec<u16> = self
            .panels()
            .iter()
            .map(|panel| panel.outer.width)
            .collect();
        // A tab is not a column. Keep the remembered width of each folded
        // one so expanding — or dragging another gutter while folded — does
        // not shrink it to a single cell.
        for (i, width) in widths.iter_mut().enumerate().take(self.fold.hidden()) {
            *width = self
                .column_widths
                .as_ref()
                .filter(|w| w.len() == 5)
                .and_then(|w| w.get(i).copied())
                .filter(|w| *w >= crate::ui::MIN_COLUMN_WIDTH)
                .unwrap_or(crate::ui::MIN_COLUMN_WIDTH);
        }
        widths
    }

    /// Returns the blank separator under the pointer. Separators remain one
    /// cell wide, which gives dragging an unambiguous target without taking
    /// clicks away from either panel's border.
    fn gutter_at(&self, x: u16, y: u16) -> Option<usize> {
        let panels = self.panels();
        // A folded-away tab is not a column; suppress the gutters against
        // them so the gap on the left edge is not a one-cell resize trap.
        let hidden = self.fold.hidden();
        panels
            .windows(2)
            .position(|pair| {
                let left = pair[0].outer;
                let right = pair[1].outer;
                let left_edge = left.x.saturating_add(left.width);
                x >= left_edge
                    && x < right.x
                    && y >= left.y.max(right.y)
                    && y < left
                        .y
                        .saturating_add(left.height)
                        .min(right.y.saturating_add(right.height))
            })
            .filter(|g| *g >= hidden)
    }

    /// The feature panels share one right-hand column, so their gutters are
    /// horizontal rather than part of the spine's column geometry.
    fn feature_gutter_at(&self, x: u16, y: u16) -> Option<usize> {
        let panels = [
            self.layout.feature_brief,
            self.layout.feature_tasks,
            self.layout.feature_decisions,
        ];
        panels.windows(2).enumerate().find_map(|(index, pair)| {
            let left = pair[0].outer;
            let right = pair[1].outer;
            if left.width == 0 || right.width == 0 {
                return None;
            }
            let x_start = left.x.max(right.x);
            let x_end = left
                .x
                .saturating_add(left.width)
                .min(right.x.saturating_add(right.width));
            let gutter_start = left.y.saturating_add(left.height);
            let gutter_end = right.y;
            (x >= x_start
                && x < x_end
                && y >= gutter_start
                && y < gutter_end
                && gutter_end.saturating_sub(gutter_start) == crate::ui::FEATURE_GUTTER_ROWS)
                .then_some(index)
        })
    }

    fn rendered_feature_panel_heights(&self) -> Vec<u16> {
        vec![
            self.layout.feature_brief.outer.height,
            self.layout.feature_tasks.outer.height,
            self.layout.feature_decisions.outer.height,
        ]
    }

    /// Move one horizontal separator while keeping both cards on either side
    /// above their floor. The gutter itself is not part of either height.
    fn resize_feature_at(&mut self, gutter: usize, y: u16) {
        let (left, right) = match gutter {
            0 => (self.layout.feature_brief, self.layout.feature_tasks),
            1 => (self.layout.feature_tasks, self.layout.feature_decisions),
            _ => return,
        };
        let pair_height = left.outer.height.saturating_add(right.outer.height);
        if pair_height < 2 {
            return;
        }
        let left_floor = if gutter == 0 {
            crate::ui::FEATURE_BRIEF_MIN_HEIGHT
        } else {
            crate::ui::FEATURE_PANEL_MIN_HEIGHT
        };
        let right_floor = crate::ui::FEATURE_PANEL_MIN_HEIGHT;
        let (left_floor, right_floor) = if pair_height >= left_floor + right_floor {
            (left_floor, right_floor)
        } else {
            let left_floor = (pair_height / 2).max(1);
            (left_floor, pair_height.saturating_sub(left_floor).max(1))
        };
        let left_height = y
            .saturating_sub(left.outer.y)
            .clamp(left_floor, pair_height.saturating_sub(right_floor));
        if self
            .feature_panel_heights
            .as_ref()
            .is_none_or(|heights| heights.len() != 3)
        {
            self.feature_panel_heights = Some(self.rendered_feature_panel_heights());
        }
        let heights = self
            .feature_panel_heights
            .as_mut()
            .expect("feature panel heights captured above");
        heights[gutter] = left_height;
        heights[gutter + 1] = pair_height - left_height;
    }

    fn resize_columns_at(&mut self, gutter: usize, x: u16) {
        let panels = self.panels();
        let left = panels[gutter].outer;
        let right = panels[gutter + 1].outer;
        let pair_width = left.width.saturating_add(right.width);
        if pair_width < 2 {
            return;
        }

        // The live view keeps its own, larger floor: dragging is how a user
        // gives a column room, not how they squeeze a terminal shut. On very
        // small terminals both scale down, but a column always retains at
        // least one cell instead of disappearing.
        let right_floor = if gutter + 1 == panels.len() - 1 {
            crate::ui::MIN_CONTENT_WIDTH
        } else {
            crate::ui::MIN_COLUMN_WIDTH
        };
        let room = crate::ui::MIN_COLUMN_WIDTH.saturating_add(right_floor);
        let scale = |n: u16| {
            if pair_width >= room || room == 0 {
                n
            } else {
                ((u32::from(n) * u32::from(pair_width)) / u32::from(room)).max(1) as u16
            }
        };
        let left_width = x.saturating_sub(left.x).clamp(
            scale(crate::ui::MIN_COLUMN_WIDTH),
            pair_width.saturating_sub(scale(right_floor)),
        );
        let rendered = self.rendered_column_widths();
        let widths = self.column_widths.get_or_insert(rendered);
        widths[gutter] = left_width;
        widths[gutter + 1] = pair_width - left_width;
    }

    /// Scroll-wheel selection change for whichever list column the cursor is
    /// over, independent of `focus` — so scrolling a background column
    /// doesn't steal focus away from a pane you're typing into.
    fn scroll_at(&mut self, x: u16, y: u16, delta: i32) {
        // A folded-away tab has nothing visible to scroll; a wheel event
        // landing there would otherwise change a hidden selection, which is
        // only ever confusing.
        if self.on_fold_tabs(x, y) {
            return;
        }
        let Some((target, _)) = self.column_at(x, y) else {
            return;
        };
        self.adjust_selection(target, delta);
    }

    /// Which list column a point falls in, anywhere on its card, along with
    /// the card itself — a long column is scrolled, so the row a click
    /// landed on is only a row index once the card's offset is added back.
    fn column_at(&self, x: u16, y: u16) -> Option<(Focus, Panel)> {
        for (focus, panel) in [
            (Focus::Projects, self.layout.projects),
            (Focus::Repositories, self.layout.repositories),
            (Focus::Checkouts, self.layout.checkouts),
            (Focus::Panes, self.layout.panes),
        ] {
            if !self.fold.hides(focus) && in_rect(panel.outer, x, y) {
                return Some((focus, panel));
            }
        }
        None
    }

    /// Whether a point is in the left page gutter the folded columns' tabs
    /// live in. Their panels are stacked there rather than laid out as
    /// cards, so this is one test rather than a search.
    fn on_fold_tabs(&self, x: u16, y: u16) -> bool {
        [self.layout.projects, self.layout.repositories]
            .iter()
            .take(self.fold.hidden())
            .any(|panel| in_rect(panel.outer, x, y))
    }

    /// A click on a card moves focus to it and leaves the selection alone;
    /// a click that lands on a row selects that row as well. Clicking the
    /// already-selected row a second time descends, the way `l` would.
    fn click_nav(&mut self, x: u16, y: u16) {
        // The content column has no rows to hit — clicking its frame just
        // puts keyboard focus back on whatever it is showing.
        if in_rect(self.layout.content.outer, x, y) {
            if self.review.is_some() {
                self.focus = Focus::Review;
            } else if self.current_pane().is_some() {
                self.focus = Focus::PaneContent;
            }
            // With nothing running there, focus would be a mode with no
            // keys and no way out but the leader.
            return;
        }

        let Some((target, panel)) = self.column_at(x, y) else {
            return;
        };
        // Two things stand between the row clicked and the row meant. The
        // card may be scrolled, so its first row is `panel.first` rather
        // than row zero; and the panes column draws each pane's children
        // under it, so a row there is not an index into the panes — a
        // click on a child row means the pane it is running in.
        let row = row_in(panel.inner, self.layout.row_height, x, y).map(|row| row + panel.first);
        if target == Focus::Panes {
            let hit = row.and_then(|row| crate::ui::pane_row_owners(self).get(row).copied());
            let already = self.focus == target && hit == self.pane_location();
            if let Some(location) = hit {
                self.select_pane_location(location);
            }
            self.focus = target;
            self.clamp();
            if already {
                self.descend();
            }
            return;
        }

        let count = match target {
            Focus::Projects => self.tree.len(),
            Focus::Repositories => self
                .current_project()
                .map(|p| p.repositories.len())
                .unwrap_or(0),
            Focus::Checkouts => self.checkout_row_count(),
            _ => 0,
        };
        let hit = row.filter(|idx| *idx < count);
        let already = self.focus == target && hit == Some(self.selection_in(target));
        if let Some(idx) = hit {
            *self.selection_mut(target) = idx;
        }
        self.focus = target;
        self.clamp();
        if already {
            self.descend();
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
            out.push((id, self.layout.content.inner));
        }
        if let Some(id) = self.overlay_pane() {
            out.push((id, self.layout.overlay.inner));
        }
        out
    }
}
