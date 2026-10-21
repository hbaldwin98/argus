// The Code stage's editor in the real page under headless Chrome: how long
// a key takes to reach the buffer and the screen, what an agent's write
// looks like as it lands, and a clash waiting on the person. Plays the
// `code-edit` scene, then what the daemon would say.
//
//   node test/screens/editor.mjs [--keys 200] [--out dir]
//
// Each key is a real key event through DevTools, timed in the page from
// the event's own timestamp: to the editor's DOM holding the letter, and
// to the start of the frame that paints it.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { here, load, open, option, quiet, scenes, screenshot, settle } from './browser.mjs';

const out = resolve(option('out', join(here, 'out')));
const keys = Number(option('keys', '200'));
mkdirSync(out, { recursive: true });

function stat(xs) {
  const s = xs.toSorted((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(2);
  return `p50 ${q(0.5)}ms  p95 ${q(0.95)}ms  max ${q(1)}ms`;
}

const scene = scenes().find((s) => s.name === 'code-edit');
if (!scene) throw new Error('no code-edit scene: run write_screens first');
const code = scene.events.findLast((e) => e.type === 'code');
const text = readFileSync(resolve(here, '../../../../..', code.path), 'utf8').replaceAll(
  '\r\n',
  '\n',
);

const { tab, origin, live, close } = await open();
try {
  await load(tab, origin, scene, [1440, 900], live);
  await tab.evaluate(
    `window.__deliver([${JSON.stringify({
      type: 'edit',
      on: 'opened',
      checkout: code.checkout,
      path: code.path,
      version: 0,
      text,
    })}]); true`,
  );
  await quiet(tab);
  const opened = await tab.evaluate(`!!document.querySelector('.code-editor .cm-content')`);
  if (!opened) throw new Error('the editor did not open');
  // CodeMirror lays itself out with styles it injects; a policy that stops
  // them leaves the gutter stacked over the text.
  const laidOut = await tab.evaluate(
    `getComputedStyle(document.querySelector('.code-editor .cm-scroller')).display === 'flex'`,
  );
  if (!laidOut) throw new Error("CodeMirror's own styles did not apply");
  // No grammar is installed in a fresh profile: Rust's is offered, and
  // installs on the person's word, from npm, checked against its pin.
  const offered = await tab.evaluate(`document.querySelector('.ed-grammar')?.textContent ?? ''`);
  console.log(`grammar offered: ${JSON.stringify(offered)}`);
  await tab.evaluate(
    `[...document.querySelectorAll('.ed-bar button')].find((b) => b.textContent === 'Install')?.click(); true`,
  );
  let coloured = false;
  for (let i = 0; i < 100 && !coloured; i++) {
    await settle(200);
    coloured = await tab.evaluate(`!!document.querySelector('.code-editor .cm-content .sx-k')`);
  }
  console.log(`coloured after install: ${coloured}`);
  if (!coloured)
    console.log(
      `  bar says: ${await tab.evaluate(`document.querySelector('.ed-bar')?.textContent`)}`,
    );
  writeFileSync(join(out, 'editor-coloured.png'), await screenshot(tab));

  const fat = `!!document.querySelector('.code-editor .cm-fat-cursor')`;
  console.log(`opens in vim's normal mode: ${await tab.evaluate(fat)}`);

  // Rust reads the file again after writes; the lines it says changed are
  // marked in the gutter.
  const marked = { ...code, lines: code.lines.map((l, i) => ({ ...l, added: i >= 9 && i < 12 })) };
  await tab.evaluate(`window.__deliver([${JSON.stringify(marked)}]); true`);
  await settle(100);
  console.log(
    `change marks: ${await tab.evaluate(`document.querySelectorAll('.cm-change.added').length`)}`,
  );

  const press = async (key) => {
    await tab.send('Input.dispatchKeyEvent', { type: 'keyDown', ...key });
    await tab.send('Input.dispatchKeyEvent', { type: 'keyUp', key: key.key });
  };
  await press({ key: 'i', text: 'i', unmodifiedText: 'i' });
  await settle(50);
  writeFileSync(join(out, 'editor-open.png'), await screenshot(tab));

  // Timing hooks: each key's event time, when the editor's state held
  // what it typed, and when the next frame began.
  await tab.evaluate(`import('/js/vendor/codemirror.js').then(({ EditorView, StateEffect }) => {
    const view = EditorView.findFromDOM(document.querySelector('.code-editor .cm-editor'));
    window.__lat = [];
    let pending = null;
    view.contentDOM.addEventListener('keydown', (e) => { pending = { t0: e.timeStamp }; }, true);
    view.dispatch({ effects: StateEffect.appendConfig.of(EditorView.updateListener.of((u) => {
      if (!u.docChanged || !pending) return;
      const p = pending;
      pending = null;
      p.state = performance.now();
      requestAnimationFrame(() => { p.frame = performance.now(); window.__lat.push(p); });
    })) });
    return true;
  })`);
  // Real code, typed into a function body: what the parser meets when a
  // person types, not the junk its error recovery is slow on.
  const typing = 'let total = items.iter().map(|x| x.len()).sum::<usize>();\n';
  for (let i = 0; i < keys; i++) {
    const ch = typing[i % typing.length];
    const key =
      ch === '\n'
        ? { key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' }
        : { key: ch, text: ch, unmodifiedText: ch };
    await tab.send('Input.dispatchKeyEvent', { type: 'keyDown', ...key });
    await tab.send('Input.dispatchKeyEvent', { type: 'keyUp', key: key.key });
    await settle(5);
  }
  await settle(100);
  await press({ key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await settle(50);
  console.log(`Escape back to normal mode: ${await tab.evaluate(fat)}`);
  const lat = await tab.evaluate('window.__lat');
  console.log(`${lat.length} of ${keys} keys timed`);
  console.log(`key event to buffer: ${stat(lat.map((l) => l.state - l.t0))}`);
  console.log(`key event to frame:  ${stat(lat.map((l) => l.frame - l.t0))}`);

  // What the page asked the daemon: writes in bursts, not one per key.
  const writes = await tab.evaluate(
    `window.__calls.filter((c) => c.call === 'edit' && c.gesture.on === 'change').length`,
  );
  console.log(`${keys} keys sent as ${writes} write(s) while none was answered`);

  // An agent's write lands two lines down: the buffer takes it at once and
  // lights it, the typing kept. The write in flight was never answered, so
  // it is folded back into what is typed.
  const line =
    text
      .split('\n')
      .slice(0, code.at + 2)
      .join('\n').length + 1;
  await tab.evaluate(
    `window.__deliver([${JSON.stringify({
      type: 'edit',
      on: 'changed',
      checkout: code.checkout,
      path: code.path,
      from: 0,
      version: 1,
      changes: [{ from: line, to: line, insert: '    // an agent wrote this line\n' }],
    })}]); true`,
  );
  await settle(120);
  const agent = await tab.evaluate(`({
    kept: document.querySelector('.cm-content').textContent.includes('an agent wrote this line'),
    lit: !!document.querySelector('.cm-agent-edit'),
  })`);
  console.log(`agent write shown: ${agent.kept}, lit: ${agent.lit}`);
  writeFileSync(join(out, 'editor-agent.png'), await screenshot(tab));

  // The agent rewrites what the cursor just typed into: held, and asked.
  const typedAt =
    text
      .split('\n')
      .slice(0, code.at - 1)
      .join('\n').length + 1;
  await tab.evaluate(
    `window.__deliver([${JSON.stringify({
      type: 'edit',
      on: 'changed',
      checkout: code.checkout,
      path: code.path,
      from: 1,
      version: 2,
      // From the line before, across where the person typed.
      changes: [{ from: typedAt - 1, to: typedAt + 4, insert: '\nAGENT' }],
    })}]); true`,
  );
  await settle(120);
  const clash = await tab.evaluate(`({
    bar: document.querySelector('.ed-bar')?.textContent,
    marked: !!document.querySelector('.cm-theirs'),
  })`);
  console.log(`clash bar: ${JSON.stringify(clash.bar)}, theirs shown: ${clash.marked}`);
  writeFileSync(join(out, 'editor-clash.png'), await screenshot(tab));

  // Another file clicked: Rust says so at once, the daemon's text follows.
  // Every frame between is recorded: none may show a skeleton or an empty
  // editor, and the new file's first frame is coloured already.
  const other = 'crates/argus-desktop/src/chrome/rail.rs';
  const otherText = readFileSync(resolve(here, '../../../../..', other), 'utf8').replaceAll(
    '\r\n',
    '\n',
  );
  await tab.evaluate(`(() => {
    window.__frames = [];
    const sample = () => {
      const content = document.querySelector('#kstage .code-editor .cm-content');
      window.__frames.push({
        skeleton: !!document.querySelector('#kstage .skeleton'),
        editor: !!content,
        rail: !!content && content.textContent.includes('Rail'),
        coloured: !!document.querySelector('#kstage .cm-content .sx-k'),
      });
      if (window.__frames.length < 40) requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
    return true;
  })()`);
  const switched = { ...code, path: other, lines: [], fileLoading: true };
  await tab.evaluate(`window.__deliver([${JSON.stringify(switched)}]); true`);
  await settle(60);
  await tab.evaluate(
    `window.__deliver([${JSON.stringify({
      type: 'edit',
      on: 'opened',
      checkout: code.checkout,
      path: other,
      version: 0,
      text: otherText,
    })}]); true`,
  );
  await settle(700);
  const frames = await tab.evaluate('window.__frames');
  const first = frames.findIndex((f) => f.rail);
  if (frames.some((f) => f.skeleton || !f.editor) || first < 0 || !frames[first].coloured)
    process.exitCode = 1;
  console.log(
    `switching files: ${frames.length} frames, skeleton in ${frames.filter((f) => f.skeleton).length}, ` +
      `no editor in ${frames.filter((f) => !f.editor).length}, new file from frame ${first}, ` +
      `coloured there: ${first >= 0 && frames[first].coloured}`,
  );
} finally {
  close();
}
