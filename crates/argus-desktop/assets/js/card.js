// A card view block: blocks in a bordered box with a heading, a chip and
// buttons of its own.
import { actionButton } from './actions.js';
import { blocks } from './blocks.js';
import { chip, toneClass } from './chip.js';
import { el } from './dom.js';
/** A non-neutral tone only tints the border: the card stays readable in
 *  every theme, and the tone still says what the card is about. */
export function card(block, ctx) {
    const header = block.title != null || block.chip
        ? el('div', { class: 'vcard-head' }, block.title != null ? el('span', { class: 'vcard-title' }, block.title) : null, block.chip ? chip(block.chip) : null)
        : null;
    const footer = block.actions.length
        ? el('div', { class: 'vcard-foot' }, block.actions.map((a) => actionButton(a, ctx)))
        : null;
    return el('div', { class: `vcard ${toneClass(block.tone)}${block.tone === 'neutral' ? '' : ' tinted'}` }, header, blocks(block.blocks, ctx), footer);
}
