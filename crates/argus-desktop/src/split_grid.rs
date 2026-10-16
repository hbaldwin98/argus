//! Where the Workspace's panes sit in relation to each other, as pure
//! arithmetic: rows of panes, each row's column fractions and the rows'
//! heights, and what adding, closing, swapping and moving a pane does to
//! them. A pane is its number, 0 for the Workspace's own and `n` for the
//! `n`th beside it.
//!
//! The layout is explicit, not derived from the number of panes: a new
//! pane is placed by the grid rule (`ceil(sqrt(n))` to a row), and from
//! there the person rearranges it by hand and sizes it by dragging.
//! Where the panes are on screen is `shown`'s and the page's; this only
//! answers the shape.

/// The narrowest and shortest a pane of the grid may be, in cells. Below
/// this a terminal is no longer usable, so a pane that would leave one
/// there is refused.
pub const MIN_COLS: u16 = 20;
pub const MIN_ROWS: u16 = 6;

/// Where a pane dropped on another lands: on its centre it swaps places,
/// on a side it joins that row beside it, and on its top or bottom it
/// starts a row of its own there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Zone {
    Centre,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// A pane's place in the grid, as fractions of its width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    rows: Vec<Vec<usize>>,
    /// Each pane's share of its row's width, row by row.
    widths: Vec<Vec<f32>>,
    /// Each row's share of the grid's height.
    heights: Vec<f32>,
}

fn even(n: usize) -> Vec<f32> {
    vec![1.0 / n.max(1) as f32; n]
}

impl Default for Layout {
    fn default() -> Self {
        Layout { rows: vec![vec![0]], widths: vec![vec![1.0]], heights: vec![1.0] }
    }
}

impl Layout {
    pub fn rows(&self) -> &[Vec<usize>] {
        &self.rows
    }

    pub fn widths(&self) -> &[Vec<f32>] {
        &self.widths
    }

    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// The panes in reading order, left to right and top to bottom.
    pub fn order(&self) -> Vec<usize> {
        self.rows.iter().flatten().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.rows.iter().map(Vec::len).sum()
    }

    fn locate(&self, pane: usize) -> Option<(usize, usize)> {
        self.rows.iter().enumerate().find_map(|(r, row)| Some((r, row.iter().position(|&p| p == pane)?)))
    }

    /// Places the new pane `len()` by the grid rule: in the first row that
    /// is shorter than `ceil(sqrt(n))`, else in a row of its own, so an
    /// untouched grid fills top first — 2, 2 over 1, 2 over 2, 3 over 2,
    /// 3 over 3. Only the row it joins is made even, or the rows' heights
    /// when it starts one; every other size stays as it was dragged.
    pub fn add(&mut self) {
        let id = self.len();
        let across = (1..=id + 1).find(|c| c * c > id).unwrap_or(1);
        if let Some(r) = self.rows.iter().position(|row| row.len() < across) {
            self.rows[r].push(id);
            self.widths[r] = even(self.rows[r].len());
        } else {
            self.rows.push(vec![id]);
            self.widths.push(vec![1.0]);
            self.heights = even(self.rows.len());
        }
    }

    /// Takes `pane` out of its row, whose width goes to its neighbour on
    /// the left, or on the right for the first. A row left empty goes, its
    /// height to the row above or else below. Nothing else changes size.
    fn detach(&mut self, pane: usize) {
        let Some((r, c)) = self.locate(pane) else { return };
        self.rows[r].remove(c);
        let freed = self.widths[r].remove(c);
        if self.rows[r].is_empty() {
            self.rows.remove(r);
            self.widths.remove(r);
            let height = self.heights.remove(r);
            if let Some(next) = self.heights.get_mut(r.saturating_sub(1)) {
                *next += height;
            }
        } else {
            self.widths[r][c.saturating_sub(1)] += freed;
        }
    }

    /// Closes `pane`: the ones numbered after it are one less. The last
    /// pane cannot be taken out.
    pub fn remove(&mut self, pane: usize) {
        if self.len() < 2 {
            return;
        }
        self.detach(pane);
        for p in self.rows.iter_mut().flatten() {
            if *p > pane {
                *p -= 1;
            }
        }
    }

    /// Two panes trade places, each keeping the size of the place.
    pub fn swap(&mut self, a: usize, b: usize) {
        if let (Some((ra, ca)), Some((rb, cb))) = (self.locate(a), self.locate(b)) {
            self.rows[ra][ca] = b;
            self.rows[rb][cb] = a;
        }
    }

    /// `pane` dropped on `onto`'s `zone`.
    pub fn place(&mut self, pane: usize, onto: usize, zone: Zone) {
        if pane == onto || self.locate(pane).is_none() || self.locate(onto).is_none() {
            return;
        }
        if zone == Zone::Centre {
            return self.swap(pane, onto);
        }
        self.detach(pane);
        let Some((r, c)) = self.locate(onto) else { return };
        match zone {
            Zone::Left | Zone::Right => {
                let at = if zone == Zone::Left { c } else { c + 1 };
                self.rows[r].insert(at, pane);
                self.widths[r] = even(self.rows[r].len());
            }
            _ => {
                // The new row takes half of the one it is dropped beside.
                let half = self.heights[r] / 2.0;
                self.heights[r] = half;
                let at = if zone == Zone::Top { r } else { r + 1 };
                self.rows.insert(at, vec![pane]);
                self.widths.insert(at, vec![1.0]);
                self.heights.insert(at, half);
            }
        }
    }

    /// The pane next to `pane` in `dir`: in its row for left and right; for
    /// up and down the one in the next row whose middle is nearest its own.
    pub fn neighbour(&self, pane: usize, dir: Dir) -> Option<usize> {
        let (r, c) = self.locate(pane)?;
        match dir {
            Dir::Left => c.checked_sub(1).map(|c| self.rows[r][c]),
            Dir::Right => self.rows[r].get(c + 1).copied(),
            Dir::Up | Dir::Down => {
                let to = if dir == Dir::Up { r.checked_sub(1)? } else { r + 1 };
                let cells = self.cells();
                let middle = |p: usize| cells.iter().find(|(id, _)| *id == p).map(|(_, rect)| rect.x + rect.w / 2.0);
                let from = middle(pane)?;
                let near = |p: &&usize| (middle(**p).unwrap_or(0.0) - from).abs();
                self.rows.get(to)?.iter().min_by(|a, b| near(a).total_cmp(&near(b))).copied()
            }
        }
    }

    /// `pane` trades places with its neighbour in `dir`.
    pub fn step(&mut self, pane: usize, dir: Dir) {
        if let Some(other) = self.neighbour(pane, dir) {
            self.swap(pane, other);
        }
    }

    /// The rectangle of every pane, in reading order.
    pub fn cells(&self) -> Vec<(usize, Rect)> {
        let mut out = Vec::new();
        let mut y = 0.0;
        for ((row, widths), height) in self.rows.iter().zip(&self.widths).zip(&self.heights) {
            let mut x = 0.0;
            for (pane, w) in row.iter().zip(widths) {
                out.push((*pane, Rect { x, y, w: *w, h: *height }));
                x += w;
            }
            y += height;
        }
        out
    }

    /// The sizes the person dragged to, if they are the shape of this
    /// layout with every share positive, normalised. Which panes are where
    /// is not a drag's to change.
    pub fn resized(&self, heights: &[f32], widths: &[Vec<f32>]) -> Option<Layout> {
        let positive = |list: &[f32]| !list.is_empty() && list.iter().all(|s| s.is_finite() && *s > 0.0);
        let shaped = heights.len() == self.rows.len()
            && widths.len() == self.rows.len()
            && widths.iter().zip(&self.rows).all(|(w, r)| w.len() == r.len() && positive(w));
        if !shaped || !positive(heights) {
            return None;
        }
        let one = |list: &[f32]| {
            let total: f32 = list.iter().sum();
            list.iter().map(|s| s / total).collect::<Vec<_>>()
        };
        Some(Layout { rows: self.rows.clone(), widths: widths.iter().map(|w| one(w)).collect(), heights: one(heights) })
    }

    /// One row's panes made the same width; a row that is not there
    /// changes nothing.
    pub fn even_row(&mut self, row: usize) {
        if let Some(widths) = self.widths.get_mut(row) {
            *widths = even(widths.len());
        }
    }

    /// The rows made the same height.
    pub fn even_rows(&mut self) {
        self.heights = even(self.heights.len());
    }

    /// Whether every pane gets at least `MIN_COLS` by `MIN_ROWS` of an
    /// area `cols` by `rows` cells.
    pub fn fits(&self, cols: u16, rows: u16) -> bool {
        self.cells().iter().all(|(_, r)| r.w * f32::from(cols) >= f32::from(MIN_COLS) && r.h * f32::from(rows) >= f32::from(MIN_ROWS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grown(n: usize) -> Layout {
        let mut layout = Layout::default();
        for _ in 1..n {
            layout.add();
        }
        layout
    }

    #[test]
    fn new_panes_fill_rows_of_ceil_sqrt_top_first() {
        assert_eq!(grown(1).rows(), [[0]]);
        assert_eq!(grown(2).rows(), [vec![0, 1]]);
        assert_eq!(grown(3).rows(), [vec![0, 1], vec![2]]);
        assert_eq!(grown(4).rows(), [vec![0, 1], vec![2, 3]]);
        assert_eq!(grown(5).rows(), [vec![0, 1, 4], vec![2, 3]]);
        assert_eq!(grown(6).rows(), [vec![0, 1, 4], vec![2, 3, 5]]);
        assert_eq!(grown(7).rows(), [vec![0, 1, 4], vec![2, 3, 5], vec![6]]);
    }

    #[test]
    fn adding_evens_only_the_row_joined_or_the_heights_for_a_new_row() {
        let mut layout = grown(3);
        layout.widths[0] = vec![0.3, 0.7];
        layout.heights = vec![0.7, 0.3];
        layout.add();
        assert_eq!(layout.widths[0], [0.3, 0.7], "the first row keeps its drag");
        assert_eq!(layout.widths[1], [0.5, 0.5], "the row joined is even");
        assert_eq!(layout.heights, [0.7, 0.3], "no new row, no new heights");
        layout.add();
        layout.add();
        layout.add();
        assert_eq!(layout.rows().len(), 3);
        assert_eq!(layout.heights, [1.0 / 3.0; 3], "a new row evens the heights");
    }

    #[test]
    fn a_short_last_row_stretches_its_panes_across_the_width() {
        let cells = grown(3).cells();
        assert_eq!((cells[2].0, cells[2].1), (2, Rect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 }));
    }

    #[test]
    fn closing_gives_the_width_to_the_left_neighbour_or_the_right_for_the_first() {
        let mut layout = grown(2);
        layout.widths[0] = vec![0.25, 0.75];
        layout.remove(1);
        assert_eq!(layout.rows(), [[0]]);
        assert_eq!(layout.widths(), [[1.0]]);
        let mut layout = grown(3);
        layout.widths[0] = vec![0.25, 0.75];
        layout.remove(0);
        assert_eq!(layout.rows(), [vec![0], vec![1]], "the rest are numbered down");
        assert_eq!(layout.widths[0], [1.0]);
    }

    #[test]
    fn closing_the_last_of_a_row_drops_the_row_and_its_height_goes_to_a_neighbour_row() {
        let mut layout = grown(3);
        layout.heights = vec![0.8, 0.2];
        layout.widths[0] = vec![0.4, 0.6];
        layout.remove(2);
        assert_eq!(layout.rows(), [vec![0, 1]]);
        assert!((layout.heights()[0] - 1.0).abs() < 1e-6);
        assert_eq!(layout.widths[0], [0.4, 0.6], "what was dragged is left");
        let mut layout = grown(3);
        layout.heights = vec![0.8, 0.2];
        layout.remove(0);
        layout.remove(0);
        assert_eq!(layout.len(), 1);
        layout.remove(0);
        assert_eq!(layout.len(), 1, "the last pane stays");
    }

    #[test]
    fn dropping_on_the_centre_swaps_and_keeps_the_sizes_of_the_places() {
        let mut layout = grown(3);
        layout.widths[0] = vec![0.3, 0.7];
        layout.place(0, 2, Zone::Centre);
        assert_eq!(layout.rows(), [vec![2, 1], vec![0]]);
        assert_eq!(layout.widths[0], [0.3, 0.7]);
    }

    #[test]
    fn dropping_on_a_side_joins_that_row_beside_the_target_and_evens_it() {
        let mut layout = grown(3);
        layout.place(2, 0, Zone::Right);
        assert_eq!(layout.rows(), [vec![0, 2, 1]]);
        assert_eq!(layout.widths[0], [1.0 / 3.0; 3]);
        assert_eq!(layout.heights(), [1.0], "its old row went");
        layout.place(1, 0, Zone::Left);
        assert_eq!(layout.rows(), [vec![1, 0, 2]]);
    }

    #[test]
    fn dropping_on_an_edge_makes_a_row_of_its_own_taking_half_the_target_rows_height() {
        let mut layout = grown(4);
        layout.place(3, 0, Zone::Top);
        assert_eq!(layout.rows(), [vec![3], vec![0, 1], vec![2]]);
        assert_eq!(layout.heights(), [0.25, 0.25, 0.5]);
        layout.place(3, 2, Zone::Bottom);
        assert_eq!(layout.rows(), [vec![0, 1], vec![2], vec![3]]);
        assert_eq!(layout.heights().iter().sum::<f32>(), 1.0);
    }

    #[test]
    fn dropping_on_itself_or_a_pane_that_is_not_there_changes_nothing() {
        let mut layout = grown(3);
        let before = layout.clone();
        layout.place(1, 1, Zone::Left);
        layout.place(1, 9, Zone::Left);
        assert_eq!(layout, before);
    }

    #[test]
    fn neighbours_are_in_the_row_or_the_nearest_in_the_next() {
        // A shape only a person rearranging makes: rows of unequal length.
        let layout = Layout {
            rows: vec![vec![0, 1], vec![2, 3, 4], vec![5]],
            widths: vec![even(2), even(3), even(1)],
            heights: even(3),
        };
        assert_eq!(layout.neighbour(0, Dir::Right), Some(1));
        assert_eq!(layout.neighbour(0, Dir::Left), None);
        assert_eq!(layout.neighbour(1, Dir::Down), Some(4), "the right half is over the right third");
        assert_eq!(layout.neighbour(4, Dir::Up), Some(1));
        assert_eq!(layout.neighbour(5, Dir::Up), Some(3));
        assert_eq!(layout.neighbour(5, Dir::Down), None);
        let mut moved = layout.clone();
        moved.step(1, Dir::Down);
        assert_eq!(moved.rows(), [vec![0, 4], vec![2, 3, 1], vec![5]]);
    }

    #[test]
    fn reading_order_is_left_to_right_then_down() {
        let mut layout = grown(3);
        layout.place(2, 0, Zone::Left);
        assert_eq!(layout.order(), [2, 0, 1]);
    }

    #[test]
    fn dragged_sizes_must_keep_the_shape_and_are_normalised() {
        let layout = grown(3);
        let dragged = layout.resized(&[1.0, 3.0], &[vec![3.0, 1.0], vec![1.0]]).unwrap();
        assert_eq!(dragged.heights(), [0.25, 0.75]);
        assert_eq!(dragged.widths()[0], [0.75, 0.25]);
        assert_eq!(dragged.rows(), layout.rows());
        assert!(layout.resized(&[1.0], &[vec![1.0, 1.0]]).is_none());
        assert!(layout.resized(&[1.0, 0.0], &[vec![1.0, 1.0], vec![1.0]]).is_none());
        assert!(layout.resized(&[1.0, 1.0], &[vec![1.0, f32::NAN], vec![1.0]]).is_none());
    }

    #[test]
    fn evening_resets_a_row_or_the_rows_alone() {
        let mut layout = grown(3);
        layout.widths[0] = vec![0.1, 0.9];
        layout.heights = vec![0.25, 0.75];
        layout.even_row(0);
        layout.even_row(9);
        assert_eq!((layout.widths[0].clone(), layout.heights.clone()), (vec![0.5, 0.5], vec![0.25, 0.75]));
        layout.even_rows();
        assert_eq!(layout.heights(), [0.5, 0.5]);
    }

    #[test]
    fn a_pane_is_refused_when_it_would_leave_one_below_the_least() {
        assert!(grown(2).fits(40, 6) && !grown(2).fits(39, 6));
        assert!(!grown(3).fits(100, 11) && grown(3).fits(100, 12));
        assert!(!grown(5).fits(59, 12) && grown(5).fits(61, 12));
    }
}
