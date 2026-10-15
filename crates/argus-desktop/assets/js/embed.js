// A saved view drawn inside another view: its blocks in a bordered frame
// under its title, with a link to open it. The embedding block carries only
// the id; the body is the one this window holds for it, so the frame
// follows the view when it is saved again. A view that loops back on
// itself, one nested past the depth cap and one not saved (yet) are a line
// saying so rather than a frame.
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { el } from './dom.js';
import { savedCtx } from './saved_ctx.js';
import { EMBED_WORDS, embedVerdict } from './saved_logic.js';
import { openSaved, savedView } from './savedlink.js';
export function embed(id, ctx) {
    const chain = ctx.chain ?? [];
    const depth = ctx.depth ?? 0;
    const verdict = embedVerdict(id, chain, depth, (held) => savedView(held) !== undefined);
    const target = savedView(id);
    if (verdict !== 'draw' || !target) {
        const words = verdict === 'draw' ? EMBED_WORDS.missing : EMBED_WORDS[verdict];
        return el('div', { class: 'vembed vembed-gone vmuted' }, `${words}: ${id}`);
    }
    const inner = savedCtx(target, ctx.saved, [...chain, id], depth + 1, ctx.path);
    return el('section', { class: 'vembed', 'aria-label': target.view.title }, el('header', { class: 'vembed-head' }, el('span', { class: 'vembed-title' }, target.view.title), target.view.status && chip(target.view.status), target.stale && chip({ label: 'stale', tone: 'warn' }), el('button', {
        class: 'vref vsaved',
        type: 'button',
        title: 'Open this view in its own column',
        onclick: () => openSaved(id, ctx),
    }, 'Open')), blocks(target.view.blocks, inner));
}
