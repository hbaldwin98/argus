// One Value as a field, a table cell or a list item shows it. Every string
// is an agent's, read from a ticket, so it goes in as text and nothing else.
import { chip, toneClass } from './chip.js';
import { el } from './dom.js';
import { progressValue } from './progress.js';
import { refValue } from './refs.js';
import { savedLink } from './savedlink.js';
import { paneOf } from './viewctx.js';
import { viewLink } from './viewlink.js';
const SHOWN_PEOPLE = 3;
// Avatar tints are drawn from the tones so they follow the theme.
const TINTS = ['active', 'info', 'good', 'warn', 'bad'];
function initials(name) {
    return name
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 2)
        .map((word) => Array.from(word)[0].toUpperCase())
        .join('');
}
function tint(name) {
    let hash = 0;
    for (const ch of name)
        hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
    return TINTS[hash % TINTS.length];
}
function avatar(name) {
    return el('span', { class: `vav ${toneClass(tint(name))}`, title: name }, initials(name));
}
/** A person: initials in a circle, and the name. */
export function person(name) {
    return el('span', { class: 'vperson' }, avatar(name), el('span', null, name));
}
function people(names) {
    const shown = names.slice(0, SHOWN_PEOPLE);
    const rest = names.length - shown.length;
    return el('span', { class: 'vperson' }, el('span', { class: 'vavs' }, shown.map(avatar)), el('span', null, shown.join(', ')), rest > 0 && el('span', { class: 'vmuted' }, `+${rest}`));
}
function parsed(at) {
    if (!/^\d{4}-\d{2}-\d{2}[T ]/.test(at))
        return null;
    const date = new Date(at);
    return Number.isNaN(date.getTime()) ? null : date;
}
/** How long ago an RFC 3339 time was, `4 min ago`; the text as given when it is not one. */
export function ago(at) {
    const date = parsed(at);
    if (!date)
        return at;
    const now = new Date();
    const minutes = Math.floor((now.getTime() - date.getTime()) / 60_000);
    // A time ahead of this clock is skew, not the future.
    if (minutes < 1)
        return 'just now';
    if (minutes < 60)
        return `${minutes} min ago`;
    if (minutes < 24 * 60)
        return `${Math.floor(minutes / 60)} h ago`;
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString())
        return 'yesterday';
    const sameYear = date.getFullYear() === now.getFullYear();
    return date.toLocaleDateString('en-US', {
        month: 'short',
        day: 'numeric',
        year: sameYear ? undefined : 'numeric',
    });
}
function shortDate(raw) {
    // A bare date is a calendar day, not an instant: read it in the local zone.
    const bare = /^(\d{4})-(\d{2})-(\d{2})$/.exec(raw);
    const date = bare
        ? new Date(Number(bare[1]), Number(bare[2]) - 1, Number(bare[3]))
        : new Date(raw);
    if (Number.isNaN(date.getTime()))
        return raw;
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}
/** One value, as a field, a cell or a list item shows it. `ctx` is the
 *  agent view it is in; an extension's table has none. */
export function value(v, ctx = null) {
    switch (v.kind) {
        case 'text':
            return el('span', null, v.text);
        case 'number':
            return el('span', { class: 'vnum' }, v.number.toLocaleString(), v.unit && el('span', { class: 'vmuted' }, ` ${v.unit}`));
        case 'bool':
            return el('span', { class: v.value ? 'vbool' : 'vbool vmuted' }, v.value ? '✓ Yes' : '✗ No');
        case 'date':
            return el('span', null, shortDate(v.date));
        case 'date_time': {
            const date = parsed(v.at);
            return el('span', { title: date ? date.toLocaleString() : null }, ago(v.at));
        }
        case 'person':
            return person(v.name);
        case 'people':
            return people(v.names);
        case 'chip':
            return chip(v.chip);
        case 'chips':
            return el('span', { class: 'vchips' }, v.chips.map(chip));
        case 'link':
            return viewLink(v.url, v.label);
        case 'ticket': {
            const key = el('span', { class: 'vkey' }, v.key);
            return el('span', { class: 'vticket' }, v.url ? viewLink(v.url, key) : key, v.title && el('span', { class: 'vmuted' }, v.title), v.status && chip(v.status));
        }
        case 'progress':
            return progressValue(v);
        case 'duration':
        case 'code':
        case 'path':
        case 'commit':
        case 'feature':
        case 'pane':
            return refValue(v, paneOf(ctx));
        case 'view':
            return savedLink(v.id, v.label, ctx);
        case 'empty':
            return el('span', { class: 'vempty' }, '—');
        case 'unknown':
            return el('span', { class: 'vmuted', title: 'needs a newer Argus' }, '—');
    }
}
