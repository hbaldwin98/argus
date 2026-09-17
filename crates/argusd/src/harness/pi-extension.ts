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

let lastTelemetry;

async function reportTelemetry(ctx, fields) {
  const id = await reportSession(ctx);
  const body = JSON.stringify(fields);
  if (body === lastTelemetry) return;
  lastTelemetry = body;
  await post("telemetry", body, id);
}

// The model, the context the latest request used, and the session's totals.
// Every field is read defensively: pi's message and model shapes differ
// between providers and releases, and a missing number is simply not sent.
function messageTelemetry(ctx, message) {
  const u = message?.usage ?? {};
  const fields = {};
  const model = message?.model ?? ctx.model?.id;
  if (model) fields.model = String(model);
  const context =
    u.totalTokens ?? (u.input ?? 0) + (u.output ?? 0) + (u.cacheRead ?? 0) + (u.cacheWrite ?? 0);
  if (context > 0) fields.context_tokens = context;
  const window = ctx.model?.contextWindow;
  if (typeof window === "number" && window > 0) fields.context_window = window;

  let input = 0, output = 0, cost = 0, priced = false;
  for (const entry of ctx.sessionManager?.getBranch?.() ?? []) {
    const m = entry?.message ?? entry;
    if (m?.role !== "assistant" || !m.usage) continue;
    input += (m.usage.input ?? 0) + (m.usage.cacheRead ?? 0) + (m.usage.cacheWrite ?? 0);
    output += m.usage.output ?? 0;
    if (typeof m.usage.cost?.total === "number") {
      cost += m.usage.cost.total;
      priced = true;
    }
  }
  if (input > 0) fields.input_tokens = input;
  if (output > 0) fields.output_tokens = output;
  if (priced) fields.cost_usd = cost;
  return fields;
}

export default function Argus(pi) {
  pi.on("session_start", async (_event, ctx) => {
    await enqueue(async () => {
      await reportSession(ctx);
      await report(ctx, "idle");
    });
  });

  pi.on("input", async (_event, ctx) => {
    await enqueue(() => report(ctx, "working"));
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

  pi.on("message_end", async (event, ctx) => {
    if (event?.message?.role !== "assistant") return;
    await enqueue(() => reportTelemetry(ctx, messageTelemetry(ctx, event.message)));
  });

  pi.on("tool_execution_start", async (event, ctx) => {
    await enqueue(() => reportTelemetry(ctx, { tool: String(event?.toolName ?? "tool") }));
  });

  pi.on("tool_execution_end", async (_event, ctx) => {
    await enqueue(() => reportTelemetry(ctx, { tool: "" }));
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
