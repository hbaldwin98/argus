// argus:managed-plugin
//
// Written by Argus before it starts a pi pane in this checkout and removed
// when the last agent pane there closes. Per-pane routing stays in the
// process environment, so one project-local extension serves every pane.

const BASE = process.env.ARGUS_HOOK_URL;
const TOKEN = process.env.ARGUS_HOOK_TOKEN;
const INSTRUCTIONS = process.env.ARGUS_INSTRUCTIONS;

let lastSession;
let lastStatus;
let delivery = Promise.resolve();

function enqueue(task) {
  delivery = delivery.then(task, task);
  return delivery;
}

function sessionId(ctx) {
  return ctx.sessionManager.getSessionId();
}

async function post(path, body = "", session) {
  if (!BASE || !TOKEN) return;
  const headers = { authorization: `Bearer ${TOKEN}` };
  if (session) headers["X-Argus-Session"] = session;
  try {
    await fetch(`${BASE}/${path}`, {
      method: "POST",
      headers,
      body,
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Status reporting must never interrupt the agent. The daemon that wrote
    // this file may have exited, leaving its loopback port intentionally dead.
  }
}

async function reportSession(ctx) {
  const id = sessionId(ctx);
  if (!id || id === lastSession) return id;
  lastSession = id;
  lastStatus = undefined;
  await post("session", id, id);
  return id;
}

function summary(text) {
  return String(text ?? "").split("\n")[0].trim().slice(0, 200);
}

async function report(ctx, status, note = "") {
  const id = await reportSession(ctx);
  if (!id) return;
  note = summary(note);
  const value = `${id}\n${status}\n${note}`;
  if (value === lastStatus) return;
  lastStatus = value;
  await post(`status/${status}`, note, id);
}

async function title(ctx, text) {
  const firstLine = summary(text);
  if (!firstLine) return;
  const id = await reportSession(ctx);
  if (id) await post("title", firstLine, id);
}

export default function Argus(pi) {
  pi.on("session_start", async (_event, ctx) => {
    await enqueue(async () => {
      await reportSession(ctx);
      await report(ctx, "idle");
    });
  });

  // `input` sees the user's text before skill and prompt-template expansion,
  // which makes it a useful pane title instead of naming the row after the
  // expanded instructions.
  pi.on("input", async (event, ctx) => {
    await enqueue(async () => {
      await report(ctx, "working");
      await title(ctx, event.text);
    });
    return { action: "continue" };
  });

  pi.on("before_agent_start", async (event, ctx) => {
    await enqueue(() => report(ctx, "working"));
    if (!INSTRUCTIONS || event.systemPrompt.includes(INSTRUCTIONS)) return;
    return { systemPrompt: `${event.systemPrompt}\n\n${INSTRUCTIONS}` };
  });

  // A low-level run can restart after retry or compaction without another
  // input event. It is active work again even if the prior attempt failed.
  pi.on("agent_start", async (_event, ctx) => {
    await enqueue(() => report(ctx, "working"));
  });

  pi.on("agent_end", async (event, ctx) => {
    const failure = [...event.messages].reverse().find(
      (message) => message.role === "assistant" && message.stopReason === "error",
    );
    if (failure) {
      await enqueue(() => report(ctx, "failed", failure.errorMessage ?? "model request failed"));
    }
  });

  pi.on("agent_settled", async (_event, ctx) => {
    await enqueue(() => report(ctx, "idle"));
  });

  // Prompt notifications are not awaited by pi and can overlap. Keep every
  // POST on one queue so a late waiting request cannot follow its end event.
  pi.on("ui_prompt_start", async (event, ctx) => {
    await enqueue(() => report(ctx, "waiting", event.title ?? "waiting for input"));
  });

  pi.on("ui_prompt_end", async (_event, ctx) => {
    await enqueue(() => report(ctx, ctx.isIdle() ? "idle" : "working"));
  });
}
