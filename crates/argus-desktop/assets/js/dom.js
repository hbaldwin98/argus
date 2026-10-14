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
    right: '<svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 3.5L9 7l-3.5 3.5"></path></svg>',
    plus: '<svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M8 3v10M3 8h10"></path></svg>',
    spinner: '<svg class="fg-accent spinner" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><circle cx="20" cy="20" r="15" fill="none" stroke="currentColor" stroke-opacity=".25" stroke-width="3"></circle><path d="M20 5a15 15 0 0 1 15 15" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"></path></svg>',
    folder: '<svg class="fg-faint" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><path d="M5 12h11l3 4h16v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"></path></svg>',
    terminal: '<svg class="fg-faint" width="40" height="40" viewBox="0 0 40 40" aria-hidden="true"><rect x="4" y="7" width="32" height="26" rx="3" fill="none" stroke="currentColor" stroke-width="2"></rect><path d="M11 16l5 4-5 4M19 25h9" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>',
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
export function icon(name) {
    const holder = document.createElement('span');
    holder.innerHTML = ICON[name];
    const svg = holder.firstElementChild;
    if (!svg)
        throw new Error(`the ${name} icon is empty`);
    return svg;
}
