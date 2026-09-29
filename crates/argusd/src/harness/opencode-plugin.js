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

// --- the conversation ----------------------------------------------------
//
// opencode keeps its conversation in its own database rather than a file
// Argus can read, so this sends it: the whole session whenever the pane's
// conversation starts or changes, then each part as it is written. The
// entries are Argus's own shape (argus_protocol::transcript); the daemon
// clips them, keeps the recent end, and shows them to whoever watches.

const roles = new Map();
const pending = new Map();
let flushing;
let replayedFor;

// Well under the daemon's limit on one push, so a long replay goes in parts.
const PUSH_BYTES = 192 * 1024;

function entry(id, body, time) {
  return { Upsert: { id: String(id), at: time ? new Date(time).toISOString() : null, body } };
}

// What one part of a message says, as entries. A tool part carries its
// call and, once it has one, its result.
function partUpdates(part, role) {
  switch (part?.type) {
    case "text": {
      if (part.synthetic || part.ignored || !String(part.text ?? "").trim()) return [];
      const body = role === "user" ? { Prompt: { text: part.text } } : { Reply: { text: part.text } };
      return [entry(part.id, body, part.time?.start)];
    }
    case "reasoning":
      if (!String(part.text ?? "").trim()) return [];
      return [entry(part.id, { Thinking: { text: part.text } }, part.time?.start)];
    case "tool": {
      const state = part.state ?? {};
      const input = state.input ?? {};
      const done = state.status === "completed" || state.status === "error";
      const summary =
        state.title || input.description || input.command || input.filePath || input.pattern || input.url || "";
      const updates = [
        entry(
          part.id,
          {
            ToolCall: {
              tool: String(part.tool ?? "tool"),
              summary: String(summary),
              input: JSON.stringify(input, null, 2),
              state: state.status === "completed" ? "Done" : state.status === "error" ? "Failed" : "Running",
            },
          },
          state.time?.start,
        ),
      ];
      if (done) {
        updates.push(
          entry(
            `${part.id}.result`,
            {
              ToolResult: {
                call: String(part.id),
                output: String(state.output ?? state.error ?? ""),
                failed: state.status === "error",
              },
            },
            state.time?.end,
          ),
        );
      }
      return updates;
    }
    default:
      return [];
  }
}

async function pushTranscript(updates, fresh) {
  if (!BASE || !TOKEN || !rootSession) return;
  // Split so no one push outgrows what the daemon takes; only the first of
  // a replay starts the conversation over.
  const batches = [[]];
  let size = 0;
  for (const update of updates) {
    const length = JSON.stringify(update).length;
    if (size + length > PUSH_BYTES && batches[batches.length - 1].length) {
      batches.push([]);
      size = 0;
    }
    batches[batches.length - 1].push(update);
    size += length;
  }
  for (const [index, batch] of batches.entries()) {
    if (!batch.length && !(fresh && index === 0)) continue;
    try {
      await fetch(`${BASE}/transcript`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${TOKEN}`,
          "content-type": "application/json",
          "X-Argus-Session": rootSession,
        },
        body: JSON.stringify({ fresh: fresh && index === 0, updates: batch }),
        signal: AbortSignal.timeout(5000),
      });
    } catch {
      // Best-effort, like status.
    }
  }
}

// Parts change many times a second while a reply streams; each entry is
// sent at most every quarter second, as it then stands.
function queueUpdates(updates) {
  for (const update of updates) pending.set(update.Upsert?.id ?? JSON.stringify(update), update);
  flushing ??= setTimeout(async () => {
    flushing = undefined;
    const batch = [...pending.values()];
    pending.clear();
    await pushTranscript(batch, false);
  }, 250);
}

// The whole of a conversation, read back through opencode's own server.
async function replay(client, sessionID) {
  if (!client?.session?.messages) return;
  replayedFor = sessionID;
  try {
    const response = await client.session.messages({ path: { id: sessionID } });
    const messages = response?.data ?? response ?? [];
    const updates = [];
    for (const message of Array.isArray(messages) ? messages : []) {
      if (message?.info?.id) roles.set(message.info.id, message.info.role);
      for (const part of message?.parts ?? []) updates.push(...partUpdates(part, message?.info?.role));
    }
    pending.clear();
    await pushTranscript(updates, true);
  } catch {
    // An opencode without the call, or one busy starting: the parts that
    // stream from here on still arrive.
  }
}

// --- the inbox -----------------------------------------------------------
//
// What Argus has for the agent — a message from the phone, an interrupt,
// the answer to a permission — arrives on a stream this plugin keeps open,
// and goes in through opencode's own server rather than as typing, so it
// can never land in a dialog. Silent and patient when nobody answers: the
// daemon may be restarting.

const permissions = new Map();
let inboxOpen = false;

// Unreferenced, so waiting to reconnect never keeps a process alive on
// its own.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms).unref?.());

async function act(client, item) {
  const session = { path: { id: rootSession } };
  try {
    if (item === "Interrupt") {
      await client.session.abort(session);
    } else if (item?.Message) {
      const body = { parts: [{ type: "text", text: item.Message.text }] };
      const send = client.session.promptAsync ?? client.session.prompt;
      await send.call(client.session, { ...session, body });
    } else if (item?.Answer) {
      const asked = permissions.get(item.Answer.question);
      await client.postSessionIdPermissionsPermissionId({
        path: { id: asked?.sessionID ?? rootSession, permissionID: item.Answer.question },
        body: { response: item.Answer.choice },
      });
    }
  } catch {
    // The agent is where it is; the phone sees what happens next.
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

// A permission the agent is waiting on, as a question the phone can answer.
function permissionQuestion(permission, answered = null) {
  return entry(permission.id, {
    Question: {
      prompt: String(permission.title ?? "Allow this?"),
      choices: [
        { id: "once", label: "Allow once" },
        { id: "always", label: "Always allow" },
        { id: "reject", label: "Reject" },
      ],
      answered,
    },
  });
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
        if (props.info?.id) roles.set(props.info.id, props.info.role);
        if (ownedByPane(props.info?.sessionID)) await messageTelemetry(props.info);
        return;
      }
      if (type === "message.part.updated") {
        const part = props.part;
        if (part && ownedByPane(part.sessionID)) {
          await reportSession(rootSession);
          if (replayedFor !== rootSession) await replay(client, rootSession);
          queueUpdates(partUpdates(part, roles.get(part.messageID) ?? "assistant"));
        }
        return;
      }
      if (type === "session.created" && props.info?.id && !props.info.parentID) {
        rootSession = props.info.id;
        usage.clear();
        lastTelemetry = undefined;
        await reportSession(rootSession);
        await report(rootSession, "idle");
        await replay(client, rootSession);
        return;
      }
      const root = ownedByPane(sessionID);
      if (root) await reportSession(rootSession);
      if (root && rootSession && replayedFor !== rootSession) await replay(client, rootSession);
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
          if (root) queueUpdates([entry(`turn-${Date.now()}`, { TurnEnd: { millis: null } })]);
          break;
        case "session.deleted":
          await report(sessionID ?? rootSession, "idle");
          children.delete(sessionID);
          lastSent.delete(sessionID);
          break;
        case "permission.asked":
        case "permission.updated":
          await report(sessionID ?? rootSession, "waiting", noteFrom(props));
          // Only a plugin that can take the answer asks the question.
          if (client && props.id) {
            permissions.set(props.id, props);
            queueUpdates([permissionQuestion(props)]);
          }
          break;
        case "permission.replied": {
          await report(sessionID ?? rootSession, "working");
          const asked = permissions.get(props.permissionID);
          if (asked) queueUpdates([permissionQuestion(asked, String(props.response ?? "answered"))]);
          break;
        }
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
