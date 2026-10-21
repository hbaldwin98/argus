// `keys/sections` under node, against the keymap Rust really sends (the
// events `cargo test -p argus-desktop` writes to test/contract/sent.ts):
// the sheet is generated from it, so every binding has to land on a row.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sent } from '../contract/sent.ts';
import { matching, sectionFor, sections } from '../../src/keys/sections.ts';

const groups = sent.find((event) => event.type === 'keymap').groups;
const sources = groups.map((g) => ({ ...g }));

const NAMES = {
  ' ': 'Space',
  ArrowDown: '↓',
  ArrowUp: '↑',
  ArrowLeft: '←',
  ArrowRight: '→',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
};
const namer = {
  leader: 'Ctrl Space',
  press: (chord) => {
    const mods = [chord.ctrl && 'Ctrl', chord.shift && 'Shift'].filter(Boolean);
    return [...mods, NAMES[chord.key] ?? chord.key].join(' ');
  },
};
const all = sections(sources, namer);
const byId = (id) => all.find((s) => s.id === id);

/** Every row's label, merged ones standing for their members. */
const rows = (s) => [...s.rows, ...s.without];

test('every binding of the keymap is on a row of its group, merged pairs apart', () => {
  assert.equal(all.length, groups.length);
  // The pane's pairs: next/previous (2 into 1), and the arrows and moves (4 into 1, twice).
  const merged = { pane: 1 + 3 + 3 };
  for (const group of groups) {
    const section = byId(group.place);
    assert.ok(section, group.place);
    assert.equal(
      rows(section).length,
      group.bindings.length - (merged[group.place] ?? 0),
      group.place,
    );
    const labels = new Set(rows(section).map((r) => r.label));
    for (const binding of group.bindings) {
      if (/^(otherSide|previousSide|focus|movePane)/.test(binding.action)) continue;
      assert.ok(labels.has(binding.label), `${group.place}: ${binding.label}`);
    }
  }
});

test('the pane section says the leader once, in no row that follows it', () => {
  const pane = byId('pane');
  assert.equal(pane.chorded, true);
  assert.deepEqual(
    pane.rows.filter((r) => r.keys.some((k) => k.includes('Ctrl Space'))),
    [],
  );
  assert.ok(pane.without.some((r) => r.keys.some((k) => k.join(' ') === 'Ctrl Space Ctrl Space')));
  assert.ok(pane.without.some((r) => r.label === 'paste'));
  assert.ok(pane.without.some((r) => r.keys.some((k) => k.join(' ') === 'Shift PgUp')));
});

test('next and previous, and the arrows, are a row each in the pane', () => {
  const pane = byId('pane');
  const row = (label) => pane.rows.find((r) => r.label.includes(label));
  assert.deepEqual(row('next / previous').keys, [['o'], ['O']]);
  assert.deepEqual(row('pane that way').keys, [['← → ↑ ↓']]);
  assert.deepEqual(row('trading places').keys, [['Shift + arrow']]);
  assert.equal(pane.rows.filter((r) => /left|right/.test(r.label)).length, 0);
});

test('a pair of keys on one binding stays one row', () => {
  const feature = byId('feature').rows.find((r) => r.label === 'next feature');
  assert.deepEqual(feature.keys, [['j'], ['↓']]);
});

test('the filter finds a row by its action or its key, grouped by section', () => {
  const found = matching(all, 'paste');
  assert.deepEqual(
    found.map((s) => s.id),
    ['pane'],
  );
  assert.ok(found.every((s) => rows(s).every((r) => /paste/i.test(r.label))));
  const byKey = matching(all, 'pgup');
  assert.ok(byKey.length > 0 && byKey.every((s) => rows(s).length > 0));
  assert.equal(matching(all, '').length, all.length);
  assert.deepEqual(matching(all, 'zzzzzz'), []);
});

test('the sheet opens on the section for where the person is', () => {
  const ids = all.map((s) => s.id);
  const where = {
    pane: false,
    dialog: null,
    rail: false,
    outline: false,
    diagram: false,
    stage: 'workspace',
  };
  assert.equal(sectionFor({ ...where, pane: true }, ids), 'pane');
  assert.equal(sectionFor({ ...where, stage: 'feature' }, ids), 'feature');
  assert.equal(sectionFor({ ...where, stage: 'feature', outline: true }, ids), 'outline');
  assert.equal(
    sectionFor({ ...where, stage: 'feature', outline: true, diagram: true }, ids),
    'diagram',
  );
  assert.equal(sectionFor({ ...where, stage: 'code' }, ids), 'code');
  assert.equal(sectionFor({ ...where, dialog: 'review', pane: true }, ids), 'review');
  assert.equal(sectionFor(where, ids), 'anywhere');
  assert.equal(sectionFor({ ...where, pane: true }, ['anywhere']), 'anywhere');
});
