// The fold of ignored folders under node: the rows it holds and the line it
// writes are decided by depth and the ignored flag alone.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { foldIgnored, foldLabel } from '../../src/kit/ignored_fold.ts';

const row = (name, depth, ignored = false) => ({ name, depth, ignored });

test('ignored folders collapse into one row that names them', () => {
  const rows = [
    row('.argus', 0, true),
    row('crates', 0),
    row('src', 1),
    row('target', 0, true),
    row('debug', 1, true),
    row('README.md', 0),
  ];
  const folded = foldIgnored(rows);
  assert.deepEqual(
    folded.shown.map((r) => r.name),
    ['crates', 'src', 'README.md'],
  );
  assert.deepEqual(
    folded.held.map((r) => r.name),
    ['.argus', 'target', 'debug'],
  );
  assert.equal(foldLabel(folded.names), '2 ignored · .argus, target');
});

test('an ignored entry below the top stays where it is', () => {
  const folded = foldIgnored([row('src', 0), row('gen', 1, true), row('lib.rs', 1)]);
  assert.equal(folded.held.length, 0);
  assert.equal(folded.shown.length, 3);
});

test('a tree with nothing ignored has no fold', () => {
  assert.deepEqual(foldIgnored([row('a', 0)]).names, []);
});
