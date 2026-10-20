// Every hover, click, right-click and Tab stop on every screen: for each
// scene the page is swept element by element, and what each gesture did is
// written to <out>/<scene>/audit.json beside its pictures. A gesture that
// changes nothing on screen is the finding: a control with no hover state,
// a click that neither draws nor calls, a Tab stop with no ring.
//
// Run it live (see browser.mjs) so a click is answered by a real Session;
// replayed, a click that needs Rust's answer reads as dead.
//
//   node test/screens/audit.mjs --live http://127.0.0.1:port [--only a,b] [--out dir]

import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { here, load, open, option, quiet, scenes, screenshot } from './browser.mjs';

const out = resolve(option('out', join(here, 'audit')));
const size = option('size', '1440x900').split('x').map(Number);

/** Finds what a person can act on: what listens, what is a control by its
 *  tag or role, what the pointer says can be pressed. A child is left out
 *  when its parent is a target and it adds no handler of its own, since
 *  the cursor is inherited and would count every span in a button. */
const TARGETS = `(() => {
  const acts = ['click', 'mousedown', 'pointerdown', 'dblclick', 'contextmenu', 'mouseenter', 'mouseover', 'input', 'change'];
  const roles = ['button', 'tab', 'menuitem', 'option', 'separator', 'switch', 'checkbox', 'link', 'treeitem', 'row'];
  const controls = ['BUTTON', 'A', 'INPUT', 'SELECT', 'TEXTAREA', 'SUMMARY', 'LABEL'];
  const pointers = ['pointer', 'col-resize', 'row-resize', 'ew-resize', 'ns-resize', 'grab', 'move', 'text'];
  const path = (node) => {
    const parts = [];
    for (let at = node; at && at !== document.body; at = at.parentElement) {
      if (at.id) { parts.unshift('#' + CSS.escape(at.id)); break; }
      const index = [...at.parentElement.children].indexOf(at) + 1;
      parts.unshift(at.tagName.toLowerCase() + ':nth-child(' + index + ')');
    }
    return parts.join(' > ');
  };
  const own = (node) => window.__listens(node).filter((type) => acts.includes(type));
  const target = (node) => {
    const style = getComputedStyle(node);
    if (style.visibility === 'hidden' || style.display === 'none' || style.pointerEvents === 'none') return false;
    const box = node.getBoundingClientRect();
    if (box.width < 2 || box.height < 2 || box.bottom < 0 || box.right < 0 || box.top > innerHeight || box.left > innerWidth) return false;
    if (node.closest('.terminal') && node.tagName !== 'TEXTAREA') return false;
    if (node.closest('[hidden]')) return false;
    // Scrolled under the footer or behind a layer: the pointer would land
    // on whatever covers it, and the sweep would blame this for it.
    const hit = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
    if (!hit || !(hit === node || node.contains(hit) || hit.contains(node))) return false;
    if (own(node).length || controls.includes(node.tagName) || roles.includes(node.getAttribute('role'))) return true;
    if (node.tabIndex >= 0 && node.hasAttribute('tabindex')) return true;
    const parent = node.parentElement;
    return pointers.includes(style.cursor) && !(parent && pointers.includes(getComputedStyle(parent).cursor));
  };
  const found = [...document.body.querySelectorAll('*')].filter(target);
  const kept = found.filter((node) => {
    const holder = node.parentElement?.closest('button, a, [role=button], [role=menuitem], [role=tab]');
    return !(holder && found.includes(holder) && own(node).length === 0);
  });
  return kept.map((node) => {
    const box = node.getBoundingClientRect();
    const text = (node.innerText || node.value || '').trim().replace(/\\s+/g, ' ');
    let menu = false;
    for (let at = node; at; at = at.parentElement) if (window.__listens(at).includes('contextmenu')) menu = true;
    return {
      path: path(node),
      tag: node.tagName.toLowerCase(),
      role: node.getAttribute('role'),
      label: node.getAttribute('aria-label') || node.getAttribute('title') || text.slice(0, 60),
      text: text.slice(0, 60),
      named: !!(text || node.getAttribute('aria-label') || node.getAttribute('aria-labelledby') || node.getAttribute('title')),
      title: node.getAttribute('title'),
      listens: own(node),
      cursor: getComputedStyle(node).cursor,
      disabled: node.disabled === true || node.getAttribute('aria-disabled') === 'true',
      menu,
      rect: { x: box.x, y: box.y, width: box.width, height: box.height },
    };
  });
})()`;

/** What is open over the stage: a menu in the layer, a dialog, a toast. */
const SURFACES = `(() => {
  const open = [];
  if (document.querySelector('#layer .menu')) open.push('menu');
  if (document.querySelector('#over')?.children.length) open.push('over');
  for (const layer of document.querySelectorAll('.review-layer')) if (!layer.hidden) open.push(layer.id);
  if (!document.querySelector('#notice')?.hidden) open.push('notice');
  return open;
})()`;

const hash = (buffer) => createHash('sha1').update(buffer).digest('hex').slice(0, 12);

function around(rect, pad = 10) {
  const x = Math.max(0, rect.x - pad);
  const y = Math.max(0, rect.y - pad);
  return {
    x,
    y,
    width: Math.min(size[0] - x, rect.width + pad * 2),
    height: Math.min(size[1] - y, rect.height + pad * 2),
  };
}

const centre = (rect) => ({ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 });

async function press(tab, at, button) {
  await tab.send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...at });
  await tab.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    ...at,
    button,
    clickCount: 1,
  });
  await tab.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    ...at,
    button,
    clickCount: 1,
  });
}

/** The sweep of one scene: hovers on the page as loaded, then each click
 *  and right-click from a fresh load whenever the last one changed it. */
async function sweep(tab, origin, scene, live) {
  const dir = join(out, scene.name);
  mkdirSync(dir, { recursive: true });
  const fresh = () => load(tab, origin, scene, size, live);
  await fresh();
  writeFileSync(join(dir, 'base.png'), await screenshot(tab));
  const targets = await tab.evaluate(TARGETS);
  const seen = new Set();
  const keep = async (name) => {
    const shot = await screenshot(tab);
    const id = hash(shot);
    if (seen.has(id)) return { same: id };
    seen.add(id);
    writeFileSync(join(dir, `${name}.png`), shot);
    return { image: `${name}.png` };
  };

  for (const [n, target] of targets.entries()) {
    const clip = around(target.rect);
    const before = await screenshot(tab, clip);
    await tab.send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...centre(target.rect) });
    await quiet(tab, 120);
    const after = await screenshot(tab, clip);
    target.hover = { changed: !before.equals(after) };
    if (target.hover.changed) {
      writeFileSync(join(dir, `hover-${n}-before.png`), before);
      writeFileSync(join(dir, `hover-${n}.png`), after);
    }
    await tab.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 0, y: 0 });
  }

  let dirty = true;
  for (const [n, target] of targets.entries()) {
    for (const [gesture, button] of [
      ['click', 'left'],
      ['right', 'right'],
    ]) {
      if (gesture === 'right' && !target.menu) continue;
      if (dirty) await fresh();
      const calls = await tab.evaluate('window.__calls.length');
      const mutations = await tab.evaluate('window.__mutations');
      await press(tab, centre(target.rect), button);
      await quiet(tab, 200);
      const sent = await tab.evaluate(
        `window.__calls.slice(${calls}).map((c) => c.call + (c.action ? ':' + c.action : '') + (c.gesture ? ':' + JSON.stringify(c.gesture).slice(0, 60) : ''))`,
      );
      const changed = (await tab.evaluate('window.__mutations')) - mutations;
      const surfaces = await tab.evaluate(SURFACES);
      const result = { calls: sent, mutations: changed, surfaces };
      if (changed > 0) Object.assign(result, await keep(`${gesture}-${n}`));
      if (surfaces.includes('menu')) result.menu = await menu(tab, dir, `${gesture}-${n}`);
      target[gesture] = result;
      dirty = changed > 0 || sent.length > 0;
    }
  }

  const stops = await tabStops(tab, fresh, dir);
  const report = { name: scene.name, title: scene.title, targets, stops };
  writeFileSync(join(dir, 'audit.json'), JSON.stringify(report, null, 2));
  return report;
}

/** A menu just opened: each item's hover, and what it offers. */
async function menu(tab, dir, name) {
  const items =
    await tab.evaluate(`[...document.querySelectorAll('#layer .menu > *')].map((item) => {
    const box = item.getBoundingClientRect();
    return { kind: item.className, text: item.innerText.trim().replace(/\\s+/g, ' '), disabled: !!item.disabled, rect: { x: box.x, y: box.y, width: box.width, height: box.height } };
  })`);
  const box = await tab.evaluate(
    `(() => { const b = document.querySelector('#layer .menu').getBoundingClientRect(); return { x: b.x, y: b.y, width: b.width, height: b.height }; })()`,
  );
  writeFileSync(join(dir, `${name}-menu.png`), await screenshot(tab, around(box, 16)));
  for (const [n, item] of items.entries()) {
    if (!item.kind.includes('mi')) continue;
    const before = await screenshot(tab, around(item.rect, 2));
    await tab.send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...centre(item.rect) });
    await quiet(tab, 100);
    const after = await screenshot(tab, around(item.rect, 2));
    item.hover = !before.equals(after);
    if (n === items.findIndex((i) => i.kind.includes('mi') && !i.disabled)) {
      writeFileSync(join(dir, `${name}-menu-hover.png`), await screenshot(tab, around(box, 16)));
    }
  }
  return items;
}

/** Tab from a fresh page until focus comes back round or stops moving:
 *  each stop, and whether its ring shows. */
async function tabStops(tab, fresh, dir) {
  await fresh();
  const stops = [];
  const where = `(() => { const a = document.activeElement; if (!a || a === document.body) return null;
    const b = a.getBoundingClientRect(); const s = getComputedStyle(a);
    return { tag: a.tagName.toLowerCase(), id: a.id, label: (a.getAttribute('aria-label') || a.innerText || '').trim().slice(0, 50),
      rect: { x: b.x, y: b.y, width: b.width, height: b.height },
      ring: (s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0) || s.boxShadow !== 'none' }; })()`;
  for (let n = 0; n < 60; n++) {
    await tab.send('Input.dispatchKeyEvent', {
      type: 'keyDown',
      key: 'Tab',
      code: 'Tab',
      windowsVirtualKeyCode: 9,
    });
    await tab.send('Input.dispatchKeyEvent', {
      type: 'keyUp',
      key: 'Tab',
      code: 'Tab',
      windowsVirtualKeyCode: 9,
    });
    await quiet(tab, 60);
    const stop = await tab.evaluate(where);
    if (!stop) break;
    const again = stops.findIndex(
      (s) =>
        s.tag === stop.tag &&
        s.id === stop.id &&
        s.rect.x === stop.rect.x &&
        s.rect.y === stop.rect.y,
    );
    if (again !== -1) {
      stops.push({ ...stop, repeat: again });
      break;
    }
    if (stop.rect.width > 1)
      writeFileSync(join(dir, `tab-${n}.png`), await screenshot(tab, around(stop.rect, 8)));
    stops.push({ ...stop, image: `tab-${n}.png` });
  }
  return stops;
}

const of = (rule, list) => list.map((t) => ({ rule, label: t.label || t.path, path: t.path }));

function findings(report) {
  const live = report.targets.filter((t) => !t.disabled);
  return [
    ...of(
      'no hover state',
      live.filter((t) => t.tag !== 'textarea' && !t.hover?.changed),
    ),
    ...of(
      'click does nothing',
      live.filter(
        (t) =>
          t.click &&
          !t.click.calls.length &&
          !t.click.mutations &&
          t.listens.length + (t.tag === 'button') > 0,
      ),
    ),
    ...of(
      'icon-only, no name',
      live.filter((t) => !t.named),
    ),
    ...of(
      'hit target under 24px',
      live.filter((t) => t.rect.width < 24 || t.rect.height < 24),
    ),
    ...report.stops
      .filter((s) => !s.ring && s.tag !== 'textarea')
      .map((s) => ({ rule: 'no focus ring', label: s.label || s.id || s.tag })),
  ];
}

mkdirSync(out, { recursive: true });
const { tab, origin, live, close } = await open();
const summary = [];
try {
  for (const scene of scenes()) {
    const report = await sweep(tab, origin, scene, live);
    const found = findings(report);
    summary.push({
      name: scene.name,
      title: scene.title,
      targets: report.targets.length,
      stops: report.stops.length,
      findings: found,
    });
    const counts = Object.entries(Object.groupBy(found, (f) => f.rule)).map(
      ([rule, list]) => `${list.length} ${rule}`,
    );
    console.log(
      `${scene.name}: ${report.targets.length} targets, ${report.stops.length} tab stops; ${counts.join(', ') || 'clean'}`,
    );
  }
} finally {
  close();
}
writeFileSync(join(out, 'summary.json'), JSON.stringify(summary, null, 2));
console.log(`audit written to ${out}`);
