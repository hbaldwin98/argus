//! Where a graph's boxes and arrows go: a layered layout, kept apart from the drawing so it is only
//! numbers (GRAPH-3). Cycles are broken by reversing the edges a depth-first walk finds going back,
//! nodes take the longest path from a root as their layer, an edge across several layers is given a
//! stand-in item in each so it has somewhere to bend, and a few barycenter sweeps order each layer
//! to cut crossings. Everything is worked out along two axes, the one the layers run down and the one
//! a layer spreads along, and only turned into x and y at the end, which is all `direction` changes.
//!
//! A port of the page's `graph_layout.ts`, which the page keeps drawing from; both stable-sort, so a
//! tie keeps the order the agent wrote and the same graph is laid out the same way every time.

use std::collections::{HashMap, HashSet, VecDeque};

use argus_protocol::{GraphEdge, GraphNode};

use crate::views::blocks::label::width as label_width;

/// The most characters of a node's or an edge's label drawn.
pub const MAX_LABEL: usize = 28;
pub const NODE_H: f32 = 34.0;
const MIN_W: f32 = 72.0;
const NODE_ROOM: f32 = 28.0;
/// The margin kept round the drawing.
pub const PAD: f32 = 16.0;
const SIDE_GAP: f32 = 28.0;
/// A stand-in's size, which is where an edge bends.
const DUMMY: f32 = 8.0;
const SWEEPS: usize = 4;
/// How far apart layers are, and the extra room a label needs between them.
const LAYER_GAP: f32 = 48.0;
const LABEL_ROW: f32 = 14.0;
/// What a node joined to itself loops out by: how far, how high, and the room the drawing keeps for it.
const LOOP_OUT: f32 = 18.0;
const LOOP_HALF: f32 = 6.0;
const LOOP_ROOM: f32 = 24.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// From the edge's `from` to its `to`, bending at right angles.
    pub points: Vec<(f32, f32)>,
    /// Where its label sits.
    pub mid: (f32, f32),
    /// A node joined to itself.
    pub is_loop: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    /// One per node, in the order given.
    pub nodes: Vec<Placed>,
    /// One per edge, in the order given; none for an edge to or from an id no node has.
    pub routes: Vec<Option<Route>>,
}

/// One box in a layer: a node, or the stand-in of an edge that crosses the layer.
struct Item {
    real: bool,
    layer: usize,
    /// Its size along the layers and across them.
    main: f32,
    cross: f32,
    /// Where it starts across the layer.
    at: f32,
    ins: Vec<usize>,
    outs: Vec<usize>,
}

/// Edges as index pairs with the ones that close a cycle turned around.
fn acyclic(count: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in edges {
        next[a].push(b);
    }
    // 0 unseen, 1 on the walk, 2 done.
    let mut state = vec![0u8; count];
    let mut back: HashSet<(usize, usize)> = HashSet::new();
    for root in 0..count {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&(n, i)) = stack.last() {
            if i >= next[n].len() {
                state[n] = 2;
                stack.pop();
                continue;
            }
            stack.last_mut().expect("the walk is not empty").1 = i + 1;
            let m = next[n][i];
            if state[m] == 1 {
                back.insert((n, m));
            } else if state[m] == 0 {
                state[m] = 1;
                stack.push((m, 0));
            }
        }
    }
    edges.iter().map(|&(a, b)| if back.contains(&(a, b)) { (b, a) } else { (a, b) }).collect()
}

/// Each node's layer: the longest path to it from a root.
fn layers(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut layer = vec![0usize; count];
    let mut left = vec![0usize; count];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in edges {
        out[a].push(b);
        left[b] += 1;
    }
    let mut ready: VecDeque<usize> = (0..count).filter(|i| left[*i] == 0).collect();
    while let Some(n) = ready.pop_front() {
        for &m in &out[n] {
            layer[m] = layer[m].max(layer[n] + 1);
            left[m] -= 1;
            if left[m] == 0 {
                ready.push_back(m);
            }
        }
    }
    layer
}

/// The mean of `of` over `around`, or `fallback` when there is nothing around.
fn mean(around: &[usize], of: impl Fn(usize) -> f32, fallback: f32) -> f32 {
    if around.is_empty() {
        fallback
    } else {
        around.iter().map(|m| of(*m)).sum::<f32>() / around.len() as f32
    }
}

/// The layout of `nodes` and `edges`; `right` runs the layers left to right and not down.
pub fn layout(nodes: &[GraphNode], edges: &[GraphEdge], right: bool) -> Layout {
    // A duplicated id is the last node that has it, as the page's map is.
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for (edge, e) in edges.iter().enumerate() {
        if let (Some(&a), Some(&b)) = (index.get(e.from.as_str()), index.get(e.to.as_str())) {
            if a != b {
                pairs.push((a, b, edge));
            }
        }
    }
    let flat = acyclic(nodes.len(), &pairs.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>());
    let layer = layers(nodes.len(), &flat);

    // Boxes are as wide as their label; `main` and `cross` follow the axes.
    let mut items: Vec<Item> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let w = MIN_W.max(label_width(&n.label, MAX_LABEL) + NODE_ROOM);
            Item { real: true, layer: layer[i], main: if right { w } else { NODE_H }, cross: if right { NODE_H } else { w }, at: 0.0, ins: Vec::new(), outs: Vec::new() }
        })
        .collect();
    // Each edge as the chain of items it passes, low layer first.
    struct Chain {
        items: Vec<usize>,
        reversed: bool,
        edge: usize,
    }
    let mut chains: Vec<Chain> = Vec::new();
    for (k, &(pa, _, edge)) in pairs.iter().enumerate() {
        let (a, b) = flat[k];
        let mut chain = vec![a];
        let mut prev = a;
        for l in layer[a] + 1..layer[b] {
            items.push(Item { real: false, layer: l, main: DUMMY, cross: DUMMY, at: 0.0, ins: Vec::new(), outs: Vec::new() });
            let d = items.len() - 1;
            items[prev].outs.push(d);
            items[d].ins.push(prev);
            chain.push(d);
            prev = d;
        }
        items[prev].outs.push(b);
        items[b].ins.push(prev);
        chain.push(b);
        chains.push(Chain { items: chain, reversed: a != pa, edge });
    }

    let depth = items.iter().map(|it| it.layer).max().map_or(0, |m| m + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for (i, it) in items.iter().enumerate() {
        rows[it.layer].push(i);
    }
    let mut slot = vec![0usize; items.len()];
    for row in &rows {
        for (i, n) in row.iter().enumerate() {
            slot[*n] = i;
        }
    }

    // Sorting is stable, so a tie keeps the order the agent wrote.
    let mut sweep = |down: bool| {
        let order: Vec<usize> = if down { (1..depth).collect() } else { (0..depth.saturating_sub(1)).rev().collect() };
        for r in order {
            let key: HashMap<usize, f32> = rows[r]
                .iter()
                .map(|&n| {
                    let around = if down { &items[n].ins } else { &items[n].outs };
                    (n, mean(around, |m| slot[m] as f32, slot[n] as f32))
                })
                .collect();
            rows[r].sort_by(|x, y| key[x].partial_cmp(&key[y]).unwrap_or(std::cmp::Ordering::Equal));
            for (i, n) in rows[r].iter().enumerate() {
                slot[*n] = i;
            }
        }
    };
    for _ in 0..SWEEPS {
        sweep(true);
        sweep(false);
    }

    // Across each layer, packed and centred; then each item drawn toward the middle of what it
    // joins, never past its neighbour.
    let extent = |row: &[usize], items: &[Item]| row.iter().map(|n| items[*n].cross).sum::<f32>() + row.len().saturating_sub(1) as f32 * SIDE_GAP;
    let widest = rows.iter().map(|row| extent(row, &items)).fold(0.0, f32::max);
    for row in &rows {
        let mut at = (widest - extent(row, &items)) / 2.0;
        for &n in row {
            items[n].at = at;
            at += items[n].cross + SIDE_GAP;
        }
    }
    let relax = |items: &mut Vec<Item>, order: Vec<usize>, down: bool| {
        for r in order {
            let mut floor = f32::NEG_INFINITY;
            for &n in &rows[r] {
                let around = if down { items[n].ins.clone() } else { items[n].outs.clone() };
                let want = mean(&around, |m| items[m].at + items[m].cross / 2.0, items[n].at + items[n].cross / 2.0) - items[n].cross / 2.0;
                items[n].at = want.max(floor);
                floor = items[n].at + items[n].cross + SIDE_GAP;
            }
        }
    };
    for _ in 0..3 {
        relax(&mut items, (1..depth).collect(), true);
        relax(&mut items, (0..depth.saturating_sub(1)).rev().collect(), false);
    }
    // The relaxing drifts everything one way, so the drawing is moved back to its edge whichever
    // way it went; clamping at 0 left a wide empty margin.
    let lowest = items.iter().map(|it| it.at).fold(f32::INFINITY, f32::min);
    let lowest = if lowest.is_finite() { lowest } else { 0.0 };
    for it in &mut items {
        it.at -= lowest;
    }
    let span = items.iter().map(|it| it.at + it.cross).fold(0.0, f32::max);

    // Layers are spaced to leave room for the longest edge label crossing them: along the layers a
    // label is as wide as its text, across them one line high.
    let labelled: Vec<&str> = edges.iter().filter_map(|e| e.label.as_deref()).filter(|l| !l.is_empty()).collect();
    let room = labelled.iter().map(|l| label_width(l, MAX_LABEL)).fold(0.0, f32::max);
    let gap = if right { LAYER_GAP + room } else { LAYER_GAP + if labelled.is_empty() { 0.0 } else { LABEL_ROW } };
    let thick: Vec<f32> = rows.iter().map(|row| row.iter().map(|n| items[*n].main).fold(0.0, f32::max)).collect();
    let mut start = Vec::with_capacity(depth);
    let mut run = 0.0;
    for t in &thick {
        start.push(run);
        run += t + gap;
    }
    let total = (run - gap).max(0.0);

    let main_at = |n: usize| start[items[n].layer] + (thick[items[n].layer] - items[n].main) / 2.0;
    let xy = |m: f32, c: f32| if right { (PAD + m, PAD + c) } else { (PAD + c, PAD + m) };

    let placed: Vec<Placed> = (0..nodes.len())
        .map(|i| {
            let (x, y) = xy(main_at(i), items[i].at);
            let (w, h) = if right { (items[i].main, items[i].cross) } else { (items[i].cross, items[i].main) };
            Placed { x, y, w, h }
        })
        .collect();

    let mut routes: Vec<Option<Route>> = vec![None; edges.len()];
    for c in &chains {
        let last = c.items.len() - 1;
        let stops: Vec<(f32, f32)> = c
            .items
            .iter()
            .enumerate()
            .map(|(i, &n)| {
                let it = &items[n];
                let (cross, top) = (it.at + it.cross / 2.0, main_at(n));
                // A box is left by its far edge and entered by its near one; a stand-in is passed
                // straight through.
                if it.real && i == 0 {
                    (top + it.main, cross)
                } else if it.real && i == last {
                    (top, cross)
                } else {
                    (top + it.main / 2.0, cross)
                }
            })
            .collect();
        // Every hop leaves along the layers, jogs across at its middle and arrives along them: right
        // angles only.
        let mut pts: Vec<(f32, f32)> = vec![xy(stops[0].0, stops[0].1)];
        for i in 1..stops.len() {
            let (pm, pc) = stops[i - 1];
            let (m, cr) = stops[i];
            let half = (pm + m) / 2.0;
            pts.extend([xy(half, pc), xy(half, cr), xy(m, cr)]);
        }
        let hop = stops.len() / 2;
        let (a, b) = (stops[hop.saturating_sub(1)], stops.get(hop).copied().unwrap_or(stops[0]));
        let mid = xy((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        if c.reversed {
            pts.reverse();
        }
        routes[c.edge] = Some(Route { points: pts, mid, is_loop: false });
    }
    // A node joined to itself loops out of its far side.
    for (i, e) in edges.iter().enumerate() {
        let (Some(&a), Some(&b)) = (index.get(e.from.as_str()), index.get(e.to.as_str())) else { continue };
        if a != b {
            continue;
        }
        let p = &placed[a];
        let (x, y) = (p.x + p.w, p.y + p.h / 2.0);
        routes[i] = Some(Route {
            points: vec![(x, y - LOOP_HALF), (x + LOOP_OUT, y - LOOP_HALF), (x + LOOP_OUT, y + LOOP_HALF), (x, y + LOOP_HALF)],
            mid: (x + LOOP_OUT, y),
            is_loop: true,
        });
    }

    let loops = if routes.iter().flatten().any(|r| r.is_loop) { LOOP_ROOM } else { 0.0 };
    let (w, h) = if right { (total, span) } else { (span, total) };
    Layout { width: w + PAD * 2.0 + loops, height: h + PAD * 2.0, nodes: placed, routes }
}

#[cfg(test)]
mod tests;
