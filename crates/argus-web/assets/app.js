// Argus on a phone: every agent the daemon runs, and each one's
// conversation.
//
// The server sends whole state (the agent list) and updates (a watched
// conversation) as JSON over one WebSocket. Everything it sends is text and
// is set as text, except a reply's `html`, which the server has already
// rendered from markdown into markup that can run nothing.

const app = document.getElementById("app");

const state = {
  device: null,
  server: null,
  workspaces: [],
  socket: null,
  connected: false,
  // pane id -> conversation
  conversations: new Map(),
  watching: null,
  // The message on its way, until the server says what became of it.
  sending: null,
  sendError: null,
  // Messages a restarted daemon lost, until dismissed or resent.
  notSent: [],
  // The one agent view built for a pane, kept while it is on screen so a
  // half-typed message survives the list changing under it.
  shell: null,
  // pane id -> "conversation" | "terminal"
  tabs: new Map(),
  // pane id -> the status last seen, to open the terminal on a new wait
  lastStatus: new Map(),
  // pane id -> { rows, cols, lines }
  screens: new Map(),
  screening: null,
  // Whether this device takes pushes, and whether it could: pushes need a
  // page served over HTTPS and a browser that has them.
  push: { on: false, possible: false, busy: false },
  templates: [],
  // The start sheet, when open, and whether a start is on its way.
  starting: null,
};

// --- the connection ------------------------------------------------------

async function start() {
  const pairing = new URLSearchParams(location.hash.slice(1)).get("pair");
  const me = await fetch("/api/me", { credentials: "same-origin" });
  if (me.ok) {
    const body = await me.json();
    state.device = body.device;
    state.push.on = body.push;
    state.push.possible = body.can_push && window.isSecureContext
      && "serviceWorker" in navigator && "PushManager" in window;
    if (state.push.possible) navigator.serviceWorker.register("/sw.js").catch(() => {});
    if (pairing) history.replaceState(null, "", location.pathname);
    connect();
  } else {
    showPairing(pairing || "");
  }
}

let retry = 500;

function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const socket = new WebSocket(`${scheme}://${location.host}/ws`);
  state.socket = socket;
  socket.addEventListener("open", () => {
    retry = 500;
    state.connected = true;
    sendVisibility();
    if (state.watching !== null) send({ type: "watch", pane: state.watching });
    if (state.screening !== null) send({ type: "screen", pane: state.screening });
    render();
  });
  socket.addEventListener("message", (event) => receive(JSON.parse(event.data)));
  socket.addEventListener("close", async () => {
    state.connected = false;
    state.socket = null;
    render();
    // Unpaired (revoked, or a cookie gone): back to pairing rather than
    // retrying forever.
    const me = await fetch("/api/me", { credentials: "same-origin" }).catch(() => null);
    if (me && me.status === 401) {
      showPairing("");
      return;
    }
    setTimeout(connect, retry);
    retry = Math.min(retry * 2, 8000);
  });
}

// A device looking at the page is not pushed what it can already see.
function sendVisibility() {
  send({ type: "visible", visible: document.visibilityState === "visible" });
}
document.addEventListener("visibilitychange", sendVisibility);

async function togglePush() {
  if (state.push.busy) return;
  state.push.busy = true;
  render();
  try {
    const registration = await navigator.serviceWorker.ready;
    if (state.push.on) {
      const subscription = await registration.pushManager.getSubscription();
      if (subscription) await subscription.unsubscribe();
      await fetch("/api/push/unsubscribe", { method: "POST", credentials: "same-origin" });
      state.push.on = false;
    } else if ((await Notification.requestPermission()) === "granted") {
      const { key } = await (await fetch("/api/push/key", { credentials: "same-origin" })).json();
      const subscription = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: fromBase64Url(key),
      });
      const saved = await fetch("/api/push/subscribe", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(subscription),
      });
      state.push.on = saved.ok;
    }
  } catch (error) {
    console.warn("argus web: notifications:", error);
  }
  state.push.busy = false;
  render();
}

function fromBase64Url(text) {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((text.length + 3) % 4);
  return Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
}

function send(message) {
  if (state.socket && state.socket.readyState === WebSocket.OPEN) {
    state.socket.send(JSON.stringify(message));
  }
}

function receive(message) {
  switch (message.type) {
    case "server":
      state.server = message;
      break;
    case "agents":
      state.workspaces = message.workspaces;
      state.templates = message.templates || [];
      break;
    case "started":
      if (state.starting) state.starting.busy = false;
      if (message.pane !== null && message.pane !== undefined) {
        state.starting = null;
        location.hash = `#/pane/${message.pane}`;
        return;
      }
      if (state.starting) state.starting.error = "The agent could not be started.";
      break;
    case "transcript":
      applyTranscript(message);
      return;
    case "earlier":
      applyEarlier(message);
      return;
    case "sent":
      if (state.sending && state.sending.pane === message.pane) {
        if (message.outcome === "refused") {
          state.sendError = message.reason || "The message was refused.";
        } else if (state.shell && state.shell.pane === message.pane) {
          state.shell.input.value = "";
          fitInput(state.shell.input);
        }
        state.sending = null;
      }
      break;
    case "screen":
      applyScreen(message);
      return;
    case "not_sent":
      state.notSent.push({ pane: message.pane, text: message.text });
      break;
    case "error":
      console.warn("argus web:", message.message);
      return;
  }
  render();
}

// --- conversations -------------------------------------------------------

function conversation(pane) {
  let c = state.conversations.get(pane);
  if (!c) {
    c = { order: [], entries: new Map(), earlier: null, asked: null, draft: null };
    state.conversations.set(pane, c);
  }
  return c;
}

function applyTranscript({ pane, fresh, earlier, updates }) {
  const c = conversation(pane);
  if (fresh) {
    c.order = [];
    c.entries = new Map();
    c.earlier = earlier;
    c.asked = null;
    c.draft = null;
  }
  const stick = nearBottom();
  for (const update of updates) apply(c, update, false);
  if (currentPane() !== pane) return;
  // A draft grows many times a second; only it is redrawn.
  if (!fresh && updates.every((u) => u.op === "draft")) {
    renderDraft(c);
    if (stick) scrollToEnd();
  } else {
    renderConversation(c, fresh || stick);
  }
}

function applyEarlier({ pane, before, earlier, updates }) {
  const c = conversation(pane);
  // An answer to a question this page is no longer asking.
  if (!c.asked || c.asked.file !== before.file || c.asked.offset !== before.offset) return;
  const fresh = [];
  for (const update of updates) {
    if (update.op === "upsert" && !c.entries.has(update.entry.id)) fresh.push(update.entry.id);
    apply(c, update, true);
  }
  c.order = fresh.concat(c.order.filter((id) => !fresh.includes(id)));
  c.earlier = earlier;
  c.asked = null;
  if (currentPane() === pane) {
    const scroller = document.scrollingElement;
    const fromBottom = scroller.scrollHeight - scroller.scrollTop;
    renderConversation(c, false);
    scroller.scrollTop = scroller.scrollHeight - fromBottom;
  }
}

function apply(c, update, earlier) {
  switch (update.op) {
    case "upsert": {
      const known = c.entries.has(update.entry.id);
      c.entries.set(update.entry.id, update.entry);
      if (!known && !earlier) c.order.push(update.entry.id);
      break;
    }
    case "append": {
      const entry = c.entries.get(update.id);
      if (entry) entry.text = (entry.text || "") + update.delta;
      break;
    }
    case "tool": {
      const entry = c.entries.get(update.id);
      if (entry) entry.state = update.state;
      break;
    }
    case "draft":
      applyDraft(c, update);
      break;
  }
  // The finished entry a draft was standing in for has arrived.
  if (update.op === "upsert" && c.draft && c.draft.done) c.draft = null;
}

// What the agent is writing right now: shown until its finished entry
// arrives, or a few seconds after it is done, whichever comes first.
function applyDraft(c, update) {
  if (update.state === "start") {
    c.draft = { thinking: Boolean(update.thinking), text: "", done: false };
  } else if (update.state === "more" && c.draft) {
    c.draft.text += update.text || "";
  } else if (update.state === "done" && c.draft) {
    c.draft.done = true;
    const draft = c.draft;
    setTimeout(() => {
      if (c.draft === draft) {
        c.draft = null;
        renderDraft(c);
      }
    }, 4000);
  }
}

function renderDraft(c) {
  const body = document.getElementById("conversation");
  if (!body) return;
  let node = document.getElementById("draft");
  if (!c.draft || !c.draft.text) {
    if (node) node.remove();
    return;
  }
  if (!node) {
    node = el("div", { id: "draft" });
    body.append(node);
  }
  node.className = c.draft.thinking ? "draft thinking" : `draft reply${c.draft.done ? "" : " writing"}`;
  node.textContent = c.draft.thinking ? `Thinking… ${c.draft.text}` : c.draft.text;
}

// --- views ---------------------------------------------------------------

function currentPane() {
  const match = location.hash.match(/^#\/pane\/(\d+)$/);
  return match ? Number(match[1]) : null;
}

window.addEventListener("hashchange", () => {
  const pane = currentPane();
  if (state.watching !== null && state.watching !== pane) {
    send({ type: "unwatch", pane: state.watching });
    state.watching = null;
  }
  if (state.screening !== null && state.screening !== pane) {
    send({ type: "unscreen", pane: state.screening });
    state.screening = null;
  }
  if (pane !== null && state.watching !== pane) {
    state.watching = pane;
    send({ type: "watch", pane });
  }
  render();
  if (pane !== null) window.scrollTo(0, document.body.scrollHeight);
});

function render() {
  if (!state.device) return;
  const pane = currentPane();
  if (pane === null) renderList();
  else renderAgent(pane);
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else node.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child !== null && child !== undefined && child !== false) {
      node.append(child instanceof Node ? child : document.createTextNode(String(child)));
    }
  }
  return node;
}

function banners() {
  const out = [];
  if (!state.connected) {
    out.push(el("div", { class: "banner bad", text: "Not connected to argus web. Retrying…" }));
  } else if (state.server && !state.server.connected) {
    out.push(el("div", { class: "banner bad", text: "argus web has lost argusd. Reconnecting…" }));
  } else if (state.server && state.server.daemon_version) {
    out.push(el("div", {
      class: "banner",
      text: `argusd is ${state.server.daemon_version}, argus web ${state.server.version}. Run argus server restart to match.`,
    }));
  }
  return out;
}

function ago(since) {
  if (!since) return "";
  const seconds = Math.max(0, Date.now() / 1000 - since);
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}

function describe(agent) {
  if (agent.loudest_child && agent.loudest !== agent.status) {
    return `${agent.loudest_child}: ${agent.loudest}`;
  }
  if (agent.note) return agent.note;
  if (agent.status === "working" && agent.tool) return `running ${agent.tool}`;
  return agent.status.replace("-", " ");
}

function renderList() {
  const rows = [];
  const withAgents = state.workspaces.filter((w) => w.agents.length > 0);
  for (const workspace of withAgents) {
    if (withAgents.length > 1 || !workspace.open) {
      rows.push(el("div", { class: "workspace", text: workspace.name }));
    }
    for (const agent of workspace.agents) {
      rows.push(el("button", {
        class: `agent s-${agent.loudest}`,
        onclick: () => { location.hash = `#/pane/${agent.pane}`; },
      },
        el("span", { class: "dot" }),
        el("span", { class: "title", text: agent.title }),
        el("span", { class: "since", text: ago(agent.since) }),
        el("span", { class: "line", text: describe(agent) }),
        el("span", {
          class: "where",
          text: [agent.project, agent.checkout, agent.template].filter(Boolean).join(" · "),
        }),
      ));
    }
  }
  if (rows.length === 0) {
    rows.push(el("p", {
      class: "empty",
      text: state.connected ? "No agents are running." : "Connecting…",
    }));
  }
  state.shell = null;
  const bell = state.push.possible
    ? el("button", {
      class: `bell${state.push.on ? " on" : ""}`,
      "aria-pressed": String(state.push.on),
      disabled: state.push.busy,
      text: state.push.on ? "Notifying" : "Notify me",
      onclick: togglePush,
    })
    : null;
  const places = state.workspaces.flatMap((w) => w.checkouts.map((c) => ({ ...c, workspace: w.name })));
  const add = state.templates.length && places.length
    ? el("button", {
      class: "add",
      "aria-label": "Start an agent",
      text: "+",
      onclick: () => { state.starting = { busy: false, error: null }; render(); },
    })
    : null;
  app.replaceChildren(
    el("header", { class: "bar" }, el("h1", {}, el("span", { class: "mark", text: "ARGUS" })), bell, add),
    ...banners(),
    ...(state.starting ? [startSheet(places)] : []),
    ...rows,
  );
}

/// Where and what to start: a checkout and a template.
function startSheet(places) {
  const sheet = state.starting;
  const where = el("select", { class: "field" },
    ...places.map((p) => el("option", {
      value: String(p.checkout),
      text: `${p.project} · ${p.name}${state.workspaces.length > 1 ? ` (${p.workspace})` : ""}`,
    })),
  );
  const what = el("select", { class: "field" }, ...state.templates.map((t) => el("option", { value: t, text: t })));
  const form = el("form", { class: "sheet" },
    el("h2", { text: "Start an agent" }),
    el("label", {}, "In", where),
    el("label", {}, "Agent", what),
    el("p", { class: "send-error", text: sheet.error || "" }),
    el("div", { class: "actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => { state.starting = null; render(); } }),
      el("button", { class: "go", type: "submit", disabled: sheet.busy, text: sheet.busy ? "Starting…" : "Start" }),
    ),
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    sheet.busy = true;
    sheet.error = null;
    send({ type: "start", checkout: Number(where.value), template: what.value });
    render();
  });
  return form;
}

function findAgent(pane) {
  for (const workspace of state.workspaces) {
    for (const agent of workspace.agents) if (agent.pane === pane) return agent;
  }
  return null;
}

function renderAgent(pane) {
  const agent = findAgent(pane);
  if (state.watching !== pane && state.connected) {
    state.watching = pane;
    send({ type: "watch", pane });
  }
  const shell = agentShell(pane);
  const status = agent ? agent.loudest : "exited";
  const working = agent && (agent.status === "working" || agent.status === "waiting");

  // The terminal opens by itself when the agent starts waiting on someone:
  // whatever it is asking is on its screen. A harness with no transcript
  // has only its screen.
  const was = state.lastStatus.get(pane);
  if (agent) state.lastStatus.set(pane, agent.status);
  // A harness with a live channel asks its questions in the conversation.
  if (agent && !agent.live && agent.status === "waiting" && was !== undefined && was !== "waiting") {
    state.tabs.set(pane, "terminal");
  }
  const readable = !agent || agent.has_transcript;
  const tab = readable ? state.tabs.get(pane) || "conversation" : "terminal";
  shell.tabConversation.setAttribute("aria-pressed", String(tab === "conversation"));
  shell.tabConversation.disabled = !readable;
  shell.tabTerminal.setAttribute("aria-pressed", String(tab === "terminal"));
  shell.body.hidden = tab !== "conversation";
  shell.term.hidden = tab !== "terminal";
  shell.keys.hidden = tab !== "terminal";
  if (tab === "terminal" && state.screening !== pane && state.connected) {
    state.screening = pane;
    send({ type: "screen", pane });
  } else if (tab !== "terminal" && state.screening === pane) {
    send({ type: "unscreen", pane });
    state.screening = null;
  }
  if (tab === "terminal") renderScreen(pane);

  shell.dot.className = `dot s-${status}`;
  shell.title.textContent = agent ? agent.title : `pane ${pane}`;
  shell.stop.hidden = !working;
  shell.banners.replaceChildren(...banners(), ...(agent && agent.status === "waiting"
    ? [el("div", { class: "banner", text: describe(agent) })]
    : []));

  shell.queue.replaceChildren(
    ...(agent ? agent.queued : []).map((message) => el("div", { class: "queued" },
      el("span", { class: "label", text: "Waiting for the agent" }),
      el("span", { class: "text", text: message.text }),
      el("button", {
        text: "Take back",
        onclick: () => send({ type: "cancel", pane, id: message.id }),
      }),
    )),
    ...state.notSent.filter((m) => m.pane === pane).map((message) => el("div", { class: "queued lost" },
      el("span", { class: "label", text: "Not sent: argusd restarted" }),
      el("span", { class: "text", text: message.text }),
      el("button", {
        text: "Resend",
        onclick: () => {
          state.notSent = state.notSent.filter((m) => m !== message);
          shell.input.value = message.text;
          fitInput(shell.input);
          render();
        },
      }),
    )),
  );

  const live = agent && agent.status !== "exited";
  shell.input.disabled = !live;
  shell.input.placeholder = live ? "Message the agent" : "This agent has exited";
  shell.primary.textContent = working ? "Queue" : "Send";
  shell.primary.disabled = !live || state.sending !== null;
  shell.now.hidden = !working;
  shell.now.disabled = state.sending !== null;
  shell.error.textContent = state.sendError || "";

  if (tab === "conversation") renderConversation(conversation(pane), false);
}

/// The agent view for `pane`: built once, then only filled in.
function agentShell(pane) {
  if (state.shell && state.shell.pane === pane && app.contains(state.shell.root)) return state.shell;
  const shell = { pane };
  shell.dot = el("span", { class: "dot" });
  shell.title = el("h1");
  shell.stop = el("button", {
    class: "stop",
    text: "Stop",
    onclick: () => send({ type: "stop", pane }),
  });
  shell.close = el("button", {
    class: "close",
    "aria-label": "Close this agent",
    text: "Close",
    onclick: () => {
      const agent = findAgent(pane);
      const name = agent ? agent.title : `pane ${pane}`;
      if (!window.confirm(`Close ${name}? Its process ends.`)) return;
      send({ type: "close", pane });
      location.hash = "#/";
    },
  });
  shell.banners = el("div");
  shell.body = el("section", { class: "conversation", id: "conversation" });
  shell.term = el("section", { class: "screen", hidden: true });
  shell.rows = [];
  shell.keys = el("div", { class: "keys", hidden: true },
    ...KEYS.map(([key, label]) => el("button", {
      type: "button",
      text: label,
      onclick: () => send({ type: "key", pane, key }),
    })),
  );
  shell.tabConversation = el("button", {
    text: "Conversation",
    onclick: () => { state.tabs.set(pane, "conversation"); render(); scrollToEnd(); },
  });
  shell.tabTerminal = el("button", {
    text: "Terminal",
    onclick: () => { state.tabs.set(pane, "terminal"); render(); },
  });
  shell.queue = el("div", { class: "queue" });
  shell.input = el("textarea", { rows: "1", maxlength: "16384", enterkeyhint: "send" });
  shell.input.addEventListener("input", () => fitInput(shell.input));
  shell.primary = el("button", { class: "go", type: "submit" });
  shell.now = el("button", {
    class: "now",
    type: "button",
    text: "Send now",
    onclick: () => submit(pane, true),
  });
  shell.error = el("p", { class: "send-error" });
  const form = el("form", { class: "composer" }, shell.input, shell.now, shell.primary);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submit(pane, false);
  });
  shell.root = el("div", { class: "agent-view" },
    el("header", { class: "bar" },
      el("button", { "aria-label": "All agents", onclick: () => { location.hash = "#/"; }, text: "‹" }),
      shell.dot,
      shell.title,
      shell.stop,
      shell.close,
    ),
    shell.banners,
    el("nav", { class: "tabs" }, shell.tabConversation, shell.tabTerminal),
    shell.body,
    shell.term,
    el("footer", { class: "dock" }, shell.keys, shell.queue, shell.error, form),
  );
  app.replaceChildren(shell.root);
  state.shell = shell;
  state.sendError = null;
  return shell;
}

function submit(pane, now) {
  const text = state.shell.input.value;
  if (!text.trim() || state.sending) return;
  state.sending = { pane };
  state.sendError = null;
  send({ type: "send", pane, text, now });
  render();
}

// The key bar: what a harness's menus and prompts are answered with.
const KEYS = [
  ["1", "1"], ["2", "2"], ["3", "3"], ["4", "4"],
  ["up", "↑"], ["down", "↓"], ["enter", "Enter"], ["esc", "Esc"], ["tab", "Tab"],
  ["y", "y"], ["n", "n"],
];

function scrollToEnd() {
  window.scrollTo(0, document.body.scrollHeight);
}

function applyScreen({ pane, fresh, rows, cols, lines }) {
  let screen = state.screens.get(pane);
  if (fresh || !screen || screen.rows !== rows || screen.cols !== cols) {
    screen = { rows, cols, lines: new Array(rows).fill(null).map(() => []), changed: new Set(), all: true };
    state.screens.set(pane, screen);
  }
  for (const [row, runs] of lines) {
    if (row < rows) {
      screen.lines[row] = runs;
      screen.changed.add(row);
    }
  }
  if (state.shell && state.shell.pane === pane && !state.shell.term.hidden) renderScreen(pane);
}

/// Draws the pane's grid at its own width, sized to fit the phone's.
function renderScreen(pane) {
  const shell = state.shell;
  const screen = state.screens.get(pane);
  if (!shell || !screen) return;
  if (screen.all || shell.rows.length !== screen.rows) {
    shell.rows = screen.lines.map(() => el("div", { class: "row" }));
    shell.term.replaceChildren(...shell.rows);
    screen.changed = new Set(screen.lines.keys());
    screen.all = false;
  }
  const width = shell.term.clientWidth || window.innerWidth;
  const size = Math.max(6, Math.min(14, (width - 12) / (screen.cols * 0.6)));
  shell.term.style.fontSize = `${size.toFixed(2)}px`;
  for (const row of screen.changed) {
    shell.rows[row].replaceChildren(...screen.lines[row].map(drawRun));
  }
  screen.changed.clear();
}

function drawRun([text, fg, bg, flags]) {
  const span = document.createElement("span");
  span.textContent = text;
  let [color, background] = [fg, bg];
  if (flags & 8) [color, background] = [bg || "var(--screen-bg)", fg || "var(--text)"];
  if (color) span.style.color = color;
  if (background) span.style.backgroundColor = background;
  if (flags & 1) span.style.fontWeight = "700";
  if (flags & 2) span.style.fontStyle = "italic";
  if (flags & 4) span.style.textDecoration = "underline";
  return span;
}

function fitInput(input) {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 160)}px`;
}

function nearBottom() {
  const scroller = document.scrollingElement;
  return scroller.scrollHeight - scroller.scrollTop - window.innerHeight < 120;
}

function renderConversation(c, scroll) {
  const body = document.getElementById("conversation");
  if (!body) return;
  const nodes = [];
  if (c.earlier) {
    nodes.push(el("button", {
      class: "earlier",
      text: c.asked ? "Loading…" : "Show earlier",
      onclick: () => {
        if (c.asked) return;
        c.asked = c.earlier;
        send({ type: "earlier", pane: currentPane(), before: c.earlier });
        renderConversation(c, false);
      },
    }));
  }
  // A tool's result is shown inside its call, when the call is held.
  const results = new Map();
  for (const id of c.order) {
    const entry = c.entries.get(id);
    if (entry && entry.kind === "tool_result") results.set(entry.call, entry);
  }
  for (const id of c.order) {
    const entry = c.entries.get(id);
    if (!entry) continue;
    if (entry.kind === "tool_result" && c.entries.has(entry.call)) continue;
    const node = renderEntry(entry, results);
    if (node) nodes.push(node);
  }
  if (nodes.length === 0 && !c.draft) nodes.push(el("p", { class: "empty", text: "Nothing said yet." }));
  body.replaceChildren(...nodes);
  renderDraft(c);
  if (scroll) window.scrollTo(0, document.body.scrollHeight);
}

const GLYPHS = { running: "◌", done: "✓", failed: "✗" };

function renderEntry(entry, results) {
  switch (entry.kind) {
    case "prompt":
      return el("div", { class: "prompt", text: entry.text });
    case "reply": {
      const node = el("div", { class: "reply" });
      // Rendered by the server into markup that runs nothing.
      if (entry.html !== undefined) node.innerHTML = entry.html;
      else node.textContent = entry.text || "";
      return node;
    }
    case "thinking":
      return el("details", { class: "detail thinking" },
        el("summary", {}, el("span", { class: "what", text: "Thinking" })),
        el("pre", { text: entry.text }),
      );
    case "tool_call": {
      const result = results.get(entry.id);
      return el("details", { class: `detail tool t-${entry.state}` },
        el("summary", {},
          el("span", { class: "glyph", text: GLYPHS[entry.state] || "·" }),
          el("span", { class: "name", text: entry.tool }),
          el("span", { class: "what", text: entry.summary }),
        ),
        entry.input ? el("pre", { text: entry.input }) : null,
        result && result.text ? el("pre", { class: `result${result.failed ? " failed" : ""}`, text: result.text }) : null,
      );
    }
    case "tool_result":
      return el("details", { class: `detail tool t-${entry.failed ? "failed" : "done"}` },
        el("summary", {},
          el("span", { class: "glyph", text: entry.failed ? GLYPHS.failed : GLYPHS.done }),
          el("span", { class: "what", text: "Result" }),
        ),
        el("pre", { class: "result", text: entry.text }),
      );
    case "notice":
      return el("div", { class: "notice", text: entry.text });
    case "turn_end":
      return el("div", {
        class: "turn-end",
        text: entry.millis ? `Turn finished in ${duration(entry.millis)}` : "Turn finished",
      });
    case "divider":
      return el("div", { class: "divider", text: entry.text });
    case "question":
      // Posed by a harness that takes its answer through Argus. Answered
      // anywhere — here, on the desktop — it stays only as a record.
      return el("div", { class: `question${entry.answered ? " answered" : ""}` },
        el("p", { class: "ask", text: entry.text }),
        el("div", { class: "choices" },
          ...(entry.choices || []).map(([id, label]) => el("button", {
            text: label,
            disabled: Boolean(entry.answered),
            "aria-pressed": String(entry.answered === id),
            onclick: () => send({ type: "answer", pane: currentPane(), question: entry.id, choice: id }),
          })),
        ),
      );
    default:
      return null;
  }
}

function duration(millis) {
  const seconds = Math.round(millis / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${seconds % 60}s`;
}

// --- pairing -------------------------------------------------------------

function showPairing(code) {
  state.device = null;
  const error = el("p", { class: "error" });
  const codeInput = el("input", {
    class: "code", inputmode: "numeric", autocomplete: "one-time-code",
    maxlength: "6", value: code, required: true,
  });
  const nameInput = el("input", { value: guessName(), maxlength: "64", required: true });
  const form = el("form", { class: "pair" },
    el("h2", { text: "Pair this device" }),
    el("p", { text: "Enter the code argus web printed in its terminal." }),
    el("label", {}, "Code", codeInput),
    el("label", {}, "Name this device", nameInput),
    el("button", { class: "go", type: "submit", text: "Pair" }),
    error,
  );
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    error.textContent = "";
    const response = await fetch("/api/pair", {
      method: "POST",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ code: codeInput.value, name: nameInput.value }),
    }).catch(() => null);
    if (!response) {
      error.textContent = "argus web is not answering.";
      return;
    }
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
      error.textContent = body.error || "Pairing was refused.";
      return;
    }
    state.device = body.device;
    history.replaceState(null, "", location.pathname);
    connect();
    render();
  });
  app.replaceChildren(
    el("header", { class: "bar" }, el("h1", {}, el("span", { class: "mark", text: "ARGUS" }))),
    form,
  );
  if (code) nameInput.focus();
  else codeInput.focus();
}

function guessName() {
  const agent = navigator.userAgent;
  if (/iPhone/.test(agent)) return "iPhone";
  if (/iPad/.test(agent)) return "iPad";
  if (/Android/.test(agent)) return "Android";
  return "Browser";
}

// Relative times drift while the list sits open.
setInterval(() => { if (currentPane() === null) render(); }, 30000);

start();
