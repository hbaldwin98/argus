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
        matches!(ev.kind, MouseEventKind::Moved) && self.resizing_gutter.is_none()
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.picker.is_some() || self.prompt.is_some() || self.dir_picker.is_some() {
            return;
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
                self.forward_mouse(pane, &ev, self.layout.overlay.inner);
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
            if let Some(gutter) = self.resizing_gutter {
                self.resize_columns_at(gutter, ev.column);
                return;
            }
        }
        if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
            if let Some(gutter) = self.gutter_at(ev.column, ev.row) {
                self.column_widths = Some(self.rendered_column_widths());
                self.resizing_gutter = Some(gutter);
                return;
            }
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
                MouseEventKind::ScrollUp => self.scroll_pane(pane, super::scroll::wheel_lines(true)),
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
        let left_width = x
            .saturating_sub(left.x)
            .clamp(scale(crate::ui::MIN_COLUMN_WIDTH), pair_width.saturating_sub(scale(right_floor)));
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
