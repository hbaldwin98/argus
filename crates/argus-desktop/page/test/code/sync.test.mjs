// The editor's buffer and the disk under node: the buffer ends as the disk
// whatever order the person's writes and an agent's land in, and where both
// touched the same text the person's word decides it. Run against the built
// page, whose CodeMirror is the bundle the window loads.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ChangeSet, Text } from '../../../assets/js/vendor/codemirror.js';
import { Synced, between, fromWire, toWire } from '../../../assets/js/code/sync.js';

const doc = (s) => Text.of(s.split('\n'));
const at = (length, from, to, insert) => ChangeSet.of([{ from, to, insert }], length);

/** The daemon as `ClientMsg::Edit` and `EditChanged` describe it
 *  (argus-protocol `code/edit`): a change applies only to the version it
 *  was made on; any other write is told as the change from one version to
 *  the next. `between` stands in for its line diff, which is tested in
 *  argusd. */
class Disk {
  constructor(text) {
    this.text = text;
    this.version = 0;
    this.told = [];
  }
  edit(base, changes) {
    if (base !== this.version) return { refused: true };
    this.text = fromWire(changes, this.text.length).apply(doc(this.text)).toString();
    this.version++;
    return { saved: this.version };
  }
  agent(text) {
    const changes = between(this.text, text);
    this.told.push({ from: this.version, version: this.version + 1, changes });
    this.text = text;
    this.version++;
  }
}

/** A client: the sync and the buffer the view would show. */
function client(text) {
  const sync = new Synced(text, 0);
  return { sync, view: doc(text) };
}

function type(c, changes) {
  c.view = changes.apply(c.view);
  c.sync.typed(changes);
}

function hear(c, told) {
  const shown = c.sync.changed(told.from, told.version, told.changes);
  assert.ok(shown, 'made on the version held');
  c.view = shown.apply(c.view);
}

test('an agent write clear of the typing lands in the buffer and the typing goes on top', () => {
  const disk = new Disk('one\ntwo\nthree');
  const c = client(disk.text);
  type(c, at(c.view.length, 0, 3, 'ONE'));
  disk.agent('one\ntwo\nthree\nfour');
  hear(c, disk.told.shift());
  assert.equal(c.view.toString(), 'ONE\ntwo\nthree\nfour');
  assert.ok(!c.sync.clashing());

  const base = c.sync.version;
  const written = disk.edit(base, c.sync.take());
  c.sync.saved(base, written.saved);
  assert.equal(disk.text, 'ONE\ntwo\nthree\nfour');
});

test('a write made while an agent wrote is refused and made again on its text', () => {
  const disk = new Disk('a\nb');
  const c = client(disk.text);
  type(c, at(c.view.length, 3, 3, '!'));
  const base = c.sync.version;
  const changes = c.sync.take();
  disk.agent('A\nb');
  hear(c, disk.told.shift());
  assert.equal(disk.edit(base, changes).refused, true);
  c.sync.refused(base, null);

  const again = c.sync.version;
  c.sync.saved(again, disk.edit(again, c.sync.take()).saved);
  assert.equal(disk.text, 'A\nb!');
  assert.ok(!c.sync.pending());
});

test('a clash is held until the person keeps theirs or the agents', () => {
  for (const keep of ['mine', 'theirs']) {
    const disk = new Disk('let x = 1;');
    const c = client(disk.text);
    type(c, at(c.view.length, 8, 9, '2'));
    disk.agent('let x = 3;');
    hear(c, disk.told.shift());
    assert.ok(c.sync.clashing());
    assert.equal(c.view.toString(), 'let x = 2;', 'the buffer is left as typed');
    assert.deepEqual(c.sync.clashes(), [{ from: 8, to: 9, theirs: '3' }]);
    assert.equal(c.sync.take(), null, 'nothing is written over a clash');

    const shown = c.sync.resolve(keep);
    if (shown) c.view = shown.apply(c.view);
    const base = c.sync.version;
    const changes = c.sync.take();
    if (changes) c.sync.saved(base, disk.edit(base, changes).saved);
    const want = keep === 'mine' ? 'let x = 2;' : 'let x = 3;';
    assert.equal(c.view.toString(), want);
    assert.equal(disk.text, want);
  }
});

test('a change set crosses the wire and back unchanged', () => {
  const changes = ChangeSet.of(
    [
      { from: 1, to: 2, insert: 'é😀' },
      { from: 4, to: 4, insert: '\n' },
    ],
    6,
  );
  assert.deepEqual(toWire(fromWire(toWire(changes), 6)), toWire(changes));
});

/** A small seeded generator, so a failure names the run that made it. */
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

function randomEdit(next, length) {
  const from = Math.floor(next() * (length + 1));
  const to = Math.min(length, from + Math.floor(next() * 4));
  const pieces = ['x', 'yz', '\n', ' ', '😀', ''];
  return { from, to, insert: pieces[Math.floor(next() * pieces.length)] };
}

test('whatever order typing, writes and agent writes land in, the buffer ends as the disk', () => {
  // The rule the whole editor rests on, from argus-protocol `code/edit`:
  // the file on disk is the authority and the buffer follows it.
  for (let seed = 1; seed <= 400; seed++) {
    const next = rng(seed);
    const disk = new Disk('fn a() {}\nfn b() {}\n');
    const c = client(disk.text);
    let inFlight = null;
    for (let step = 0; step < 40; step++) {
      const roll = next();
      if (roll < 0.35) {
        const e = randomEdit(next, c.view.length);
        // A surrogate pair is never split by a real editor.
        const s = c.view.toString();
        if (
          /[\uD800-\uDBFF]/.test(s[e.from - 1] ?? '') ||
          /[\uD800-\uDBFF]/.test(s[e.to - 1] ?? '')
        )
          continue;
        type(c, ChangeSet.of([e], c.view.length));
      } else if (roll < 0.5) {
        const e = randomEdit(next, disk.text.length);
        const t = disk.text;
        if (
          /[\uD800-\uDBFF]/.test(t[e.from - 1] ?? '') ||
          /[\uD800-\uDBFF]/.test(t[e.to - 1] ?? '')
        )
          continue;
        disk.agent(t.slice(0, e.from) + e.insert + t.slice(e.to));
      } else if (roll < 0.7 && !inFlight) {
        const base = c.sync.version;
        const changes = c.sync.take();
        if (changes) inFlight = { base, changes };
      } else if (roll < 0.85 && inFlight) {
        // The daemon answers the write in order with what it told before it.
        const answer = disk.edit(inFlight.base, inFlight.changes);
        while (disk.told.length) hear(c, disk.told.shift());
        if (answer.saved) c.sync.saved(inFlight.base, answer.saved);
        else c.sync.refused(inFlight.base, null);
        inFlight = null;
      } else {
        while (disk.told.length) hear(c, disk.told.shift());
        if (c.sync.clashing()) {
          const shown = c.sync.resolve(next() < 0.5 ? 'mine' : 'theirs');
          if (shown) c.view = shown.apply(c.view);
        }
      }
      assert.equal(
        c.view.toString(),
        c.sync.doc().toString(),
        `seed ${seed}: the view is the buffer`,
      );
    }
    // Settle: answer what is in flight, hear everything, resolve, flush.
    for (let round = 0; round < 20; round++) {
      if (inFlight) {
        const answer = disk.edit(inFlight.base, inFlight.changes);
        while (disk.told.length) hear(c, disk.told.shift());
        if (answer.saved) c.sync.saved(inFlight.base, answer.saved);
        else c.sync.refused(inFlight.base, null);
        inFlight = null;
      }
      while (disk.told.length) hear(c, disk.told.shift());
      if (c.sync.clashing()) {
        const shown = c.sync.resolve('mine');
        if (shown) c.view = shown.apply(c.view);
      }
      const base = c.sync.version;
      const changes = c.sync.take();
      if (changes) inFlight = { base, changes };
      else if (!c.sync.pending()) break;
    }
    assert.equal(c.view.toString(), disk.text, `seed ${seed}: the buffer ends as the disk`);
  }
});
