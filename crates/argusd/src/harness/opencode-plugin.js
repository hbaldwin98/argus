// argus:managed-plugin
//
// Written by Argus when it starts an opencode pane in this checkout, and
// deleted when the last one closes. Edits are lost; put your own plugins in
// another file beside this one.
//
// Everything it needs comes from the environment Argus hands the pane, so a
// single file serves every pane in the checkout and each opencode process
// still reports to its own row.

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

// Telemetry is the pane's own conversation only, summed across its
// assistant messages. Each message.updated repeats the whole message, so
// usage is kept per message id and totalled, never added on each update.
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

async function messageTelemetry(info) {
  if (info?.role !== "assistant" || !info.id) return;
  const tokens = info.tokens ?? {};
  const cache = tokens.cache ?? {};
  usage.set(info.id, {
    cost: Number(info.cost) || 0,
    input: (tokens.input ?? 0) + (cache.read ?? 0) + (cache.write ?? 0),
    output: (tokens.output ?? 0) + (tokens.reasoning ?? 0),
  });
  let cost = 0, input = 0, output = 0;
  for (const u of usage.values()) {
    cost += u.cost;
    input += u.input;
    output += u.output;
  }
  const context = usage.get(info.id).input + usage.get(info.id).output;
  await reportTelemetry(
    {
      model: info.modelID ? String(info.modelID) : null,
      context_tokens: context > 0 ? context : null,
      input_tokens: input,
      output_tokens: output,
      cost_usd: cost,
    },
    info.sessionID,
  );
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

async function act(client, item) {
  const session = { path: { id: rootSession } };
  try {
    if (item?.Message) {
      const body = { parts: [{ type: "text", text: item.Message.text }] };
      const send = client.session.promptAsync ?? client.session.prompt;
      await send.call(client.session, { ...session, body });
    }
  } catch {
    // The agent is where it is; what happens next shows in its pane.
  }
}

async function openInbox(client) {
  if (inboxOpen || !BASE || !TOKEN || !client) return;
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
              if (line.startsWith("data: ")) await act(client, JSON.parse(line.slice(6)));
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

// One line a human can act on, from whichever field this event carries it
// in. An empty note is better than a serialized event under a row.
function noteFrom(props) {
  const raw =
    props.title ??
    props.error?.data?.message ??
    props.error?.name ??
    props.message ??
    "";
  return typeof raw === "string" ? raw.split("\n")[0].trim().slice(0, 200) : "";
}

export const ArgusStatus = async ({ client } = {}) => {
  if (!BASE || !TOKEN) return {};

  return {
    // The instant signal: this fires when the prompt is submitted, before
    // any model call, which is what turns the row over as you hit enter.
    "chat.message": async ({ sessionID }, ) => {
      if (ownedByPane(sessionID)) {
        await reportSession(rootSession);
        await report(rootSession, "working");
      } else {
        await report(sessionID, "working");
      }
    },

    // Where the agent is told it can name its own row. opencode has no
    // session-start hook whose output reaches the model, and the system
    // prompt is the one place a standing fact like this survives
    // compaction.
    "experimental.chat.system.transform": async (_input, output) => {
      if (INSTRUCTIONS) output.system.push(INSTRUCTIONS);
    },

    "tool.execute.before": async ({ tool, sessionID }) => {
      if (ownedByPane(sessionID)) await reportTelemetry({ tool: String(tool) }, sessionID);
    },

    "tool.execute.after": async ({ sessionID }) => {
      if (ownedByPane(sessionID)) await reportTelemetry({ tool: "" }, sessionID);
    },

    event: async ({ event }) => {
      const type = event?.type;
      const props = event?.properties ?? {};
      const sessionID = props.sessionID ?? props.info?.id;

      if (type === "session.created" && props.info?.id && props.info?.parentID) {
        children.add(props.info.id);
        await report(props.info.id, "working");
        return;
      }
      // OpenCode keeps the same process and plugin when the user starts a
      // new conversation. A newly created root replaces the root this pane
      // is showing; otherwise every event from the new conversation would
      // be mistaken for another session and the previous status would stick.
      if (type === "message.updated") {
        if (ownedByPane(props.info?.sessionID)) await messageTelemetry(props.info);
        return;
      }
      if (type === "message.part.updated") {
        const part = props.part;
        if (part && ownedByPane(part.sessionID)) await reportSession(rootSession);
        return;
      }
      if (type === "session.created" && props.info?.id && !props.info.parentID) {
        rootSession = props.info.id;
        usage.clear();
        lastTelemetry = undefined;
        await reportSession(rootSession);
        await report(rootSession, "idle");
        return;
      }
      const root = ownedByPane(sessionID);
      if (root) await reportSession(rootSession);
      if (rootSession) void openInbox(client);

      switch (type) {
        // The authoritative one. opencode drops the session to `idle` both
        // when a turn ends and when the user aborts it, which is what keeps
        // a manually stopped agent from sitting at "working" forever.
        case "session.status": {
          const kind = props.status?.type ?? props.status;
          if (kind === "idle") await report(sessionID ?? rootSession, "idle");
          else if (kind === "busy" || kind === "retry") {
            await report(sessionID ?? rootSession, "working");
          }
          break;
        }
        case "session.idle":
          await report(sessionID ?? rootSession, "idle");
          break;
        case "session.deleted":
          await report(sessionID ?? rootSession, "idle");
          children.delete(sessionID);
          lastSent.delete(sessionID);
          break;
        case "permission.asked":
        case "permission.updated":
          await report(sessionID ?? rootSession, "waiting", noteFrom(props));
          break;
        case "permission.replied":
          await report(sessionID ?? rootSession, "working");
          break;
        case "session.compacted":
          await report(sessionID ?? rootSession, "working");
          break;
        case "session.error":
          await report(sessionID ?? rootSession, "failed", noteFrom(props));
          break;
        default:
          break;
      }
    },
  };
};
