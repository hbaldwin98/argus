// The editor's colours under node, on the real tree-sitter runtime and a
// real grammar (JSON's, small enough to keep beside the test, the file the
// registry pins). The tree the editor keeps is told each change and parsed
// again from the old one; whatever is typed, it must be the tree a parse
// from nothing gives, or colours drift from the text.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { ChangeSet, Language, Parser, Query, Text } from '../../../assets/js/vendor/codemirror.js';
import { classOf, edits, reader } from '../../../assets/js/code/colours.js';

await Parser.init();
const json = await Language.load(
  new Uint8Array(readFileSync(new URL('fixtures/tree-sitter-json.wasm', import.meta.url))),
);
const highlights = readFileSync(new URL('fixtures/json-highlights.scm', import.meta.url), 'utf8');

function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

/** Every node with where it starts and ends, rows and columns included:
 *  a tree's text alone would not show a point put in the wrong place. */
function shape(tree) {
  const out = [];
  const cursor = tree.walk();
  for (;;) {
    const n = cursor.currentNode;
    out.push(
      `${n.type}@${n.startPosition.row}:${n.startPosition.column}-${n.endPosition.row}:${n.endPosition.column}`,
    );
    if (cursor.gotoFirstChild() || cursor.gotoNextSibling()) continue;
    let up = false;
    while (cursor.gotoParent())
      if (cursor.gotoNextSibling()) {
        up = true;
        break;
      }
    if (!up) break;
  }
  cursor.delete();
  return out.join(' ');
}

const PIECES = ['{', '}', '[', ']', '"a"', ':', ',', ' ', '\n', '1', '"é😀"', 'true', ''];

test('a tree told each change and parsed again is the tree a fresh parse gives', () => {
  // The rule `colours` rests on (tree-sitter's incremental parsing): edits
  // positioned as the edits before them left the text, in UTF-16 units.
  const parser = new Parser();
  parser.setLanguage(json);
  for (let seed = 1; seed <= 150; seed++) {
    const next = rng(seed);
    let doc = Text.of(['{', '  "a": [1, 2, "x😀"],', '  "b": {"c": true}', '}']);
    let tree = parser.parse(reader(doc));
    for (let step = 0; step < 25; step++) {
      const specs = [];
      let at = 0;
      // One to three changes in a set, as a multi-cursor edit makes.
      for (let n = 1 + Math.floor(next() * 3); n > 0 && at <= doc.length; n--) {
        const from = at + Math.floor(next() * (doc.length - at + 1));
        const to = Math.min(doc.length, from + Math.floor(next() * 4));
        const s = doc.toString();
        const splits = (p) => p > 0 && p < s.length && /[\uD800-\uDBFF]/.test(s[p - 1]);
        if (splits(from) || splits(to)) break;
        specs.push({ from, to, insert: PIECES[Math.floor(next() * PIECES.length)] });
        at = to + 1;
      }
      if (specs.length === 0) continue;
      const changes = ChangeSet.of(specs, doc.length);
      const after = changes.apply(doc);
      for (const e of edits(changes, doc, after)) tree.edit(e);
      const again = parser.parse(reader(after), tree);
      const fresh = parser.parse(reader(after));
      assert.equal(shape(again), shape(fresh), `seed ${seed}, step ${step}`);
      tree.delete();
      fresh.delete();
      tree = again;
      doc = after;
    }
    tree.delete();
  }
});

test('a capture is coloured by its first word, as the daemon colours the read view', () => {
  assert.equal(classOf('keyword.control.return'), 'sx-k');
  assert.equal(classOf('string.special.key'), 'sx-s');
  assert.equal(classOf('constant.builtin'), 'sx-n');
  assert.equal(classOf('variable'), null, 'plain variables stay uncoloured');
  const query = new Query(json, highlights);
  assert.ok(
    query.captureNames.some((name) => classOf(name)),
    "JSON's query has captures we colour",
  );
});
