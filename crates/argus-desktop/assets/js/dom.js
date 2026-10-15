// Building the page's elements. Text from the daemon — titles, notes,
// names — only ever goes in as text; markup is only ever the icons below.
export const $ = (id) => {
    const node = document.getElementById(id);
    if (!node)
        throw new Error(`the page has no #${id}`);
    return node;
};
const ICON = {
    down: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 5.5L7 9l3.5-3.5"></path></svg>',
    up: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8 13V3M3.5 7.5L8 3l4.5 4.5"></path></svg>',
    right: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 3.5L9 7l-3.5 3.5"></path></svg>',
    branch: '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="4.5" cy="3.5" r="1.5"></circle><circle cx="4.5" cy="12.5" r="1.5"></circle><circle cx="11.5" cy="5.5" r="1.5"></circle><path d="M4.5 5v6M11.5 7c0 3-7 2-7 4"></path></svg>',
    'folder-sm': '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M2 5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"></path></svg>',
    clock: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="5.5"></circle><path d="M8 5v3l2 1.5"></path></svg>',
    plus: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10"></path></svg>',
    expand: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"></path></svg>',
    split: '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><path d="M8 3v10"></path></svg>',
    dockLeft: '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="2" y="3" width="4.5" height="10" rx="1.5" fill="currentColor"></rect></svg>',
    dockRight: '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="9.5" y="3" width="4.5" height="10" rx="1.5" fill="currentColor"></rect></svg>',
    dockBottom: '<svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="10" rx="1.5"></rect><rect x="2" y="9" width="12" height="4" rx="1.5" fill="currentColor"></rect></svg>',
    close: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7"></path></svg>',
    bell: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10.5h8l-1-1.5V6.5a3 3 0 0 0-6 0V9z"></path><path d="M6.8 12.5a1.3 1.3 0 0 0 2.4 0"></path></svg>',
    'bell-off': '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10.5h8l-1-1.5V6.5a3 3 0 0 0-6 0V9z"></path><path d="M6.8 12.5a1.3 1.3 0 0 0 2.4 0"></path><path d="M3 3l10 10"></path></svg>',
    gear: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2"></circle><path d="M8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2"></path></svg>',
    spinner: '<svg class="fg-accent spinner" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="15" fill="none" stroke="currentColor" stroke-opacity=".25" stroke-width="3"></circle><path d="M20 5a15 15 0 0 1 15 15" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"></path></svg>',
    folder: '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><path d="M5 12h11l3 4h16v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"></path></svg>',
    terminal: '<svg class="empty-icon" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><rect x="4" y="7" width="32" height="26" rx="3" fill="none" stroke="currentColor" stroke-width="2"></rect><path d="M11 16l5 4-5 4M19 25h9" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
    // The icons an extension's view may name for its entry in the sidebar
    // (`argus_protocol::extensions::EXT_ICONS`), and the generic one.
    box: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M8 2l5.5 3v6L8 14l-5.5-3V5z"></path><path d="M2.5 5L8 8l5.5-3M8 8v6"></path></svg>',
    server: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><rect x="2.5" y="2.5" width="11" height="4.5" rx="1"></rect><rect x="2.5" y="9" width="11" height="4.5" rx="1"></rect><path d="M5 4.75h.01M5 11.25h.01"></path></svg>',
    database: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5"><ellipse cx="8" cy="4" rx="5" ry="2"></ellipse><path d="M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4M3 8c0 1.1 2.2 2 5 2s5-.9 5-2"></path></svg>',
    globe: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="5.5"></circle><path d="M2.5 8h11M8 2.5c1.8 1.6 2.6 3.4 2.6 5.5S9.8 11.9 8 13.5C6.2 11.9 5.4 10.1 5.4 8S6.2 4.1 8 2.5z"></path></svg>',
    list: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M6 4h7.5M6 8h7.5M6 12h7.5M2.75 4h.01M2.75 8h.01M2.75 12h.01"></path></svg>',
    rocket: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M9.5 2.5c2 0 4 0 4 0s0 2-0 4L8.5 11.5l-4-4z"></path><path d="M4.5 7.5L2.5 8l2 1.5M8.5 11.5L8 13.5 6.5 11.5M10.5 5.5h.01"></path></svg>',
    console: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5l3.5 3.5L3 11.5M8 12h5"></path></svg>',
    puzzle: '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M3 5h2.5a1.5 1.5 0 1 1 3 0H11v2.5a1.5 1.5 0 1 1 0 3V13H3z"></path></svg>',
};
export function el(tag, props, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(props ?? {})) {
        if (value == null || value === false)
            continue;
        if (key === 'class')
            node.className = String(value);
        else if (key.startsWith('on') && typeof value === 'function') {
            node.addEventListener(key.slice(2), value);
        }
        else
            node.setAttribute(key, value === true ? '' : String(value));
    }
    append(node, children);
    return node;
}
function append(node, children) {
    for (const child of children) {
        if (child == null || child === false)
            continue;
        if (Array.isArray(child))
            append(node, child);
        else if (child instanceof Node)
            node.appendChild(child);
        else
            node.appendChild(document.createTextNode(String(child)));
    }
}
/** Grey bars standing where content will be, one per `[width, indent]`;
 *  a width under 1 is a fraction of the row. */
export function ghosts(bars) {
    return el('div', { class: 'skeleton', 'aria-hidden': 'true' }, bars.map(([width, indent]) => {
        const bar = el('div', { class: 'ghost' });
        bar.style.width = width < 1 ? `${width * 100}%` : `${width}px`;
        if (indent)
            bar.style.marginLeft = `${indent}px`;
        return bar;
    }));
}
export function icon(name) {
    const holder = document.createElement('span');
    holder.innerHTML = ICON[name];
    const svg = holder.firstElementChild;
    if (!svg)
        throw new Error(`the ${name} icon is empty`);
    return svg;
}
