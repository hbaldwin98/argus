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

// Several of these fire per turn, and the row only changes on a change.
let lastSent;
let lastSessionSent;

async function report(status, note = "") {
  if (!BASE || !TOKEN) return;
  const key = `${status} ${note}`;
  if (key === lastSent) return;
  lastSent = key;
  try {
    await fetch(`${BASE}/status/${status}`, {
      method: "POST",
      headers: { authorization: `Bearer ${TOKEN}` },
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
      headers: { authorization: `Bearer ${TOKEN}` },
      body: sessionID,
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Session identity is best-effort for the same reason as status.
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

export const ArgusStatus = async () => {
  if (!BASE || !TOKEN) return {};

  return {
    // The instant signal: this fires when the prompt is submitted, before
    // any model call, which is what turns the row over as you hit enter.
    "chat.message": async ({ sessionID }) => {
      if (ownedByPane(sessionID)) {
        await reportSession(rootSession);
        await report("working");
      }
    },

    // Where the agent is told it can name its own row. opencode has no
    // session-start hook whose output reaches the model, and the system
    // prompt is the one place a standing fact like this survives
    // compaction.
    "experimental.chat.system.transform": async (_input, output) => {
      if (INSTRUCTIONS) output.system.push(INSTRUCTIONS);
    },

    event: async ({ event }) => {
      const type = event?.type;
      const props = event?.properties ?? {};
      const sessionID = props.sessionID;

      if (props.info?.id && props.info?.parentID) {
        children.add(props.info.id);
      }
      // OpenCode keeps the same process and plugin when the user starts a
      // new conversation. A newly created root replaces the root this pane
      // is showing; otherwise every event from the new conversation would
      // be mistaken for another session and the previous status would stick.
      if (type === "session.created" && props.info?.id && !props.info.parentID) {
        rootSession = props.info.id;
        await reportSession(rootSession);
        await report("idle");
        return;
      }
      if (!ownedByPane(sessionID)) return;
      await reportSession(rootSession);

      switch (type) {
        // The authoritative one. opencode drops the session to `idle` both
        // when a turn ends and when the user aborts it, which is what keeps
        // a manually stopped agent from sitting at "working" forever.
        case "session.status": {
          const kind = props.status?.type ?? props.status;
          if (kind === "idle") await report("idle");
          else if (kind === "busy" || kind === "retry") await report("working");
          break;
        }
        case "session.idle":
          await report("idle");
          break;
        case "permission.asked":
        case "permission.updated":
          await report("waiting", noteFrom(props));
          break;
        case "permission.replied":
        case "session.compacted":
          await report("working");
          break;
        case "session.error":
          await report("failed", noteFrom(props));
          break;
        default:
          break;
      }
    },
  };
};
