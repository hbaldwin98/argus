// argus:managed-plugin
//
// Written by Argus when it starts an opencode pane in this checkout, and
// deleted when the last one closes. Edits are lost; put your own plugins in
// another file beside this one.
//
// Everything it needs comes from the environment Argus hands the pane, so a
// single file serves every pane in the checkout and each opencode process
// still reports to its own row. That holds only while the plugin runs in a
// process the pane started: opencode V2 hosts plugins in a background server
// shared by every client, which inherits the environment of whichever pane
// started it, so Argus launches opencode with `--standalone`.
//
// opencode V2 loads the default export and calls `setup`; V1 from 1.18.29
// calls `server` and takes the hooks it returns. Both report through the
// same functions below.

const BASE = process.env.ARGUS_HOOK_URL;
const TOKEN = process.env.ARGUS_HOOK_TOKEN;
const INSTRUCTIONS = process.env.ARGUS_INSTRUCTIONS;

// The session this pane is showing. opencode's server emits events for
// subagent sessions too, and one of those going idle says nothing about
// whether the pane is still busy.
let rootSession;
const children = new Set();

// Several of these fire per turn, and each row only changes on a change.
const lastSent = new Map();
let lastSessionSent;

async function report(sessionID, status, note = "") {
  if (!BASE || !TOKEN || !sessionID) return;
  const key = `${status} ${note}`;
  if (key === lastSent.get(sessionID)) return;
  lastSent.set(sessionID, key);
  try {
    await fetch(`${BASE}/status/${status}`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${TOKEN}`,
        "X-Argus-Session": sessionID,
      },
      body: note,
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Deliberately silent. The daemon that wrote this file may already have
    // exited, and a port that now belongs to nobody must degrade to a stale
    // row — never to an error the user sees in the middle of a turn.
  }
}

async function reportSession(sessionID) {
  if (!BASE || !TOKEN || !sessionID || sessionID === lastSessionSent) return;
  lastSessionSent = sessionID;
  try {
    await fetch(`${BASE}/session`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${TOKEN}`,
        "X-Argus-Session": sessionID,
      },
      body: sessionID,
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Session identity is best-effort for the same reason as status.
  }
}

// Telemetry is the pane's own conversation only, summed across its model
// calls. V1 repeats a whole message on each update, so usage is kept per
// call and totalled, never added on each update.
const usage = new Map();
let lastTelemetry;

async function reportTelemetry(fields, sessionID) {
  if (!BASE || !TOKEN) return;
  const body = JSON.stringify(fields);
  if (body === lastTelemetry) return;
  lastTelemetry = body;
  try {
    await fetch(`${BASE}/telemetry`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${TOKEN}`,
        ...(sessionID ? { "X-Argus-Session": sessionID } : {}),
      },
      body,
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Best-effort, like status.
  }
}

// The newest call's size, which is what the context holds now. A call that
// does not measure it (a compaction) leaves the last one standing.
let lastContext = null;

// One model call's usage, under a key that names the call. Both versions
// spell tokens the same way: input, output, reasoning and cache.read/write.
async function recordUsage(key, { cost, tokens = {}, model, sessionID, measuresContext = true }) {
  const cache = tokens.cache ?? {};
  usage.set(key, {
    cost: Number(cost) || 0,
    input: (tokens.input ?? 0) + (cache.read ?? 0) + (cache.write ?? 0),
    output: (tokens.output ?? 0) + (tokens.reasoning ?? 0),
  });
  let total = { cost: 0, input: 0, output: 0 };
  for (const u of usage.values()) {
    total.cost += u.cost;
    total.input += u.input;
    total.output += u.output;
  }
  if (measuresContext) {
    const context = usage.get(key).input + usage.get(key).output;
    lastContext = context > 0 ? context : null;
  }
  await reportTelemetry(
    {
      model: model ? String(model) : null,
      context_tokens: lastContext,
      input_tokens: total.input,
      output_tokens: total.output,
      cost_usd: total.cost,
    },
    sessionID,
  );
}

function forgetUsage() {
  usage.clear();
  lastContext = null;
  lastTelemetry = undefined;
}

// --- the inbox -----------------------------------------------------------
//
// What Argus has for the agent — a message for it, such as a review
// comment — arrives on a stream this plugin keeps open, and goes in through
// opencode's own server rather than as typing, so it can never land in a
// dialog. Silent and patient when nobody answers: the daemon may be
// restarting.

let inboxOpen = false;

// Unreferenced, so waiting to reconnect never keeps a process alive on
// its own.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms).unref?.());

async function act(send, item) {
  try {
    if (item?.Message) await send(rootSession, item.Message.text);
  } catch {
    // The agent is where it is; what happens next shows in its pane.
  }
}

// `send(sessionID, text)` puts one message to the agent, in whichever
// version's words.
async function openInbox(send) {
  if (inboxOpen || !BASE || !TOKEN || !send) return;
  inboxOpen = true;
  let wait = 500;
  for (;;) {
    try {
      const response = await fetch(`${BASE}/inbox`, {
        headers: { authorization: `Bearer ${TOKEN}`, "X-Argus-Session": rootSession ?? "" },
      });
      if (response.ok && response.body) {
        wait = 500;
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          let end;
          while ((end = buffer.indexOf("\n\n")) >= 0) {
            const event = buffer.slice(0, end);
            buffer = buffer.slice(end + 2);
            for (const line of event.split("\n")) {
              if (line.startsWith("data: ")) await act(send, JSON.parse(line.slice(6)));
            }
          }
        }
      }
    } catch {
      // Nobody listening yet, or any more.
    }
    await sleep(wait);
    wait = Math.min(wait * 2, 10000);
  }
}

// A session belongs to this pane unless we saw it created with a parent.
// The first session we hear about is the pane's own.
function ownedByPane(sessionID) {
  if (!sessionID) return true;
  if (children.has(sessionID)) return false;
  rootSession ??= sessionID;
  return sessionID === rootSession;
}

// One line a human can act on. An empty note is better than a serialized
// event under a row.
function firstLine(raw) {
  return typeof raw === "string" ? raw.split("\n")[0].trim().slice(0, 200) : "";
}

// --- what both versions report alike ------------------------------------

// The instant signal: this fires when the prompt is submitted, before any
// model call, which is what turns the row over as you hit enter.
async function promptSubmitted(sessionID) {
  if (ownedByPane(sessionID)) {
    await reportSession(rootSession);
    await report(rootSession, "working");
  } else {
    await report(sessionID, "working");
  }
}

async function toolStarted(tool, sessionID) {
  if (ownedByPane(sessionID)) await reportTelemetry({ tool: String(tool) }, sessionID);
}

async function toolEnded(sessionID) {
  if (ownedByPane(sessionID)) await reportTelemetry({ tool: "" }, sessionID);
}

async function childCreated(sessionID) {
  children.add(sessionID);
  await report(sessionID, "working");
}

// OpenCode keeps the same process and plugin when the user starts a new
// conversation. A newly created root replaces the root this pane is
// showing; otherwise every event from the new conversation would be
// mistaken for another session and the previous status would stick.
async function rootCreated(sessionID) {
  rootSession = sessionID;
  forgetUsage();
  await reportSession(rootSession);
  await report(rootSession, "idle");
}

// Every other event first settles whose it is, and opens the inbox once
// the pane has a session for it to talk to.
async function claim(sessionID, send) {
  if (ownedByPane(sessionID)) await reportSession(rootSession);
  if (rootSession) void openInbox(send);
  return sessionID ?? rootSession;
}

async function sessionDeleted(sessionID) {
  await report(sessionID, "idle");
  children.delete(sessionID);
  lastSent.delete(sessionID);
}

// The authoritative one. opencode drops the session to `idle` both when a
// turn ends and when the user aborts it, which is what keeps a manually
// stopped agent from sitting at "working" forever.
async function statusChanged(sessionID, status) {
  const kind = status?.type ?? status;
  if (kind === "idle") await report(sessionID, "idle");
  else if (kind === "busy" || kind === "retry") await report(sessionID, "working");
}

// --- opencode V2 ---------------------------------------------------------

// The model of the call in flight, per session: V2 names it when a step
// starts and counts its tokens when the step ends.
const stepModel = new Map();

async function v2Event(event, send) {
  const type = event?.type;
  const data = event?.data ?? {};
  if (type === "session.created") {
    if (data.parentID) await childCreated(data.sessionID);
    else await rootCreated(data.sessionID);
    return;
  }
  if (type === "session.step.started") {
    stepModel.set(data.sessionID, data.model?.id);
    // A model call under way: clears a retry's note once the retry is
    // answered, and costs nothing while the row already says working.
    await report(data.sessionID, "working");
    return;
  }
  if (type === "session.step.ended") {
    if (ownedByPane(data.sessionID)) {
      await recordUsage(event.id ?? data.assistantMessageID, {
        cost: data.cost,
        tokens: data.tokens,
        model: stepModel.get(data.sessionID),
        sessionID: data.sessionID,
      });
    }
    return;
  }
  // A summary is a paid model call like any other, but the transcript it
  // read is the one it replaces, so it says nothing of the context left.
  if (type === "session.compaction.ended") {
    if (ownedByPane(data.sessionID)) {
      await recordUsage(event.id ?? `compaction ${data.sessionID}`, {
        cost: data.cost,
        tokens: data.tokens,
        model: data.model?.id,
        sessionID: data.sessionID,
        measuresContext: false,
      });
    }
    return;
  }
  // A form names its session inside itself; every other event beside it.
  const sessionID = await claim(data.sessionID ?? data.form?.sessionID, send);
  switch (type) {
    // V2's public stream has no `session.status` or `session.idle`; those
    // are V1's. An execution is one run of the agent loop, and it ends in
    // exactly one of succeeded, interrupted (a manual stop among them) or
    // failed, which is what opencode's own TUI goes idle on.
    case "session.execution.started":
      await report(sessionID, "working");
      break;
    case "session.execution.succeeded":
    case "session.execution.interrupted":
      await report(sessionID, "idle");
      break;
    case "session.deleted":
      stepModel.delete(sessionID);
      await sessionDeleted(sessionID);
      break;
    case "permission.asked":
      await report(sessionID, "waiting", firstLine(data.message ?? data.action));
      break;
    // The question tool: the agent has stopped to ask the user something,
    // and only an answer or a dismissal sets it going again.
    case "form.created":
      await report(sessionID, "waiting", firstLine(data.form?.title));
      break;
    case "permission.replied":
    case "form.replied":
    case "form.cancelled":
    case "session.compaction.started":
      await report(sessionID, "working");
      break;
    // A provider error the loop will try again: still working, but a row
    // stuck on one call should say why.
    case "session.retry.scheduled":
      await report(sessionID, "working", firstLine(`retrying: ${data.error?.message ?? data.error?.type ?? ""}`));
      break;
    case "session.execution.failed":
      await report(sessionID, "failed", firstLine(data.error?.message ?? data.error?.type));
      break;
    default:
      break;
  }
}

async function setup(ctx) {
  if (!BASE || !TOKEN) return;
  const send = (sessionID, text) => ctx.session.prompt({ sessionID, text });
  const registrations = await Promise.all([
    ctx.session.hook("prompt", (ev) => promptSubmitted(ev.sessionID)),
    // Where the agent is told it can name its own row. The system prompt
    // is the one place a standing fact like this survives compaction.
    ctx.session.hook("context", (ev) => {
      if (INSTRUCTIONS) ev.system.push({ type: "text", text: INSTRUCTIONS });
    }),
    ctx.tool.hook("execute.before", (ev) => toolStarted(ev.tool, ev.sessionID)),
    ctx.tool.hook("execute.after", (ev) => toolEnded(ev.sessionID)),
  ]);
  const stop = new AbortController();
  void (async () => {
    try {
      for await (const event of ctx.event.subscribe({ signal: stop.signal })) {
        await v2Event(event, send);
      }
    } catch {
      // The subscription ends with the plugin; nothing to report then.
    }
  })();
  return async () => {
    stop.abort();
    for (const registration of registrations) await registration?.dispose?.();
  };
}

// --- opencode V1 ---------------------------------------------------------

async function server({ client } = {}) {
  if (!BASE || !TOKEN) return {};
  const send = client
    ? (sessionID, text) => {
        const body = { parts: [{ type: "text", text }] };
        const prompt = client.session.promptAsync ?? client.session.prompt;
        return prompt.call(client.session, { path: { id: sessionID }, body });
      }
    : undefined;

  return {
    "chat.message": async ({ sessionID }) => promptSubmitted(sessionID),

    // V1 has no session-start hook whose output reaches the model either.
    "experimental.chat.system.transform": async (_input, output) => {
      if (INSTRUCTIONS) output.system.push(INSTRUCTIONS);
    },

    "tool.execute.before": async ({ tool, sessionID }) => toolStarted(tool, sessionID),
    "tool.execute.after": async ({ sessionID }) => toolEnded(sessionID),

    event: async ({ event }) => {
      const type = event?.type;
      const props = event?.properties ?? {};
      if (type === "session.created" && props.info?.id) {
        if (props.info.parentID) await childCreated(props.info.id);
        else await rootCreated(props.info.id);
        return;
      }
      if (type === "message.updated") {
        const info = props.info;
        if (info?.role === "assistant" && info.id && ownedByPane(info.sessionID)) {
          await recordUsage(info.id, {
            cost: info.cost,
            tokens: info.tokens,
            model: info.modelID,
            sessionID: info.sessionID,
          });
        }
        return;
      }
      if (type === "message.part.updated") {
        const part = props.part;
        if (part && ownedByPane(part.sessionID)) await reportSession(rootSession);
        return;
      }
      const sessionID = await claim(props.sessionID ?? props.info?.id, send);
      switch (type) {
        case "session.status":
          await statusChanged(sessionID, props.status);
          break;
        case "session.idle":
          await report(sessionID, "idle");
          break;
        case "session.deleted":
          await sessionDeleted(sessionID);
          break;
        case "permission.asked":
        case "permission.updated":
          await report(sessionID, "waiting", firstLine(props.title ?? props.message));
          break;
        case "permission.replied":
        case "session.compacted":
          await report(sessionID, "working");
          break;
        case "session.error":
          await report(
            sessionID,
            "failed",
            firstLine(props.error?.data?.message ?? props.error?.name ?? props.message),
          );
          break;
        default:
          break;
      }
    },
  };
}

export default { id: "argus-status", setup, server };
