// A status pill. An agent picks a tone, a meaning, and the page owns the hue,
// so a view looks the same in every theme whoever wrote it.
import { el } from './dom.js';
/** The class that colours something by its tone: `t-good`. */
export function toneClass(tone) {
    return `t-${tone}`;
}
export function chip(c) {
    return el('span', { class: `vchip ${toneClass(c.tone)}` }, c.label);
}
