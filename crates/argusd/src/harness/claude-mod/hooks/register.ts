// Argus's way into a Claude Code session it runs.
//
// Argus starts a Claude pane with this folder named in
// CLAUDE_CODE_PLUGIN_DIRS, so Claude Code loads it inside the session (a
// mod: Claude Code 2.1.287 or later). Two things go through it rather than
// around it:
//
// - The pane's inbox. What a person says to the agent from another surface
//   arrives here and goes in through Claude Code's own prompt, not typed
//   into its terminal, where a dialog could take it as its answer. The pane
//   reads as live once the inbox is open, and only then does Argus stop
//   typing, so a Claude Code without mods is typed into as before.
// - The reply as it streams. Claude Code writes a reply to its transcript
//   only as each block ends; the pieces seen here are posted as the pane's
//   draft, without Argus standing in the path of the API traffic.
//
// Everything is best-effort: a daemon that has gone makes this quiet, never
// an error in the session.

import type { EngineInterface, Register, TurnStepChunk } from 'claude-code'

type InboxItem =
  | 'Interrupt'
  | { Message: { text: string; steer: boolean } }
  | { Answer: { question: string; choice: string } }

type Draft = { Start: { thinking: boolean } } | { More: { text: string } } | 'Done'

/** The least time between two posts of a draft; what streams between is sent together. */
const DRAFT_EVERY = 100

/** How long a closed inbox waits before it is opened again, doubling to the most. */
const REOPEN_FIRST = 500
const REOPEN_MOST = 10_000

// The main loop's running turn, which an interrupt ends and a steer joins.
// A subagent's run raises no turn.start. Kept in the module: a reload loses
// it, and the next turn sets it again.
let turn: string | undefined
let relaying = false
// The reply being drafted, which its turn's end closes: an interrupted
// step's own hook is abandoned before it can.
let drafting: DraftState | undefined
// Draft changes waiting to be posted, and what wakes the poster for them.
let outgoing: Draft[] = []
let wake: (() => void) | undefined

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const started = await next(e)
    if (!relaying) {
      relaying = true
      void relay($)
      void postDrafts($)
    }
    return started
  })

  on('turn.start', ($, e, next) => {
    turn = e.turnId
    return next(e)
  })

  on('turn.complete', ($, e, next) => {
    if (e.agentId) return next(e)
    if (e.turnId === turn) turn = undefined
    if (drafting) {
      close(drafting)
      send(drafting)
      drafting = undefined
    }
    return next(e)
  })

  // Read as it passes and passed on untouched; a subagent's reply is not
  // the pane's.
  on('turn.step', async function* ($, e, next) {
    const stream = next(e)
    if (e.agentId) return yield* stream
    const draft: DraftState = { block: undefined, pending: [], sentAt: 0 }
    drafting = draft
    for await (const chunk of stream) {
      see(draft, chunk)
      yield chunk
    }
    close(draft)
    send(draft)
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
    if (item === 'Interrupt') {
      if (turn) await $.turn.abort({ turnId: turn })
      return
    }
    if ('Message' in item) {
      const { text, steer } = item.Message
      if (steer && turn) {
        // Read at the turn's next step. The person at the terminal sees no
        // row for it, so they are told.
        await $.session.append({ message: { type: 'user', content: [{ type: 'text', text }] } })
        $.ui.toast(`Argus: ${text}`)
        return
      }
      // Queued until the session is idle, as typing it would be.
      void $.prompt.submit({ text, asUser: true })
    }
    // An answer is for a question this plugin posed, and it poses none:
    // Claude Code's own dialogs are answered on its screen.
  } catch {
    // The agent is where it is; the phone sees what happens next.
  }
}

/**
 * One reply's draft as it streams: a block's start, its text gathered for a
 * tenth of a second at a time, and its end.
 */
type DraftState = {
  block: number | undefined
  pending: Draft[]
  sentAt: number
}

function see(draft: DraftState, chunk: TurnStepChunk) {
  if (chunk.kind === 'text' || chunk.kind === 'thinking') {
    if (chunk.text === '' && chunk.index === draft.block) return
    if (chunk.index !== draft.block) {
      close(draft)
      draft.block = chunk.index
      draft.pending.push({ Start: { thinking: chunk.kind === 'thinking' } })
    }
    const last = draft.pending[draft.pending.length - 1]
    if (last && typeof last === 'object' && 'More' in last) last.More.text += chunk.text
    else if (chunk.text !== '') draft.pending.push({ More: { text: chunk.text } })
  } else if (chunk.kind !== 'engine') {
    // A tool call, or the reply's end: whatever was being written is done.
    close(draft)
  }
  const settled = draft.pending.some(d => d === 'Done' || (typeof d === 'object' && 'Start' in d))
  if (settled || Date.now() - draft.sentAt >= DRAFT_EVERY) send(draft)
}

function close(draft: DraftState) {
  if (draft.block === undefined) return
  draft.block = undefined
  draft.pending.push('Done')
}

function send(draft: DraftState) {
  if (draft.pending.length === 0) return
  outgoing.push(...draft.pending)
  draft.pending = []
  draft.sentAt = Date.now()
  wake?.()
}

// Posts the draft for the session's life, one post at a time so the daemon
// takes them in the order they were made. It holds the session's own `$`:
// a step's hook is abandoned when its turn is interrupted, and a post made
// through it would never land.
async function postDrafts($: EngineInterface) {
  const [base, token] = await Promise.all([$.env.get('ARGUS_HOOK_URL'), $.env.get('ARGUS_HOOK_TOKEN')])
  if (!base || !token) return
  for (;;) {
    while (outgoing.length === 0) await new Promise<void>(resolve => (wake = resolve))
    const changes = outgoing
    outgoing = []
    try {
      await $.http.fetch(`${base}/draft`, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token}`,
          'content-type': 'application/json',
          'x-argus-session': await $.session.id(),
        },
        body: JSON.stringify(changes),
      })
    } catch {
      // A draft that does not land is only a draft.
    }
  }
}
