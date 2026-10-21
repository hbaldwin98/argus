// The command palette: one search over the keymap's commands and the
// panes, features and checkouts Rust says it can go to, grouped, each
// result with its key or its status beside it. `>` narrows the search to
// commands and `#` to features.
//
// What it lists comes from Rust while it is open (`src/keys/palette.rs`),
// and choosing one is a call: the palette runs nothing itself.
import { $, el, icon, sentence } from '../kit/dom.js';
import { extensionResults } from '../extensions/views.js';
import { actionFor } from './keymap.js';
import { keycaps } from './chords.js';
import { state } from '../state.js';
import { call } from '../connection/tauri.js';
const GROUPS = ['Commands', 'Panes', 'Features', 'Checkouts'];
/** Rows a group shows before the search narrows it. */
const SHOWN = 6;
let query = '';
let at = 0;
/** The highlighted result's `key`: a list Rust rebuilds while the palette
 *  is open can move it, and Enter runs what is highlighted. */
let held = null;
let results = [];
/** What had the keys before the palette, to give them back to. */
let before = null;
export function paletteOpen() {
    return $('over').querySelector('.palette') !== null;
}
export function openPalette() {
    if (paletteOpen())
        return;
    before = document.activeElement;
    query = '';
    top();
    const field = el('input', {
        class: 'palette-field',
        placeholder: 'Search, or > for commands, # for features',
        'aria-label': 'Search actions, panes, features and checkouts',
        autocomplete: 'off',
        spellcheck: 'false',
    });
    field.addEventListener('input', () => {
        query = field.value;
        top();
        drawResults();
    });
    field.addEventListener('keydown', (e) => {
        const action = actionFor(['palette'], e);
        if (!action)
            return;
        e.preventDefault();
        if (action === 'next' || action === 'previous') {
            const count = results.length;
            if (count)
                highlight((at + (action === 'next' ? 1 : count - 1)) % count);
            drawResults();
        }
        else if (action === 'choose') {
            const chosen = results[at];
            if (chosen)
                choose(chosen);
        }
        else if (action === 'closeLayer') {
            closePalette();
        }
    });
    $('over').replaceChildren(el('div', {
        class: 'scrim top',
        onmousedown: (e) => {
            if (e.target === e.currentTarget)
                closePalette();
        },
    }, el('section', { class: 'dialog palette', role: 'dialog', 'aria-modal': 'true', 'aria-label': 'Palette' }, el('div', { class: 'palette-head' }, icon('search'), field, el('span', { class: 'kbd' }, 'Esc')), el('div', { class: 'palette-results', id: 'palette-results', role: 'listbox' }), el('div', { class: 'dfoot palette-foot' }, hint(['↑', '↓'], 'move'), hint(['↵'], 'open'), hint(['Tab'], 'next group'), el('span', { class: 'grow' }), hint(['>'], 'commands'), hint(['#'], 'features')))));
    call('palette', { open: true });
    drawResults();
    field.focus();
}
/** A footer hint: its keycaps, then what they do. */
function hint(keys, what) {
    return el('span', { class: 'dwhy' }, keys.map((k) => el('span', { class: 'kbd' }, k)), what);
}
/** Puts the palette away, giving the keys back to what had them. */
export function closePalette() {
    if (!paletteOpen())
        return;
    $('over').replaceChildren();
    call('palette', { open: false });
    if (before instanceof HTMLElement)
        before.focus();
    before = null;
}
/** What Rust sent changed while the palette is open. */
export function drawPalette() {
    if (paletteOpen())
        drawResults();
}
/** Highlights whatever ranks first once the results are drawn. */
function top() {
    at = 0;
    held = null;
}
/** Highlights the `i`th result. */
function highlight(i) {
    at = i;
    held = results[i]?.key ?? null;
}
function choose(result) {
    closePalette();
    result.choose();
}
const COMMAND_ICON = {
    workspace: 'workspace',
    feature: 'list',
    newFeature: 'list',
    checkouts: 'branch',
    branches: 'branch',
    code: 'code',
    openCode: 'code',
    console: 'console',
    toggleConsole: 'console',
    palette: 'search',
    search: 'search',
    shortcuts: 'keyboard',
    settings: 'gear',
    compose: 'plus',
    startShell: 'terminal',
    startAgent: 'plus',
};
function commandResult(command) {
    const place = command.place && command.place !== 'anywhere'
        ? state.keymap.find((g) => g.place === command.place)?.title
        : null;
    return {
        group: 'Commands',
        key: `command ${command.action}`,
        text: sentence(command.label),
        icon: COMMAND_ICON[command.action] ?? 'right',
        aside: command.chords.length
            ? el('span', { class: 'aside' }, place && el('span', { class: 'fg-faint' }, place), keycaps(command.chords))
            : null,
        choose: () => call('command', { action: command.action }),
    };
}
function word(text, cls) {
    return el('span', { class: cls }, text);
}
function everything() {
    const palette = state.palette;
    return [
        ...state.commands.map(commandResult),
        ...extensionResults(),
        ...(palette?.panes ?? []).map((pane) => ({
            group: 'Panes',
            key: `pane ${pane.id}`,
            text: pane.title,
            icon: 'terminal',
            detail: pane.place,
            aside: word(pane.status, `st ${pane.status}`),
            choose: () => {
                call('select', { pane: pane.id });
                $('input').focus();
            },
        })),
        ...(palette?.features ?? []).map((feature) => ({
            group: 'Features',
            key: `feature ${feature.slug}`,
            text: `#${feature.number} ${feature.title}`,
            icon: 'list',
            aside: word(feature.standing, 'st fg-faint'),
            choose: () => call('go_to_feature', { slug: feature.slug }),
        })),
        ...(palette?.checkouts ?? []).map((checkout) => ({
            group: 'Checkouts',
            key: `checkout ${checkout.id}`,
            text: `${checkout.repository} / ${checkout.name}`,
            icon: 'branch',
            detail: checkout.branch || undefined,
            aside: checkout.git && !checkout.clean ? word(checkout.git, 'git') : null,
            choose: () => call('go_to_checkout', { checkout: checkout.id }),
        })),
    ];
}
/** Where `needle`'s characters fall in order in `hay`, ignoring case, and
 *  how well: runs and word starts score, a late start costs. */
export function fuzzy(needle, hay) {
    const n = needle.toLowerCase().replace(/\s+/g, '');
    const h = hay.toLowerCase();
    if (!n)
        return { score: 0, hits: [] };
    const hits = [];
    let score = 0;
    let from = 0;
    for (const ch of n) {
        const i = h.indexOf(ch, from);
        if (i < 0)
            return null;
        const last = hits[hits.length - 1];
        if (last !== undefined && i === last + 1)
            score += 5;
        if (i === 0 || /[\s/#\-_.]/.test(h[i - 1] ?? ''))
            score += 3;
        hits.push(i);
        from = i + 1;
    }
    return { score: score - (hits[0] ?? 0) * 0.1 - h.length * 0.01, hits };
}
/** `text` with the characters at `hits` marked. */
function marked(text, hits) {
    const out = [];
    let plain = '';
    [...text].forEach((ch, i) => {
        if (hits.includes(i)) {
            if (plain)
                out.push(plain);
            plain = '';
            out.push(el('mark', null, ch));
        }
        else
            plain += ch;
    });
    if (plain)
        out.push(plain);
    return out;
}
function drawResults() {
    const list = document.getElementById('palette-results');
    if (!list)
        return;
    let needle = query.trim();
    let only = null;
    if (needle.startsWith('>'))
        only = 'Commands';
    else if (needle.startsWith('#'))
        only = 'Features';
    if (only)
        needle = needle.slice(1).trim();
    // Each group keeps its own order until there is something to rank by.
    const scored = everything()
        .filter((r) => !only || r.group === only)
        .map((r) => ({ r, m: fuzzy(needle, only === 'Features' ? r.text.slice(1) : r.text) }))
        .filter((x) => x.m !== null);
    const rows = [];
    const buttons = [];
    results = [];
    for (const group of GROUPS) {
        let members = scored.filter((x) => x.r.group === group);
        if (needle)
            members.sort((a, b) => b.m.score - a.m.score);
        else if (!only)
            members = members.slice(0, SHOWN);
        if (!members.length)
            continue;
        rows.push(el('div', { class: 'mlabel' }, group));
        for (const { r, m } of members) {
            const i = results.length;
            results.push(r);
            // A feature's hits were counted past its `#`.
            const hits = only === 'Features' ? m.hits.map((h) => h + 1) : m.hits;
            const button = el('button', {
                class: 'mi palette-row',
                role: 'option',
                'aria-selected': 'false',
                onmousedown: (e) => e.preventDefault(),
                onclick: () => choose(r),
                onmousemove: () => {
                    if (at === i)
                        return;
                    highlight(i);
                    drawResults();
                },
            }, el('span', { class: 'pic', 'aria-hidden': 'true' }, icon(r.icon ?? 'puzzle')), el('span', { class: 'grow ttl' }, marked(r.text, hits)), r.detail && el('span', { class: 'meta mono' }, r.detail), r.aside);
            buttons.push(button);
            rows.push(button);
        }
    }
    if (!results.length) {
        const loading = only === 'Features' && !state.palette?.featuresLoaded;
        rows.push(el('div', { class: 'palette-empty meta' }, loading ? 'Reading the features…' : 'No match.'));
    }
    const kept = results.findIndex((r) => r.key === held);
    highlight(kept >= 0 ? kept : Math.min(at, Math.max(0, results.length - 1)));
    const active = buttons[at];
    active?.classList.add('on');
    // What Enter does, said on the row it will do it to.
    active?.insertBefore(el('span', { class: 'open-hint' }, 'Open ', el('span', { class: 'kbd' }, '↵')), results[at]?.aside ?? null);
    active?.setAttribute('aria-selected', 'true');
    list.replaceChildren(...rows);
    list.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
}
