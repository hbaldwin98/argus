// Argus's way into a Claude Code session it runs.
//
// Argus starts a Claude pane with this folder named in
// CLAUDE_CODE_PLUGIN_DIRS, so Claude Code loads it inside the session (a
// mod: Claude Code 2.1.287 or later). Two things go through it rather than
// around it:
//
// - The pane's inbox. What Argus has to say to the agent — a review comment
//   left for it — arrives here and goes in through Claude Code's own prompt,
//   not typed into its terminal, where a dialog could take it as its answer.
//   The pane reads as live once the inbox is open, and only then does Argus
//   stop typing, so a Claude Code without mods is typed into as before.
// - What the session has cost and how full its context is, which the
//   settings hooks cannot read: Claude Code's transcript records neither the
//   window nor the cost.
//
// Everything is best-effort: a daemon that has gone makes this quiet, never
// an error in the session.

import type { EngineInterface, Register } from 'claude-code'

type InboxItem = { Message: { text: string } }

/** How long a closed inbox waits before it is opened again, doubling to the most. */
const REOPEN_FIRST = 500
const REOPEN_MOST = 10_000

// Whether the inbox relay and the poster are running, whether the session's
// usage is due to be reported, and what wakes the poster for it.
let relaying = false
let usageDue = false
let wake: (() => void) | undefined

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const started = await next(e)
    if (!relaying) {
      relaying = true
      void relay($)
      void postToPane($)
    }
    return started
  })

  on('turn.complete', ($, e, next) => {
    if (!e.agentId) reportUsage()
    return next(e)
  })

  // Passed on untouched, and the usage reported once each of the pane's own
  // steps ends, so a long turn's context shows as it grows; a subagent's
  // step is not the pane's.
  on('turn.step', async function* ($, e, next) {
    const stream = next(e)
    if (e.agentId) return yield* stream
    for await (const chunk of stream) yield chunk
    reportUsage()
    return await stream.result
  })
}

// The inbox is a stream held open for the session's life. A mod has no
// streaming HTTP, so the helper holds it and prints one item a line. It
// closes when the daemon goes or refuses it — the session it is opened as
// is not the pane's — and is opened again, as whatever session this is by
// then (a /clear starts another).
async function relay($: EngineInterface) {
  const helper = await $.env.get('ARGUS_HOOK')
  if (!helper) return
  let wait = REOPEN_FIRST
  for (;;) {
    try {
      const session = await $.session.id()
      let pending = ''
      for await (const { stream, text } of $.process.spawn({ argv: [helper, 'inbox', session] })) {
        if (stream !== 'stdout') continue
        wait = REOPEN_FIRST
        pending += text
        let end
        while ((end = pending.indexOf('\n')) >= 0) {
          const line = pending.slice(0, end).trim()
          pending = pending.slice(end + 1)
          if (line) await act($, JSON.parse(line) as InboxItem)
        }
      }
    } catch {
      // Nobody listening yet, or any more.
    }
    // Outside the try: once the module unloads this rejects, which is what
    // ends the loop.
    await $.clock.sleep(wait)
    wait = Math.min(wait * 2, REOPEN_MOST)
  }
}

async function act($: EngineInterface, item: InboxItem) {
  try {
    // Queued until the session is idle, as typing it would be.
    if ('Message' in item) void $.prompt.submit({ text: item.Message.text, asUser: true })
  } catch {
    // The agent is where it is; what happens next shows in its pane.
  }
}

function reportUsage() {
  usageDue = true
  wake?.()
}

// Posts for the session's life, one post at a time. It holds the session's
// own `$`: a step's hook is abandoned when its turn is interrupted, and a
// post made through it would never land.
async function postToPane($: EngineInterface) {
  const [base, token] = await Promise.all([$.env.get('ARGUS_HOOK_URL'), $.env.get('ARGUS_HOOK_TOKEN')])
  if (!base || !token) return
  for (;;) {
    while (!usageDue) await new Promise<void>(resolve => (wake = resolve))
    try {
      const headers = {
        authorization: `Bearer ${token}`,
        'content-type': 'application/json',
        'x-argus-session': await $.session.id(),
      }
      usageDue = false
      const { context, cost } = await $.session.usage()
      const report = { context_tokens: context.tokens, context_window: context.window, cost_usd: cost?.usd }
      await $.http.fetch(`${base}/telemetry`, { method: 'POST', headers, body: JSON.stringify(report) })
    } catch {
      // A figure that does not land is only that.
    }
  }
}
