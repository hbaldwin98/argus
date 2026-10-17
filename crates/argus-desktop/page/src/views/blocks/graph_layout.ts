// Where a graph's boxes and arrows go: a layered layout, kept apart from
// the drawing so it is only numbers. Cycles are broken by reversing the
// edges a depth-first walk finds going back, nodes take the longest path
// from a root as their layer, an edge across several layers is given a
// stand-in node in each so it has somewhere to bend, and a few barycenter
// sweeps order each layer to cut crossings. Everything is worked out along
// two axes — the one the layers run down and the one a layer spreads
// along — and only turned into x and y at the end, which is all
// `direction` changes.

export interface LayoutNode {
  id: string;
  label: string;
}

export interface LayoutEdge {
  from: string;
  to: string;
  label?: string | null;
}

export interface Placed {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Route {
  /** Points from the edge's `from` to its `to`, bending at right angles. */
  points: [number, number][];
  /** Where its label sits. */
  mid: [number, number];
  /** Drawn from `from` to `to` going back against the layers, or a loop. */
  loop: boolean;
}

export interface Layout {
  width: number;
  height: number;
  nodes: Placed[];
  routes: (Route | undefined)[];
}

/** A label's width, as the drawing measures it too. */
export const CHAR_W = 7;
export const MAX_LABEL = 28;
const NODE_H = 34;
const MIN_W = 72;
const PAD = 16;
const SIDE_GAP = 28;
const DUMMY = 8;
const SWEEPS = 4;

export function clip(text: string): string {
  const chars = Array.from(text);
  return chars.length > MAX_LABEL ? chars.slice(0, MAX_LABEL - 1).join('') + '…' : text;
}

export function textW(text: string): number {
  return Array.from(clip(text)).length * CHAR_W;
}

interface Item {
  /** Its index in `nodes`, or -1 for a stand-in. */
  real: number;
  layer: number;
  /** Its size along the layers and across them. */
  main: number;
  cross: number;
  /** Where it starts across the layer. */
  at: number;
  ins: number[];
  outs: number[];
}

/** Edges as index pairs with the ones that close a cycle turned around. */
function acyclic(count: number, edges: [number, number][]): [number, number][] {
  const next: number[][] = Array.from({ length: count }, () => []);
  for (const [a, b] of edges) next[a]?.push(b);
  // 0 unseen, 1 on the walk, 2 done.
  const state = Array.from({ length: count }, () => 0);
  const back = new Set<string>();
  for (let root = 0; root < count; root++) {
    if (state[root] !== 0) continue;
    state[root] = 1;
    const stack: [number, number][] = [[root, 0]];
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      if (!top) break;
      const [n, i] = top;
      const outs = next[n] ?? [];
      if (i >= outs.length) {
        state[n] = 2;
        stack.pop();
        continue;
      }
      top[1] = i + 1;
      const m = outs[i] ?? 0;
      if (state[m] === 1) back.add(`${n}>${m}`);
      else if (state[m] === 0) {
        state[m] = 1;
        stack.push([m, 0]);
      }
    }
  }
  return edges.map(([a, b]) => (back.has(`${a}>${b}`) ? [b, a] : [a, b]));
}

function layers(count: number, edges: [number, number][]): number[] {
  const layer = Array.from({ length: count }, () => 0);
  const left = Array.from({ length: count }, () => 0);
  const out: number[][] = Array.from({ length: count }, () => []);
  for (const [a, b] of edges) {
    out[a]?.push(b);
    left[b] = (left[b] ?? 0) + 1;
  }
  const ready: number[] = [];
  for (let i = 0; i < count; i++) if (left[i] === 0) ready.push(i);
  while (ready.length > 0) {
    const n = ready.shift() ?? 0;
    for (const m of out[n] ?? []) {
      layer[m] = Math.max(layer[m] ?? 0, (layer[n] ?? 0) + 1);
      left[m] = (left[m] ?? 0) - 1;
      if (left[m] === 0) ready.push(m);
    }
  }
  return layer;
}

export function layout(
  nodes: readonly LayoutNode[],
  edges: readonly LayoutEdge[],
  right: boolean,
): Layout {
  const index = new Map(nodes.map((n, i) => [n.id, i]));
  const pairs: { a: number; b: number; edge: number }[] = [];
  edges.forEach((e, edge) => {
    const a = index.get(e.from);
    const b = index.get(e.to);
    if (a !== undefined && b !== undefined && a !== b) pairs.push({ a, b, edge });
  });
  const flat = acyclic(
    nodes.length,
    pairs.map((p) => [p.a, p.b]),
  );
  const layer = layers(nodes.length, flat);

  // Boxes are as wide as their label; `main` and `cross` follow the axes.
  const items: Item[] = nodes.map((n, real) => {
    const w = Math.max(MIN_W, textW(n.label) + 28);
    return {
      real,
      layer: layer[real] ?? 0,
      main: right ? w : NODE_H,
      cross: right ? NODE_H : w,
      at: 0,
      ins: [],
      outs: [],
    };
  });
  // Each edge as the chain of items it passes, low layer first.
  const chains: { items: number[]; reversed: boolean; edge: number }[] = [];
  pairs.forEach((p, k) => {
    const [a, b] = flat[k] ?? [p.a, p.b];
    const reversed = a !== p.a;
    const chain = [a];
    let prev = a;
    for (let l = (layer[a] ?? 0) + 1; l < (layer[b] ?? 0); l++) {
      items.push({ real: -1, layer: l, main: DUMMY, cross: DUMMY, at: 0, ins: [], outs: [] });
      const d = items.length - 1;
      items[prev]?.outs.push(d);
      items[d]?.ins.push(prev);
      chain.push(d);
      prev = d;
    }
    items[prev]?.outs.push(b);
    items[b]?.ins.push(prev);
    chain.push(b);
    chains.push({ items: chain, reversed, edge: p.edge });
  });

  const depth = items.reduce((m, it) => Math.max(m, it.layer), 0) + 1;
  const rows: number[][] = Array.from({ length: depth }, () => []);
  items.forEach((it, i) => rows[it.layer]?.push(i));
  const slot = new Map<number, number>();
  const renumber = (): void => rows.forEach((r) => r.forEach((n, i) => slot.set(n, i)));
  renumber();

  const sweep = (down: boolean): void => {
    const order = down ? rows.slice(1) : rows.slice(0, -1).toReversed();
    for (const row of order) {
      const key = new Map<number, number>();
      for (const n of row) {
        const around = (down ? items[n]?.ins : items[n]?.outs) ?? [];
        const mean = around.length
          ? around.reduce((s, m) => s + (slot.get(m) ?? 0), 0) / around.length
          : (slot.get(n) ?? 0);
        key.set(n, mean);
      }
      // Array.sort is stable, so a tie keeps the order the agent wrote.
      row.sort((x, y) => (key.get(x) ?? 0) - (key.get(y) ?? 0));
      row.forEach((n, i) => slot.set(n, i));
    }
  };
  for (let i = 0; i < SWEEPS; i++) {
    sweep(true);
    sweep(false);
  }

  // Across each layer, packed and centred; then each item drawn toward the
  // middle of what it joins, never past its neighbour.
  const extent = (row: number[]): number =>
    row.reduce((s, n) => s + (items[n]?.cross ?? 0), 0) + Math.max(0, row.length - 1) * SIDE_GAP;
  const widest = Math.max(...rows.map(extent), 0);
  for (const row of rows) {
    let at = (widest - extent(row)) / 2;
    for (const n of row) {
      const it = items[n];
      if (!it) continue;
      it.at = at;
      at += it.cross + SIDE_GAP;
    }
  }
  const centre = (n: number): number => (items[n]?.at ?? 0) + (items[n]?.cross ?? 0) / 2;
  const relax = (order: number[][], down: boolean): void => {
    for (const row of order) {
      let floor = -Infinity;
      for (const n of row) {
        const it = items[n];
        if (!it) continue;
        const around = down ? it.ins : it.outs;
        const want = around.length
          ? around.reduce((s, m) => s + centre(m), 0) / around.length - it.cross / 2
          : it.at;
        it.at = Math.max(want, floor);
        floor = it.at + it.cross + SIDE_GAP;
      }
    }
  };
  for (let i = 0; i < 3; i++) {
    relax(rows.slice(1), true);
    relax(rows.slice(0, -1).toReversed(), false);
  }
  const lowest = Math.min(...items.map((it) => it.at), 0);
  for (const it of items) it.at -= lowest;
  const span = Math.max(...items.map((it) => it.at + it.cross), 0);

  // Layers are spaced to leave room for the longest edge label crossing
  // them: along the layers a label is as wide as its text, across them one
  // line high.
  const labelled = edges.filter((e) => e.label);
  const room = labelled.reduce((m, e) => Math.max(m, textW(e.label ?? '')), 0);
  const gap = right ? 48 + room : 48 + (labelled.length ? 14 : 0);
  const thick = rows.map((r) => r.reduce((m, n) => Math.max(m, items[n]?.main ?? 0), 0));
  const start: number[] = [];
  let run = 0;
  thick.forEach((t, l) => {
    start[l] = run;
    run += t + gap;
  });
  const total = Math.max(0, run - gap);

  const mainAt = (n: number): number => {
    const it = items[n];
    return (start[it?.layer ?? 0] ?? 0) + ((thick[it?.layer ?? 0] ?? 0) - (it?.main ?? 0)) / 2;
  };
  const xy = (m: number, c: number): [number, number] =>
    right ? [PAD + m, PAD + c] : [PAD + c, PAD + m];

  const placed: Placed[] = nodes.map((n, i) => {
    const it = items[i];
    const [x, y] = xy(mainAt(i), it?.at ?? 0);
    return {
      id: n.id,
      x,
      y,
      w: right ? (it?.main ?? 0) : (it?.cross ?? 0),
      h: right ? (it?.cross ?? 0) : (it?.main ?? 0),
    };
  });

  const routes: (Route | undefined)[] = Array.from({ length: edges.length }, () => undefined);
  for (const c of chains) {
    const pts: [number, number][] = [];
    let mid: [number, number] = [0, 0];
    const last = c.items.length - 1;
    const stops: [number, number][] = [];
    c.items.forEach((n, i) => {
      const it = items[n];
      if (!it) return;
      const cross = it.at + it.cross / 2;
      const top = mainAt(n);
      // A box is left by its far edge and entered by its near one; a
      // stand-in is passed straight through.
      if (it.real >= 0 && i === 0) stops.push([top + it.main, cross]);
      else if (it.real >= 0 && i === last) stops.push([top, cross]);
      else stops.push([top + it.main / 2, cross]);
    });
    // Every hop leaves along the layers, jogs across at its middle and
    // arrives along them: right angles only.
    stops.forEach(([m, cr], i) => {
      if (i === 0) {
        pts.push(xy(m, cr));
        return;
      }
      const [pm, pc] = stops[i - 1] ?? [m, cr];
      const half = (pm + m) / 2;
      pts.push(xy(half, pc), xy(half, cr), xy(m, cr));
    });
    const hop = Math.floor(stops.length / 2);
    const [a, b] = [stops[hop - 1] ?? stops[0], stops[hop] ?? stops[0]];
    if (a && b) mid = xy((a[0] + b[0]) / 2, (a[1] + b[1]) / 2);
    const points = c.reversed ? pts.toReversed() : pts;
    routes[c.edge] = { points, mid, loop: false };
  }
  // A node joined to itself loops out of its far side.
  edges.forEach((e, i) => {
    const a = index.get(e.from);
    const b = index.get(e.to);
    if (a === undefined || a !== b) return;
    const p = placed[a];
    if (!p) return;
    const [x, y] = [p.x + p.w, p.y + p.h / 2];
    routes[i] = {
      points: [
        [x, y - 6],
        [x + 18, y - 6],
        [x + 18, y + 6],
        [x, y + 6],
      ],
      mid: [x + 18, y],
      loop: true,
    };
  });

  const loops = routes.some((r) => r?.loop) ? 24 : 0;
  const [w, h] = right ? [total, span] : [span, total];
  return {
    width: w + PAD * 2 + loops,
    height: h + PAD * 2,
    nodes: placed,
    routes,
  };
}
