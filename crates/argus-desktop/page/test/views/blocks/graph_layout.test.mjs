// `views/blocks/graph_layout` under node: the drawing fills the box it says
// it needs, since the panel scales that box to its width and any margin
// left in it shrinks every label with it.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { layout } from '../../../src/views/blocks/graph_layout.ts';

const PAD = 16;
// A stand-in's centre sits half its size in from where it starts.
const SLACK = 4;

function extents(l) {
  const xs = [
    ...l.nodes.flatMap((p) => [p.x, p.x + p.w]),
    ...l.routes.flatMap((r) => (r ? r.points.map(([x]) => x) : [])),
  ];
  const ys = [
    ...l.nodes.flatMap((p) => [p.y, p.y + p.h]),
    ...l.routes.flatMap((r) => (r ? r.points.map(([, y]) => y) : [])),
  ];
  return { left: Math.min(...xs), top: Math.min(...ys) };
}

function graph(names, links) {
  return {
    nodes: names.map((id) => ({ id, label: `${id} a fairly long label here` })),
    edges: links.map(([from, to, label]) => ({ from, to, label })),
  };
}

// The loop a saved view drew a third of its width wide (reliab's change
// flow): back edges and long skips drifted every layer right.
test('a graph with long skips and back edges starts at its padding', () => {
  const g = graph(
    [
      'edit',
      'approve',
      'inner',
      'route',
      'prop',
      'repro',
      'stop',
      'summary',
      'fix',
      'ack',
      'reviewer',
      'verdict',
      'push',
      'ci',
    ],
    [
      ['edit', 'inner'],
      ['approve', 'push'],
      ['inner', 'route', 'findings'],
      ['route', 'prop'],
      ['route', 'repro', 'agent kinds'],
      ['route', 'stop', 'human kinds'],
      ['stop', 'ack'],
      ['prop', 'summary', 'clean'],
      ['repro', 'fix'],
      ['fix', 'inner', 're-run'],
      ['summary', 'reviewer'],
      ['reviewer', 'verdict'],
      ['verdict', 'push', 'pass'],
      ['push', 'ci'],
      ['ack', 'inner', 'acked'],
      ['edit', 'reviewer', 'protected change'],
    ],
  );
  for (const right of [false, true]) {
    const { left, top } = extents(layout(g.nodes, g.edges, right));
    assert.ok(left <= PAD + SLACK, `left margin ${left} (right: ${right})`);
    assert.ok(top <= PAD + SLACK, `top margin ${top} (right: ${right})`);
  }
});

function random(seed) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

test('any graph starts at its padding along both axes', () => {
  for (let seed = 1; seed <= 300; seed++) {
    const r = random(seed);
    const n = 1 + Math.floor(r() * 12);
    const names = Array.from({ length: n }, (_, i) => `n${i}`);
    const links = Array.from({ length: Math.floor(r() * n * 2) }, () => [
      names[Math.floor(r() * n)],
      names[Math.floor(r() * n)],
    ]);
    const g = graph(names, links);
    const right = r() < 0.5;
    const { left, top } = extents(layout(g.nodes, g.edges, right));
    assert.ok(left <= PAD + SLACK, `seed ${seed}: left margin ${left}`);
    assert.ok(top <= PAD + SLACK, `seed ${seed}: top margin ${top}`);
  }
});
