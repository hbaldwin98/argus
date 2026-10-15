// The side panel the views agents showed open in, beside whatever stage is
// up so the rail and the stage stay in sight, and the tab at the window's
// edge that brings it back. Rust holds the views and which one is open;
// the panel draws them and says what was pressed.
import { blocks } from './blocks.js';
import { chip } from './chip.js';
import { $, el, icon } from './dom.js';
import { state } from './state.js';
import { call } from './tauri.js';
import { ago } from './values.js';
import { viewLink } from './viewlink.js';
let shown = { views: [], open: null };
export function drawViews(page) {
    shown = page;
    const panel = $('views-inner');
    const launcher = $('views-tab');
    const open = page.open && page.views.find((v) => v.pane === page.open?.pane && v.view.id === page.open?.id);
    $('views').hidden = !open;
    launcher.hidden = Boolean(open) || page.views.length === 0;
    if (!open) {
        panel.replaceChildren();
        launcher.replaceChildren(el('span', { class: 'vdot', 'aria-hidden': 'true' }), `${page.views.length} ${page.views.length === 1 ? 'view' : 'views'}`);
        return;
    }
    // Keep the reader's place when the view comes again or another is added.
    const scroller = panel.querySelector('.vpanel-body');
    const top = scroller?.getAttribute('data-for') === key(open) ? scroller.scrollTop : 0;
    panel.replaceChildren(tabs(page.views, open), header(open), body(open));
    const fresh = panel.querySelector('.vpanel-body');
    if (fresh)
        fresh.scrollTop = top;
}
function key(v) {
    return `${v.pane}/${v.view.id}`;
}
function tabs(views, open) {
    return el('div', { class: 'vpanel-tabs', role: 'tablist', 'aria-label': 'Views' }, views.map((v) => el('button', {
        class: `vpanel-tab${v === open ? ' on' : ''}`,
        role: 'tab',
        type: 'button',
        'aria-selected': String(v === open),
        title: v.from ? `${v.view.title} · from ${v.from}` : v.view.title,
        onclick: () => call('open_view', { pane: v.pane, id: v.view.id }),
    }, v.refreshing && el('span', { class: 'vspin', 'aria-label': 'refreshing' }), el('span', { class: 'ttl' }, v.view.title))), el('span', { class: 'grow' }), el('button', {
        class: 'icon',
        type: 'button',
        'aria-label': 'Hide the views',
        title: 'Hide',
        onclick: () => call('hide_views', {}),
    }, icon('close')));
}
function header(v) {
    const at = new Date(Number(v.at) * 1000).toISOString();
    const subject = v.subject;
    return el('div', { class: 'vpanel-head' }, el('div', { class: 'vpanel-title' }, el('span', { class: 'type-heading' }, v.view.title), v.view.status && chip(v.view.status)), el('div', { class: 'vpanel-meta' }, subject &&
        viewLink(subject.url, el('span', { class: 'track' }, subject.tracker, ' ', el('b', null, subject.key))), el('span', { class: 'meta', title: new Date(at).toLocaleString() }, v.from ? `read by ${v.from} · ${ago(at)}` : ago(at), v.refreshing ? ' · asked to read it again' : ''), el('span', { class: 'grow' }), el('button', {
        class: 'btn sm',
        type: 'button',
        disabled: v.refreshing,
        title: 'Ask the agent to read it again',
        onclick: () => call('refresh_view', { pane: v.pane, id: v.view.id }),
    }, 'Refresh'), el('button', {
        class: 'btn sm',
        type: 'button',
        title: 'Close this view for every window',
        onclick: () => call('close_view', { pane: v.pane, id: v.view.id }),
    }, 'Close')));
}
function body(v) {
    const drawn = (source) => v.diagrams.find((d) => d.source === source);
    return el('div', { class: 'vpanel-body', 'data-for': key(v) }, blocks(v.view.blocks, key(v), drawn));
}
/** The newest view of a ticket, matched by its URL or its tracker and key. */
export function viewOf(link) {
    return shown.views.find((v) => v.subject !== null &&
        (v.subject.url === link.url ||
            (v.subject.tracker === link.tracker && v.subject.key === link.key)));
}
/** The panel's width as the person dragged it, or the stylesheet's own. */
export function viewsWidth(width) {
    const root = document.documentElement.style;
    if (width === null)
        root.removeProperty('--views-w');
    else
        root.setProperty('--views-w', `${width}px`);
}
/** The panel's left edge: the panel follows the pointer while it is held,
 *  and the width is reported once it is let go, for Rust to hold to its
 *  bounds, keep and send back. A double-click gives back the usual width. */
export function listenToViewsEdge() {
    const sash = $('views-sash');
    sash.addEventListener('mousedown', (e) => {
        if (e.button !== 0)
            return;
        e.preventDefault();
        const right = $('views').getBoundingClientRect().right;
        let width = Math.round(right - e.clientX);
        document.body.classList.add('resizing');
        const move = (m) => {
            width = Math.round(right - m.clientX);
            viewsWidth(width);
        };
        const up = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
            document.body.classList.remove('resizing');
            viewsWidth(state.rail.viewsWidth);
            call('views_width', { width: Math.max(0, width) });
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
    });
    sash.addEventListener('dblclick', () => call('views_width', { width: null }));
}
/** Brings the newest view out, from the tab at the window's edge. */
export function openNewestView() {
    const newest = shown.views[0];
    if (newest)
        call('open_view', { pane: newest.pane, id: newest.view.id });
}
