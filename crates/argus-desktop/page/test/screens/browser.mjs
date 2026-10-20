// The page under headless Chrome, for shoot.mjs and audit.mjs: ../assets
// served as the window serves it, a stand-in for Tauri, and the DevTools
// protocol to drive it. Nothing is installed: the browser is the machine's
// Chrome or Edge, and the protocol is spoken over Node's own WebSocket.
//
// A scene is played one of two ways. Replayed, `connect` hands the page the
// events scenes.json recorded and every call goes nowhere. Live, the page
// talks to `cargo test -p argus-desktop serve_screens -- --ignored`, a real
// Session behind a test server, so a click is answered as the window would
// answer it. `--theme night` draws any scene in that theme, as the Settings
// dialog would.

import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createServer, request } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const here = dirname(fileURLToPath(import.meta.url));
const assets = resolve(here, '../../../assets');

export function option(name, fallback) {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? fallback : process.argv[at + 1];
}

export function scenes() {
  const only = option('only', '').split(',').filter(Boolean);
  const all = JSON.parse(readFileSync(option('scenes', join(here, 'scenes.json')), 'utf8'));
  return all.filter((scene) => only.length === 0 || only.includes(scene.name));
}

export const settle = (ms) => new Promise((done) => setTimeout(done, ms));

const types = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.woff2': 'font/woff2',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

/** Serves the assets, and passes /__live/* on to the live Session's server
 *  so the page calls it from its own origin. */
export function serve(live) {
  const server = createServer((req, res) => {
    if (req.url.startsWith('/__live/')) {
      const target = new URL(req.url.slice('/__live'.length), live);
      const forward = request(target, { method: req.method, headers: req.headers }, (answer) => {
        res.writeHead(answer.statusCode, answer.headers);
        answer.pipe(res);
      });
      forward.on('error', () => res.writeHead(502).end());
      req.pipe(forward);
      return;
    }
    const path = normalize(join(assets, decodeURIComponent(new URL(req.url, 'http://x').pathname)));
    const file = path.endsWith('\\') || path.endsWith('/') ? join(path, 'index.html') : path;
    if (!file.startsWith(assets) || !existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream' });
    res.end(readFileSync(file));
  });
  return new Promise((done) => server.listen(0, '127.0.0.1', () => done(server)));
}

function chrome() {
  const found = [
    process.env.CHROME,
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ].find((path) => path && existsSync(path));
  if (!found) throw new Error('no Chrome or Edge found; set CHROME');
  return found;
}

/** Starts the browser and waits for the port it chose to be written. */
export async function launch() {
  const profile = mkdtempSync(join(tmpdir(), 'argus-screens-'));
  const child = spawn(chrome(), [
    '--headless=new',
    '--remote-debugging-port=0',
    `--user-data-dir=${profile}`,
    '--hide-scrollbars',
    '--no-first-run',
    '--no-default-browser-check',
    '--font-render-hinting=none',
    'about:blank',
  ]);
  const portFile = join(profile, 'DevToolsActivePort');
  for (let tries = 0; !existsSync(portFile); tries++) {
    if (tries > 200) throw new Error('Chrome did not start');
    await settle(50);
  }
  await settle(50);
  const port = readFileSync(portFile, 'utf8').split('\n')[0];
  const close = () => {
    child.kill();
    setTimeout(() => rmSync(profile, { recursive: true, force: true }), 500);
  };
  return { port, close };
}

/** One page's DevTools session: `send` answers with the command's result. */
export async function devtools(port) {
  const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const socket = new WebSocket(pages.find((page) => page.type === 'page').webSocketDebuggerUrl);
  await new Promise((done) => socket.addEventListener('open', done));
  let next = 0;
  const waiting = new Map();
  const listeners = new Map();
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(data);
    if (message.id !== undefined) {
      const { done, fail } = waiting.get(message.id);
      waiting.delete(message.id);
      if (message.error) fail(new Error(message.error.message));
      else done(message.result);
    } else {
      listeners.get(message.method)?.(message.params);
    }
  });
  const send = (method, params = {}) =>
    new Promise((done, fail) => {
      const id = ++next;
      waiting.set(id, { done, fail });
      socket.send(JSON.stringify({ id, method, params }));
    });
  const once = (method) => new Promise((done) => listeners.set(method, done));
  const evaluate = async (expression) => {
    const { result, exceptionDetails } = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (exceptionDetails)
      throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  };
  return { send, once, evaluate, close: () => socket.close() };
}

/** Runs before the page: stands in for Tauri, keeps every call, counts the
 *  DOM's changes, and notes which events each element listens for, since
 *  the page attaches every handler with addEventListener (kit/dom.ts). */
function stand(scene, live) {
  return `(() => {
    const replay = ${live ? 'null' : JSON.stringify(scene.events)};
    const name = ${JSON.stringify(scene.name)};
    window.__calls = [];
    window.__pending = 0;
    window.__mutations = 0;
    const listening = new WeakMap();
    window.__listens = (node) => [...(listening.get(node) ?? [])];
    const add = EventTarget.prototype.addEventListener;
    EventTarget.prototype.addEventListener = function (type, ...rest) {
      if (!listening.has(this)) listening.set(this, new Set());
      listening.get(this).add(type);
      return add.call(this, type, ...rest);
    };
    let channel = null;
    const theme = ${JSON.stringify(option('theme', null))};
    const deliver = (events) => {
      for (const event of events) {
        if (theme && event.type === 'settings') event.chosen.theme = theme;
        channel.onmessage(event);
      }
    };
    const ask = (path, body) => {
      window.__pending++;
      return fetch('/__live/' + path, { method: 'POST', body: JSON.stringify(body) })
        .then((answer) => answer.json())
        .then(deliver)
        .catch((err) => console.error(path, err))
        .finally(() => window.__pending--);
    };
    class Channel { constructor() { this.onmessage = null; } }
    window.__TAURI__ = { core: { Channel, invoke(command, args) {
      window.__calls.push(command === 'call' ? args.call : { call: command });
      if (command === 'connect') {
        channel = args.events;
        if (replay) setTimeout(() => deliver(replay), 0);
        else ask('connect', { scene: name });
      } else if (command === 'call' && !replay) {
        ask('call', args.call);
      }
      return Promise.resolve(null);
    } } };
    document.addEventListener('DOMContentLoaded', () => {
      new MutationObserver((records) => { window.__mutations += records.length; })
        .observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
    });
  })();`;
}

/** Waits until the page has nothing in flight and has stopped changing. */
export async function quiet(tab, least = 150) {
  await settle(least);
  for (let tries = 0; tries < 40; tries++) {
    const before = await tab.evaluate('window.__pending + "/" + window.__mutations');
    await settle(80);
    const after = await tab.evaluate('window.__pending + "/" + window.__mutations');
    if (before === after && before.startsWith('0/')) return;
  }
}

/** Loads the page fresh on `scene`, at `size`, and runs its own steps. */
export async function load(tab, origin, scene, [width, height], live) {
  await tab.send('Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 2,
    mobile: false,
  });
  const { identifier } = await tab.send('Page.addScriptToEvaluateOnNewDocument', {
    source: stand(scene, live),
  });
  const loaded = tab.once('Page.loadEventFired');
  await tab.send('Page.navigate', { url: `${origin}/index.html` });
  await loaded;
  await tab.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
  await tab.evaluate('document.fonts.ready.then(() => true)');
  await quiet(tab, 300);
  for (const step of scene.then ?? []) {
    if (step.click) {
      const found = await tab.evaluate(
        `(() => { const el = document.querySelector(${JSON.stringify(step.click)}); if (el) el.click(); return !!el; })()`,
      );
      if (!found) console.warn(`${scene.name}: nothing matches ${step.click}`);
    }
    await quiet(tab);
  }
  // A pointer left where the last scene put it would hover something here.
  await tab.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 0, y: 0 });
}

export async function screenshot(tab, clip) {
  const params = { format: 'png' };
  if (clip) params.clip = { ...clip, scale: 1 };
  const { data } = await tab.send('Page.captureScreenshot', params);
  return Buffer.from(data, 'base64');
}

/** The server, browser and tab a run needs, and how to put them away. */
export async function open() {
  const live = option('live', null);
  const server = await serve(live);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const browser = await launch();
  const tab = await devtools(browser.port);
  await tab.send('Page.enable');
  await tab.send('Runtime.enable');
  const close = () => {
    tab.close();
    browser.close();
    server.close();
  };
  return { tab, origin, live: !!live, close };
}
